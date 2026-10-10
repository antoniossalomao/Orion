"""Índice vetorial em memória (numpy) sobre vetores guardados em tabela comum.

Troca do sqlite-vec: o CI mostrou que o Python do `uv` no macOS (o MacBook do
futuro) vem sem `enable_load_extension`, então a extensão não carregaria lá.
Para uma memória pessoal (dezenas de milhares de trechos) a força bruta com
numpy responde em milissegundos e funciona igual em Windows, macOS e Linux.
Os vetores moram na tabela `vectors` (a fonte da verdade, vai no backup); a
matriz só é carregada na primeira busca e invalidada a cada escrita.
"""

from __future__ import annotations

from collections.abc import Callable, Iterable, Sequence

import numpy as np


def normalizar(v: Sequence[float]) -> np.ndarray:
    """float32 de norma 1: o produto interno passa a ser o cosseno."""
    a = np.asarray(v, dtype=np.float32)
    norma = float(np.linalg.norm(a))
    return a / norma if norma else a


def para_blob(v: Sequence[float]) -> bytes:
    return normalizar(v).tobytes()


class VectorIndex:
    def __init__(self) -> None:
        self._dados: dict[str, tuple[np.ndarray, np.ndarray]] = {}

    def invalidar(self, kind: str | None = None) -> None:
        if kind is None:
            self._dados.clear()
        else:
            for key in list(self._dados):
                if key == kind or key.startswith(kind + "@"):
                    self._dados.pop(key, None)

    def topk(
        self,
        kind: str,
        consulta: Sequence[float],
        k: int,
        carregar: Callable[[], Iterable[tuple[int, bytes]]],
    ) -> list[int]:
        """Ids dos `k` vetores mais parecidos com `consulta` (do mais para o menos)."""
        if kind not in self._dados:
            linhas = list(carregar())
            ids = np.fromiter((i for i, _ in linhas), dtype=np.int64, count=len(linhas))
            if linhas:
                matriz = np.vstack([np.frombuffer(b, dtype=np.float32) for _, b in linhas])
            else:
                matriz = np.zeros((0, 0), dtype=np.float32)
            self._dados[kind] = (ids, matriz)
        ids, matriz = self._dados[kind]
        q = normalizar(consulta)
        if len(ids) == 0 or matriz.shape[1] != q.shape[0]:
            return []
        scores = matriz @ q
        if len(ids) > k:
            candidatos = np.argpartition(-scores, k)[:k]
        else:
            candidatos = np.arange(len(ids))
        ordem = candidatos[np.argsort(-scores[candidatos], kind="stable")]
        return [int(ids[i]) for i in ordem]
