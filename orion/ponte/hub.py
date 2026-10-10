"""O lado do servidor: a ponte conectada (no máximo uma) e os comandos que o servidor lhe manda.

Regra 50: o servidor só consegue pedir `abrir` (uma tela do Orion) e `colar` (texto que o Antônio
já viu e aprovou na tela). Nada que leia a tela ou a área de transferência parte daqui.
"""

from __future__ import annotations

import asyncio
import re
import threading
import time
from typing import Any

COMANDOS = frozenset({"abrir", "colar"})
_ROTA = re.compile(r"^#/[a-z0-9/_-]{0,40}$")
MAX_COLAR = 8000


class ComandoInvalido(ValueError):
    """Comando fora da lista, ou com argumento que não passa na validação."""


def validar(comando: dict[str, Any]) -> dict[str, Any]:
    """Devolve o comando normalizado (só as chaves conhecidas) ou levanta `ComandoInvalido`."""
    cmd = comando.get("cmd")
    if cmd not in COMANDOS:
        raise ComandoInvalido("comando desconhecido")
    if cmd == "abrir":
        rota = comando.get("rota", "#/")
        if not isinstance(rota, str) or not _ROTA.match(rota):
            raise ComandoInvalido("rota inválida")
        return {"cmd": "abrir", "rota": rota}
    texto = comando.get("texto")
    if not isinstance(texto, str) or not texto.strip() or len(texto) > MAX_COLAR:
        raise ComandoInvalido(f"texto vazio ou maior que {MAX_COLAR} caracteres")
    return {"cmd": "colar", "texto": texto}


class PonteHub:
    """Uma conexão por vez: parear de novo (ou abrir a ponte duas vezes) derruba a antiga."""

    def __init__(self, relogio: Any = time.time) -> None:
        self._lock = threading.Lock()
        self._loop: asyncio.AbstractEventLoop | None = None
        self._fila: asyncio.Queue[dict[str, Any] | None] | None = None
        self._relogio = relogio
        self.desde: float | None = None
        self.enviados = 0

    @property
    def conectada(self) -> bool:
        return self._fila is not None

    def anexar(self) -> asyncio.Queue[dict[str, Any] | None]:
        """Chamada pelo handler do WebSocket (dentro do laço de eventos)."""
        fila: asyncio.Queue[dict[str, Any] | None] = asyncio.Queue(maxsize=50)
        with self._lock:
            antiga, self._fila = self._fila, fila
            self._loop = asyncio.get_running_loop()
            self.desde = self._relogio()
        if antiga is not None:
            antiga.put_nowait(None)  # o handler antigo fecha o socket dele
        return fila

    def soltar(self, fila: asyncio.Queue[dict[str, Any] | None]) -> None:
        with self._lock:
            if self._fila is fila:
                self._fila, self._loop, self.desde = None, None, None

    def enviar(self, comando: dict[str, Any]) -> bool:
        """Valida e enfileira. False se não há ponte conectada. Seguro de qualquer thread (a
        escuta de palmas chama daqui)."""
        pronto = validar(comando)
        with self._lock:
            fila, loop = self._fila, self._loop
        if fila is None or loop is None:
            return False
        try:
            loop.call_soon_threadsafe(self._empurrar, fila, pronto)
        except RuntimeError:  # laço já fechado: a ponte caiu entre o teste e o envio
            return False
        self.enviados += 1
        return True

    @staticmethod
    def _empurrar(fila: asyncio.Queue[dict[str, Any] | None], comando: dict[str, Any]) -> None:
        if fila.full():
            return  # ponte travada: melhor perder o comando que acumular
        fila.put_nowait(comando)

    def estado(self) -> dict[str, Any]:
        return {"conectada": self.conectada, "desde": self.desde, "enviados": self.enviados}
