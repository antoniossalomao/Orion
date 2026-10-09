"""Modo pânico e não perturbe (regra 48).

O estado mora no `meta` do banco (`modo:panico`, `modo:nao_perturbe_ate`): vale para o servidor e
para a CLI (`orion panico` grava aqui e o servidor lê a cada uso) e sobrevive a reinício.

**Pânico é corte, não pausa.** Com ele ligado:
- o modelo não vê nem consegue chamar ferramenta `egress`, `exec` ou `external` (`Agent`);
- a memória da tela e a palavra de ativação param; os jobs que usam rede não rodam (`JobRunner`);
- nada volta sozinho: só sai por comando explícito (`orion panico --sair`, `/panico sair` no
  Telegram ou o botão do painel com a senha de novo). Entrada e saída vão para o audit.

**Não perturbe só segura.** Por horário (`ORION_DND_AT=22:30-07:00`) ou até uma hora escolhida:
os avisos não urgentes ficam na fila sem entregar (lembrete e agendamento, que você marcou, são
urgentes) e a memória da tela pausa. Nada é descartado: quando acaba, a fila sai.
"""

from __future__ import annotations

import json
import logging
import time
from collections.abc import Callable
from datetime import datetime, timedelta
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from .memory import MemoryStore

log = logging.getLogger("orion.modos")

CHAVE_PANICO = "modo:panico"
CHAVE_DND = "modo:nao_perturbe_ate"


class ModoError(RuntimeError):
    """Operação recusada (ex.: o audit falhou ao sair do pânico). Mensagem segura de mostrar."""


def _hm(texto: str) -> tuple[int, int]:
    h, m = (int(x) for x in texto.strip().split(":"))
    if not (0 <= h < 24 and 0 <= m < 60):
        raise ValueError(f"horário inválido: {texto!r} (use HH:MM)")
    return h, m


def proxima(hhmm: str, agora: float) -> float:
    """Epoch da próxima vez que o relógio marca `hhmm` (hoje, ou amanhã se já passou)."""
    h, m = _hm(hhmm)
    dt = datetime.fromtimestamp(agora)
    alvo = dt.replace(hour=h, minute=m, second=0, microsecond=0)
    if alvo <= dt:
        alvo += timedelta(days=1)
    return alvo.timestamp()


def na_janela(janela: str, agora: float) -> bool:
    """`22:30-07:00` (atravessa a meia-noite) ou `13:00-14:00`; vazia nunca está."""
    if not janela:
        return False
    ini, fim = (_hm(p) for p in janela.split("-"))
    dt = datetime.fromtimestamp(agora)
    atual = (dt.hour, dt.minute)
    if ini <= fim:
        return ini <= atual < fim
    return atual >= ini or atual < fim


class Modos:
    def __init__(
        self,
        memory: MemoryStore,
        *,
        dnd_at: str = "",
        audit: Callable[[dict[str, Any]], Any] | None = None,
        clock: Callable[[], float] = time.time,
    ) -> None:
        self.memory = memory
        self._janela = dnd_at
        self._audit = audit
        self._clock = clock

    # ── pânico ────────────────────────────────────────────────────────────
    def _estado_panico(self) -> dict[str, Any] | None:
        bruto = self.memory.meta_get(CHAVE_PANICO)
        if not bruto:
            return None
        try:
            dado = json.loads(bruto)
        except ValueError:
            return {"desde": None, "origem": "?"}  # corrompido: continua em pânico (falha fechada)
        return dado if isinstance(dado, dict) else {"desde": None, "origem": "?"}

    def panico(self) -> bool:
        return self._estado_panico() is not None

    def entrar_panico(self, origem: str) -> bool:
        """Liga o pânico (False se já estava). Entrar nunca é barrado: falha do audit só vai ao
        log, porque cortar é sempre mais seguro que continuar."""
        if self.panico():
            return False
        self.memory.meta_set(
            CHAVE_PANICO, json.dumps({"desde": self._clock(), "origem": origem[:30]})
        )
        try:
            self._registrar("entrar", origem)
        except Exception:
            log.exception("audit da entrada no pânico falhou (o pânico está ligado)")
        log.warning("MODO PÂNICO ligado (%s)", origem)
        return True

    def sair_panico(self, origem: str) -> bool:
        """Desliga o pânico (False se não estava). Sem o audit da saída, não sai (regra 8)."""
        if not self.panico():
            return False
        try:
            self._registrar("sair", origem)
        except Exception as e:  # noqa: BLE001 — qualquer falha do audit barra a saída (regra 8)
            raise ModoError(f"não saí do pânico: o audit falhou ({type(e).__name__})") from None
        self.memory.meta_set(CHAVE_PANICO, None)
        log.warning("modo pânico desligado (%s)", origem)
        return True

    def _registrar(self, acao: str, origem: str) -> None:
        if self._audit is not None:
            self._audit(
                {"tool": "modo_panico", "action": acao, "risk": None, "reason": f"origem: {origem}"}
            )

    # ── não perturbe ──────────────────────────────────────────────────────
    def nao_perturbe_ate(self) -> float | None:
        """Até quando vale o não perturbe ligado à mão (None: não está ligado à mão)."""
        bruto = self.memory.meta_get(CHAVE_DND)
        try:
            ate = float(bruto) if bruto else None
        except ValueError:
            return None
        return ate if ate is not None and ate > self._clock() else None

    def nao_perturbe(self) -> bool:
        return self.nao_perturbe_ate() is not None or na_janela(self._janela, self._clock())

    def ligar_nao_perturbe(self, ate: str | float) -> float:
        """`ate`: "HH:MM" (a próxima vez que o relógio marcar) ou epoch. Devolve o epoch."""
        fim = proxima(ate, self._clock()) if isinstance(ate, str) else float(ate)
        if fim <= self._clock():
            raise ValueError("o fim do não perturbe precisa estar no futuro")
        self.memory.meta_set(CHAVE_DND, str(fim))
        return fim

    def desligar_nao_perturbe(self) -> None:
        self.memory.meta_set(CHAVE_DND, None)

    # ── estado (painel, API, Telegram) ────────────────────────────────────
    def estado(self) -> dict[str, Any]:
        p = self._estado_panico()
        ate = self.nao_perturbe_ate()
        return {
            "panico": p is not None,
            "panico_desde": (p or {}).get("desde"),
            "panico_origem": (p or {}).get("origem"),
            "nao_perturbe": self.nao_perturbe(),
            "nao_perturbe_ate": ate,
            "nao_perturbe_horario": self._janela or None,
        }


__all__ = ["CHAVE_DND", "CHAVE_PANICO", "ModoError", "Modos", "na_janela", "proxima"]
