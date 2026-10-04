# Capacidades do backend

O contrato público `GET /capabilities` tem `contract_version: 1`, `backend: orion`,
`app_version`, `api: online`, `model`, `auth_required`, `features` e `unavailable`.

`model: ready` indica agente configurado, sem consultar o provedor nem consumir tokens.
Não garante que uma chamada futura terá sucesso. API disponível e modelo disponível
são estados separados. `chat` exige agente e token administrativo configurados;
`approvals` e `notifications` exigem token configurado. O cliente ainda precisa enviar
seu token para usar essas rotas.

Sessões, histórico, exportação, grafo, métricas, integrações, upload, voz, seleção de
modelo, plugins, skills e MCP ficam com flag `false` enquanto não implementados.
O cliente deve tratar flags ausentes como `false`. Os motivos são `not_implemented`,
`gateway_not_configured` e `auth_not_configured`.

O contrato público não contém caminhos, endereços do gateway, credenciais, nomes de
contas ou lista de ferramentas. `GET /capabilities/details` exige o mesmo Bearer token
das rotas administrativas e oferece modelo configurado, componentes e nomes das
ferramentas para diagnóstico. Não retorna segredos de configuração.

Validação: 49 testes de capacidades, API, chat e arquivos da interface passaram.
As combinações com/sem gateway e token verificam flags honestas, proteção dos detalhes,
ausência de segredos e ausência de chamadas ao provedor durante a detecção.
