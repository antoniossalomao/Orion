"""Servidor controlado de ensaio: sem rede, arquivos pessoais ou credenciais."""

import os

from mcp.server.mcpserver import MCPServer

server = MCPServer("orion-test-server")


@server.tool()
def eco(texto: str) -> dict[str, str | int]:
    return {"texto": texto, "pid": os.getpid()}


@server.tool()
def ambiente() -> dict[str, str | None]:
    return {"secret": os.environ.get("ORION_TEST_SECRET")}


if __name__ == "__main__":
    server.run(transport="stdio")
