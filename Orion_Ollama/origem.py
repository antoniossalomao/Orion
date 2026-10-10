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


# ── /mcp: nenhuma página do navegador fala com ele ─────────────────────────────
# O servidor MCP expõe a memória e os sub-agentes sem login. Quem o usa (Claude Code, Cursor) é
# cliente de linha de comando e NÃO manda `Origin`; um navegador sempre manda. Então qualquer
# `Origin` que não seja a própria página do cérebro é recusado, inclusive `null` (iframe
# sandboxed, `data:`, `file://`), que o CORS do restante da API ainda tolera por causa do pywebview.
ORIGENS_DO_PROPRIO_CEREBRO = ("http://127.0.0.1:8000", "http://localhost:8000")
PREFIXO_MCP = "/mcp"


def origem_permitida_no_mcp(origem: str | None) -> bool:
    return origem is None or origem in ORIGENS_DO_PROPRIO_CEREBRO


class RecusaOrigemEstranhaNoMcp:
    """Middleware ASGI: 403 para `/mcp` quando a requisição vem de uma página de outro site."""

    def __init__(self, app, prefixo: str = PREFIXO_MCP) -> None:
        self.app = app
        self.prefixo = prefixo

    async def __call__(self, scope, receive, send):
        if scope["type"] in ("http", "websocket") and scope["path"].startswith(self.prefixo):
            cab = {k.decode("latin-1").lower(): v.decode("latin-1") for k, v in scope["headers"]}
            if not origem_permitida_no_mcp(cab.get("origin")):
                if scope["type"] == "websocket":
                    await send({"type": "websocket.close", "code": 1008})
                    return
                corpo = b'{"detail":"origem nao permitida"}'
                await send({
                    "type": "http.response.start", "status": 403,
                    "headers": [(b"content-type", b"application/json"),
                                (b"content-length", str(len(corpo)).encode())],
                })
                await send({"type": "http.response.body", "body": corpo})
                return
        await self.app(scope, receive, send)
