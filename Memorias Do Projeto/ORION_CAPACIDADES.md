# Capacidades do backend

O contrato público `GET /capabilities` tem `contract_version: 1`, `backend: orion`,
`app_version`, `api: online`, `model`, `auth_required`, `features` e `unavailable`.

`model: ready` indica agente configurado, sem consultar o provedor nem consumir tokens.
Não garante que uma chamada futura terá sucesso. API disponível e modelo disponível
são estados separados. `chat` exige agente e token administrativo configurados;
`sessions`, `history`, `history_clear`, `export`, `approvals` e `notifications`
exigem token configurado. O cliente ainda precisa enviar
seu token para usar essas rotas.

Grafo, métricas, integrações, upload, voz, seleção de
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
Desde C03, sessões estão disponíveis quando o token administrativo está configurado.
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

## Sessões persistentes (C03 — 04/10/2026)

| Operação | Contrato |
|---|---|
| `GET /sessoes?canal=web&limite=50` | Lista somente o canal escolhido; retorna `sessoes`, `total` da página e `ativa` (ou `null`). Não cria conversas ao consultar. Limite entre 1 e 100. |
| `POST /sessoes` | Corpo opcional `{ "canal": "web", "titulo": "Minha conversa" }`. Cria e seleciona atomicamente. Sem corpo, usa web. Título opcional de até 120 caracteres. |
| `POST /sessoes/ativar` | `{ "sessao_id": "<id>", "canal": "web" }`. Canal omitido usa web. Seleciona e retorna snapshot das últimas 50 mensagens para o adaptador existente. |

Todas exigem Bearer token administrativo, inclusive a listagem. Os IDs novos são UUIDs
hexadecimais de 32 caracteres; o cliente os trata como identificadores opacos. Os itens
mantêm `sessao_id`, `titulo`, `criada` ISO UTC, `ativa`, `favorita: false` e
`somente_leitura`; acrescentam `canal` e `ultima_atividade`. Favoritos não foram implementados.

O token atual é de administrador pessoal: o portador pode escolher explicitamente um
canal. A separação impede listar/ativar uma sessão usando outro canal; não substitui
identidades e permissões por usuário. ID inexistente e ID de outro canal retornam o mesmo
404. Sessão importada ou arquivada retorna 409 ao tentar ativar, sem mudar a seleção.
Nenhum ID sintético `legado` nem conexão ao SurrealDB é criado; registros importados usam
IDs internos, preservam seu canal e aparecem como somente leitura. Ler essas conversas
pela interface está disponível desde C04; a flag exige token administrativo.

SQLite migra automaticamente de v1/v2 para v3, preservando mensagens e escolhendo a sessão
aberta mais recente de cada canal na primeira migração. `active_sessions` passa a guardar
uma seleção explícita. Novas mensagens e respostas tardias não mudam esse ponteiro, mesmo
com timestamps iguais ou relógio recuando. Seleção e criação sobrevivem ao reinício.
Arquivar a conversa ativa limpa a seleção; o próximo pedido começa uma conversa vazia,
sem reabrir uma anterior por acidente. A migração não reabre conversas importadas.

Ativar entrega um snapshot limitado com `role`, `content`, timestamp e proveniência das
mensagens user/assistant. C04 acrescenta histórico, paginação, exportação e limpeza abaixo.
Renomear e arquivar pela API permanecem no C05.

Validação final C03: 488 testes do backend (15 específicos de sessões), 89 testes
completos de navegador e 89 testes Node passaram. Pyright, Ruff, formatação e checks
do legado passaram. O navegador testou o front contra a API nova real, com SQLite e
gateway temporários/simulados. Log final: `/workspace/artifacts/orion-c03/e2e.log`.


## Histórico, exportação e limpeza (C04 — 05/10/2026)

| Operação | Contrato |
|---|---|
| `GET /historico` | Parâmetros `canal=web`, `sessao` opcional, `limite=50` (1–100), `antes` (ID exclusivo) e `completo=false`. Retorna `sessao`, `total`, `mensagens`, `mais`, `proximo_antes` e `somente_leitura`. Sem ID, consulta a seleção atual sem criar conversa. |
| `GET /exportar` | Mesmos `canal`, `sessao` e `completo`; retorna `markdown`, `total_msgs` e `sessao`. Exporta todas as páginas em ordem, com timestamps UTC e proveniência, mesmo quando a interface só carregou a página recente. |
| `DELETE /historico` | `canal` e `sessao` opcionais. Avança o limite de contexto da sessão; preserva mensagens, fatos, documentos, índices e vetores. Retorna `ok`, `sessao` e `mensagem`. |

As três rotas exigem Bearer administrativo e respeitam o canal. Importadas e arquivadas
podem ser consultadas/exportadas; limpar devolve 409. ID de outro canal e inexistente
continuam indistinguíveis (404). Recursos funcionam sem gateway, quando há token.
Cada mensagem contém `id`, `role`, `content`, `timestamp` ISO UTC e `provenance` nullable.
O cursor usa ID em vez de timestamp: importações e datas iguais não pulam mensagens.
Mensagens internas de ferramentas não entram na conversa exportada/renderizada.

A política mantém o registro permanente, como o legado. O limite é persistido em `meta`
por sessão, sem migração destrutiva, e vale para o próximo contexto do agente e para o
snapshot de ativação. `completo=true` consulta/exporta também as mensagens anteriores à
limpeza. Busca de memória e consolidação continuam usando o registro durável; limpar uma
conversa não equivale a esquecer um fato. Aprovação pendente ou aprovada ainda não consumida
e resposta em andamento bloqueiam a limpeza com 409, sem esconder ações aguardando decisão.

O front restaura a conversa selecionada ao recarregar, oferece “Carregar mensagens
anteriores” com preservação da posição e “Ver registro completo” em modo de leitura.
Timestamps e detalhes de fontes/ferramentas ficam disponíveis no histórico. Exportar e copiar
usam a conversa visualizada, incluindo importadas, mesmo que outra permaneça ativa no backend.
O envio fica bloqueado durante carga/troca e em registros de leitura. A confirmação de
limpeza é vinculada à conversa exibida e não limpa outra se houver troca antes de confirmar.
404 de uma sessão individual não desabilita o histórico ou a exportação do backend.

Validação C04: 503 testes do backend e 89 testes Node passaram. A suíte completa de
navegador passou com 92 cenários; após os ajustes finais de apresentação e proteção de
falta de histórico, os quatro cenários do C04 passaram novamente. Ruff, formatação,
Pyright, sintaxe JavaScript e checks do legado passaram. Capturas desktop/700 px foram
inspecionadas. Evidências locais em `/workspace/artifacts/orion-c04/`.
