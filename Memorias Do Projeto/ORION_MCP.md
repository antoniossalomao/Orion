# MCP no Orion

## Base verificada — C07 (05/10/2026)

SDK oficial: **mcp 2.3.0**, licença **MIT**, dependência exata em `pyproject.toml` e
`uv.lock`. Fonte verificada no [PyPI](https://pypi.org/project/mcp/2.3.0/),
[repositório oficial](https://github.com/modelcontextprotocol/python-sdk) e
[guia de migração](https://py.sdk.modelcontextprotocol.io/migration/).

A versão 2 usa `mcp.Client` e `mcp.server.mcpserver.MCPServer`. O módulo antigo
`mcp.server.fastmcp` foi removido; não copiar exemplos de v1 sem adaptação.

| Protocolo | SDK declara | Orion comprovou |
|---|---|---|
| 2026-07-28 | Sim; protocolo atual | Descoberta automática, tools/list, call_tool, resultado estruturado e encerramento stdio |
| 2025-11-25 | Sim; handshake anterior | Mesmo fluxo com modo legacy |
| 2025-06-18 / 2025-03-26 / 2024-11-05 | Sim | Não comprovado; guard do Orion recusa essas versões |

`orion/extensions/protocol.py` recusa SDK diferente do fixado e protocolo fora das duas
versões comprovadas, com erro legível. A prova não representa compatibilidade universal
com servidores, clients ou extensões MCP. Sampling, elicitation, Tasks, HTTP e administração
de plugins não foram habilitados por esta entrega; a flag MCP do produto segue falsa.

Servidor controlado: `tests/extensions/mcp_server.py`, com uma ferramenta `eco` de leitura,
sem rede ou credenciais pessoais. O teste verifica descoberta/chamada e encerramento do
processo filho (checagem de PID em POSIX), além dos erros de versão.

Reprodução:

```bash
uv sync --locked
uv run pytest tests/extensions/test_mcp_sdk.py -q
```

A mesma prova roda num ambiente limpo instalado apenas com `mcp==2.3.0`, pytest e
pytest-asyncio, usando o repositório no PYTHONPATH. SDK e APIs reais foram usados;
nenhuma conta ou servidor externo foi conectado.

Validação C07: seis testes do SDK passaram no ambiente do projeto e novamente em um
ambiente limpo, com subprocessos e protocolo reais. A suíte completa do backend passou
com 517 testes após instalar as dependências novas. Ruff, formatação e Pyright passaram.
Evidências locais em `/workspace/artifacts/orion-c07/`. Windows/pywebview real ainda não
foi validado; nenhuma compatibilidade extra é inferida do catálogo do SDK.
