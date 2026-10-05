"""Servidor controlado de ensaio: sem rede, arquivos pessoais ou credenciais."""

import os

from mcp.server.mcpserver import MCPServer
from mcp.types import ToolAnnotations

server = MCPServer("orion-test-server")


@server.tool()
def eco(texto: str) -> dict[str, str | int]:
    return {"texto": texto, "pid": os.getpid()}


@server.tool(annotations=ToolAnnotations(read_only_hint=True))
def ambiente() -> dict[str, str | None]:
    return {"secret": os.environ.get("ORION_TEST_SECRET")}


changes = 0


@server.tool()
def alterar_fixture() -> dict[str, int]:
    global changes
    changes += 1
    return {"changes": changes}


@server.tool()
def estado_fixture() -> dict[str, int]:
    return {"changes": changes}


if __name__ == "__main__":
    server.run(transport="stdio")
