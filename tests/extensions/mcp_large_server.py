"""Catálogo grande controlado, sem arquivos ou rede; mede seleção e remoção."""

from mcp.server.mcpserver import MCPServer

server = MCPServer("large-catalog")
for index in range(120):

    def read() -> dict[str, str]:
        return {"texto": "dado de ensaio"}

    server.tool(name=f"dado_{index:03}", description="inventário de peças industriais")(read)


@server.tool(description="consultar astronomia planetas órbitas")
def planetas() -> dict[str, str]:
    return {"texto": "Saturno"}


@server.tool()
def retirar() -> dict[str, bool]:
    server.remove_tool("planetas")
    return {"ok": True}


if __name__ == "__main__":
    server.run(transport="stdio")
