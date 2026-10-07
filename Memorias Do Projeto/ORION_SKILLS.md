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
