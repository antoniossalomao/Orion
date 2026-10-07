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
