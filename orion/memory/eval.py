"""Avaliação da memória: perguntas reais → o trecho certo aparece no top-k?

O baseline do legado (HR 10%, MRR 0.057) nunca teve um conjunto fixo sobre a
memória pessoal. Aqui cada caso diz quais textos DEVEM aparecer nos k primeiros
resultados. Os casos reais ficam fora do git (`tests/eval_pessoal.local.json`):

    python -m orion.memory.eval tests/eval_pessoal.local.json --db <orion.db> --min-hit-rate 0.8

Formato: lista de {"question": "...", "expect": ["trecho que deve aparecer", ...]}; há um exemplo
em `tests/eval_pessoal.example.json`. Por padrão mede só a palavra-chave; com `--embeddings` usa a
API configurada (ORION_EMBED_API_KEY) e mede a busca híbrida — gasta cota do plano gratuito.
"""

from __future__ import annotations

import argparse
import json
import sys
import unicodedata
from dataclasses import dataclass, field
from pathlib import Path

from .store import MemoryStore


def _plano(texto: str) -> str:
    sem = unicodedata.normalize("NFKD", texto)
    return "".join(c for c in sem if not unicodedata.combining(c)).casefold()


@dataclass(frozen=True)
class Case:
    question: str
    expect: tuple[str, ...]  # basta UM destes trechos aparecer num dos k primeiros resultados


@dataclass
class Report:
    n: int
    hit_rate: float
    mrr: float
    misses: list[Case] = field(default_factory=list)


def load_cases(caminho: Path | str) -> list[Case]:
    dados = json.loads(Path(caminho).read_text(encoding="utf-8"))
    return [Case(d["question"], tuple(d["expect"])) for d in dados]


def run_eval(store: MemoryStore, cases: list[Case], k: int = 5) -> Report:
    acertos, soma_rr, falhas = 0, 0.0, []
    for caso in cases:
        alvo = [_plano(e) for e in caso.expect]
        rank = next(
            (
                i
                for i, h in enumerate(store.search(caso.question, k=k), start=1)
                if any(a in _plano(h.text) for a in alvo)
            ),
            None,
        )
        if rank is None:
            falhas.append(caso)
        else:
            acertos += 1
            soma_rr += 1 / rank
    n = len(cases) or 1
    return Report(len(cases), acertos / n, soma_rr / n, falhas)


def main(argv: list[str] | None = None) -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    p.add_argument("casos")
    p.add_argument("--db", required=True)
    p.add_argument("--k", type=int, default=5)
    p.add_argument("--min-hit-rate", type=float, default=0.0)
    p.add_argument(
        "--embeddings", action="store_true", help="mede a busca híbrida (gasta cota da API)"
    )
    a = p.parse_args(argv)
    embedder = None
    if a.embeddings:
        from ..app import embedder_from_settings
        from ..config import Settings

        embedder = embedder_from_settings(Settings())
        if embedder is None:
            print("sem chave de embeddings (ORION_EMBED_API_KEY): nada a medir", file=sys.stderr)
            return 2
    store = MemoryStore(a.db, embedder=embedder)
    if embedder is not None:
        print(f"vetores gerados agora: {store.embed_pending()}")
    rel = run_eval(store, load_cases(a.casos), k=a.k)
    modo = "híbrida" if embedder else "palavra-chave"
    print(f"casos={rel.n} busca={modo} hit@{a.k}={rel.hit_rate:.1%} mrr={rel.mrr:.3f}")
    for c in rel.misses:
        print(f"  ERROU: {c.question}")
    return 0 if rel.hit_rate >= a.min_hit_rate else 1


if __name__ == "__main__":
    sys.exit(main())
