"""Servidor controlado de ensaio: sem rede, arquivos pessoais ou credenciais."""

import os

from mcp.server.mcpserver import MCPServer

server = MCPServer("orion-test-server")


@server.tool()
def eco(texto: str) -> dict[str, str | int]:
    return {"texto": texto, "pid": os.getpid()}


if __name__ == "__main__":
    server.run(transport="stdio")
