"""Jobs dentro do processo (fase 3–4 do NUCLEO): o que o `proactive_loop` do legado fazia.

Um laço assíncrono chama `tick()` a cada poucos segundos. Cada passo falha sozinho
(um erro no backup não impede o aviso de lembrete) e o resultado fica no relatório:

- lembretes vencidos → fila de avisos (um por lembrete, uma vez só);
- agendamentos vencidos → fila de avisos e próximo horário (a ferramenta registrada
  NÃO é executada: ação sem alguém olhando exige a política e a aprovação, fase 4);
- embeddings pendentes (a API gratuita pode ter caído);
- vault do Obsidian reindexado;
- backup diário da memória, mantendo os últimos N;
- consolidação das conversas em fatos (se houver gateway).

Quem entrega os avisos é o canal (Telegram, web) lendo `GET /notifications`.
"""

from __future__ import annotations

import asyncio
import logging
import time
from collections.abc import Callable
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path
from typing import Any

from .memory import MemoryStore
from .memory.consolidate import Consolidator
from .memory.ops import Operations
from .memory.scope import data_scope

log = logging.getLogger("orion.jobs")


@dataclass
class TickReport:
    lembretes: int = 0
    agendamentos: int = 0
    embeddings: int = 0
    vault: dict[str, int] | None = None
    backup: str | None = None
    consolidacao: dict[str, Any] | None = None
    erros: list[str] = field(default_factory=list)


class JobRunner:
    def __init__(
        self,
        memory: MemoryStore,
        ops: Operations,
        *,
        backup_dir: Path | None = None,
        backup_keep: int = 7,
        vault_dir: Path | None = None,
        consolidator: Consolidator | None = None,
        embed_every_s: float = 300.0,
        vault_every_s: float = 3600.0,
        backup_every_s: float = 3600.0,
        consolidate_every_s: float = 6 * 3600.0,
        clock: Callable[[], float] = time.time,
    ) -> None:
        self.memory, self.ops = memory, ops
        self._backup_dir, self._backup_keep = backup_dir, backup_keep
        self._vault_dir = vault_dir
        self._consolidator = consolidator
        self._clock = clock
        self._every = {
            "embed": embed_every_s,
            "vault": vault_every_s,
            "backup": backup_every_s,
            "consolidar": consolidate_every_s,
        }
        self._last: dict[str, float] = {}

    def _devido(self, passo: str) -> bool:
        agora = self._clock()
        if agora - self._last.get(passo, float("-inf")) < self._every[passo]:
            return False
        self._last[passo] = agora
        return True

    async def tick(self) -> TickReport:
        rel = TickReport()
        await asyncio.to_thread(self._passos_sincronos, rel)
        if self._consolidator is not None and self._devido("consolidar"):
            try:
                r = await self._consolidator.run()
                rel.consolidacao = {"ran": r.ran, "falas": r.messages, "fatos": r.facts_added}
                if not r.ok:
                    self._last["consolidar"] = self._clock() - self._every["consolidar"] * 0.8
            except Exception as e:
                log.exception("job consolidação falhou")
                rel.erros.append(f"consolidação: {e}")
        return rel

    def _passos_sincronos(self, rel: TickReport) -> None:
        scopes = [None] + [
            r[0] for r in self.memory.query("SELECT id FROM projects WHERE archived=0")
        ]
        for project_id in scopes:
            with data_scope(project_id, include_personal=False):
                for name, fn in (
                    ("lembretes", self._lembretes),
                    ("agendamentos", self._agendamentos),
                ):
                    try:
                        setattr(rel, name, getattr(rel, name) + fn())
                    except Exception:
                        log.exception("job %s falhou", name)
                        rel.erros.append(f"{name}: falha no processamento")
        if self._devido("embed"):
            self._passo(rel, "embeddings", self.memory.embed_pending)
        if self._vault_dir is not None and self._devido("vault"):
            self._passo(rel, "vault", lambda: self.memory.index_vault(self._vault_dir or "."))
        if self._backup_dir is not None and self._devido("backup"):
            self._passo(rel, "backup", self._backup)

    @staticmethod
    def _passo(rel: TickReport, nome: str, fn: Callable[[], Any]) -> None:
        try:
            setattr(rel, nome, fn())
        except Exception as e:
            log.exception("job %s falhou", nome)
            rel.erros.append(f"{nome}: {e}")

    def _lembretes(self) -> int:
        n = 0
        for r in self.ops.due_reminders(self._clock()):
            quando = datetime.fromtimestamp(r["due_at"]).strftime("%d/%m %H:%M")
            texto = f"Lembrete ({quando}): {r['title']}" + (f" — {r['note']}" if r["note"] else "")
            self.ops.notify("lembrete", texto, ref=f"reminder:{r['id']}")
            self.ops.mark_reminder_notified(r["id"])
            n += 1
        return n

    def _agendamentos(self) -> int:
        n = 0
        agora = self._clock()
        for s in self.ops.due_schedules(agora):
            texto = f"Agendamento: {s['title']}"
            if s["tool"]:
                texto += f" (ferramenta '{s['tool']}' registrada; não é executada sozinha)"
            self.ops.notify("agendamento", texto, ref=f"schedule:{s['id']}:{s['next_run']}")
            self.ops.mark_schedule_fired(s["id"], agora)
            n += 1
        return n

    def _backup(self) -> str | None:
        feito = self.memory.daily_backup(self._backup_dir or ".", manter=self._backup_keep)
        return str(feito) if feito else None

    async def run_forever(self, tick_s: float) -> None:
        """Laço do lifespan: primeira rodada depois de `tick_s` (a subida não espera job)."""
        while True:
            await asyncio.sleep(tick_s)
            try:
                rel = await self.tick()
                if rel.erros:
                    log.warning("jobs com erro: %s", "; ".join(rel.erros))
            except asyncio.CancelledError:
                raise
            except Exception:
                log.exception("rodada de jobs falhou")
