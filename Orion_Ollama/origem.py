"""origem.py — quais origens de navegador podem falar com os WebSockets locais.

WebSocket não passa por CORS: sem esta checagem, qualquer página aberta no
navegador conecta em ws://127.0.0.1:8765 (hub) e ws://127.0.0.1:8000/ws/voice.
`None` = cliente que não é navegador (mic_engine, Telegram) e não manda Origin.
`"null"`/`file://` = o pywebview do front. Limite conhecido: iframe sandboxed
também manda `null` — a barreira completa é o login da fase 5 do NUCLEO.
"""

ORIGENS_PERMITIDAS = (
    None,
    "null",
    "file://",
    "http://127.0.0.1:8000",
    "http://localhost:8000",
)


def origem_permitida(origem: str | None) -> bool:
    return origem in ORIGENS_PERMITIDAS
