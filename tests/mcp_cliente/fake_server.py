"""Servidor MCP de verdade (SDK oficial) para os testes do cliente: `python fake_server.py`."""

import time

from mcp.server.mcpserver import MCPServer

servidor = MCPServer("fake")


@servidor.tool()
def somar(a: int, b: int) -> int:
    """Soma dois números."""
    return a + b


@servidor.tool()
def ler_texto(path: str) -> str:
    """Devolve o caminho recebido (o servidor de verdade leria o arquivo)."""
    return f"conteudo de {path}"


@servidor.tool()
def apagar(path: str) -> str:
    """Apaga (de mentira) um caminho."""
    return f"apagado {path}"


@servidor.tool()
def falha() -> str:
    """Sempre falha."""
    raise ValueError("boom")


@servidor.tool()
def dormir(segundos: float) -> str:
    """Demora `segundos` para responder."""
    time.sleep(segundos)
    return "acordei"


@servidor.tool()
def nome_estranho_com_espacos_e_acentos_ção() -> str:
    """Nome que precisa ser sanitizado."""
    return "ok"


if __name__ == "__main__":
    import sys

    if len(sys.argv) > 2 and sys.argv[1] == "http":  # python fake_server.py http <porta>
        servidor.run("streamable-http", host="127.0.0.1", port=int(sys.argv[2]))
    else:
        servidor.run("stdio")
