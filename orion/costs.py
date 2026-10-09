"""Custo zero e cota gratuita (regra 46).

O Orion só fala com provedores de cota gratuita, modelo local e as CLIs das assinaturas (que não
incluem API). Este módulo conta quanto da cota gratuita **o próprio Orion** já usou, a partir do
registro de saída (`external_calls`, regra 47): não é a cota que o provedor vê (outro programa com
a mesma chave também gasta), é o que passou por aqui.

- `QUOTAS`: limite por provedor, por dia (Brave por mês), configurável por `ORION_QUOTA_*`. Os
  padrões são **estimativas** do gratuito de cada um (mudam sem aviso: confira no painel deles).
- `pode_usar(provedor, opcional=True)`: job opcional (pesquisa noturna, ciclo de sono, leitura
  semanal, consolidação) não roda com 90% usado, e o aviso `cota` sai uma vez por dia.
- O chat nunca é bloqueado (`opcional=False` sempre pode); passar de 100% vira aviso no painel.
- `PROVEDORES_GRATUITOS`: hosts que contam como gratuitos; o `orion doctor` avisa se um endereço
  configurado sair dessa lista sem `ORION_ALLOW_PAID=true`.
"""

from __future__ import annotations

import logging
import time
from collections.abc import Callable
from dataclasses import dataclass
from datetime import datetime
from typing import TYPE_CHECKING, Any, Literal
from urllib.parse import urlsplit

if TYPE_CHECKING:
    from .config import Settings
    from .memory import MemoryStore
    from .memory.ops import Operations

log = logging.getLogger("orion.costs")

# Host -> o que é. Local (OmniRoute, Ollama) é gratuito por definição: quem decide para onde o
# OmniRoute manda é a configuração dele (decisão #2 do NUCLEO: só combos de free tier).
PROVEDORES_GRATUITOS = frozenset(
    {
        "127.0.0.1",
        "localhost",
        "::1",
        "api.groq.com",  # Whisper e modelos abertos, cota gratuita
        "generativelanguage.googleapis.com",  # Gemini (AI Studio), cota gratuita
        "api.search.brave.com",  # plano gratuito
    }
)
LIMIAR_OPCIONAL = 0.9  # job opcional para com 90% da cota


@dataclass(frozen=True)
class Cota:
    provedor: str  # prefixo no registro de saída: "gateway:" pega todas as camadas
    nome: str
    limite: int  # 0 = sem limite
    periodo: Literal["dia", "mes"] = "dia"


def cotas_de(settings: Settings) -> dict[str, Cota]:
    return {
        "gateway": Cota("gateway:", "Gateway de modelos", settings.quota_gateway_dia),
        "groq": Cota("groq", "Groq (transcrição)", settings.quota_groq_dia),
        "gemini": Cota("gemini", "Gemini (busca, embeddings, imagem)", settings.quota_gemini_dia),
        "brave": Cota("brave", "Brave Search", settings.quota_brave_mes, "mes"),
    }


def host_gratuito(url: str) -> bool:
    return (urlsplit(url).hostname or "").lower() in PROVEDORES_GRATUITOS


class Custos:
    def __init__(
        self,
        memory: MemoryStore,
        cotas: dict[str, Cota],
        *,
        ops: Operations | None = None,
        clock: Callable[[], float] = time.time,
    ) -> None:
        self.memory, self.cotas, self._ops, self._clock = memory, cotas, ops, clock

    def _inicio(self, periodo: str) -> float:
        dt = datetime.fromtimestamp(self._clock())
        dt = dt.replace(hour=0, minute=0, second=0, microsecond=0)
        if periodo == "mes":
            dt = dt.replace(day=1)
        return dt.timestamp()

    def uso(self, provedor: str) -> int:
        """Chamadas no período corrente (hoje, ou este mês para o Brave)."""
        c = self.cotas[provedor]
        return self.memory.external_calls_count(c.provedor, self._inicio(c.periodo))

    def uso_hoje(self, provedor: str) -> int:
        return self.uso(provedor)

    def fracao(self, provedor: str) -> float:
        c = self.cotas[provedor]
        return self.uso(provedor) / c.limite if c.limite else 0.0

    def pode_usar(self, provedor: str, opcional: bool, quem: str = "") -> bool:
        """O chat (`opcional=False`) sempre pode. Job opcional para com 90% e avisa 1× por dia."""
        if not opcional or provedor not in self.cotas or not self.cotas[provedor].limite:
            return True
        if self.fracao(provedor) < LIMIAR_OPCIONAL:
            return True
        self._avisar(provedor, quem)
        return False

    def _avisar(self, provedor: str, quem: str) -> None:
        if self._ops is None:
            return
        dia = datetime.fromtimestamp(self._clock()).strftime("%Y%m%d")
        chave = f"cota:aviso:{provedor}:{dia}"
        if self.memory.counter_get(chave):
            return
        self.memory.counter_set(chave, 1)
        c = self.cotas[provedor]
        periodo = "hoje" if c.periodo == "dia" else "neste mês"
        oque = f"{quem} ficou para depois" if quem else "os jobs opcionais param"
        self._ops.notify(
            "cota",
            f"Cota gratuita de {c.nome} quase no fim ({self.uso(provedor)}/{c.limite} {periodo}): "
            f"{oque}. O chat continua.",
            ref=f"cota:{provedor}:{dia}",
        )
        log.warning("cota de %s em %.0f%%: job opcional adiado", provedor, self.fracao(provedor))

    def resumo(self) -> list[dict[str, Any]]:
        """Para o card "Cota de hoje": usado, limite e o estado de cada provedor."""
        saida = []
        for chave, c in self.cotas.items():
            usado = self.uso(chave)
            pct = round(100 * usado / c.limite) if c.limite else 0
            saida.append(
                {
                    "provedor": chave,
                    "nome": c.nome,
                    "usado": usado,
                    "limite": c.limite,
                    "periodo": c.periodo,
                    "pct": pct,
                    "estado": (
                        "sem_limite"
                        if not c.limite
                        else "estourada"
                        if usado >= c.limite
                        else "alta"
                        if pct >= LIMIAR_OPCIONAL * 100
                        else "ok"
                    ),
                }
            )
        return saida


__all__ = ["PROVEDORES_GRATUITOS", "Cota", "Custos", "cotas_de", "host_gratuito"]
