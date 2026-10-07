# Plugins Orion

## Manifesto e registro — C19 (06/10/2026)

Bundle usa `manifest.json`, formato Orion v1 (`schema_version: 1`, `orion_api: 1`), ID
minúsculo/namespace, versão semver, nome, descrição e licença. `skills` referencia pastas;
`mcp` descreve conexão stdio com entrypoint Python relativo ou HTTP com endereço explícito.
Credenciais somente por `secret_ref: ORION_MCP_*`; tokens literais/hooks são recusados.
Manifesto não instala dependências nem inicia conexão/código.

`capabilities` declara nomes `native:ferramenta`, `mcp:conexao:tool`, `resource:conexao`,
`prompt:conexao` ou `scripts`. São pedidos para revisão, não concessões. Riscos declarados
pelo bundle não constituem classificação confiável automaticamente. Dependências apontam
ID/versão exatos; namespace/paths/IDs duplicados e JSON com chaves repetidas são recusados.

SQLite separado de chats registra origem, versão, hash, manifesto e estado (`installed`,
`disabled`, `waiting_connection`, `active`, `error`). Nova versão não ativa sozinha;
reutilizar versão com hash diferente é erro. Registro idempotente preserva o estado.
Instalação/ativação/concessões e lifecycle são entregas seguintes; C19 não conecta contas.

## Pasta local — C20

Installer lê snapshot limitado (512 arquivos, 16 MB totais, 2 MB por arquivo), valida
manifesto e todas as skills/referências/MCP entrypoints no staging e só então publica
objeto pelo hash. Rejeita symlinks, hardlinks, arquivos especiais, segredos, traversal,
colisões de maiúsculas/Unicode e nomes incompatíveis com Windows. Nenhum import/hook,
pip/npm ou conexão acontece na instalação.

Cópia em `data_dir/extensions/bundles/id/hash` é independente da origem e somente leitura;
SQLite aponta apenas para objeto completo. Nova instalação fica `disabled`, sem grants.
Falha entre publicar objeto e gravar metadados remove objeto e staging sem ativação
parcial. Hash novamente validado ao reutilizar objeto. Imutabilidade é de conteúdo e
permissões locais, não proteção contra o próprio dono do SO alterando armazenamento.

## Distribuição ZIP — C21

Formato v1: `.zip` (inclusive `.orion-plugin.zip`), manifest.json na raiz, ZIP padrão
stored/deflate. Arquivo até 16 MB, expansão 16 MB, 512 arquivos, 2 MB por arquivo e
razão máxima 200. ZIP64, multipart, criptografia, compressão alternativa e self-extracting
não são suportados. Central directory é limitada antes da criação dos objetos ZipInfo.

Não usa extractall: todos os nomes/tipos/tamanhos/CRC são verificados, com bloqueio de
zip-slip, symlink/arquivo especial, drive Windows/ADS/nomes reservados, Unicode/case
collisions e conflito arquivo/diretório. Snapshot resultante passa pelo mesmo validador,
staging e publicação da pasta. Hash da mesma coleção de arquivos é igual nos dois formatos.

## Estado operacional atual — C22 a C35

Versões podem ser preparadas, revisadas, selecionadas e revertidas enquanto desativadas;
o registro troca ponteiros atomicamente. Desinstalar remove somente bundles próprios,
preservando conversas e resultados. Instalação não concede execução: ativação administrativa
requer digest, capacidades e classificações revisadas, confiança explícita no código local
ou autorização do servidor remoto. Dependências precisam estar instaladas e ativas na versão
exata. Concessões incluem o contexto pessoal/projeto e podem ser revogadas.

O manager publica skills/MCP somente após validação e retira tools/autoridades ao desativar.
Schemas externos têm seleção progressiva, limite de oito tools e orçamento de 24 KB. Skills
carregam corpo/referências sob orçamento e restrições de ferramentas. Escrita/execução externas
requerem aprovação por origem/revisão; reconectar muda identidade e invalida decisões antigas.
Resources/prompts são escolhas explícitas, dados de usuário, sem sampling/elicitation automático.

Interface Extensões oferece importação ZIP/pasta, revisão de capacidades/escopo, versões,
rollback, desativação, diagnóstico e configuração/teste de conexões. Testar faz handshake e
discovery: iniciar um servidor local confiável executa seu código, ainda que não chame tools.
Startup conserva configurações e pacotes desativados. Código local não é sandbox de SO;
scripts de pacotes ainda não são ativados pelo manager (runner local revisado é separado).

Perfil incluído: Orion Pesquisa 1.0.0, com duas skills sobre as ferramentas de pesquisa
explicitamente configuradas. Provedor Brave exige referência de credencial; ausência de conta
não é simulada como disponibilidade. Fontes e atividade permanecem no histórico. Projetos
isolam recuperação/skills/MCP e results usam armazenamento independente dos bundles.
Contas reais e Windows/pywebview permanecem sem prova neste ambiente Linux.

## Orion Memória e Vault (C43)

Pacote 1.0.0 com retomar-contexto e sintetizar-notas, capabilities nativas buscar_memoria
/listar_fatos. Usa fatos, notas e documentos já indexados; não instala MCP nem lê outra pasta
por inferência. A busca respeita o projeto e compartilhamento pessoal explícito. As skills
citam fontes, distinguem decisão/hipótese/lacuna e não autorizam escrita no vault. Para ler
mais notas, o operador precisa indexar a fonte autorizada no contexto correto.

Desativar/remover o pacote conserva índice, fatos e arquivos. Fontes no chat mostram o nome
legível da nota, sem prefixos internos de projeto/upload. Verificado com canários de dois
projetos/memória pessoal, raízes de ensaio, fontes reais e bytes do arquivo intactos. Um pedido
de salvar fato fora das tools da skill é recusado pela política. O gateway é controlado:
essa evidência não declara qualidade editorial de um modelo real.

## Orion Arquivos

O pacote 1.0.0 oferece planejar-organizacao e revisar-documento. A primeira prepara
cópias de textos, PDFs e imagens em Organizados; a segunda usa documentos indexados.
Em Fontes, revise todos os caminhos e confirme as cópias. A origem é preservada,
destinos existentes não são substituídos e planos parciais não são repetidos.
A pasta precisa estar autorizada no PathGuard e dentro da raiz do projeto selecionado.
Não há shell ou scripts; use pastas locais confiáveis. Windows ainda exige ensaio real.
