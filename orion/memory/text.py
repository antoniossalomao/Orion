"""Utilitários de texto da memória: consulta FTS5 e divisão em trechos."""

from __future__ import annotations

import re
from itertools import pairwise

_STOPWORDS = frozenset(
    "a o as os um uma uns umas de do da dos das em no na nos nas por para com sem sobre entre "
    "e ou mas que se como qual quais quem onde quando é ser foi são era eu me meu minha meus "
    "minhas você seu sua tu te ele ela isso isto esse essa este esta já não sim mais muito "
    "tem tinha ter há foi vai vou sei fala falou disse".split()
)
_PALAVRA = re.compile(r"\w+", re.UNICODE)
_FRONTMATTER = re.compile(r"\A---\s*\n.*?\n---\s*\n", re.DOTALL)


def fts_query(texto: str) -> str | None:
    """Monta uma consulta FTS5 segura: termos entre aspas, ligados por OR.

    Sem stemmer em português, termos longos viram prefixo (`trabalho` →
    `trabal*`) para casar flexões (trabalhar, trabalhei). None se não sobrar termo.
    """
    termos = [t.lower() for t in _PALAVRA.findall(texto)]
    uteis = [t for t in termos if t not in _STOPWORDS and (len(t) > 1 or t.isdigit())] or termos
    saida = [
        f'"{t[: max(4, len(t) - 3)]}"*' if len(t) >= 6 else f'"{t}"' for t in dict.fromkeys(uteis)
    ]
    return " OR ".join(saida) or None


def strip_frontmatter(markdown: str) -> str:
    return _FRONTMATTER.sub("", markdown, count=1)


def chunk_text(texto: str, tamanho: int = 800, sobreposicao: int = 100) -> list[str]:
    """Agrupa parágrafos em trechos de até `tamanho` caracteres, com uma
    sobreposição de `sobreposicao` caracteres (em limite de palavra) entre eles."""
    paragrafos = [p.strip() for p in re.split(r"\n\s*\n", texto) if p.strip()]
    pedacos: list[str] = []
    atual = ""
    for p in paragrafos:
        while len(p) > tamanho:  # parágrafo gigante: corta em limite de palavra
            corte = p.rfind(" ", 0, tamanho)
            corte = corte if corte > tamanho // 2 else tamanho
            if atual:
                pedacos.append(atual)
                atual = ""
            pedacos.append(p[:corte].strip())
            p = p[corte:].strip()
        if atual and len(atual) + len(p) + 2 > tamanho:
            pedacos.append(atual)
            atual = ""
        atual = f"{atual}\n\n{p}" if atual else p
    if atual:
        pedacos.append(atual)
    if sobreposicao <= 0 or len(pedacos) < 2:
        return pedacos
    saida = [pedacos[0]]
    for anterior, trecho in pairwise(pedacos):
        cauda = anterior[-sobreposicao:]
        inicio = cauda.find(" ")
        cauda = cauda[inicio + 1 :] if inicio >= 0 else cauda
        saida.append(f"{cauda} {trecho}")
    return saida
