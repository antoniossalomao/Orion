# Orion (antiga Lyra)

> **Em replanejamento.** Desenvolvimento na `main` deste repositório; o histórico
> completo do legado, inclusive o que já foi apagado daqui, fica no repositório
> [Lyra](https://github.com/antoniossalomao/Lyra) (backup). Plano completo em
> [ORION_NUCLEO.md](Memorias%20Do%20Projeto/ORION_NUCLEO.md). Este README descreve o
> **legado (Lyra)**, que roda no PC atual (Ryzen 7 3700X · RTX 2060 Super 8GB ·
> 64GB RAM, Windows) até a venda.

> **Licença:** proprietário, todos os direitos reservados — ver [LICENSE](LICENSE).
> Público como portfólio, não como software livre.

Documentação (3 arquivos, nada mais):

| Arquivo | Conteúdo |
|---|---|
| `README.md` (este) | Legado: arquitetura, como subir, configuração, testes, segurança, licença |
| [ORION_NUCLEO.md](Memorias%20Do%20Projeto/ORION_NUCLEO.md) | **Plano do Orion:** princípios, arquitetura-alvo, inventário do legado, fases, decisões |
| [ORION_TECNICO.md](Memorias%20Do%20Projeto/ORION_TECNICO.md) | Referência do código legado (Lyra): regras críticas, API, backend, memória/RAG, frontends, gotchas |

---

## Arquitetura

```
 ┌─────────────────────────────────────────────────────────────┐
 │  Frontend: v1 pywebview + Three.js                          │
 └──────────────────────────────┬──────────────────────────────┘
                                │ HTTP/SSE/WS :8000 (v1 via hub WS :8765)
 ┌──────────────────────────────▼──────────────────────────────┐
 │  cerebro_maestro.py  (FastAPI :8000)                         │
 │   • Cascata cloud-first: Groq → Gemini → Claude(CLI) → qwen3 │
 │   • RAG híbrido: BM25 + Qdrant denso + RRF + reranker        │
 │   • Grafo de memória (SurrealDB RELATE), agentes, telemetria │
 └───┬──────────────┬───────────────┬───────────────┬──────────┘
 ┌───▼───┐   ┌──────▼─────┐   ┌─────▼─────┐   ┌─────▼──────┐
 │Qdrant │   │ SurrealDB  │   │  Ollama   │   │ APIs cloud │
 │ :6333 │   │  :8090     │   │  :11434   │   │ Groq/Gemini│
 │vetores│   │ episódico  │   │ qwen3:8b  │   │  /Claude   │
 └───────┘   └────────────┘   └───────────┘   └────────────┘
```

| Serviço | Porta | Papel |
|---|---|---|
| `cerebro_maestro.py` (FastAPI) | 8000 | Orquestrador: cascata, RAG, endpoints, MCP (`/mcp`), dashboard |
| `embed_service.py` (FastAPI) | 8001 | BGE-M3 1024d + reranker bge-reranker-v2-m3 (GPU). O cérebro depende dele |
| hub WS (`orion_app.py`) | 8765 | Ponte frontend v1 ↔ cérebro |
| Qdrant | 6333 | Vetores (`lyra_memory_v2`, ~3.09M, BGE-M3 1024d) |
| SurrealDB | 8090 | Memória episódica + grafo (ns `lyra_core`, db `Db_CORTEX`) |
| Ollama | 11434 | `qwen3:8b` local (último andar da cascata) |

- **Cascata cloud-first:** o chat tenta Groq, Gemini e Claude antes do
  `qwen3:8b`. Sem chaves de API, a Lyra funciona 100% offline (só o último andar).
- **Windows:** chamadas internas usam sempre `127.0.0.1`, nunca `localhost`
  (resolver IPv6 adiciona ~2s por chamada).

## Requisitos (estado atual — muda com a reescrita)

- Windows 10/11, Python 3.12 (Python global, sem venv)
- GPU NVIDIA com CUDA 12.4 (BGE-M3 + reranker)
- Ollama com `qwen3:8b` (e `qwen3:0.6b` para o sidecar de spec-decoding)
- CLI `claude` no PATH (andar Claude da cascata)
- Qdrant **v1.17.1** (fixado — ver regra em ORION_TECNICO §2) e SurrealDB 3.0.5

## Configuração

```
python -m pip install -r requirements.txt   # inclui torch 2.6+cu124 (bge-m3)
```

`Orion_Ollama/.env` (copiar de `.env.example`):

| Variável | Uso |
|---|---|
| `GROQ_API_KEY`, `GEMINI_API_KEY` | Obrigatórias para a cascata de chat |
| `TELEGRAM_BOT_TOKEN`, `TELEGRAM_ALLOWED_USERS` | Opcionais, bot Telegram (sem allowlist o bot recusa iniciar) |
| `AUTH_JWT_SECRET` | Segredo do JWT de login (Lyra 2.0); default só serve para dev |

## Como subir

```bat
bin\startup\start_qdrant.bat
bin\startup\start_surreal.bat
bin\startup\start_ollama.bat
bin\startup\start_embed.bat     :: embed_service :8001
bin\startup\start_cerebro.bat   :: espera 6333/8090/11434/8001 e sobe o FastAPI
```

Cada `.bat` só sobe se a porta estiver livre e rotaciona o próprio log (>5MB → `.old`).
No boot do Windows, `bin/startup/orion_boot.vbs` (atalho em Startup) sobe
qdrant + surreal + embed + cérebro; o Ollama tem atalho próprio.

**Reiniciar o cérebro:** `lsof`/`kill` do Git Bash não enxergam processos
Windows. Use PowerShell:

```powershell
Get-CimInstance Win32_Process -Filter "Name = 'python.exe'" |
  Where-Object { $_.CommandLine -like '*cerebro_maestro*' } | Stop-Process -Force
```

## Frontend

| Pasta | Stack | Como abre |
|---|---|---|
| `Orion_Core/Front_end_Orion/` | pywebview + Three.js (v1) | `python Orion_Core/Front_end_Orion/orion_app.py` |

Os front-ends v2 (React) e v3 (SvelteKit + Tauri) foram removidos em 30/09/2026
(commit `88f0b1b`); a interface do Orion é decidida na fase 6.

## Integrações opcionais (passos manuais)

1. **Telegram** — `@BotFather` → `/newbot` → token em `TELEGRAM_BOT_TOKEN`;
   seu ID numérico (via `@userinfobot`) em `TELEGRAM_ALLOWED_USERS`. Subir:
   `bin\startup\start_telegram.bat`. Depois de validado, adicionar
   `Shell "...\bin\startup\start_telegram.bat"` ao `orion_boot.vbs`.
2. **Google Workspace (Gmail + Calendar)** — Google Cloud Console → ativar Gmail
   API e Calendar API → credencial OAuth "Aplicativo para computador" → salvar
   em `Orion_Core/google_auth/credentials.json` → rodar uma vez
   `python Orion_Ollama/orion_google_workspace.py` (gera `token.json`).
3. **Screenpipe** (gravação contínua de tela, MCP em `:3030`) —
   `npm install -g @screenpipe/cli`, subir com `bin\startup\start_screenpipe.bat`,
   depois adicionar ao `orion_boot.vbs`.
4. **Atualizar SurrealDB** (Lyra parada, PowerShell admin) —
   `winget upgrade SurrealDB.SurrealDB --accept-source-agreements --accept-package-agreements`.
5. **MCP da Lyra no Claude Code** — em `~/.claude/settings.json`:
   `{"mcpServers": {"lyra": {"url": "http://127.0.0.1:8000/mcp"}}}`.

`navegar_web` (Playwright Chromium) já está instalado e funcional.

## Testes

```
python Orion_Ollama/test_smoke.py            # todos os endpoints
python Orion_Ollama/test_smoke.py --rapido   # sem o teste de chat
```

Exigem os serviços no ar. Não há testes unitários isolados nem CI.
Dashboard ao vivo: **http://127.0.0.1:8000/dashboard** (serviços, CPU/RAM/GPU/VRAM,
vetores, telemetria da cascata).

## Layout

```
Orion_Ollama/                  # backend
  cerebro_maestro.py            # entrypoint FastAPI :8000 — estado compartilhado + wiring dos routers
  routers/ · models/ · utils/   # endpoints por domínio, schemas Pydantic, auth/segredos
  config.py                     # portas, URLs, model IDs
  llm_cascade.py                # cascata Groq → Gemini → Claude(CLI) → Ollama
  rag_engine.py                 # RAG híbrido + grafo + persistência de eventos
  session_manager.py            # histórico, sessões, briefing
  proactive_loop.py             # loop proativo (lembretes, self-healing, shadow thoughts, reconciliação)
  surreal_client.py · logger.py
  tools/                        # 58 ferramentas por domínio (orion_tools.py = shim)
  embed_service.py              # BGE-M3 + reranker :8001
  orion_agent.py · orion_agentes.py        # ReAct autônomo · enxame paralelo
  orion_shadow_thoughts.py       # ciclo de sono NREM/REM/DEEP
  orion_seguranca.py             # rate limit, câmara de eco, audit, keyring, self-healing
  orion_browser.py · orion_google_workspace.py · orion_telegram.py · orion_voice_live.py
  bm25_index.py · reconciliar_episodios.py · test_smoke.py
Orion_Core/                    # front-end v1, voz, sentidos, memória bruta
  Front_end_Orion/              # pywebview + Three.js + hub WS :8765
  audio_manager.py (TTS) · mic_engine.py (STT) · commands.py
bin/startup/                  # .bat de cada serviço + orion_boot.vbs
Memorias Do Projeto/          # ORION_NUCLEO.md (plano do Orion), ORION_TECNICO.md (referência do legado)
```

**Fora do git (runtime):** `Orion_Core/Sons/cache/`, `Orion_Ollama/telemetria*`,
`Orion_Ollama/orion_tools_ext/`, checkpoints de ingestão, `.env`, credenciais OAuth,
`qdrant_data/`, `db_cortex/`, `bm25s_index*/`, `.claude/settings.local.json`.

---

## Segurança

- Todos os serviços escutam só em `127.0.0.1`; não foram hardened para exposição em rede.
- **Sem login:** nenhuma rota exige autenticação. As rotas `/auth/*` (PBKDF2-SHA256 +
  JWT) foram desmontadas em 01/10/2026 por falta de cliente; o login volta na
  reescrita (fase 5 do NUCLEO).
- **CORS** aceita a origem `"null"` (necessária pro pywebview do v1). Página com
  iframe sandboxed também manda `Origin: null`, então um site aberto no navegador
  consegue falar com `/chat`. Corrigir junto com o gating de auth.
- **`/mcp`** expõe os endpoints REST como ferramentas MCP, sem autenticação.
- `executar_comando` roda PowerShell arbitrário; contenção = rate limit +
  Câmara de Eco (confirmação explícita no chat para ações de alto risco).
- `.env`, chaves de API e credenciais OAuth nunca são versionados.

**Reportar vulnerabilidade:** não abra Issue pública — e-mail
antonio.assuino.salomao@gmail.com com passos para reproduzir, impacto e
commit afetado. Sem SLA formal.

## Licença e contribuição

Proprietário — todos os direitos reservados ([LICENSE](LICENSE)). Uso, cópia ou
redistribuição exigem autorização prévia por escrito. Projeto pessoal: não
aceita pull requests. Issues podem ser abertas, sem garantia de resposta — o
roadmap é interno. Autorização de uso: antonio.assuino.salomao@gmail.com
