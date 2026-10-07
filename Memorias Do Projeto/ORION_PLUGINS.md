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
