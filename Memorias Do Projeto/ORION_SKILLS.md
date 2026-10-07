# Agent Skills no Orion

## Formato e descoberta — C15 (06/10/2026)

Base: [especificação oficial](https://agentskills.io/specification), consultada em
06/10/2026. YAML com PyYAML **6.0.3**, licença MIT, dependência exata no lockfile.
Não há importação de Python, pip/npm ou execução de scripts ao descobrir/carregar.

Pacote contém `SKILL.md` com frontmatter `name` e `description`; campos opcionais:
`license`, `compatibility`, `metadata` string/string e `allowed-tools` experimental.
Orion usa nomes ASCII minúsculos, números e hífens, até 64 caracteres, iguais ao
nome do diretório; descrição até 1024, compatibilidade até 500. Frontmatter até 16 KB,
sem aliases/anchors/tags/chaves duplicadas, corpo até 64 KB. Campos desconhecidos são
recusados nesta versão; requisito de compatibilidade não instala dependências.

Descoberta lê só frontmatter, apresentando namespace, origem e versão declarada em
metadata (ou `unversioned`). Corpo é validado ao carregar; referências ficam dentro
da raiz, sem symlinks, caminhos absolutos, travessia, drive Windows ou file://.
Links HTTP/HTTPS são informativos e não abrem rede. Referências só entram no contexto
quando pedidas. Colisões no namespace são recusadas; scripts permanecem arquivos.

Validação inclui frontmatter malicioso, referência externa/percent-encoded/symlink,
colisão, carregamento progressivo e script com marcador que permanece sem executar.

## Contexto progressivo — C16

`ORION_SKILL_SOURCES` lista raízes absolutas com namespace/origem e `enabled: false`
por padrão. Falha em uma fonte produz diagnóstico estável; não instala nem executa código.
`/chat` aceita `skills` (até três IDs `plugin:skill`) e `referencias` explícitas (`skill`,
`path`). Sem escolha explícita, relevância lexical exige pelo menos duas palavras de
metadados. Somente fontes ativadas participam; descrição basta para decidir relevância.

Corpos e referências escolhidos têm orçamento conjunto de 12 KB, fonte, versão, digest
e truncamento. Referências precisam estar declaradas no corpo e dentro da raiz. Corpo
não concede privilégios: `allowed-tools` intersecta scopes das skills e filtra schemas;
PolicyEngine recusa chamadas fora dessa interseção. Uma ferramenta listada mas sem
ToolSpec continua negada; execução listada mantém aprovação. Seleção marca taint.

Procedência registra skill/versão/origem no turno. A estratégia lexical inicial pode
exigir escolha explícita para consultas curtas; não há classificação paga/remota.

## Chat e paleta — C17

`/plugin:skill pedido` resolve o namespace sem alterar `/nova`, `/modelo` ou caminhos
de arquivo. `//` continua sendo barra literal. Sugestões e paleta listam skills ativadas;
escolher na paleta acrescenta o prefixo sem apagar o rascunho. Origem/versão aparecem
no composer e no turno, inclusive depois de recarregar o histórico.

`GET /skills` exige admin e devolve só metadados, sem carregar corpo. Capability `skills`
representa catálogo/invocação disponíveis quando admin está configurado, mesmo vazio.
Legado não é sondado para essa rota. Skill inválida/desativada preserva o texto; recusa
HTTP após mudança no servidor recupera o rascunho sem reenvio automático.

Validação C17: backend 563, Node 90, navegador completo 98; Ruff/Pyright, sintaxe JS e
legado passaram. Capturas do composer e paleta em `/workspace/artifacts/orion-c17/`
foram inspecionadas visualmente. O histórico guarda a procedência da skill escolhida.
