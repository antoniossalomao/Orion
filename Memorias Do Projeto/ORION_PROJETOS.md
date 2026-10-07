# Projetos e escopo de dados

O SQLite migra dados anteriores para o contexto pessoal, preservando IDs e histórico.
Um projeto possui instruções opcionais, raiz local autorizada e a opção explícita
`share_personal`, inicialmente desligada. Arquivar não apaga conversas ou documentos.

Cada turno deriva o projeto da sessão persistida. Fatos, documentos, conversas recuperadas
por FTS e vetores são filtrados antes de ordenar os resultados. Compartilhar memória permite
ler somente dados pessoais adicionais, nunca dados de outro projeto. Editar/esquecer fatos
continua restrito ao próprio contexto. Indexar o vault não remove documentos de outros escopos.

Skills, conexões MCP, resources/prompts e concessões usam `personal` ou `project:<id>`.
Compartilhar memória não concede extensões pessoais. Schemas de outros projetos não entram
no orçamento do modelo. Aprovações incluem projeto e revisão; alterar configuração invalida
pedidos antigos. Mudança da sessão antes do início de um stream cancela o pedido.

Arquivos e delegação exigem caminhos absolutos dentro da raiz do projeto, após resolver
symlinks, além do PathGuard e da política normal. Ferramentas globais sem implementação de
escopo e comandos genéricos ficam indisponíveis dentro de projetos. Código MCP local
explicitamente confiável continua executando com os direitos do usuário: o escopo do Orion
não é uma sandbox do sistema operacional.

Dados recuperados e instruções do projeto entram como mensagens de usuário, separados do
núcleo de sistema. Documentos contaminam a sessão para exigir revisão das ações posteriores.
A consolidação automática pessoal ignora conversas de projeto; fatos de projeto podem ser
registrados pelas ferramentas já escopadas, sem consolidação pessoal silenciosa.

Validação C32: canários distintos em dois projetos, FTS e vetores, compartilhamento pessoal,
deduplicação, reinício, proteção contra symlink, escrita nativa, concessões de skills,
resources MCP e revisão de aprovação. Fixtures não demonstram isolamento do processo MCP.
