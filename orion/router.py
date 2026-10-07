"""Roteamento por tipo de tarefa: pergunta curta vai ao modelo rápido, trabalho pesado ao forte.

Substitui o regex de palavras-chave do legado por uma pontuação **explicável**: cada sinal soma
pontos e o motivo (os sinais que valeram) acompanha a decisão, para o painel e o log mostrarem por
que aquele modelo foi escolhido. Sem LLM no meio: decidir qual modelo usar não pode gastar cota.

Camadas: `rapido` (conversa curta, comando de ferramenta), `pesado` (código, análise, texto longo,
raciocínio em etapas) e `visao` (a mensagem traz imagem). Quem escolhe o **modelo** de cada camada é
a configuração (`ORION_GATEWAY_MODEL_FAST`, `_HEAVY`, `ORION_VISION_MODEL`); aqui só se escolhe a
camada. Camada sem modelo configurado cai no modelo padrão do gateway (ver `ChatGateway`).

Você sempre pode mandar: `#pesado`, `#rapido` ou `#visao` no começo da mensagem força a camada.
"""

from __future__ import annotations

import re
import unicodedata
from dataclasses import dataclass

RAPIDO, PESADO, VISAO = "rapido", "pesado", "visao"
CAMADAS = (RAPIDO, PESADO, VISAO)
LIMIAR_PESADO = 3


def _sem_acento(s: str) -> str:
    return unicodedata.normalize("NFD", s).encode("ascii", "ignore").decode("ascii").lower()


_FORCA = re.compile(r"^\s*#(pesado|rapido|visao)\b")
_CODIGO = re.compile(
    r"```|\bTraceback\b|\bStack ?trace\b|^\s*at \S+\(.*:\d+\)|\bException\b|\bError:", re.M
)
_ITEM_NUMERADO = re.compile(r"^\s*\d+[.)]\s+\S", re.M)
# raízes de pedidos que costumam exigir raciocínio ou texto grande (comparadas sem acento)
_TRABALHO = tuple(
    _sem_acento(x)
    for x in (
        "analis", "compar", "refator", "arquitet", "planej", "otimiz", "depur", "debug",
        "implement", "audit", "revis", "projet", "diagnostic", "migr", "passo a passo",
        "em detalhe", "detalhadamente", "escreva um", "escreva uma", "elabor", "estrateg",
        "trade-off", "tradeoff", "prós e contras",
    )
)  # fmt: skip
_TEMA_TECNICO = ("codigo", "algoritmo", "diagrama", "uml", "arquitetura", "banco de dados", "sql")


@dataclass(frozen=True)
class Rota:
    camada: str
    motivo: str


def classificar(texto: str, *, imagens: int = 0) -> Rota:
    """Escolhe a camada para a mensagem. Determinístico: o mesmo texto cai sempre na mesma."""
    forca = _FORCA.match(_sem_acento(texto))
    if forca:
        return Rota(forca.group(1), f"forçado por #{forca.group(1)}")
    if imagens > 0:
        return Rota(VISAO, f"{imagens} imagem(ns) na mensagem")

    t = _sem_acento(texto)
    pontos, sinais = 0, []

    def somar(n: int, sinal: str) -> None:
        nonlocal pontos
        pontos += n
        sinais.append(f"{sinal} (+{n})")

    n = len(texto.strip())
    if n > 1200:
        somar(3, f"texto longo ({n} caracteres)")
    elif n > 500:
        somar(2, f"texto médio ({n} caracteres)")
    elif n > 250:
        somar(1, f"texto de {n} caracteres")
    if _CODIGO.search(texto):
        somar(2, "código ou erro colado")
    achados = [r for r in _TRABALHO if r in t]
    if achados:
        somar(min(4, 2 * len(achados)), "pede " + ", ".join(sorted(achados)[:3]))
    if sum(1 for _ in _ITEM_NUMERADO.finditer(texto)) >= 3 or texto.count("?") >= 3:
        somar(1, "várias perguntas ou itens")
    if any(x in t for x in _TEMA_TECNICO):
        somar(1, "tema técnico")

    if pontos >= LIMIAR_PESADO:
        return Rota(PESADO, "; ".join(sinais))
    return Rota(RAPIDO, "; ".join(sinais) or "pergunta ou comando curto")
