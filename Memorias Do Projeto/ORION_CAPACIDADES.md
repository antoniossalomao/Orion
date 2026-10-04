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

## Cliente da interface (C02)

O cliente consulta `/health`, comum aos dois backends. O formato `components.gateway`
identifica o backend novo, cujo contrato v1 vem de `/capabilities`. O formato
`cerebro.ok` identifica o legado e habilita seu adaptador de compatibilidade.
Respostas desconhecidas não habilitam recursos por suposição.

A detecção compartilha a requisição em andamento, publica o estado no evento
`capabilities` e invalida o cache ao trocar `base_url`. O ping atualiza esse estado
sem toast. API HTTP alcançável, modelo configurado e flags de recursos são separados.
Um backend novo sem gateway mostra “Modelo indisponível” e “API disponível”.

`pedidoChat` e `pedidoRetomada` selecionam o corpo correto: o backend novo usa
`canal: web`; o legado mantém `modelo` e a retomada original. O hub do desktop
só encaminha conversas do legado. Bearer token e eventos SSE são preservados.

Rotas opcionais são protegidas por flags. Recurso ausente gera `UnsupportedError`,
sem pedido HTTP. No legado, o primeiro 404 de uma rota opcional marca sua flag
como ausente até trocar de backend; os pollings seguintes não repetem essa rota
nem alteram o estado de conexão. 404 de aprovação individual não desabilita o
sistema de aprovações.

Os controles de sessões, modelo, anexos e voz ficam desabilitados quando ausentes.
Integrações, grafo e atividade explicam a indisponibilidade. Sem modelo, Enter não
apaga o rascunho; sugestões preenchem o campo para continuar depois. O painel de
serviços mostra os componentes reais do backend novo, sem atribuir a ele os bancos
do legado. Medidores sem dados deixam de anunciar valores ARIA inexistentes.

No navegador, o front deve ser servido pela mesma origem do backend (`/ui/`) ou
por um proxy autorizado. A escolha de endereço não abre CORS no servidor.
Windows/pywebview real e um provedor externo continuam fora da validação deste ambiente.

Validação C02: 88 testes da suíte completa de navegador passaram. Após ajustar a
posição do aviso e a altura do campo ao redimensionar, os 18 cenários afetados de
capacidades, offline e janela estreita passaram novamente. O teste de contraste
aguarda a animação de entrada terminar antes de medir, como os demais overlays.
As sete suítes Node, Ruff, formatação e whitespace passaram; a suíte Python do núcleo
passou com 473 testes e Pyright terminou sem erros. Os testes de capacidades usam a
API FastAPI real com armazenamento temporário e gateway simulado, incluindo o ciclo
aprovar → retomar → executar. Nenhum provedor ou conta externa foi acessado.

Capturas locais: `/workspace/artifacts/orion-front/c02-sem-modelo.png` (1440×900) e
`/workspace/artifacts/orion-front/c02-janela-estreita.png` (700×650).
