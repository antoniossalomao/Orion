"""Ingestão limitada e recuperável: originais no banco, texto isolado por projeto."""

from __future__ import annotations

import asyncio
import json
import os
import shutil
import subprocess
import sys
import tempfile
import threading
import time
import uuid
from pathlib import Path
from urllib.parse import quote

from fastapi import APIRouter, Depends, HTTPException, Request, Response
from pydantic import BaseModel, Field

from .memory.scope import data_scope

MAX_UPLOAD = 6 * 1024 * 1024
MAX_TOTAL = 256 * 1024 * 1024
_WORKERS = threading.BoundedSemaphore(2)


class DocumentoMover(BaseModel):
    """`project_id` None = memória pessoal. Obrigatório no corpo: omitir não vale como pessoal."""

    project_id: str | None = Field(pattern=r"^[a-f0-9]{32}$")


PRAZO_EXTRACAO_S = 12.0  # PDF com texto; o OCR soma tempo por página (ver `prazo`)
PRAZO_POR_PAGINA_OCR_S = 20.0
DEPOIS_DA_EXTRACAO = 85  # o resto da barra (85 a 100) é a indexação


def prazo(ocr_paginas: int) -> float:
    return PRAZO_EXTRACAO_S + PRAZO_POR_PAGINA_OCR_S * ocr_paginas


def _andamento(memory, id_: str, pct: int) -> None:
    with memory.transaction() as c:
        c.execute("UPDATE uploads SET progress=? WHERE id=?", (max(0, min(100, pct)), id_))


def _extrair(
    raw: bytes, kind: str, *, ocr_paginas: int, ocr_langs: str, avisar
) -> dict[str, object]:
    """Roda o extrator num processo à parte e repassa o andamento (linhas `{"progress": N}`)."""
    tesseract = (shutil.which("tesseract") or "") if ocr_paginas > 0 else ""
    with tempfile.TemporaryDirectory(prefix="orion-document-") as temp:
        input_path = Path(temp) / "input"
        input_path.write_bytes(raw)
        env = {k: os.environ[k] for k in ("SYSTEMROOT", "WINDIR", "TEMP", "TMP") if k in os.environ}
        if tesseract and "TESSDATA_PREFIX" in os.environ:
            env["TESSDATA_PREFIX"] = os.environ["TESSDATA_PREFIX"]
        argv = [sys.executable, "-I", "-m", "orion.document_extract", str(input_path), kind]
        if tesseract:
            argv += [tesseract, str(ocr_paginas), ocr_langs]
        limite = time.monotonic() + prazo(ocr_paginas if tesseract else 0)
        resultado: dict[str, object] = {"error": "document_extraction_failed"}
        saida: list[bytes] = []
        try:
            with subprocess.Popen(  # noqa: S603 — argv fixo, sem shell
                argv, env=env, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL
            ) as proc:

                def ler() -> None:
                    assert proc.stdout is not None
                    total = 0
                    for linha in proc.stdout:
                        total += len(linha)
                        if total > 14 * 1024 * 1024:
                            proc.kill()
                            return
                        saida.append(linha)
                        if linha.startswith(b'{"progress"'):
                            try:
                                avisar(int(json.loads(linha)["progress"]))
                            except (ValueError, KeyError, TypeError):
                                pass

                leitor = threading.Thread(target=ler, daemon=True)
                leitor.start()
                try:
                    proc.wait(timeout=max(0.1, limite - time.monotonic()))
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait()
                    leitor.join(2)
                    return {"error": "document_extraction_timeout"}
                leitor.join(5)
            ultima = next((x for x in reversed(saida) if not x.startswith(b'{"progress"')), b"")
            if proc.returncode == 0 and ultima:
                resultado = json.loads(ultima)
        except (OSError, ValueError):
            pass
        return resultado


def process(
    memory,
    id_: str,
    project_id: str | None,
    *,
    ocr_paginas: int = 0,
    ocr_langs: str = "por+eng",
):
    with _WORKERS:
        rows = memory.query(
            "SELECT * FROM uploads WHERE id=? AND project_id IS ?", (id_, project_id)
        )
        if not rows:
            raise HTTPException(404, "document_not_found")
        row = dict(rows[0])
        with memory.transaction() as c:
            c.execute(
                "UPDATE uploads SET status='processing',error=NULL,progress=0 WHERE id=?", (id_,)
            )
        result = _extrair(
            row["raw"],
            row["kind"],
            ocr_paginas=ocr_paginas,
            ocr_langs=ocr_langs,
            avisar=lambda pct: _andamento(memory, id_, pct * DEPOIS_DA_EXTRACAO // 100),
        )
        text = result.get("text")
        if isinstance(text, str):
            _andamento(memory, id_, DEPOIS_DA_EXTRACAO)
            with data_scope(project_id, include_personal=False):
                memory.index_document(f"upload:{id_}/{row['name']}", row["name"], text)
        with memory.transaction() as c:
            c.execute(
                "UPDATE uploads SET status=?,error=?,progress=100 WHERE id=?",
                ("ready" if isinstance(text, str) else "error", result.get("error"), id_),
            )


ESPERA_S = 2.0  # o upload espera isto pela indexação; se demora, segue em segundo plano
_TAREFAS: set[asyncio.Task] = set()


def recuperar_interrompidos(memory) -> int:
    """Documento que ficou `processing`/`received` porque o Orion caiu no meio vira erro (com
    "Tentar de novo"), em vez de uma barra de progresso parada para sempre."""
    with memory.transaction() as c:
        return c.execute(
            "UPDATE uploads SET status='error', error='document_interrupted', progress=100"
            " WHERE status IN ('processing','received')"
        ).rowcount


def router(require_admin):
    api = APIRouter(prefix="/documents", dependencies=[Depends(require_admin)])

    async def rodar(s, id_: str, project_id: str | None, *, espera: float = ESPERA_S) -> None:
        """Indexa em segundo plano (E4.3). Espera um pouco para o caso comum (arquivo pequeno)
        voltar já pronto; o resto o front acompanha pelo `progress`."""
        if s.memory.query("SELECT 1 FROM uploads WHERE id=? AND status='processing'", (id_,)):
            raise HTTPException(409, "document_busy")
        tarefa = asyncio.create_task(
            asyncio.to_thread(
                process,
                s.memory,
                id_,
                project_id,
                ocr_paginas=s.settings.ocr_max_pages,
                ocr_langs=s.settings.ocr_langs,
            )
        )
        _TAREFAS.add(tarefa)
        tarefa.add_done_callback(_TAREFAS.discard)
        try:
            await asyncio.wait_for(asyncio.shield(tarefa), espera)
        except TimeoutError:
            tarefa.add_done_callback(lambda t: t.cancelled() or t.exception())  # sem aviso solto

    def validate(s, project_id):
        if project_id:
            rows = s.memory.query("SELECT archived FROM projects WHERE id=?", (project_id,))
            if not rows or rows[0][0]:
                raise HTTPException(409, "project_missing_or_archived")

    def rows(s, project_id):
        return [
            dict(r)
            for r in s.memory.query(
                "SELECT id,name,kind,status,error,progress,created_at,length(raw) AS size FROM "
                "uploads WHERE project_id IS ? ORDER BY created_at DESC LIMIT 200",
                (project_id,),
            )
        ]

    @api.get("")
    def listing(request: Request, project_id: str | None = None):
        return rows(request.app.state.orion, project_id)

    @api.post("")
    async def upload(request: Request, name: str, project_id: str | None = None):
        s = request.app.state.orion
        validate(s, project_id)
        kind = Path(name).suffix.lower()
        if (
            not name.strip()
            or len(name) > 160
            or any(ord(ch) < 32 or ch in "/\\" for ch in name)
            or kind not in {".pdf", ".txt", ".md"}
        ):
            raise HTTPException(422, "document_type_or_name_invalid")
        raw = bytearray()
        async for part in request.stream():
            raw.extend(part)
            if len(raw) > MAX_UPLOAD:
                raise HTTPException(413, "document_size_limit")
        if not raw:
            raise HTTPException(422, "document_empty")
        id_ = uuid.uuid4().hex
        with s.memory.transaction() as c:
            total = c.execute("SELECT COALESCE(sum(length(raw)),0) FROM uploads").fetchone()[0]
            if total + len(raw) > MAX_TOTAL:
                raise HTTPException(413, "document_storage_limit")
            c.execute(
                "INSERT INTO uploads(id,project_id,name,kind,raw,status,created_at) "
                "VALUES (?,?,?,?,?,?,?)",
                (id_, project_id, name, kind, bytes(raw), "received", s.memory.clock()),
            )
        await rodar(s, id_, project_id)
        return next(r for r in rows(s, project_id) if r["id"] == id_)

    @api.post("/{id_}/retry")
    async def retry(id_: str, request: Request, project_id: str | None = None):
        s = request.app.state.orion
        validate(s, project_id)
        await rodar(s, id_, project_id)
        return next(r for r in rows(s, project_id) if r["id"] == id_)

    @api.get("/{id_}")
    def detalhe(id_: str, request: Request, project_id: str | None = None):
        """Estado de um documento (status e `progress` de 0 a 100): o front consulta a cada 1 s
        enquanto ele indexa."""
        achado = [r for r in rows(request.app.state.orion, project_id) if r["id"] == id_]
        if not achado:
            raise HTTPException(404, "document_not_found")
        return achado[0]

    @api.post("/{id_}/reindex")
    async def reindexar(id_: str, request: Request, project_id: str | None = None):
        """Apaga os trechos e os vetores do documento e indexa de novo a partir do original
        (por exemplo, depois de instalar o tesseract para um PDF escaneado)."""
        s = request.app.state.orion
        validate(s, project_id)
        achados = s.memory.query(
            "SELECT name, status FROM uploads WHERE id=? AND project_id IS ?", (id_, project_id)
        )
        if not achados:
            raise HTTPException(404, "document_not_found")
        if achados[0]["status"] == "processing":
            raise HTTPException(409, "document_busy")
        with data_scope(project_id, include_personal=False):
            s.memory.remove_document(f"upload:{id_}/{achados[0]['name']}")
        await rodar(s, id_, project_id)
        return next(r for r in rows(s, project_id) if r["id"] == id_)

    @api.patch("/{id_}")
    async def mover(
        id_: str, corpo: DocumentoMover, request: Request, project_id: str | None = None
    ):
        """ "Disponível em": muda o escopo do documento. O texto indexado é refeito a partir do
        original no escopo novo; o do escopo antigo sai da busca antes (nunca fica nos dois)."""
        s = request.app.state.orion
        validate(s, corpo.project_id)
        if corpo.project_id == project_id:
            return next(r for r in rows(s, project_id) if r["id"] == id_)
        achados = s.memory.query(
            "SELECT name FROM uploads WHERE id=? AND project_id IS ?", (id_, project_id)
        )
        if not achados:
            raise HTTPException(404, "document_not_found")
        with data_scope(project_id, include_personal=False):
            s.memory.remove_document(f"upload:{id_}/{achados[0][0]}")
        with s.memory.transaction() as c:
            c.execute("UPDATE uploads SET project_id=? WHERE id=?", (corpo.project_id, id_))
        await rodar(s, id_, corpo.project_id)
        return next(r for r in rows(s, corpo.project_id) if r["id"] == id_)

    @api.get("/{id_}/download")
    def download(id_: str, request: Request, project_id: str | None = None):
        records = request.app.state.orion.memory.query(
            "SELECT raw,name FROM uploads WHERE id=? AND project_id IS ?", (id_, project_id)
        )
        if not records:
            raise HTTPException(404, "document_not_found")
        raw, name = records[0]
        return Response(
            raw,
            media_type="application/octet-stream",
            headers={
                "Content-Disposition": f"attachment; filename*=UTF-8''{quote(name)}",
                "X-Content-Type-Options": "nosniff",
                "Cache-Control": "no-store",
                "Content-Security-Policy": "default-src 'none'; sandbox",
            },
        )

    return api
