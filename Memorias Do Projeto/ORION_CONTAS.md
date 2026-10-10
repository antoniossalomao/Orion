# Contas e conexões MCP

Uma conta OAuth guarda nome, servidor, contexto, escopos pedidos/concedidos, prazo e estado.
Uma conexão seleciona a referência dessa conta; URL e contexto precisam corresponder.
Bearer por referência e OAuth são alternativas. Ativar/reiniciar uma conexão não solicita
novo consentimento silencioso; se faltar autorização, a pessoa abre Autorizar conta.

O fluxo usa OAuthClientProvider do SDK MCP 2.3.0, PKCE, state e issuer validados pelo SDK.
Callback local exato, TTL de três minutos e uso único; URLs de callback são removidas do
access log. Tokens, refresh token, registro do cliente e metadata de issuer ficam somente
no keyring do SO, por conta e revisão. Sem cofre disponível, o fluxo falha sem fallback em
arquivo plaintext. SDK 2.3 não recompõe prazo/metadata ao carregar TokenStorage: o adaptador
reconstrói ambos para evitar tratar token expirado como válido ou renovar no host errado.
Concessões além dos escopos apresentados são recusadas; nenhuma tool de conta aparece no LLM.

Revogação local muda a revisão antes de encerrar conexões, revoga catálogo/decisões associadas
e limpa credenciais. Essa revogação impede uso pelo Orion; não declara que revogou o consentimento
no provedor externo (a pessoa também pode removê-lo na página da conta). Conversas, fontes e
resultados continuam salvos. As contas MCP não equivalem às contas Google geridas por um
servidor Calendar: cada conector tem seu fluxo e armazenamento, que precisam ser revisados.

Verificado com provedor OAuth controlado: PKCE real, callback inválido/repetido, expiração,
refresh após recarregar provider, scope excessivo, revogação, REST autenticado e cofre ausente.
Não houve login pessoal. Callback remoto/HTTPS de deployment e Windows/keyring real aguardam
validação específica; o fluxo atual é de app local.
