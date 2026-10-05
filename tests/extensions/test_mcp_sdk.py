import os
import sys
from pathlib import Path

import pytest
from mcp import Client, StdioServerParameters

from orion.extensions.protocol import MCPCompatibilityError, ensure_compatible


@pytest.mark.parametrize("mode,protocol", [("auto", "2026-07-28"), ("legacy", "2025-11-25")])
async def test_stdio_descoberta_chamada_e_encerramento(mode, protocol):
    params = StdioServerParameters(
        command=sys.executable, args=[str(Path(__file__).with_name("mcp_server.py"))]
    )
    async with Client(params, mode=mode, cache=None, read_timeout_seconds=5) as client:
        negotiated = client.session.protocol_version
        assert negotiated == protocol
        ensure_compatible(negotiated)
        tools = await client.list_tools()
        assert [tool.name for tool in tools.tools] == [
            "eco",
            "ambiente",
            "alterar_fixture",
            "estado_fixture",
        ]
        result = await client.call_tool("eco", {"texto": "prova de leitura"})
        assert not result.is_error and result.structured_content is not None
        assert result.structured_content["texto"] == "prova de leitura"
        pid = result.structured_content["pid"]
        assert isinstance(pid, int) and pid != os.getpid()
    if os.name == "posix":
        with pytest.raises(ProcessLookupError):
            os.kill(pid, 0)


@pytest.mark.parametrize("protocol", ["2024-11-05", "2025-03-26", "1900-01-01"])
def test_protocolo_nao_comprovado_tem_erro_legivel(protocol):
    with pytest.raises(MCPCompatibilityError, match="protocolo MCP não validado"):
        ensure_compatible(protocol)


def test_sdk_incompativel_recusado():
    with pytest.raises(MCPCompatibilityError, match="SDK MCP incompatível"):
        ensure_compatible("2026-07-28", sdk="1.0.0")
