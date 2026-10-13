"""Biblioteca de resultados (C34): o que o Orion gerou fica numa pasta própria, com versões.

Depois de `gerar_documento` ou `gerar_imagem` rodarem (sob a política, com aprovação onde havia),
o agente chama `Library.registrar`, que **copia** o arquivo para `<dados>/resultados/` com nome
gerado aqui (`<id>-<nome seguro>`) e registra de onde veio (conversa, projeto, ferramenta). Por
isso a API só serve arquivos que moram nessa pasta: nenhum caminho vindo do modelo é lido de volta.
Mesmo nome gerado de novo vira a versão seguinte; as antigas ficam. HTML e SVG não são exibidos
no navegador: a API entrega tudo como download (ou como texto/imagem raster para a prévia).
"""

from __future__ import annotations

import difflib
import json
import logging
import re
import shutil
from pathlib import Path
from typing import Any

from .memory import MemoryStore

log = logging.getLogger("orion.resultados")

MAX_COPIA = 25 * 1024 * 1024
PREVIA_TEXTO = (".txt", ".md", ".csv", ".json")
PREVIA_PDF = ".pdf"
MAX_DIFF_LINHAS = 2000
PREVIA_IMAGEM = {
    ".png": "image/png",
    ".jpg": "image/jpeg",
    ".jpeg": "image/jpeg",
    ".webp": "image/webp",
}
_NOME = re.compile(r"[^A-Za-z0-9._ -]+")


def nome_seguro(nome: str) -> str:
    limpo = _NOME.sub("_", Path(nome).name).strip(" .") or "resultado"
    return limpo[:80]


class Library:
    def __init__(self, memory: MemoryStore, pasta: Path) -> None:
        self.memory, self.pasta = memory, pasta

    def registrar(self, session_id: str | None, tool: str, resultado: str) -> dict[str, Any] | None:
        """Lê o JSON devolvido pela ferramenta e, se ela gerou um arquivo, guarda a cópia."""
        try:
            dados = json.loads(resultado)
        except ValueError:
            return None
        if not isinstance(dados, dict) or not dados.get("ok"):
            return None
        origem: Path | None
        if tool == "gerar_documento" and isinstance(dados.get("path"), str):
            origem, kind = Path(dados["path"]), "documento"
        elif tool == "gerar_imagem" and isinstance(dados.get("arquivo"), str):
            origem, kind = self._imagens / Path(dados["arquivo"]).name, "imagem"
        else:
            return None
        try:
            if not origem.is_file() or origem.stat().st_size > MAX_COPIA:
                return None
            nome = nome_seguro(origem.name)
            self.pasta.mkdir(parents=True, exist_ok=True)
            tamanho = origem.stat().st_size
            prov = self.memory.add_artifact(
                session_id=session_id, kind=kind, name=nome, stored="", size=tamanho, tool=tool
            )
            guardado = f"{prov['id']}-{nome}"
            shutil.copyfile(origem, self.pasta / guardado)
            with self.memory.transaction() as c:
                c.execute("UPDATE library_results SET stored=? WHERE id=?", (guardado, prov["id"]))
            return self.memory.get_artifact(prov["id"])
        except OSError:
            log.warning("não consegui guardar o resultado na biblioteca", exc_info=True)
            return None

    @property
    def _imagens(self) -> Path:
        return self.pasta.parent / "imagens"

    def caminho(self, aid: int) -> Path | None:
        """Arquivo da biblioteca, só se ainda existir e estiver dentro da pasta dela."""
        a = self.memory.get_artifact(aid)
        if not a or not a["stored"]:
            return None
        alvo = (self.pasta / a["stored"]).resolve()
        if not alvo.is_relative_to(self.pasta.resolve()) or not alvo.is_file():
            return None
        return alvo

    def apagar(self, aid: int) -> bool:
        guardado = self.memory.delete_artifact(aid)
        if guardado is None:
            return False
        alvo = (self.pasta / guardado).resolve()
        if guardado and alvo.is_relative_to(self.pasta.resolve()):
            alvo.unlink(missing_ok=True)
        return True

    def diff(self, aid: int, com: int) -> dict[str, Any] | None:
        """Diff de linhas entre duas versões de texto (E6.2): `com` é a base, `aid` o novo.
        None se um dos dois não existe; `ValueError` se algum não é texto."""
        novo, base = self.caminho(aid), self.caminho(com)
        if novo is None or base is None:
            return None
        if novo.suffix.lower() not in PREVIA_TEXTO or base.suffix.lower() not in PREVIA_TEXTO:
            raise ValueError("só texto, Markdown e CSV têm diff (imagem: compare lado a lado)")
        a = base.read_text(encoding="utf-8", errors="replace").splitlines()
        b = novo.read_text(encoding="utf-8", errors="replace").splitlines()
        linhas = list(difflib.unified_diff(a, b, "base", "novo", lineterm="", n=3))
        return {
            "de": com,
            "para": aid,
            "iguais": not linhas,
            "adicionadas": sum(1 for x in linhas if x.startswith("+") and not x.startswith("+++")),
            "removidas": sum(1 for x in linhas if x.startswith("-") and not x.startswith("---")),
            "linhas": linhas[:MAX_DIFF_LINHAS],
            "truncado": len(linhas) > MAX_DIFF_LINHAS,
        }

    def limpar(
        self, manter: int, nome: str | None = None, *, simular: bool = False
    ) -> dict[str, Any]:
        """Apaga as versões além das `manter` mais novas de cada resultado (E6.3). Com `simular`,
        só conta. Nada fora da pasta da biblioteca é tocado."""
        sobra = self.memory.surplus_artifacts(manter, nome)
        total = {"total": len(sobra), "bytes": sum(int(x["bytes"]) for x in sobra)}
        if simular:
            return {**total, "apagados": 0}
        apagados = 0
        for x in sobra:
            if self.apagar(int(x["id"])):
                apagados += 1
        return {**total, "apagados": apagados}
