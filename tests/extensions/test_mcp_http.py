import asyncio
import socket

import pytest
import uvicorn
from mcp.server.mcpserver import MCPServer
from pydantic import ValidationError
from starlette.middleware.base import BaseHTTPMiddleware
from starlette.responses import JSONResponse

from orion.extensions.host import HTTPConfig, MCPHost

TEST_SECRET = "token-mcp-apenas-fixture"


@pytest.fixture
async def endpoint():
    server = MCPServer("http-ensaio")

    @server.tool()
    def eco(texto: str) -> dict[str, str]:
        return {"texto": texto}

    app = server.streamable_http_app(stateless_http=True)

    async def auth(request, call_next):
        if request.headers.get("authorization") != f"Bearer {TEST_SECRET}":
            return JSONResponse({"error": "unauthorized"}, status_code=401)
        return await call_next(request)

    app.add_middleware(BaseHTTPMiddleware, dispatch=auth)

    sock = socket.socket()
    sock.bind(("127.0.0.1", 0))
    port = sock.getsockname()[1]
    serving = uvicorn.Server(uvicorn.Config(app, log_level="critical", lifespan="on"))
    task = asyncio.create_task(serving.serve(sockets=[sock]))
    try:
        async with asyncio.timeout(5):
            while not serving.started:  # noqa: ASYNC110 — uvicorn não fornece evento público
                await asyncio.sleep(0.01)
        yield f"http://127.0.0.1:{port}/mcp"
    finally:
        serving.should_exit = True
        await task
        sock.close()


async def test_http_auth_and_no_token_at_rest(endpoint, monkeypatch, caplog):
    monkeypatch.setenv("ORION_MCP_ENSAIO", TEST_SECRET)
    cfg = HTTPConfig(
        id="http", url=endpoint, enabled=True, authorized=True, secret_ref="ORION_MCP_ENSAIO"
    )
    assert TEST_SECRET not in cfg.model_dump_json()
    host = MCPHost([cfg])
    await host.start()
    try:
        assert host.statuses()[0]["state"] == "connected"
        result = await host.connections["http"].call("eco", {"texto": "HTTP real"})
        assert result.structured_content == {"texto": "HTTP real"}
    finally:
        await host.close()
    assert TEST_SECRET not in str(host.statuses()) + caplog.text


async def test_http_without_auth_unavailable_and_missing_secret(endpoint, monkeypatch):
    monkeypatch.delenv("ORION_MCP_INEXISTENTE", raising=False)
    for url, secret, expected in [
        (endpoint, None, "auth_failed"),
        (endpoint, "ORION_MCP_INEXISTENTE", "auth_missing"),
        ("http://127.0.0.1:1/mcp", None, "transport_error"),
    ]:
        host = MCPHost(
            [HTTPConfig(id="http", url=url, enabled=True, authorized=True, secret_ref=secret)],
            timeout=1,
        )
        await host.start()
        assert host.statuses()[0]["error"] == expected
        await host.close()


@pytest.mark.parametrize(
    "url",
    [
        "http://example.org/mcp",
        "https://x:t@example.org/mcp",
        "https://example.org/mcp?token=x",
        "file:///etc/passwd",
    ],
)
def test_invalid_urls_and_separate_secret_namespace(url):
    with pytest.raises(ValidationError):
        HTTPConfig(id="bad", url=url)
    with pytest.raises(ValidationError):
        HTTPConfig(id="bad", url="https://example.org/mcp", secret_ref="ORION_ADMIN_TOKEN")
