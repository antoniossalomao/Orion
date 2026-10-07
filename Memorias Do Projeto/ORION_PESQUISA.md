# Orion Pesquisa — C28

O provedor escolhido é **Brave Search API**. A referência MCP oficial
[brave/brave-search-mcp-server](https://github.com/brave/brave-search-mcp-server)
foi consultada: repositório não arquivado, manutenção observada em 05/10/2026,
licença MIT confirmada no LICENSE. README oficial explica chave obrigatória,
planos Search/Answers e allowlist de ferramentas. Não copiamos o servidor nem
instalamos seu Node/npm automaticamente; Orion implementa adaptador HTTP de
leitura sobre o endpoint `/res/v1/web/search`.

Cotas, preços e permissões dependem da conta/assinatura Brave. Não declaramos
cota gratuita nem saúde de uma conta que não foi conectada. A página pública de
documentação consultada recusou acesso neste ambiente; README e licença públicos
foram obtidos e os contratos foram validados com fixtures controladas.

Por padrão pesquisa está desligada. Configuração explícita:

```sh
ORION_RESEARCH='{"enabled":true,"secret_ref":"ORION_RESEARCH_BRAVE_KEY"}'
```

Guarde a chave no cofre do SO (`orion.secrets.set_secret`) ou forneça a referência
por variável de ambiente. Não escreva seu valor no manifesto, browser, exemplo,
argv, arquivo versionado ou log. Sem chave, busca retorna `research_auth_missing`
antes de qualquer acesso ao provedor. Ativar essa configuração ainda não configura
assinatura, nem concede capacidades ao pacote Orion Pesquisa.

`pesquisar_internet` devolve até 10 fontes com título/URL/trecho; default 5,
consulta até 1000 caracteres, 10 chamadas/minuto, sem replay de erro/cota.
`buscar_url` é leitura de HTTPS público na porta 443, sem cookies/credenciais ou
proxy herdados. Todos os IPs retornados pelo DNS devem ser globais e o socket usa
o IP revisado, mantendo validação TLS/SNI do hostname original. Endereços locais,
privados, metadata, credenciais na URL e redirecionamentos inválidos são recusados.
Credencial Brave só vai ao host fixo do provedor; redirecionamento autenticado é
recusado. Fetch segue no máximo três redirecionamentos revisados, limita resposta
a 1 MB e texto ao contexto a 16 KB. HTML não executa scripts. Compressão é recusada
para evitar expansão ilimitada. Conexões têm timeout e deadline de leitura.

Conteúdo externo mantém taint e a política de escrita/execução. SHA-256 identifica
os bytes lidos; uma URL/trecho de busca não prova que uma afirmação é verdadeira.
Conferir fontes, contradições e datas continua fazendo parte das skills.

Fixtures provam ausência de chave, cota, parsing/limites, URL inválida, DNS privado
IPv4/IPv6, DNS fixado e redirect privado bloqueado antes de conectar. **Conta Brave
real e chamadas de pesquisa pagas não foram usadas/autorizadas nesta entrega.**
A rede de alguns ambientes exige proxy; esse fetch não herda proxy porque precisa
controlar o destino. Uma falha de rede retorna código e não tenta contornar o bloqueio.

## Pacote Orion Pesquisa — C29

O catálogo autenticado oferece o pacote mantido no próprio Orion, com
`pesquisar-assunto` e `comparar-fontes`. Instalar pelo botão **Adicionar** é o mesmo
pipeline de snapshot/staging/hash utilizado para ZIP/pasta. O pacote solicita somente
`pesquisar_internet` e `buscar_url`, sem escrita, scripts ou MCP redundante. A licença
do conteúdo é a do projeto Orion; a licença MIT do servidor oficial Brave não é
atribuída ao pacote proprietário do Orion.

Ativação pede revisão do hash e concessões. Provedor desligado mantém o pacote
aguardando configuração; a interface não declara pesquisa operacional. Sem conceder
busca/fetch, as instruções podem ajudar na comparação de fontes que a pessoa fornece,
mas não recebem ferramentas de navegação. As duas skills restringem o turno às
capacidades concedidas. Selecionar uma skill inclui suas ferramentas no orçamento de
schemas, mesmo quando a consulta não tem o mesmo vocabulário da descrição da tool.

Instruções exigem fonte real, contexto/data, distinção entre fato/interpretação/hipótese,
confronto de evidências e abstenção sem suporte. Trecho de busca não prova uma afirmação.
Fixtures passam fontes divergentes e resultado vazio ao gateway simulado; isso prova
transporte/escopo e limites do fluxo, **não qualidade factual de um modelo real**.
Pedido sem intenção de pesquisa não seleciona as skills na fixture de gatilho negativo.
