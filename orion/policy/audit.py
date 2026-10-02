"""Audit: redação de segredos e limite de chamadas.

Regra 5: segredos nunca vão para log/audit. `redact` mascara por nome de chave e
por padrões de token conhecidos, e trunca textos longos.
"""

from __future__ import annotations

import re
import threading
import time
from collections import defaultdict, deque
from collections.abc import Callable, Mapping
from typing import Any

_CHAVES_SECRETAS = re.compile(
    r"(token|secret|passw|senha|api[_-]?key|authorization|cookie|bearer)", re.I
)
_PADROES_TOKEN = re.compile(
    r"(gsk_[A-Za-z0-9]{20,}|AIza[0-9A-Za-z_\-]{30,}|sk-[A-Za-z0-9_\-]{20,}|"
    r"ghp_[A-Za-z0-9]{30,}|\d{8,10}:[A-Za-z0-9_\-]{30,}|eyJ[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{5,})"
)
MASCARA = "***"


def redact(valor: Any, limite: int = 500) -> Any:
    """Cópia de `valor` sem segredos e com textos limitados a `limite` caracteres."""
    if isinstance(valor, Mapping):
        return {
            k: (MASCARA if _CHAVES_SECRETAS.search(str(k)) else redact(v, limite))
            for k, v in valor.items()
        }
    if isinstance(valor, (list, tuple)):
        return [redact(v, limite) for v in valor]
    if isinstance(valor, str):
        texto = _PADROES_TOKEN.sub(MASCARA, valor)
        return texto if len(texto) <= limite else texto[:limite] + "…"
    return valor


class RateLimiter:
    """Janela deslizante por ferramenta (portado do legado), thread-safe."""

    def __init__(
        self, limites: Mapping[str, tuple[int, int]], clock: Callable[[], float] = time.monotonic
    ) -> None:
        self._limites = dict(limites)
        self._clock = clock
        self._janelas: dict[str, deque[float]] = defaultdict(deque)
        self._lock = threading.Lock()

    def check(self, ferramenta: str) -> str | None:
        """Registra a chamada e devolve None, ou o motivo se estourou o limite."""
        if ferramenta not in self._limites:
            return None
        maximo, janela = self._limites[ferramenta]
        agora = self._clock()
        with self._lock:
            fila = self._janelas[ferramenta]
            while fila and agora - fila[0] > janela:
                fila.popleft()
            if len(fila) >= maximo:
                espera = int(janela - (agora - fila[0]))
                return (
                    f"limite: '{ferramenta}' atingiu {maximo} chamadas em {janela}s "
                    f"(aguarde ~{espera}s)"
                )
            fila.append(agora)
        return None
