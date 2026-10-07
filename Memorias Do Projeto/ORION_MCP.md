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

## Lifecycle local — C09

`ORION_MCP_CONNECTIONS` aceita uma lista JSON de conexões `stdio`, com `id`, `command`
absoluto e `args` separados (sem shell). Opcional: `cwd` absoluto. O padrão é
`enabled: false`; habilitar requer `trusted: true`, após revisar o código local.
Orion ainda não oferece sandbox de sistema operacional para servidores locais.

O lifespan conecta configurações habilitadas e encerra cada subprocesso pelo SDK.
Cada conexão tem tarefa proprietária de seus contextos AnyIO. Falha de inicialização
mantém o chat nativo funcionando. Ambiente herda somente as variáveis operacionais
permitidas pelo SDK; variáveis ORION e chaves arbitrárias não são copiadas. Stderr do
servidor é descartado, stdout permanece reservado ao protocolo; diagnóstico próprio
contém apenas ID, estado e código. Ler uma configuração não inicia o processo.

O host não registra ferramentas descobertas no agente automaticamente. Esse registro
aguarda classificação local e concessões. Os testes verificam subprocessos reais,
ambiente sem segredo de ensaio e shutdown pelo lifespan. Não foi testado em Windows.

## HTTP — C10

Transport `http` usa o Streamable HTTP do SDK com TLS para destinos externos; HTTP
simples somente no loopback. Configuração requer `authorized: true` para habilitar,
URL sem credenciais/query/fragmento e opcional `secret_ref: ORION_MCP_NOME`.
Referências são resolvidas no ambiente/cofre apenas ao conectar, nunca gravadas no
bundle; o namespace exclui `ORION_ADMIN_TOKEN`. Nenhuma conta pessoal foi conectada.

Erros estáveis: `auth_missing`, `auth_failed`, `transport_error`, `start_timeout` e
`protocol_incompatible`. O SDK permite redirects somente dentro da mesma origem
(com regras próprias), e o client não herda proxies/credenciais do ambiente.
Logs de protocolo de dependências são reduzidos a categoria/nível, sem payload,
argumentos ou traceback. Isso evita exposição em DEBUG; os diagnósticos de conexão
usam códigos próprios. Servidor HTTP de ensaio autenticado rodou com Uvicorn real.

## Classificação e identidade — C11

Somente nomes presentes em `classifications` da configuração revisada são registrados
para o agente, com risco fixo `read`, `write`, `exec` ou `destructive`. Annotations do
servidor não concedem acesso. Toda ação externa além de leitura exige confirmação;
resultados externos contaminam o contexto para a política de prompt injection.

Registro emparelha Tool e ToolSpec, com nome curto derivado de identidade canônica e
hash de configuração/schema/metadados. Origem e revisão aparecem no audit; limite de
20 chamadas por minuto por identidade. Troca de revisão revoga aprovações e referências
antigas. Nomes iguais em conexões distintas não colidem com ferramentas nativas.

## Falhas e cancelamento — C12

Cada chamada tem prazo e no máximo quatro chamadas concorrentes por conexão. O SDK
recebe cancelamento da espera. Sem confirmação remota, `possibly_active: true` sinaliza
que o efeito pode continuar; isso também vale para queda após envio. Nunca repetir
automaticamente uma ação. Erros remotos viram códigos estáveis, sem payload bruto.

Parar a conexão cancela esperas ativas e fecha o SDK; reconnect explícito descarta o
client e cria nova geração de identidade, revogando aprovações anteriores. Os testes
verificam chamada lenta, cancelamento, queda durante execução, ausência de replay e
encerramento de PID em POSIX. Não atestam interrupção universal de ações externas.

## Descoberta por turno — C13

O agente preserva ferramentas nativas e escolhe até oito externas por seleção explícita
ou palavras relevantes no catálogo resumido, sem enviar todos os schemas. Orçamento
externo: 24 KB por turno. Medição registra bytes/quantidade; tokens são estimativa
(bytes/4), não medição de tokenizer. Sem relevância ou seleção, nenhuma externa entra.
O agente recusa chamada externa fora do conjunto deste turno.

Descoberta é atualizada antes do turno e antes de retomar aprovação. Remoção, erro de
catálogo, schema inválido ou troca de identidade revogam mappings/aprovações da conexão.
Refresh concorrente é serializado. O teste usa servidor real com 121 ferramentas:
consulta astronomia envia duas schemas (uma nativa e uma MCP); consulta de inventário
respeita o teto de oito externas. Remoção no servidor invalida a aprovação antiga.
