# Orion

Assistente pessoal com identidade masculina: técnico, direto, não-servil. Não é
um chatbot. É uma extensão do dia a dia do Antônio: lembra do que importa, age no
computador dele e responde de qualquer lugar.

*"Não é uma IA. É uma extensão do sistema nervoso."*

> **Licença:** proprietário, todos os direitos reservados — ver [LICENSE](LICENSE).
> Público como portfólio, não como software livre.

## Status

O Orion está sendo construído sobre uma **base que já funciona**: o código deste
repositório (backend FastAPI, memória híbrida, 55 ferramentas, voz e bot do
Telegram) roda hoje num PC Windows com GPU dedicada (Ryzen 7 3700X · RTX 2060
Super 8GB · 64GB RAM). A arquitetura nova é mais leve, multiplataforma e de custo
zero. Ela entra no lugar da base fase a fase, e cada fase apaga o que substituiu.

**Próximo passo:** fase 0 — exportar os dados pessoais da base antes de o PC ser
vendido e provar o export com `uv run orion verify-export` (plano completo, com os critérios para a
venda: [ORION_CORTE.md](Memorias%20Do%20Projeto/ORION_CORTE.md)).

**Reescrita em andamento (`orion/`):** fundação (fase 1), memória em SQLite com importador completo,
agendador, backup e consolidação (fase 3), política de ferramentas com audit em banco, gateway, agente e `/chat`
(fase 2), `orion-desktop` e cliente MCP (fase 4), login com senha, canal Telegram com voz e foto e
`orion autostart` (fase 5) já existem e têm testes; falta ligar o que só você pode: modelos reais (OmniRoute),
chave de embeddings, servidores MCP de e-mail/agenda/navegador, Tailscale e o bot. O que foi feito, o que não
foi verificado e os próximos passos: [ORION_MELHORIAS.md](Memorias%20Do%20Projeto/ORION_MELHORIAS.md); como ligar:
[ORION_OPERACAO.md](Memorias%20Do%20Projeto/ORION_OPERACAO.md).

| Documento | Conteúdo |
|---|---|
| `README.md` (este) | Visão geral, para onde o projeto vai, como rodar a base, segurança, licença |
| [ORION_NUCLEO.md](Memorias%20Do%20Projeto/ORION_NUCLEO.md) | Plano: princípios, arquitetura-alvo, inventário da base, fases, decisões |
| [ORION_REGRAS.md](Memorias%20Do%20Projeto/ORION_REGRAS.md) | Regras do Ring 0: onde o código impõe cada uma e o teste que a prova |
| [ORION_MELHORIAS.md](Memorias%20Do%20Projeto/ORION_MELHORIAS.md) | Auditoria de 02/10/2026: achados, o que foi feito, o que não foi verificado |
| [ORION_TECNICO.md](Memorias%20Do%20Projeto/ORION_TECNICO.md) | Referência técnica da base: regras críticas, API, memória/RAG, gotchas |
| [ORION_CORTE.md](Memorias%20Do%20Projeto/ORION_CORTE.md) | Plano de corte: o que provar antes de vender o PC, orion mínimo, rollback, decisões suas |
| [ORION_FERRAMENTAS.md](Memorias%20Do%20Projeto/ORION_FERRAMENTAS.md) | Triagem das 55 ferramentas do legado: portada, substituída, a portar ou descartar |
| [ORION_OPERACAO.md](Memorias%20Do%20Projeto/ORION_OPERACAO.md) | Como ligar o Orion novo: login, Tailscale, servidores MCP, ferramentas, Telegram com voz |
| [ORION_FRONT.md](Memorias%20Do%20Projeto/ORION_FRONT.md) | Front-end: brief de design e engenharia, orçamentos, o que foi verificado |

---

## Para onde vai

```
Celular ── Telegram ─────────────┐
Celular ── web (Tailscale) ──────┤
Notebook ── web / casca desktop ─┤
                                 ▼
                     Orion (Python, FastAPI, 1 processo)
                     ├─ Agente: persona fixa + ferramentas + memória
                     ├─ Modelos ──► OmniRoute (local) ──► free tiers por chave de API
                     ├─ Tarefa pesada ──► CLIs oficiais: claude -p · codex exec · gemini -p
                     ├─ Ferramentas ──► servidores MCP (prontos + orion-desktop próprio)
                     │                   └─ política: leitura livre · escrita com log ·
                     │                      destrutiva só com confirmação
                     ├─ Memória ──► SQLite (FTS5 + vetores em numpy), um arquivo
                     │               └─ embeddings por API gratuita
                     └─ Jobs ──► lembretes · consolidação da memória · backup
```

- **Custo zero:** free tiers com fallback automático por cota. As assinaturas
  (Claude, Gemini, Codex) entram só para tarefa pesada, pelas CLIs oficiais.
- **Leve e multiplataforma:** um processo, memória num único arquivo, sem GPU,
  Docker ou servidores pesados. Roda num notebook Windows de 8GB agora e num
  MacBook M2 depois.
- **Celular primeiro:** Telegram e web pela rede privada do Tailscale, sem expor
  portas.
- **Ações sob controle:** ação destrutiva só roda com confirmação explícita,
  inclusive pelo celular.

| Fase | Entrega |
|---|---|
| 0 | Exportar os dados pessoais da base |
| 1 | Fundação: pacote `orion/` (uv, ruff, pyright, pytest), app com `/health`, CI em Windows e macOS |
| 2 | Cérebro: gateway de modelos, agente com persona, chat em streaming, delegação às CLIs |
| 3 | Memória: SQLite + busca híbrida, importação dos dados, vault do Obsidian, backup |
| 4 | Ferramentas: servidores MCP + `orion-desktop`, política de confirmação, audit |
| 5 | Canais: Telegram, login, Tailscale, autostart |
| 6 | Interface, voz e identidade visual |
| 7 | Limpeza: a base sai do repositório |

Critérios de pronto e decisões em aberto: [ORION_NUCLEO.md](Memorias%20Do%20Projeto/ORION_NUCLEO.md) §6–§7.

---

## A base atual

O que roda hoje no PC Windows. Referência completa em
[ORION_TECNICO.md](Memorias%20Do%20Projeto/ORION_TECNICO.md).

### Arquitetura

```
 ┌─────────────────────────────────────────────────────────────┐
 │  Frontend: v1 pywebview + Three.js                          │
 └──────────────────────────────┬──────────────────────────────┘
                                │ HTTP/SSE/WS :8000 (v1 via hub WS :8765)
 ┌──────────────────────────────▼──────────────────────────────┐
 │  cerebro_maestro.py  (FastAPI :8000)                         │
 │   • Cascata: Groq → Gemini → Claude(CLI)                     │
 │   • RAG híbrido: BM25 + Qdrant denso + RRF + reranker        │
 │   • Grafo de memória (SurrealDB RELATE), agentes, telemetria │
 └───┬──────────────┬───────────────┬───────────────┬──────────┘
 ┌───▼───┐   ┌──────▼─────┐   ┌─────▼─────┐   ┌─────▼──────┐
 │Qdrant │   │ SurrealDB  │   │  embed    │   │ APIs cloud │
 │ :6333 │   │  :8090     │   │  :8001    │   │ Groq/Gemini│
 │vetores│   │ episódico  │   │  BGE-M3   │   │  /Claude   │
 └───────┘   └────────────┘   └───────────┘   └────────────┘
```

| Serviço | Porta | Papel |
|---|---|---|
| `cerebro_maestro.py` (FastAPI) | 8000 | Orquestrador: cascata, RAG, endpoints, MCP (`/mcp`), dashboard |
| `embed_service.py` (FastAPI) | 8001 | BGE-M3 1024d + reranker bge-reranker-v2-m3 (GPU). O cérebro depende dele |
| hub WS (`orion_app.py`) | 8765 | Ponte entre o frontend v1 e o cérebro |
| Qdrant | 6333 | Vetores (~3,09M, BGE-M3 1024d) |
| SurrealDB | 8090 | Memória episódica + grafo |

- **Cascata:** o chat tenta Groq, Gemini e Claude, nessa ordem. Não há modelo
  local: sem chave de API ou sem internet, o chat não responde.
- **Windows:** chamadas internas usam sempre `127.0.0.1`, nunca `localhost`
  (o resolver IPv6 adiciona ~2s por chamada).

### Requisitos

- Windows 10/11, Python 3.12 (Python global, sem venv)
- GPU NVIDIA com CUDA 12.4 (BGE-M3 + reranker)
- CLI `claude` no PATH (andar Claude da cascata)
- Qdrant **v1.17.1** (fixado — ver regra em ORION_TECNICO §2) e SurrealDB 3.0.5

### Configuração

```
python -m pip install -r requirements.txt   # inclui torch 2.6+cu124 (bge-m3)
```

`Orion_Ollama/.env` (copiar de `.env.example`):

| Variável | Uso |
|---|---|
| `GROQ_API_KEY`, `GEMINI_API_KEY` | Obrigatórias para a cascata de chat |
| `TELEGRAM_BOT_TOKEN`, `TELEGRAM_ALLOWED_USERS` | Opcionais, bot do Telegram (sem allowlist o bot recusa iniciar) |
| `SURREAL_USER`, `SURREAL_PASS` | Credencial do SurrealDB (default `root/root`, só para bind local) |
| `SURREAL_NS`, `QDRANT_COLLECTION` | Namespace e coleção dos dados (default `orion_core` / `orion_memory`) |
| `ATORES_LEGADOS` | Nomes de ator antigos que a leitura trata como fala do Orion |

### Como subir

```bat
bin\startup\start_qdrant.bat
bin\startup\start_surreal.bat
bin\startup\start_embed.bat     :: embed_service :8001
bin\startup\start_cerebro.bat   :: espera 6333/8090/8001 e sobe o FastAPI
```

Cada `.bat` só sobe se a porta estiver livre e rotaciona o próprio log (>5MB → `.old`).
No boot do Windows, `bin/startup/orion_boot.vbs` (atalho em Startup) sobe
qdrant + surreal + embed + cérebro.

**Reiniciar o cérebro:** `lsof`/`kill` do Git Bash não enxergam processos
Windows. Use PowerShell:

```powershell
Get-CimInstance Win32_Process -Filter "Name = 'python.exe'" |
  Where-Object { $_.CommandLine -like '*cerebro_maestro*' } | Stop-Process -Force
```

### Frontend

`python Orion_Core/Front_end_Orion/orion_app.py` abre a interface desktop
(pywebview + Three.js, funciona offline): a constelação de Órion em 3D reagindo
ao estado (em espera, ouvindo, processando, respondendo), chat com sessões,
grafo de memória 3D, voz ao vivo e anexos. A interface definitiva do Orion é
decidida na fase 6.

### Integrações opcionais (passos manuais)

1. **Telegram** — `@BotFather` → `/newbot` → token em `TELEGRAM_BOT_TOKEN`;
   seu ID numérico (via `@userinfobot`) em `TELEGRAM_ALLOWED_USERS`. Subir:
   `bin\startup\start_telegram.bat`. Depois de validado, adicionar
   `Shell "...\bin\startup\start_telegram.bat"` ao `orion_boot.vbs`.
2. **Google Workspace (Gmail + Calendar)** — Google Cloud Console → ativar Gmail
   API e Calendar API → credencial OAuth "Aplicativo para computador" → salvar
   em `Orion_Core/google_auth/credentials.json` → rodar uma vez
   `python Orion_Ollama/orion_google_workspace.py` (gera `token.json`).
3. **Atualizar SurrealDB** (base parada, PowerShell admin) —
   `winget upgrade SurrealDB.SurrealDB --accept-source-agreements --accept-package-agreements`.
4. **MCP no Claude Code** — em `~/.claude/settings.json`:
   `{"mcpServers": {"orion": {"url": "http://127.0.0.1:8000/mcp"}}}`.

`navegar_web` (Playwright Chromium) já está instalado e funcional.

### Testes

Reescrita e legado (lógica pura, sem serviços no ar):

```
uv sync
uv run pytest -q                      # política, memória, gateway, agente, app, legado
uv run ruff check . && uv run pyright
node --test "tests/front/*.test.js"   # markdown/XSS e lógica pura do front

# testes de navegador do front (Playwright + axe-core; backend de mentira incluso)
uv sync --group e2e && (cd tests/front_e2e && npm ci)
uv run playwright install chromium    # ou ORION_E2E_CHROME=/caminho/do/chrome
uv run pytest tests/front_e2e -q

# ver o front sem subir o cérebro de verdade:
uv run python -m tests.front_e2e.mock_backend      # abra http://127.0.0.1:8000/ui/
```

O CI (`.github/workflows/ci.yml`) roda isso em Linux, Windows e macOS.

Smoke test do legado, com os serviços no ar:

```
python Orion_Ollama/test_smoke.py            # todos os endpoints
python Orion_Ollama/test_smoke.py --rapido   # sem o teste de chat
```

Dashboard ao vivo: **http://127.0.0.1:8000/dashboard** (serviços, CPU/RAM/GPU/VRAM,
vetores, telemetria da cascata).

### A reescrita (`orion/`)

```
uv sync
ORION_ADMIN_TOKEN=<16+ caracteres> ORION_GATEWAY_URL=http://127.0.0.1:20128/v1 \
ORION_GATEWAY_MODEL=<modelo> uv run orion         # sobe em 127.0.0.1:8000
uv run orion set-password                          # define/troca a senha do login (encerra as sessões abertas)
# Windows sem instalar nada: Actions > "Orion.exe (Windows)" gera o orion.exe (usuário admin; ver ORION_OPERACAO.md §1.1)
uv run orion mcp-check                             # sobe os servidores do mcp.json e lista as ferramentas e suas classes
uv run orion backup                                # backup diário da memória (mantém 7)
uv run orion autostart [--install]                 # arquivo de início automático do seu SO (mostra; --install grava)
uv run orion restore <backup.db> [--force]         # restaura um backup (confere antes; --force substitui)
uv run orion verify-export <pasta-do-backup> --assistente <nome-antigo> \
    [--env .env --google-auth <pasta> --vault <pasta>]   # fase 0: prova o export (não toca no banco real)
uv run orion import-surreal <pasta-do-backup> --assistente <nome-antigo>   # importa o export do legado
python -m orion.memory.eval <casos.json> --db <orion.db> [--embeddings]    # mede a busca com suas perguntas
```

| Variável | Uso |
|---|---|
| `uv run orion set-password` | Login: toda rota da API exige sessão (cookie) ou o token abaixo. Sem senha e sem token a API fica desligada (503) |
| `ORION_ADMIN_TOKEN` | Credencial de **máquina** (16+ caracteres): `Authorization: Bearer` para scripts e o app desktop. O navegador usa a senha |
| `ORION_SESSION_TTL_H`, `ORION_COOKIE_SECURE`, `ORION_AUTH_USER` | Validade da sessão (padrão 168 h), cookie `Secure` (automático em HTTPS) e o nome do usuário (padrão `admin`) |
| `ORION_GATEWAY_URL`, `ORION_GATEWAY_MODEL`, `ORION_GATEWAY_API_KEY` | Gateway de modelos (OmniRoute ou API compatível com a da OpenAI); sem eles `/chat` responde 503 |
| `ORION_DATA_DIR` | Onde fica o `orion.db` (padrão: pasta de dados do usuário no SO) |
| `ORION_EXTRA_SAFE_ROOTS` | Lista JSON de pastas extras onde as ferramentas escrevem sem confirmação (ex.: Documents no OneDrive) |
| `ORION_HOST`, `ORION_PORT`, `ORION_ALLOWED_HOSTS` | Só `127.0.0.1` por padrão; bind público é recusado. Host de fora (ex.: nome do Tailscale, lista JSON) só sobe **com login** (senha ou token); guia em [ORION_OPERACAO.md](Memorias%20Do%20Projeto/ORION_OPERACAO.md). A porta padrão (8000) é a do legado: não suba os dois juntos |
| `ORION_EMBED_API_KEY` (ou no cofre do SO), `ORION_EMBED_MODEL`, `ORION_EMBED_DIM` | Embeddings pela API gratuita do Gemini (padrão `gemini-embedding-001`, 768). Sem chave a busca é só por palavra-chave. Trocar modelo ou dimensão: o banco sobe sem vetores (veja o log) até `reset_vectors()` |
| `ORION_TELEGRAM_TOKEN` (ou no cofre do SO), `ORION_TELEGRAM_ALLOWED_USERS` | Canal Telegram novo: sobe com token + IDs permitidos (`123,456` ou `[123]`) + gateway; sem lista não sobe (default-deny). **Pare o bot do legado antes** (dois clientes no mesmo token dão erro 409; o do legado também não fala com o `/chat` novo). Aprovações chegam com botões ✅/❌. Voz: `ORION_TRANSCRIBE_API_KEY` (Whisper no Groq; também liga `transcrever_audio`); foto vai ao modelo como imagem só no turno |
| `ORION_DESKTOP_TOOLS` | `true` liga o `orion-desktop`: comando, arquivos, documentos, área de transferência, notificação, abrir app, Git somente-leitura, saúde, processos em segundo plano e vigilância de pastas (padrão desligado). Só leitura provada roda direto; o resto e a leitura de segredos pedem aprovação |
| `ORION_WEB_TOOLS`, `ORION_SEARCH_API_KEY`, `ORION_WEATHER_CITY` | `true` liga `buscar_url`, `consultar_clima` e `pesquisar_com_ia` (padrão desligado: página lida pode mandar o modelo buscar outra URL com dados na query) |
| `ORION_MCP_CONFIG`, `ORION_MCP_ENABLED` | `mcp.json` com os servidores MCP (padrão `<dados>/mcp.json`; exemplo em `mcp.example.json`). A classe de risco de cada ferramenta vem do arquivo, nunca do servidor |
| `ORION_BACKUP_DIR`, `ORION_BACKUP_KEEP` | Backup diário da memória (padrão `<dados>/backups`, mantém 7); aponte para o iCloud/OneDrive |
| `ORION_VAULT_DIR` | Vault do Obsidian reindexado de hora em hora na memória (vazio: não indexa) |
| `ORION_JOBS_ENABLED`, `ORION_JOBS_TICK_S`, `ORION_CONSOLIDATE` | Jobs em segundo plano (lembretes, agendamentos, embeddings, vault, backup, consolidação em fatos; padrão ligados, rodada a cada 30 s). Os avisos saem em `GET /notifications` (confirmar em `POST /notifications/{id}/ack`) |

### Layout

```
Orion_Ollama/                  # backend
  cerebro_maestro.py            # entrypoint FastAPI :8000 — estado compartilhado + wiring dos routers
  routers/ · models/            # endpoints por domínio, schemas Pydantic
  config.py                     # portas, URLs, model IDs
  llm_cascade.py                # cascata Groq → Gemini → Claude(CLI)
  rag_engine.py                 # RAG híbrido + grafo + persistência de eventos
  session_manager.py            # histórico, sessões, briefing
  proactive_loop.py             # loop proativo (lembretes, self-healing, shadow thoughts, reconciliação)
  surreal_client.py · logger.py
  tools/                        # 55 ferramentas por domínio (orion_tools.py = shim)
  embed_service.py              # BGE-M3 + reranker :8001
  orion_agentes.py              # enxame paralelo de sub-agentes
  orion_shadow_thoughts.py      # ciclo de sono NREM/REM/DEEP
  orion_seguranca.py            # rate limit, câmara de eco, audit, self-healing
  orion_browser.py · orion_google_workspace.py · orion_telegram.py · orion_voice_live.py
  bm25_index.py · reconciliar_episodios.py · test_smoke.py
orion/                         # REESCRITA: policy/ (risco, aprovações, audit), memory/ (SQLite, importador, ops, embeddings, consolidação), channels/ (Telegram: texto, voz, foto), tools/ (memória, operação, orion-desktop, web), auth, mcp_client, netguard, transcribe, gateway, agent, delegate, jobs, migration, autostart, app
tests/                         # pytest (orion + legado) e Node (front); ver "Testes"
.github/workflows/ci.yml       # ruff, pyright, pytest, node em Linux/Windows/macOS
pyproject.toml · uv.lock       # a reescrita usa uv; requirements.txt é só do legado
Orion_Core/                    # front-end v1, voz, sentidos
  Front_end_Orion/              # pywebview + Three.js + hub WS :8765 (ver ORION_FRONT.md); também servido em /ui/ pelo orion.app
  audio_manager.py (TTS) · mic_engine.py (STT) · commands.py
bin/startup/                   # .bat de cada serviço + orion_boot.vbs
Memorias Do Projeto/           # ORION_NUCLEO (plano), _CORTE, _FERRAMENTAS, _REGRAS, _MELHORIAS, _OPERACAO, _FRONT, _TECNICO (base)
```

**Fora do git (runtime):** `Orion_Core/Sons/cache/`, `Orion_Ollama/telemetria*`,
`.env`, credenciais OAuth, `qdrant_data/`, `db_cortex/`, `bm25s_index*/`,
`.claude/settings.local.json`.

---

## Segurança

Base atual:

- Todos os serviços escutam só em `127.0.0.1`; não foram preparados para exposição em rede.
- **Sem login (só o legado):** nenhuma rota do legado exige autenticação; a reescrita (`orion/`) exige
  login em toda rota (regras 13, 17 e 25).
- **Câmara de Eco:** `executar_comando` e `iniciar_processo_bg` só rodam sem confirmação se o
  comando for provadamente leitura (lista positiva); escrever dentro do código do Orion, em
  diretório de sistema ou arquivo sensível também pede confirmação. Regras em
  [ORION_REGRAS.md](Memorias%20Do%20Projeto/ORION_REGRAS.md). A confirmação ainda é por frase no chat.
- `buscar_url` e a URL inicial do `navegar_web` só aceitam host público (bloqueia loopback, LAN e
  Tailscale). O agente do navegador navega livre depois da primeira página.
- O hub `:8765` e `/ws/voice` recusam `Origin` de site externo; `/upload` tem limite de 25 MB.
- O chat só renderiza imagem gerada pelo próprio Orion (exfiltração por prompt injection).
- **CORS** aceita a origem `"null"` (necessária pro pywebview do v1). Página com
  iframe sandboxed também manda `Origin: null`, então um site aberto no navegador
  consegue falar com `/chat`.
- **`/mcp`** expõe os endpoints REST como ferramentas MCP, sem autenticação.
- `.env`, chaves de API e credenciais OAuth nunca são versionados.

Na arquitetura nova (já em `orion/`): login com senha em toda rota (sessão por cookie, bloqueio por
tentativas erradas), o acesso de fora passa só pela rede privada do Tailscale, o bot do Telegram responde
apenas ao ID do Antônio e aprovação de ação é só por botão.

**Reportar vulnerabilidade:** não abra Issue pública — e-mail
antonio.assuino.salomao@gmail.com com passos para reproduzir, impacto e
commit afetado. Sem SLA formal.

## Licença e contribuição

Proprietário — todos os direitos reservados ([LICENSE](LICENSE)). Uso, cópia ou
redistribuição exigem autorização prévia por escrito. Projeto pessoal: não
aceita pull requests. Issues podem ser abertas, sem garantia de resposta — o
roadmap é interno. Autorização de uso: antonio.assuino.salomao@gmail.com
