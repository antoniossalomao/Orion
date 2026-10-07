"""Pesquisa de ensaio via SDK real, sem rede/credenciais ou efeitos externos."""

from mcp.server.mcpserver import MCPServer

server = MCPServer("Orion Pesquisa Fixture")


@server.tool()
def pesquisar_fixture(consulta: str) -> dict:
    return {
        "consulta": consulta,
        "fontes": [
            {"titulo": "Fonte A", "url": "https://example.com/a"},
            {"titulo": "Fonte B", "url": "https://example.com/b"},
        ],
    }


@server.tool()
def ler_fixture(url: str) -> dict:
    return {"url": url, "texto": "Evidência controlada de ensaio; não é leitura de conta real."}


if __name__ == "__main__":
    server.run(transport="stdio")
