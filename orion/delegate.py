"""Delegação de tarefa pesada às CLIs oficiais (claude -p, codex exec, gemini -p).

Usa as assinaturas que o Antônio já paga pelo caminho oficial, sem token
extraído. Cada chamada tem pasta de trabalho, tempo-limite e contador de uso
diário; se a cota de uma acabar, tenta a próxima. Nunca passa por shell: o
prompt vai como UM argumento. Ferramenta de classe EXEC: sempre pede confirmação.
"""

from __future__ import annotations

import logging
import os
import re
import shutil
import subprocess
import time
from collections.abc import Callable, Mapping
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import Any

from .memory import MemoryStore

log = logging.getLogger("orion.delegate")

_COTA = re.compile(
    r"quota|rate.?limit|usage limit|too many requests|limit reached|\b429\b|exceeded", re.I
)


@dataclass(frozen=True)
class CliAgent:
    name: str
    argv: tuple[str, ...]  # "{prompt}" vira o texto da tarefa
    daily_limit: int = 20


DEFAULT_AGENTS = (
    CliAgent("claude", ("claude", "-p", "{prompt}")),
    CliAgent("codex", ("codex", "exec", "{prompt}")),
    CliAgent("gemini", ("gemini", "-p", "{prompt}")),
)


class Delegator:
    def __init__(
        self,
        store: MemoryStore,
        agents: tuple[CliAgent, ...] = DEFAULT_AGENTS,
        timeout_s: float = 600.0,
        max_output: int = 20_000,
        run: Callable[..., subprocess.CompletedProcess[str]] = subprocess.run,
        which: Callable[[str], str | None] = shutil.which,
        clock: Callable[[], float] = time.time,
        env: Mapping[str, str] | None = None,
    ) -> None:
        self._store = store
        self._agents = {a.name: a for a in agents}
        self._order = [a.name for a in agents]
        self._timeout = timeout_s
        self._max = max_output
        self._run = run
        self._which = which
        self._clock = clock
        # O segredo do próprio Orion não vai para o processo filho (regra 5).
        base = os.environ if env is None else env
        self._env = {k: v for k, v in base.items() if not k.upper().startswith("ORION_")}

    def _chave(self, nome: str) -> str:
        return f"delegar:{nome}:{datetime.fromtimestamp(self._clock()).strftime('%Y%m%d')}"

    def delegate(self, tarefa: str, pasta: str, agente: str | None = None) -> dict[str, Any]:
        if not tarefa.strip():
            return {"ok": False, "erro": "tarefa vazia"}
        raiz = Path(pasta).expanduser()
        if not raiz.is_dir():
            return {"ok": False, "erro": f"pasta inexistente: {pasta}"}
        if agente and agente not in self._agents:
            return {
                "ok": False,
                "erro": f"agente desconhecido: {agente} (use {', '.join(self._order)})",
            }

        ordem = ([agente] if agente else []) + [n for n in self._order if n != agente]
        tentativas: list[dict[str, str]] = []
        for nome in ordem:
            ag = self._agents[nome]
            if self._which(ag.argv[0]) is None:
                tentativas.append({"agente": nome, "motivo": "CLI não instalada"})
                continue
            if self._store.counter_get(self._chave(nome)) >= ag.daily_limit:
                tentativas.append({"agente": nome, "motivo": "limite diário atingido"})
                continue
            self._store.counter_incr(self._chave(nome))
            # "Tarefa:" evita que um prompt iniciado por "-" vire opção da CLI.
            argv = [a.replace("{prompt}", f"Tarefa: {tarefa}") for a in ag.argv]
            try:
                r = self._run(
                    argv,
                    cwd=str(raiz),
                    capture_output=True,
                    text=True,
                    timeout=self._timeout,
                    stdin=subprocess.DEVNULL,
                    env=self._env,
                    check=False,
                    encoding="utf-8",
                    errors="replace",
                )
            except subprocess.TimeoutExpired:
                return {
                    "ok": False,
                    "agente": nome,
                    "erro": f"tempo-limite de {self._timeout:.0f}s",
                    "tentativas": tentativas,
                }
            except OSError as e:
                tentativas.append({"agente": nome, "motivo": f"não executou: {e}"})
                continue
            saida = (r.stdout or "").strip()
            if r.returncode == 0:
                return {
                    "ok": True,
                    "agente": nome,
                    "saida": saida[: self._max],
                    "truncado": len(saida) > self._max,
                    "tentativas": tentativas,
                }
            erro = (r.stderr or saida).strip()
            if _COTA.search(erro):
                self._store.counter_set(self._chave(nome), ag.daily_limit)  # esgotada até amanhã
                tentativas.append({"agente": nome, "motivo": "cota esgotada"})
                log.warning("cota da CLI %s esgotada; tentando a próxima", nome)
                continue
            return {
                "ok": False,
                "agente": nome,
                "erro": erro[:2000] or f"código {r.returncode}",
                "tentativas": tentativas,
            }
        return {"ok": False, "erro": "nenhum agente disponível", "tentativas": tentativas}
