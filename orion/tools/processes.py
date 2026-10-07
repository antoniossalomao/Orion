"""Processos em segundo plano do `orion-desktop` (fase 4): `iniciar_processo_bg`,
`status_processo_bg`, `listar_processos_bg` e a vigilância de pastas.

`iniciar_processo_bg` é execução: a política classifica o comando como `executar_comando`
(leitura provada roda direto; o resto pede confirmação). O processo roda com o ambiente limpo
(sem `ORION_*` nem segredos), sem entrada padrão, com a saída num arquivo de log só do dono, e
sobrevive ao Orion (não é encerrado no desligamento). A lista de processos vive na memória do
Orion: reiniciar o Orion esquece a lista, não o processo nem o log.

Quando um processo termina, o `JobRunner` chama `finished()` e avisa pela fila de notificações.
A vigilância de pastas é de `Operations` (`watch_*`), que guarda a lista no banco.
"""

from __future__ import annotations

import contextlib
import re
import subprocess
import sys
import threading
import time
from collections.abc import Callable
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from ..memory.ops import Operations
from ..policy.audit import redact
from .desktop import argv_do_shell, env_limpo
from .registry import Tool

MAX_ATIVOS = 5
MAX_LOG_LIDO = 16_000  # bytes do fim do log que `status` lê


@dataclass
class Proc:
    id: int
    nome: str
    comando: str
    pid: int
    log: Path
    iniciado: float
    popen: Any
    notificar: bool
    notificado: bool = False
    codigo: int | None = field(default=None)

    def atualizar(self) -> int | None:
        if self.codigo is None:
            self.codigo = self.popen.poll()
        return self.codigo


def _cauda(caminho: Path, linhas: int) -> str:
    try:
        with caminho.open("rb") as f:
            f.seek(0, 2)
            tamanho = f.tell()
            f.seek(max(0, tamanho - MAX_LOG_LIDO))
            bruto = f.read()
    except OSError:
        return ""
    texto = bruto.decode("utf-8", errors="replace")
    return "\n".join(texto.splitlines()[-max(1, linhas) :])


class ProcessManager:
    def __init__(
        self,
        log_dir: Path,
        *,
        windows: bool = sys.platform == "win32",
        launcher: Callable[..., Any] = subprocess.Popen,
        clock: Callable[[], float] = time.time,
        max_ativos: int = MAX_ATIVOS,
        manter_logs_dias: int = 7,
    ) -> None:
        self._dir = log_dir
        self._windows = windows
        self._launcher = launcher
        self._clock = clock
        self._max = max_ativos
        self._keep_s = manter_logs_dias * 86400
        self._lock = threading.Lock()
        self._procs: dict[int, Proc] = {}
        self._seq = 0

    def start(self, nome: str, comando: str, notificar_ao_concluir: bool = True) -> dict[str, Any]:
        nome = nome.strip()[:60] or "processo"
        argv = argv_do_shell(comando, windows=self._windows)
        if argv is None:
            return {"erro": "nenhum shell encontrado (PowerShell no Windows, sh no macOS/Linux)"}
        with self._lock:
            if sum(1 for p in self._procs.values() if p.atualizar() is None) >= self._max:
                return {"erro": f"já há {self._max} processos em segundo plano rodando"}
            self._seq += 1
            pid_interno = self._seq
        self._dir.mkdir(parents=True, exist_ok=True)
        self._podar_logs()
        seguro = re.sub(r"[^A-Za-z0-9_-]", "_", nome)[:40]
        log = self._dir / f"{int(self._clock())}_{pid_interno}_{seguro}.log"
        extra: dict[str, Any] = (
            {"creationflags": getattr(subprocess, "CREATE_NO_WINDOW", 0)}
            if self._windows
            else {"start_new_session": True}
        )
        try:
            with log.open("wb") as saida:
                with contextlib.suppress(OSError):
                    log.chmod(0o600)
                popen = self._launcher(
                    argv,
                    cwd=Path.home(),
                    env=env_limpo(),
                    stdin=subprocess.DEVNULL,
                    stdout=saida,
                    stderr=subprocess.STDOUT,
                    **extra,
                )
        except OSError as e:
            return {"erro": f"não foi possível iniciar: {type(e).__name__}"}
        proc = Proc(
            pid_interno, nome, comando, popen.pid, log, self._clock(), popen, notificar_ao_concluir
        )
        with self._lock:
            self._procs[pid_interno] = proc
        return {"ok": True, "processo": self._resumo(proc)}

    def _resumo(self, p: Proc) -> dict[str, Any]:
        codigo = p.atualizar()
        return {
            "id": p.id,
            "nome": p.nome,
            "pid": p.pid,
            "status": "rodando" if codigo is None else ("concluido" if codigo == 0 else "erro"),
            "codigo": codigo,
            "iniciado": p.iniciado,
            "comando": redact(p.comando, 300),
        }

    def status(self, processo_id: int, linhas_log: int = 30) -> dict[str, Any]:
        p = self._procs.get(int(processo_id))
        if p is None:
            return {
                "erro": f"processo {processo_id} desconhecido (a lista zera ao reiniciar o Orion)"
            }
        return {
            "ok": True,
            **self._resumo(p),
            "log": redact(_cauda(p.log, min(int(linhas_log), 200)), MAX_LOG_LIDO),
            "arquivo_log": str(p.log),
        }

    def listar(self, somente_ativos: bool = True) -> dict[str, Any]:
        with self._lock:
            lista = [self._resumo(p) for p in self._procs.values()]
        if somente_ativos:
            lista = [p for p in lista if p["status"] == "rodando"]
        return {"ok": True, "total": len(lista), "processos": lista}

    def finished(self) -> list[Proc]:
        """Processos que terminaram desde a última chamada e pediram aviso (uma vez só)."""
        with self._lock:
            novos = [
                p for p in self._procs.values() if p.atualizar() is not None and not p.notificado
            ]
            for p in novos:
                p.notificado = True
        return [p for p in novos if p.notificar]

    def _podar_logs(self) -> None:
        corte = self._clock() - self._keep_s
        for velho in self._dir.glob("*.log"):
            with contextlib.suppress(OSError):
                if velho.stat().st_mtime < corte:
                    velho.unlink()


def process_tools(manager: ProcessManager, ops: Operations) -> list[Tool]:
    def iniciar_processo_bg(
        nome: str, comando: str, notificar_ao_concluir: bool = True
    ) -> dict[str, Any]:
        return manager.start(nome, comando, notificar_ao_concluir)

    def status_processo_bg(processo_id: int, linhas_log: int = 30) -> dict[str, Any]:
        return manager.status(processo_id, linhas_log)

    def listar_processos_bg(somente_ativos: bool = True) -> dict[str, Any]:
        return manager.listar(somente_ativos)

    def iniciar_vigilancia_pasta(pasta: str) -> dict[str, Any]:
        try:
            return {"ok": True, "vigilancia": ops.watch_add(pasta)}
        except ValueError as e:
            return {"erro": str(e), "ok": False}

    def parar_vigilancia_pasta(pasta: str) -> dict[str, Any]:
        return {"ok": ops.watch_remove(pasta), "pasta": pasta}

    def listar_vigilancias() -> dict[str, Any]:
        lista = ops.watch_list()
        return {"ok": True, "total": len(lista), "vigilancias": lista}

    obj = "object"
    return [
        Tool(
            "iniciar_processo_bg",
            "Roda um comando demorado em segundo plano (a saída vai para um log). Pede confirmação "
            "como executar_comando; avisa quando terminar.",
            {
                "type": obj,
                "properties": {
                    "nome": {"type": "string"},
                    "comando": {"type": "string"},
                    "notificar_ao_concluir": {"type": "boolean"},
                },
                "required": ["nome", "comando"],
            },
            iniciar_processo_bg,
        ),
        Tool(
            "status_processo_bg",
            "Estado de um processo em segundo plano e o fim do log.",
            {
                "type": obj,
                "properties": {
                    "processo_id": {"type": "integer"},
                    "linhas_log": {"type": "integer"},
                },
                "required": ["processo_id"],
            },
            status_processo_bg,
        ),
        Tool(
            "listar_processos_bg",
            "Lista os processos em segundo plano (por padrão só os que ainda rodam).",
            {"type": obj, "properties": {"somente_ativos": {"type": "boolean"}}},
            listar_processos_bg,
        ),
        Tool(
            "iniciar_vigilancia_pasta",
            "Passa a avisar quando aparecerem arquivos novos numa pasta (checagem a cada poucos "
            "segundos; só os nomes).",
            {"type": obj, "properties": {"pasta": {"type": "string"}}, "required": ["pasta"]},
            iniciar_vigilancia_pasta,
        ),
        Tool(
            "parar_vigilancia_pasta",
            "Para de vigiar uma pasta.",
            {"type": obj, "properties": {"pasta": {"type": "string"}}, "required": ["pasta"]},
            parar_vigilancia_pasta,
        ),
        Tool(
            "listar_vigilancias",
            "Lista as pastas vigiadas.",
            {"type": obj, "properties": {}},
            listar_vigilancias,
        ),
    ]
