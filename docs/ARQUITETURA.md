# Arquitetura (resumo)

Detalhes e histórico: [ARQUITETURA.md completa](../Memorias%20Do%20Projeto/ARQUITETURA.md). Este arquivo é só o mapa.

**Processos:** `orion.exe` (app iced e núcleo no mesmo processo, sem porta HTTP entre eles), `orion-llama.exe` (worker de modelos locais, limite de 30% da RAM) e `Orion Setup.exe` (instalador iced que baixa o pacote do GitHub Releases).

| Crate | Papel |
|---|---|
| `orion-policy` | Núcleo imutável: classes de risco, aprovação, shell, caminhos, rede |
| `orion-store` | SQLite, SQLCipher, migrações, FTS5 |
| `orion-ai` | Provedores, roteador, transcrição |
| `orion-core` | Agente, ferramentas, canais (Telegram), jobs, config, vault na memória |
| `orion-server` | CLI `orion`, rotas da API local, desktop |
| `orion-app` | Interface iced (telas, onboarding, Memória, Configurações) |
| `orion-sky` | Céu animado e constelação (instalador e onboarding) |
| `orion-setup`, `orion-updater` | Instalador e atualizador (minisign + SHA-256) |
| `orion-platform` | Windows: bandeja, atalhos, notificações, autostart, cofre |
| `orion-voice`, `orion-local`, `orion-llama` | Voz, catálogo/download de modelos, worker llama.cpp |
| `orion-vault`, `orion-mcp` | Vault (Git, frontmatter, wikilinks) e cliente/servidor MCP |
| `orion-testkit` | Apoio a testes (pausado) |
