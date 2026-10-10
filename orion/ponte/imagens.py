"""A captura do "O que é isso?" (E2.5): fica **só na memória** do servidor local, por 5 minutos.

A ponte entrega a imagem aqui; a janela de pergunta a busca daqui para mostrar a prévia. Nada vai a
disco. Só sai do computador se você perguntar e aceitar o aviso da primeira vez (`/ponte/explicar`).
"""

from __future__ import annotations

import secrets
import threading
import time
from collections.abc import Callable

from ..vision import MAX_IMAGEM

TTL_S = 300.0
MAX_GUARDADAS = 3
_ASSINATURAS = (
    (b"\xff\xd8\xff", "image/jpeg"),
    (b"\x89PNG\r\n\x1a\n", "image/png"),
)


class ImagemInvalida(ValueError):
    """Vazia, grande demais ou de um formato que o provedor de visão não aceita."""


def tipo_da_imagem(dados: bytes) -> str | None:
    for assinatura, mime in _ASSINATURAS:
        if dados.startswith(assinatura):
            return mime
    if dados[:4] == b"RIFF" and dados[8:12] == b"WEBP":
        return "image/webp"
    return None


class Imagens:
    def __init__(self, relogio: Callable[[], float] = time.time) -> None:
        self._itens: dict[str, tuple[float, bytes, str]] = {}
        self._relogio = relogio
        self._lock = threading.Lock()

    def _podar(self) -> None:
        agora = self._relogio()
        for k in [k for k, v in self._itens.items() if agora - v[0] >= TTL_S]:
            del self._itens[k]
        while len(self._itens) > MAX_GUARDADAS:
            del self._itens[min(self._itens, key=lambda k: self._itens[k][0])]

    def guardar(self, dados: bytes) -> str:
        if not dados:
            raise ImagemInvalida("imagem vazia")
        if len(dados) > MAX_IMAGEM:
            raise ImagemInvalida(f"imagem passa de {MAX_IMAGEM // 1024 // 1024} MB")
        mime = tipo_da_imagem(dados)
        if mime is None:
            raise ImagemInvalida("só JPEG, PNG ou WebP")
        id_ = secrets.token_urlsafe(12)
        with self._lock:
            self._itens[id_] = (self._relogio(), dados, mime)
            self._podar()
        return id_

    def pegar(self, id_: str) -> tuple[bytes, str] | None:
        with self._lock:
            self._podar()
            item = self._itens.get(id_)
        return (item[1], item[2]) if item else None
