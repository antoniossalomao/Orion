# Orion como servidor MCP de leitura

Referências: [[ORION_EXTENSOES]], [[ORION_MCP]], [[ORION_VALIDACAO_AMBIENTES]].

Em Configurações > Conexão > Clientes externos, gere uma credencial, escolha um único
contexto e as leituras necessárias. Guarde o token mostrado uma vez. Ele não permite
administrar plugins, conexões ou REST. Validade de 1–720 horas; revogação nesta tela.
Banco guarda SHA-256; tokens de administração, OAuth e MCP de saída não são aceitos.

Endpoint local: `http://127.0.0.1:8000/mcp-export/rpc`. Transporte Streamable HTTP,
stateless, respostas JSON. Envie a credencial no cabeçalho Authorization: Bearer.
Use a porta configurada do Orion. Não coloque token na URL ou no repositório.
O servidor exige hosts/origens locais e não publica serviço na internet.

| Permissão | Ferramentas | Recurso |
| --- | --- | --- |
| search | search_context(query) | — |
| facts | list_facts() | orion://facts |
| sources | read_source(id) | orion://sources |
| artifacts | list_artifacts(), read_artifact(id, version) | orion://artifacts |

Discovery e cada chamada conferem a credencial novamente. Um projeto não herda memória
pessoal na exportação, mesmo quando o chat compartilha memória. Escrita, scripts,
shell, ações do agente, contas, cofre, configuração e caminhos arbitrários não são
exportados. Leitura de fontes indexadas mantém proveniência e marca conteúdo externo.
PathGuard bloqueia nomes conhecidos de segredos; isso não identifica todos os segredos
que uma pessoa eventualmente escreveu em uma nota ou fato autorizado.

Limites: pedido 16 KB, resposta 32 KB; até 15 fatos/trechos, 20 fontes/resultados,
10 trechos por fonte, texto de resultado abreviado em 12.000 caracteres; imagens não
exportadas. Leituras registradas na política e limitadas a 60/min por categoria.
Revogar impede novas leituras, sem recolher dados já recebidos pelo cliente.

SDK oficial Python MCP 2.3.0. Protocolos comprovados: 2026-07-28 e 2025-11-25.
Clientes desktop Claude/Codex ainda exigem ensaio específico; não presumir OAuth
automático em um cliente que suporta somente esse modo. Esta versão usa bearer
manual. O endpoint legado /mcp não é montado pelo backend novo.

## Compatibilidade C49

Comprovados em serviço HTTP real local: SDK Python 2.3.0 (2026-07-28 e 2025-11-25)
e SDK Node @modelcontextprotocol/sdk 1.32.1 (2025-11-25). Dois clientes simultâneos
com escopos A/B, chamadas diretas fora da allowlist, revogação sem derrubar B e
reinício do servidor com concessões/revogações persistidas.

O ensaio Node roda com ORION_TEST_NODE_SDK apontando para o pacote instalado;
sem a dependência, esse teste fica explicitamente skipped. O SDK Node foi instalado
sem scripts de instalação em pasta de ensaio, não como dependência de produção.
Claude Desktop e Codex desktop não estão disponíveis neste ambiente: compatibilidade
com esses apps permanece pendente. Sem prova específica, C49 é parcial.
