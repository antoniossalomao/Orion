"""Embeddings por API gratuita (Gemini), com lote, retry e backoff.

O `MemoryStore` só precisa de `dim` e `embed(textos)`. Quando a API cai ou a cota
acaba, o erro sobe e o store segue só por palavra-chave: o que faltou entra depois
em `embed_pending` (job). `embed_query` usa o modo de consulta do modelo
(RETRIEVAL_QUERY), que casa melhor com trechos indexados como RETRIEVAL_DOCUMENT.
"""

from __future__ import annotations

import logging
import time
from collections.abc import Callable

import httpx

log = logging.getLogger("orion.embeddings")

GEMINI_BASE = "https://generativelanguage.googleapis.com/v1beta"
MAX_LOTE = 100  # limite do batchEmbedContents
_RETENTAVEL = {429, 500, 502, 503, 504}


class EmbeddingError(RuntimeError):
    pass


class GeminiEmbedder:
    def __init__(
        self,
        api_key: str,
        *,
        model: str = "gemini-embedding-001",
        dim: int = 768,
        client: httpx.Client | None = None,
        base_url: str = GEMINI_BASE,
        retries: int = 3,
        backoff_s: float = 1.0,
        sleep: Callable[[float], None] = time.sleep,
        timeout_s: float = 30.0,
    ) -> None:
        if not api_key:
            raise ValueError("chave da API de embeddings vazia")
        self.dim = dim
        self._model = model
        self._key = api_key
        self._client = client or httpx.Client(timeout=timeout_s)
        self._base = base_url.rstrip("/")
        self._retries = retries
        self._backoff = backoff_s
        self._sleep = sleep

    def embed(self, texts: list[str]) -> list[list[float]]:
        """Vetores para indexar (documentos)."""
        return self._embed(texts, "RETRIEVAL_DOCUMENT")

    def embed_query(self, text: str) -> list[float]:
        return self._embed([text], "RETRIEVAL_QUERY")[0]

    def _embed(self, texts: list[str], tarefa: str) -> list[list[float]]:
        saida: list[list[float]] = []
        for i in range(0, len(texts), MAX_LOTE):
            saida.extend(self._lote(texts[i : i + MAX_LOTE], tarefa))
        return saida

    def _lote(self, texts: list[str], tarefa: str) -> list[list[float]]:
        corpo = {
            "requests": [
                {
                    "model": f"models/{self._model}",
                    "content": {"parts": [{"text": t}]},
                    "taskType": tarefa,
                    "outputDimensionality": self.dim,
                }
                for t in texts
            ]
        }
        # a chave vai em cabeçalho, nunca na URL (URL aparece em log e traceback)
        url = f"{self._base}/models/{self._model}:batchEmbedContents"
        for tentativa in range(self._retries + 1):
            resp = None
            try:
                resp = self._client.post(url, json=corpo, headers={"x-goog-api-key": self._key})
            except httpx.TransportError as e:
                erro = type(e).__name__
            else:
                if resp.status_code < 400:
                    return self._vetores(resp, len(texts))
                erro = f"HTTP {resp.status_code}"
                if resp.status_code not in _RETENTAVEL:
                    raise EmbeddingError(f"embeddings recusados ({erro}): {resp.text[:200]}")
            if tentativa == self._retries:
                raise EmbeddingError(
                    f"embeddings indisponíveis depois de {tentativa + 1} tentativas ({erro})"
                )
            espera = max(self._backoff * 2**tentativa, _retry_after(resp))
            log.warning("embeddings: %s, nova tentativa em %.1fs", erro, espera)
            self._sleep(espera)
        raise EmbeddingError("inalcançável")  # pragma: no cover

    def _vetores(self, resp: httpx.Response, esperados: int) -> list[list[float]]:
        try:
            vetores = [list(map(float, e["values"])) for e in resp.json()["embeddings"]]
        except (KeyError, TypeError, ValueError) as e:
            raise EmbeddingError(f"resposta de embeddings fora do formato: {e}") from e
        if len(vetores) != esperados or any(len(v) != self.dim for v in vetores):
            raise EmbeddingError(
                f"esperava {esperados} vetores de {self.dim} dimensões, "
                f"veio {len(vetores)} de {sorted({len(v) for v in vetores})}"
            )
        return vetores


def _retry_after(resp: httpx.Response | None) -> float:
    try:
        return float(resp.headers.get("retry-after", 0)) if resp is not None else 0.0
    except ValueError:
        return 0.0
