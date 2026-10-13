"""Reranker local (E4.6): reordena os melhores trechos da busca por um cross-encoder em ONNX.

A busca híbrida (palavra-chave + vetor, fundidas por RRF) devolve candidatos; o reranker lê a
pergunta e cada candidato JUNTOS e dá uma nota de relevância, o que costuma separar melhor o trecho
certo dos que só parecem. **Pronto e desligado** (`ORION_RERANK=false`): só liga se
`python -m orion.memory.eval <casos> --db <orion.db> --rerank` mostrar que o acerto sobe nas suas
perguntas. Roda no computador (nada sai); precisa de `onnxruntime` e `tokenizers` (extra `rerank`)
e de uma pasta com `model.onnx` e `tokenizer.json` (um cross-encoder pequeno, ex.: um bge-reranker
convertido para ONNX), apontada por `ORION_RERANK_MODEL_DIR` ou colocada em
`<dados>/modelos/rerank`. O Orion não baixa o modelo sozinho."""

from __future__ import annotations

import logging
import math
from collections.abc import Callable, Sequence
from pathlib import Path
from typing import Any, Protocol

log = logging.getLogger("orion.rerank")

MAX_TOKENS = 512


class RerankIndisponivel(RuntimeError):
    """Faltam as bibliotecas ou os arquivos do modelo."""


class Reranker(Protocol):
    def score(self, pergunta: str, trechos: Sequence[str]) -> list[float]:
        """Uma nota entre 0 e 1 por trecho (maior = mais relevante), na ordem recebida."""
        ...


def rerank(r: Reranker, pergunta: str, trechos: Sequence[str]) -> list[str]:
    """Os trechos do mais para o menos relevante (estável: empate mantém a ordem de entrada)."""
    notas = r.score(pergunta, trechos)
    ordem = sorted(range(len(trechos)), key=lambda i: -notas[i])
    return [trechos[i] for i in ordem]


def _sigmoide(x: float) -> float:
    return 1.0 / (1.0 + math.exp(-max(-30.0, min(30.0, x))))


class OnnxReranker:
    def __init__(
        self,
        pasta: Path | str,
        *,
        carregar: Callable[[Path], tuple[Any, Any]] | None = None,
    ) -> None:
        self._pasta = Path(pasta)
        modelo, tok = self._pasta / "model.onnx", self._pasta / "tokenizer.json"
        if not modelo.is_file() or not tok.is_file():
            raise RerankIndisponivel(f"faltam model.onnx e tokenizer.json em {self._pasta}")
        self._sessao, self._tokenizer = (carregar or self._carregar)(self._pasta)

    @staticmethod
    def _carregar(pasta: Path) -> tuple[Any, Any]:
        try:
            import onnxruntime  # type: ignore[import-not-found]
            from tokenizers import Tokenizer  # type: ignore[import-not-found]
        except ImportError as e:  # extra opcional
            raise RerankIndisponivel("instale o extra: uv sync --extra rerank") from e
        sessao = onnxruntime.InferenceSession(
            str(pasta / "model.onnx"), providers=["CPUExecutionProvider"]
        )
        tok = Tokenizer.from_file(str(pasta / "tokenizer.json"))
        tok.enable_truncation(max_length=MAX_TOKENS)
        tok.enable_padding()
        return sessao, tok

    def score(self, pergunta: str, trechos: Sequence[str]) -> list[float]:
        if not trechos:
            return []
        import numpy as np

        enc = self._tokenizer.encode_batch([(pergunta, t) for t in trechos])
        entradas: dict[str, Any] = {
            "input_ids": np.array([e.ids for e in enc], dtype=np.int64),
            "attention_mask": np.array([e.attention_mask for e in enc], dtype=np.int64),
        }
        nomes = {i.name for i in self._sessao.get_inputs()}
        if "token_type_ids" in nomes:
            entradas["token_type_ids"] = np.array([e.type_ids for e in enc], dtype=np.int64)
        saida = self._sessao.run(None, {k: v for k, v in entradas.items() if k in nomes})[0]
        return [_sigmoide(float(x)) for x in np.asarray(saida).reshape(len(trechos), -1)[:, 0]]


def reranker_da_pasta(pasta: Path | str) -> Reranker | None:
    """None (e um aviso no log) se não dá para usar; o chamador segue sem reranker."""
    try:
        return OnnxReranker(pasta)
    except RerankIndisponivel as e:
        log.warning("reranker desligado: %s", e)
        return None
