"""ponte.py — regras puras da ponte pywebview ↔ cérebro (sem webview nem httpx).

Ficam aqui, e não em `orion_app.py`, para serem testáveis em qualquer CI: o launcher
importa `webview`, que só existe no PC do usuário.
"""

from __future__ import annotations

import threading
from typing import Any
from urllib.parse import urlparse

_ESQUEMAS_WEB = ("http", "https")


def mensagens_do_evento(evento: Any) -> list[dict[str, Any]]:
    """Traduz um evento SSE do `/chat` em mensagens do hub WebSocket (:8765).

    O front trata as mesmas chaves nos dois caminhos (hub e SSE direto): `ai_chunk`, `tier`
    e, desde o `orion.app`, `tool`, `approval` e `error`. Evento desconhecido → nada.
    """
    if not isinstance(evento, dict):
        return []
    saida: list[dict[str, Any]] = []
    if evento.get("text"):
        saida.append({"ai_chunk": str(evento["text"])})
    if evento.get("tier"):
        saida.append({"tier": str(evento["tier"])})
    if isinstance(evento.get("tool"), dict):
        saida.append({"tool": evento["tool"]})
    if isinstance(evento.get("approval"), dict):
        saida.append({"approval": evento["approval"]})
    if isinstance(evento.get("error"), str):
        saida.append({"error": evento["error"]})
    return saida


def url_externa_permitida(url: Any) -> bool:
    """Só http(s) com host, ou mailto:, abrem fora do app (nunca file:, javascript:, data:...)."""
    if not isinstance(url, str) or not url.strip() or len(url) > 4096:
        return False
    p = urlparse(url.strip())
    if p.scheme in _ESQUEMAS_WEB:
        return bool(p.hostname)
    return p.scheme == "mailto" and bool(p.path)


def cabecalhos_chat(token: str) -> dict[str, str]:
    """`Authorization` só quando há token: o legado não conhece o cabeçalho."""
    token = (token or "").strip()
    return {"Authorization": f"Bearer {token}"} if token else {}


def mensagem_de_falha(status: int | None, detalhe: str = "") -> str:
    """Texto curto para o usuário quando o cérebro recusa ou cai (aparece no chat)."""
    if status in (401, 403):
        return "O cérebro recusou o acesso: confira ORION_ADMIN_TOKEN."
    if status:
        return f"O cérebro respondeu com erro {status}."
    return "O cérebro está offline." if not detalhe else f"Falha ao falar com o cérebro: {detalhe}"


class Geracao:
    """Número do pedido em curso, para o launcher poder cancelar o stream de um `process_command`.

    Cada pedido pega um número (`nova`). `cancelar` invalida o em curso: a thread do pedido confere
    `vigente(n)` a cada linha e para; nada que ela ainda tivesse a dizer chega ao hub.
    """

    def __init__(self) -> None:
        self._n = 0
        self._trava = threading.Lock()

    def nova(self) -> int:
        with self._trava:
            self._n += 1
            return self._n

    def cancelar(self) -> None:
        with self._trava:
            self._n += 1

    def vigente(self, n: int) -> bool:
        return n == self._n
