# ORION — Referência Técnica do Legado

> Como o código **atual (legado)** funciona: regras, arquitetura, API, gotchas.
> Serve de base para portar o que vale para o Orion — o plano novo está em
> [ORION_NUCLEO.md](ORION_NUCLEO.md). Consolidado em 30/09/2026 (último registro de
> trabalho no legado: 12/08/2026). Operação do legado: [README.md](../README.md) ·
> Histórico completo: histórico do git deste repositório.

---

## 1. Stack

> **30/09–01/10/2026:** sem GPU dedicada e com orçamento R$0, o embedding/reranker
> na GPU sai na reescrita. O Ollama (andar local, draft, llava-phi3, especialista
> local) já saiu do código em 01/10/2026. Esta seção descreve o código atual.

| Camada | Tecnologia | Porta / notas |
|---|---|---|
| Orquestrador | `cerebro_maestro.py` (FastAPI, POO) | 8000 |
| Embedding + rerank | `embed_service.py` — BAAI/bge-m3 1024d + bge-reranker-v2-m3 (GPU) | 8001 |
| Hub WS (v1) | `Orion_Core/Front_end_Orion/orion_app.py` | 8765 |
| Vetores | Qdrant standalone **v1.17.1 (fixado)** — coleção em `QDRANT_COLLECTION` (`.env`, default `orion_memory`) | 6333 |
| Grafo + documentos | SurrealDB 3.0.5, engine `surrealkv://`, ns em `SURREAL_NS` (`.env`, default `orion_core`), db `Db_CORTEX` (case-sensitive) | 8090 |
| LLM cloud | Groq `openai/gpt-oss-120b` → Gemini `gemini-3.5-flash` → Claude (CLI) | — |
| STT | faster-whisper "small" CPU int8 + Silero VAD | — |
| TTS | Gemini TTS (voz Charon) → edge-tts AntonioNeural → silêncio | — |
| Frontend | v1 pywebview + Three.js (v2 React e v3 SvelteKit/Tauri removidos em 30/09 — §7.2) | — |

Python 3.12 **global** (sem venv; `venv_embed` aposentado em 30/06). torch
2.6.0+cu124 (bge-m3 exige ≥2.6, pesos só em `.bin`). Rollback do upgrade:
`bin/backup_pip_freeze_python312_30-06-2026.txt` (fora do git).

## 2. Regras críticas (não quebrar)

**Rede e processos**
- **`127.0.0.1`, nunca `localhost`**, em toda chamada interna: o resolver do Windows
  tenta IPv6 primeiro e paga ~2s por chamada (`/buscar` 5.6s → 1.26s ao corrigir).
- **Todo serviço faz bind em `127.0.0.1`.** Qdrant: `QDRANT__SERVICE__HOST=127.0.0.1`
  (default é `0.0.0.0`); SurrealDB: `--bind 127.0.0.1:8090`. Em 04/08 ambos estavam na
  LAN (inclusive via Radmin VPN). Conferir com `netstat` após qualquer mudança de boot.
- **Self-healing chama os próprios `start_*.bat`** (`orion_seguranca._RESTART_CMDS`),
  nunca comandos inline duplicados — a versão inline subia o Qdrant num storage
  vazio (memória "zerada" silenciosamente) e o SurrealDB com engine errada.
  Sucesso medido por polling de até 20s (Qdrant leva 5-8s para carregar).
- `qdrant.exe` não está no PATH e `NoDefaultCurrentDirectoryInExePath=1` está
  setado: sempre caminho absoluto no `.bat`.
- Porta: teste de socket (`TcpClient.ConnectAsync().Wait(300)`, ~13ms), nunca
  `Test-NetConnection` (~7s por chamada).

**Dados**
- **Nunca atualizar o binário do Qdrant sem backup de `qdrant_data/`** — 1.17.1 →
  1.18.2 descartou ~666k vetores e reverter não os trouxe de volta.
- **Embedding único: BGE-M3 1024d via `:8001/embed`.** Trocar exige re-vetorizar tudo.
- Todo `PointStruct` tem `categoria` e `fonte` no payload (filtros dependem disso).
- IDs do Qdrant: `uuid.uuid5(uuid.NAMESPACE_URL, chave)` — determinístico e idempotente.
- Ingestão (SurrealDB) e vetorização (Qdrant) são desacopladas: comparar
  `SELECT count() FROM <tabela> GROUP ALL` com a contagem por categoria.
- Mudança de schema sempre aditiva; snapshot antes.

**SurrealDB**
- Paginação por cursor (`WHERE id > $cursor ORDER BY id LIMIT N`), nunca `LIMIT/START` (full scan).
- SDK v1.x: `query()` devolve lista direta — nunca `res[0].get("result")`.
- HTTP `/sql`: headers `surreal-ns`/`surreal-db` (não `NS`/`DB`).
- `ORDER BY` em campo que não está no `SELECT` → 400 (`Idiom missing here`).
- Record id vem escapado com crase (`sessao:\`uuid\``): escrever com
  `type::record("sessao", $id)`, nunca montando o id na mão.
- Tabela que nunca teve `CREATE` devolve status `ERR` com `result` = string, e
  `SurrealClient.result()` não distingue OK de ERR: checar `isinstance(dados[0], dict)`.
- Tabela `topico` não existe: tópicos são ids implícitos `topico:x` vindos de `sobre.out`.
- Credencial `root/root` aceita só porque o bind é local.

**Python no Windows**
- `import pyarrow` **antes** de `torch`/`qdrant_client`/`sentence_transformers` —
  senão access violation `0xc0000005` sem traceback.
- `sys.stdout.reconfigure(encoding="utf-8", errors="replace")` logo após `import sys`
  em todo script que imprime unicode com saída redirecionada.
- Scripts com `sentence_transformers`: `log()` grava no arquivo antes de tentar `print()`.
- `pyttsx3.runAndWait()` trava o event loop — TTS async via subprocess.
- pywebview: `url=` com caminho absoluto; `SetProcessDPIAware()` antes de `GetSystemMetrics`.
- PowerShell 5.1 emite na codepage OEM (850): prefixar
  `$OutputEncoding = [Console]::OutputEncoding = [System.Text.Encoding]::UTF8;` e
  escapar apóstrofo como `''`.

**Async e APIs**
- Chamada síncrona dentro de `async def` (RAG, tools, `httpx.post`) sempre via
  `await asyncio.to_thread(...)` — uvicorn tem um event loop só; bloquear trava tudo.
- Mensagens enviadas aos provedores: sanitizar para `{role, content}` — o Groq
  rejeita campos extras (`fontes_rag`).
- Usar a variável local da requisição (`msg.texto`), nunca `mensagens[-1]` do
  estado global (race entre `/chat` concorrentes).
- Clientes HTTP reutilizáveis (keep-alive) para health checks.

**Ambiente de desenvolvimento**
- Git Bash não enxerga processos Windows nativos — matar via PowerShell
  `Get-CimInstance … | Stop-Process` (ver README).
- Variável de ambiente com path (`X=/algo cmd`) no Git Bash é convertida pelo MSYS —
  usar PowerShell `$env:X = "/algo"` (quebrava o build do v3 com `BASE_PATH`).
- BM25 é carregado com `mmap=True` no startup: o índice não pode ser reescrito com
  o cérebro rodando (o script de rebuild foi apagado em 01/10 junto com os datasets).
- `cargo` com antivírus: `CARGO_BUILD_JOBS=1` (evita `os error 32`).
- Editar arquivo não afeta o `cerebro_maestro.py` rodando; restart só quando pedido.

**GPU**
- Cérebro não carrega modelo de embedding (usa `:8001`). Vetorização em lote na GPU:
  `ENCODE_BATCH=32`, `MAX_SEQ_LEN=512`, checar VRAM com
  `torch.cuda.memory_allocated()` (não `memory_reserved()`). ~88k vetores/30min.
- `embed_service`: idle 600s estaciona o BGE-M3 na RAM (`.to('cpu')`, reativa em
  0.32s); reranker descarrega por completo.

## 3. Backend (`Orion_Ollama/`)

### 3.1 Organização

| Módulo | Papel |
|---|---|
| `cerebro_maestro.py` | Entrypoint: estado compartilhado, CORS, mount `/imagens` (`/ui` e `/ui-novo` só montam se o build do v2/v3 existir — não existe mais), wiring dos routers, MCP |
| `config.py` | Portas, URLs, model IDs, limites |
| `llm_cascade.py` | `LLMCascade` — streaming (`/chat`) e batch (`run()`, agentes) por provedor |
| `rag_engine.py` | `RAGEngine` — `search()`, `record_event()`, `search_graph()`, utilitários de texto |
| `session_manager.py` | `SessionManager` — histórico, sessão ativa, briefing, contador de turnos (com locks) |
| `proactive_loop.py` | `ProactiveLoop` — lembretes, agendamentos, processos bg, enxames, self-healing, shadow thoughts, reconciliação |
| `surreal_client.py`, `logger.py` | Cliente SurrealDB único; logger com `atexit` |
| `routers/` | `system`, `sessions`, `memory`, `agents` (só `/enxame*`), `misc`, `chat`. `auth`, `gateway`, `tools`, `logs`, `models_hub` e `prompts` foram apagados em 01/10/2026 (sem cliente) |
| `models/` | Schemas Pydantic por domínio |
| `tools/` | 16 módulos por domínio + `_lazy.py`/`_shared.py`; `orion_tools.py` é shim |

Nomes antigos → novos: globals de sessão → `SessionManager`; `buscar_hibrido`/
`registrar_evento`/`buscar_grafo_surreal` → métodos de `RAGEngine` (só o wrapper
`registrar_evento` continua); `_stream_*` → wrappers sobre `LLMCascade`.

### 3.2 Cascata e roteamento

- Andares: Groq → Gemini → Claude (CLI, sem tool-calling nativo). Sem modelo local.
  `modelo: auto|groq|gemini|claude` no `/chat` força um andar sem fallback.
- **MoE roteado:** lista declarativa `ESPECIALISTAS` — `codigo` (trigger por
  keywords) → `claude, groq, gemini`; `geral` (catch-all, sempre por último)
  → `groq, gemini, claude`. Novo especialista = nova entrada na lista.
- Loop de tool-calling compartilhado, `_MAX_ITERACOES_TOOLS = 25`.
- Roteador de ferramentas: `_TOOL_KEYWORDS_RE` (regex com `\b`, prefixo de palavra).
- Compressão de histórico: >14 mensagens → as 8 mais antigas viram sumário (lock).
- Session briefing no startup: últimas mensagens + resumo + objetivos em aberto +
  tópicos/ferramentas das últimas 24h (via grafo).
- Telemetria: `telemetria.json` (acumulado) + `telemetria_historico.jsonl`
  (snapshot a cada ~5min, últimas 2000 linhas).

### 3.3 API HTTP (:8000)

| Método | Rota | Descrição |
|---|---|---|
| GET | `/` | Ping `{servico, ativo, versao}` |
| GET | `/dashboard` | Dashboard HTML de monitoramento |
| GET | `/health` | Latência de Qdrant/SurrealDB/embedder + VRAM + contagem de vetores |
| GET | `/metrics` | CPU/RAM/GPU/VRAM + latência do último chat |
| GET | `/stats`, `/stats/historico?limite=` | Telemetria da cascata (acumulada / snapshots) |
| GET | `/integracoes` | Status de Telegram, Voice Live, mic, TTS, enxame, upload |
| POST | `/chat` | Chat SSE. Body `{texto, modelo}` |
| POST | `/tts/mudo`, `/tts/falar` | Voz global on/off; falar texto arbitrário |
| GET | `/grafo/completo?limite=` | Grafo nodes+links (visualização 3D do v1) |
| GET | `/memoria/categorias` | Composição da base por categoria |
| GET/DELETE | `/historico[?sessao=]` | Histórico (com `fontes_rag`); DELETE limpa só a RAM |
| GET | `/exportar` | Exporta a sessão em markdown |
| GET/POST | `/sessoes` | Lista (inclui `legado`) / cria sessão |
| POST | `/sessoes/ativar` | Troca sessão |
| POST | `/upload` | Upload de imagem/áudio (filename sanitizado) |
| POST/GET | `/enxame`, `/enxame/{id}`, `/enxame/{id}/consolidar` | Enxame de sub-agentes (chamado pelas ferramentas de enxame) |
| WS | `/ws/voice` | Voice Live (Gemini Live) |
| GET/POST | `/mcp` | Endpoints REST como ferramentas MCP (exclui `/chat`, `/dashboard`, `/upload`, `DELETE /historico`, `/tts/mudo` e destrutivos) |
| static | `/imagens` | Imagens geradas |

**Removidas em 01/10/2026** (sem cliente depois da saída do v2/v3): `/status`,
`/buscar`, `/grafo?q=`, `/resumo_sessao`, `PATCH`/`DELETE /sessoes/{id}`,
`/sessoes/{id}/favoritar`, `/enxames`, `/agente*`, `/shadow_thoughts`, `/auth/*`,
`/tools*`, `/logs`, `/ollama/models*`, `/prompts*`, `/ws/gateway`, mounts `/ui` e
`/ui-novo`. Clientes restantes: front v1, `orion_telegram.py`, `mic_engine.py`,
`dashboard.html` e as ferramentas de enxame.

`embed_service` (:8001): `GET /health` (inclui `estacionado_ram`), `POST /embed`
(`{texto|textos}`), `POST /rerank`, `POST /unload`.

### 3.4 Segurança em runtime

Todo dispatch de ferramenta passa por `_executar_tool_segura()`:

1. **Rate limit** (`orion_seguranca.checar_rate_limit`, deque + lock): `executar_comando`
   20/5min · `iniciar_processo_bg` 5/5min · `escrever_arquivo` 30/60s ·
   `organizar_pasta` 3/5min · `consultar_especialista` 3/10min ·
   `navegar_web` 10/5min.
2. **Câmara de Eco** (reescrita em 02/10/2026; delega a `orion/policy`): `avaliar_risco_acao`
   avalia `executar_comando`/`iniciar_processo_bg` (**lista positiva**: só roda sem confirmação o
   que for provadamente leitura; sem redirecionamento, substituição, bloco de script ou `::`;
   `.env`/credenciais não contam como leitura), `escrever_arquivo`/`gerar_documento` (a raiz do
   projeto, diretório de sistema, extensão/arquivo sensível e fora de Documents/Downloads/Desktop
   pedem confirmação) e `organizar_pasta` (raiz de drive, pasta pessoal, sistema, código do Orion).
   Risco alto → `BLOQUEADO_RISCO`, grava hash SHA256 da ação exata. Aprovação só com frase de
   confirmação com verbo ("sim, executa mesmo assim"; nunca "sim" solto), **no turno
   imediatamente seguinte** (`_contador_turnos`) e dentro de 10min. Uso único. Quirk conhecido:
   Groq às vezes repete a mesma tool call → bloqueio "fantasma" que expira sozinho. A blocklist
   antiga (substrings) deixava passar `ri -r -fo`, `rmdir /s /q`, `-enc`, `[IO.Directory]::Delete`
   e `irm | iex`. Na reescrita a confirmação deixa de ser frase: ver ORION_REGRAS.md (regra 2).
3. **Audit log** (`audit_log` no SurrealDB, buffer + thread de flush a cada 5s;
   `tool_filtro` validado por regex).
4. **Self-healing** no loop proativo a cada ~5min: SurrealDB, Qdrant, embed_service
   (via `asyncio.to_thread`; não checa o próprio FastAPI).

Outras barreiras (02/10/2026): `url_guard.py` (SSRF em `buscar_url`/`navegar_web`), `origem.py`
(`Origin` do hub `:8765` e de `/ws/voice`), limite de 25 MB no `/upload`.

CORS: `allow_origins=["null", "http://127.0.0.1:8000", "http://localhost:8000"]`,
`allow_credentials=False`. `"null"` é exigido pelo pywebview do v1.

Auth (apagado em 01/10/2026, sem cliente): era PBKDF2-SHA256 (stdlib, 260k
iterações), JWT HS256, cookie httpOnly `samesite=lax`, 30 dias, secret em
`AUTH_JWT_SECRET`. Código em `utils/auth.py` no histórico do git, para portar na
fase 5 do NUCLEO.

## 4. Memória e RAG

### 4.1 Base vetorial (~3.09M pontos)

> **Decidido em 30/09/2026:** tudo abaixo exceto `episodio` sai da memória
> (datasets genéricos). Tabelas §4.8 e scripts de ingestão saem junto na reescrita.

| Categoria | Pontos |
|---|---|
| `conhecimento_geral` (Wikipedia PT) | 1.112.246 |
| `conversa_geral` | 1.001.379 |
| `raciocinio_matematico` | 402.473 |
| `instrucao_ptbr` | 325.410 |
| `programacao` | 238.857 |
| `conhecimento_qa` | 6.828 |
| `referencia_database/frontend/webdev/llm` (docs curadas) | 1.480 |
| `episodio` (conversas) | cresce em runtime |

Índice de payload `categoria: keyword` criado.

### 4.2 Busca híbrida (`RAGEngine.search`, usada pelo `/chat`)

BM25 (`bm25s`, índice em `bm25s_index/` + `bm25s_meta.pkl`) + denso (Qdrant) →
RRF → reranker cross-encoder → freshness. `score_final = relevância·0.94 +
freshness·0.06`, meia-vida por categoria: `programacao`/`documentacao` 180d,
`conhecimento_geral` 3650d, `raciocinio_matematico`/`episodio` sem penalidade.
Sem decaimento temporal genérico (removido — ver NUCLEO §8). `retrieval_count` e
`last_accessed_at` continuam gravados, fora do ranking. `top_k` ajustado por
carga cognitiva (alta −2, baixa +2). Se o embed_service cair, só BM25.
Latência quente ~630-660ms (medida no antigo `/buscar`).

### 4.3 Grafo (SurrealDB)

`record_event()` grava o evento no SurrealDB e no Qdrant (não atômico) e cria
`RELATE evento->precedeu->evento` e `RELATE evento->sobre->topico:keyword`.
`search_graph()` faz traversal `<-sobre<-evento`. `/grafo/completo` filtra links
órfãos antes de responder (link para nó fora da janela `limite` derrubava o
3d-force-graph).

### 4.4 Shadow Thoughts (`orion_shadow_thoughts.py`)

A cada ~3h (180 iterações do loop). NREM: dedup só em
`categoria=episodio`, paginado por cursor; marca `duplicado=True`. REM: arestas
cross-domain via grafo. DEEP: resume eventos com mais de 4 semanas. 1ª execução:
28 duplicatas, 2.5s.

### 4.5 Innovations (01/07)

1. **Goal Drift Detector** — `classify_intent` (`objetivo|conclusao|passo|resposta`),
   gravado no evento; objetivos em aberto entram no briefing.
2. **Freshness Tags** — §4.2.
3. **Session Replay** — tópicos dominantes + ferramentas das últimas 24h no briefing.
4. **Cognitive Load Throttling** — `baixa|media|alta` pela latência do último chat.
5. **Response Provenance** — IDs Qdrant das fontes em `fontes_rag` (histórico, SurrealDB, payload). Sem UI.

### 4.6 Speculative Decoding (removido)

Sidecar de detecção de alucinação com um draft local em paralelo à cascata.
Saiu com o Ollama em 01/10/2026; código no histórico do git.

### 4.7 Reconciliação SurrealDB ↔ Qdrant

`reconciliar_episodios.py` (CLI ou `silencioso=True`) re-embeda eventos sem vetor.
Roda sozinho a cada ~1h no loop proativo; só loga se achar órfãos.

### 4.8 Datasets ingeridos

Schema: `{titulo, texto (≤3000), fonte, categoria}`. Os scripts de ingestão e
vetorização foram apagados em 01/10/2026 (datasets genéricos saem); continuam no
histórico do git.

| Tabela | Datasets |
|---|---|
| `wiki_conhecimento` | wikimedia/wikipedia 20231101.pt (1.11M artigos) |
| `base_codigo` | Evol-Instruct-Code-80k, code_instructions_120k_alpaca, CodeAlpaca-20k, python_code_instructions_18k |
| `base_instrucoes_ptbr` | Canarim-Instruct-PTBR, aya_dataset (por) |
| `base_raciocinio` | gsm8k, MetaMathQA |
| `base_conversas` | OpenHermes-2.5 |
| `base_conhecimento_qa` | br-quad-2.0, faquad |
| `base_medicina_ptbr` | AKCIT/MedPT (384k pares) |

### 4.9 Validação do RAG

`validador_cortical.py` (apagado em 01/10) gerava perguntas sintéticas de eventos e
media Hit Rate/MRR. **Último baseline (n=100, 01/07): HR 10.0%, MRR 0.057** — baixo
porque exigia o episódio exato contra 3M docs genéricos. Na reescrita vira
avaliação em pytest sobre a memória pessoal.

## 5. Agentes

**Enxame (`orion_agentes.py`)** — tarefa → N subtarefas em paralelo
(`asyncio.Semaphore`, `max_paralelo=2` default, nunca >3; timeout 120s por
subtarefa; recusa com GPU >85% ou VRAM livre <1.5GB). Tabelas `enxame` e
`subtarefa` (`pendente|rodando|concluida|erro`). Denylist `TOOLS_BLOQUEADAS`:
execução de comando, escrita, processos bg, agendamentos,
`salvar_memoria`, notificações, `consultar_especialista`, `*_enxame`. Loop proativo
consolida e notifica enxames concluídos.

**ReAct (`orion_agent.py`, apagado em 01/10/2026)** — `executar_agente(objetivo,
max_iteracoes, ferramentas_bloqueadas)` via `LLMCascade.run()`, mesma denylist,
persistia em `agente_run`.

## 6. Voz, visão e integrações

- **Wake-word** (`mic_engine.py`): "orion" (e variantes de transcrição) → Silero VAD → faster-whisper → `/chat`.
- **Voice Live** (`orion_voice_live.py`, `/ws/voice`): modelo
  `gemini-2.5-flash-native-audio-latest`, `response_modalities=["AUDIO"]` (uma
  modalidade só) + `output_audio_transcription`, voz Charon. Protocolo: PCM16 16kHz mono →
  servidor; PCM16 24kHz mono ← servidor; JSON texto/done/erro.
- **TTS** (`audio_manager.py`): Gemini `gemini-2.5-flash-preview-tts` voz Charon
  (free tier 3 req/min, ~4s/frase) → edge-tts AntonioNeural → silêncio. Pipeline
  automático pausado.
- **Visão:** só Gemini Vision (sem fallback local desde 01/10/2026).
- **Tradução e clipboard com IA:** só Gemini.
- **`navegar_web`** (`orion_browser.py`): browser-use 0.13.1 com
  `browser_use.llm.google.chat.ChatGoogle` (`gemini-2.5-flash`; o 2.0 tem quota zero).
- **Telegram** (`orion_telegram.py`): só texto, consome `/chat` via SSE; default-deny sem allowlist.
- **Google Workspace** (`orion_google_workspace.py`): Gmail + Calendar via OAuth2.
- **`gerar_imagem`**: Pollinations.ai (Flux, sem key).
- **Clima**: wttr.in (default Marília-SP).

## 7. Frontends

### 7.1 v1 — pywebview + Three.js (redesenhado em 03/10/2026)

Brief de design, diagnóstico do front anterior, sistema de design, orçamentos e o que
foi verificado: [ORION_FRONT.md](ORION_FRONT.md). Aqui, só o que a base precisa saber.

`Orion_Core/Front_end_Orion/`:

```
index.html · orion.svg/.ico · orion_app.py (launcher) · ponte.py (regras puras da ponte)
css/  tokens · base · layout · components · chat · views     (3 temas, densidade, escala em rem)
js/   util · md · sse · store · charts · fuzzy · slash         (puros, UMD, testados em Node)
      core · api · transport · ui · sky · sound · voice        (infra e mídia)
      chat · composer · sidebar · palette · busca · app · views/* (interface)
vendor/ three r128, 3d-force-graph (carregado só na 1ª visita à Memória), Inter
```

Scripts simples com namespace `Orion.*`, sem bundler (módulo ES não carrega em `file://`
no pywebview). Contratos com o backend inalterados: hub WS `:8765`
(`state`/`intensity`/`user_text`/`tier`/`ai_chunk`, mais `tool`/`approval`/`error` quando o
cérebro é o `orion.app`), API do pywebview (`process_command`, `toggle_maximize`,
`minimize_app`, `close_app`, e agora `get_config`/`open_external`), rotas HTTP da §3.3,
`/ws/voice`.

- **Dois caminhos de chat, mesmos eventos internos** (`chat:evento`): no app desktop,
  `process_command` + hub (o `mic_engine` continua enxergando a conversa); em qualquer outro
  lugar (navegador, ou desktop sem o hub), `fetch` em streaming no `/chat` (SSE). O front detecta o que
  existe e degrada sem erro quando um endpoint não existe (legado × `orion.app`).
- **Só desktop:** o app não roda no celular (o canal de bolso é o Telegram). O `orion.app` ainda serve a interface em `/ui/` (`ORION_SERVE_UI`, mesma origem, sem CORS) para abrir no navegador do PC; janela estreita (≤ 860 px) vira trilho de ícones, sem gaveta.
- **Aprovações de ação** (`orion.policy`): o `/chat` emite `tool` e `approval`; o front mostra o
  cartão com o comando exato, `Aprovar e executar`/`Negar` (`POST /approvals/{id}/decide`) e
  retoma a resposta (`/approvals/{id}/resume`). Token em Configurações › Conexão (o app desktop
  usa `ORION_ADMIN_TOKEN` do ambiente, via `get_config`).
- **Constelação:** 22 estrelas com ascensão reta/declinação reais, profundidade pelo log da
  distância, nebulosa M42. Estados: em espera, ouvindo, processando (traço âmbar), respondendo.
  Na home 60 fps, paralaxe e rótulo ao passar o mouse; atrás das outras telas ≤ 20 fps, pixel
  ratio 1, pausa com a aba oculta. Sem WebGL: céu estático em CSS.
- **Segurança do front:** markdown escapa antes de formatar; link só http(s)/mailto com
  `rel="noopener noreferrer nofollow"` e host visível; imagem só de `/imagens/<arquivo>`; CSP no
  `index.html` (`script-src 'self'` + hash do único script inline); links abrem fora do app por
  `open_external` (só http/https/mailto, validado em `ponte.py`); rótulos do grafo por
  `textContent`/escape.
- **Acessibilidade:** axe-core sem violação em 5 telas × 3 temas (Noite, Grafite, Alto
  contraste), teclado completo (paleta `Ctrl+K`, comandos `/`, `Ctrl+F`, `Ctrl+.`, `Alt+1..5`, `?`), foco visível, telas ocultas
  `inert`, menu `/` e paleta como combobox, `aria-live` só na resposta pronta.
- Preferências em `localStorage` com prefixo `orion_` (tema, densidade, escala, movimento,
  modelo, sons, endereço, token). Chamadas internas em `127.0.0.1`, nunca `localhost`.

### 7.2 v2 e v3 — removidos em 30/09/2026

Código no histórico do git (antes do commit `88f0b1b`, nas pastas dos front-ends
v2 e v3). O que vale
lembrar para a fase 6:

- **v2** — React + Vite + TS, servido em `/ui`. Chat SSE com stop/retry/copiar,
  markdown sanitizado (DOMPurify), settings em 7 abas, dark-only, sem CDN.
- **v3** — SvelteKit (Svelte 5 runes, `adapter-static` SPA), servido em `/ui-novo`.
  `streamChat()` lia o SSE do `POST /chat` na mão; rune `$state` só funciona em
  `.svelte`/`.svelte.ts` (`svelte-check` e build não pegam isso). Grafo em Canvas 2D
  com `alpha` decaindo ×0.985 até <0.01 e velocidade limitada a 8px/frame. Orb com
  64 pontos Fibonacci e 3 vizinhos ligados. Tokens `--bg #0b0b0d`, `--accent
  #4fc3d9`, `[data-reduce-motion="1"]`. Atalhos `Ctrl+K`, `Ctrl+Shift+O`, `Esc`.
- **Tauri** — casca com 3 comandos Rust (`get_secret`, `set_setting`,
  `start_backend`) atrás de um contrato de casca (`BrowserShell`/`TauriShell`), o
  frontend sem saber onde roda.

## 8. Avaliações registradas

- **SurrealDB 3.0 vetorial (HNSW/DISKANN) vs Qdrant — não migrar:** índice quente em
  RAM estimado em 15-20GB para 3.08M×1024, reescrita do RAG, Qdrant estável. Se
  revisitar, testar numa coleção pequena (`episodio`).
- **Mem0/Kore — não adotar:** Kore é conceitualmente o que a base já faz; a resolução
  de conflitos por LLM do Mem0 é problema do REM do Shadow Thoughts.
- **A2A — não adotar:** resolve interoperabilidade entre fornecedores; ideias
  reaproveitáveis: estado `input-required` e capacidades declaradas (já refletidas em
  `ESPECIALISTAS`).
- **C++/Rust no backend — não faz sentido:** partes pesadas já são nativas (Qdrant,
  SurrealDB); gargalo é modelo/VRAM.
- **Modelos locais:** fora por enquanto (Ollama removido em 01/10/2026).
  Groq deprecou `llama-3.x` em 17/06/2026 — cascata já usa `gpt-oss-120b`.

## 9. Versão 2.0 do legado (plano de 11/08/2026 e execução)

> Histórico. Em 30/09/2026 os front-ends v2 e v3 e a casca Tauri foram removidos:
> o cutover descrito aqui não acontece mais e o gating de auth passa para a
> reescrita (NUCLEO, fase 5).

### 9.1 Referências analisadas

Cinco projetos (clonados em `TESTE/`, fora do git): o que foi aproveitado.

| Projeto | Aproveitado | Descartado |
|---|---|---|
| Open WebUI (FastAPI + SvelteKit) | Subset mínimo de auth, organização `routers/models/utils`, tela "criar admin no 1º boot" | LDAP, OAuth/OIDC, SCIM, Redis, config no banco |
| LibreChat | Ideia de artifacts, tokens de tema | Auth enterprise (Passport com 7+ estratégias) |
| AnythingLLM | Modo single-user como referência; upload → embeddings com progresso | Multi-processo, convites, workspaces |
| Jan (Tauri) | Segredos/settings no lado nativo; hub de modelos; telas de MCP/monitor/logs | — |
| OpenClaw | Padrão Gateway (1 processo dono do estado, WS tipado), `SecretRef`, pareamento QR | Adaptadores multi-canal |

Paridade Open WebUI (12/08) — inaplicável por ser multi-usuário: RBAC, grupos,
analytics, leaderboard, canais, compartilhamento com ACL.

### 9.2 Auth mínimo — proteção de acesso

**Decidido: (A) proteção de acesso**, não multi-usuário. Uma conta admin; a senha
protege o app. Tabela `users` no SurrealDB (aditiva), PBKDF2-SHA256, JWT HS256 em
cookie httpOnly (30 dias), `CurrentUserDependency` pronta, rotas `/auth/status`,
`/auth/setup` (só com 0 usuários), `/auth/login`, `/auth/logout`. Frontend v3
mostra "criar conta" ou "entrar" conforme `/auth/status`.

**Pendente por decisão consciente:** nenhuma rota existente exige login — gatear
trancaria o usuário fora do v2 (sem tela de login). Era para entrar junto com o
cutover pro v3 e a correção do CORS `"null"`; com o v2/v3 removidos, vai para a
reescrita. As rotas `/auth/*` foram desmontadas em 01/10/2026.

### 9.3 Backend: `routers/models/utils` + Gateway WS

- Backend continua **Python + FastAPI** (escolha certa mesmo do zero: ecossistema de IA).
- `cerebro_maestro.py` 1594 → ~930 linhas: só estado compartilhado, helpers e
  wiring. Todos os endpoints vivem em routers (classes POO com dependências via
  construtor; estado mutável compartilhado via getters/setters).
- **Gateway WS** (`/ws/gateway`, envelope `{action, payload}`) é camada
  **aditiva**, só ações de leitura, reaproveitando os métodos dos routers REST.
  Não substitui REST/SSE — trocar `/chat` para WS exigiria reescrever o consumo
  do v2. Fase considerada encerrada nesse ponto. Removido em 01/10/2026 (sem cliente).

### 9.4 Capacidades novas (o que a v1 não tinha)

| Item | Status |
|---|---|
| Onboarding/login no 1º boot | ✅ v3 |
| Favoritar sessões (`POST /sessoes/{id}/favoritar`) | ✅ — removido em 01/10 |
| Monitor de sistema + viewer de logs (`/health`, `/metrics`, `GET /logs`) | ✅ `SystemPanel` — `/logs` removido em 01/10 |
| Painel de ferramentas: listar + ligar/desligar (`/tools`, `/tools/{nome}/toggle`, estado em memória) | ✅ — removido em 01/10 |
| Preview de artifacts (`html`/`svg` em iframe sandboxed, inline por bloco) | ✅ |
| Hub de modelos locais (listar + pull com progresso SSE, confirmação obrigatória) | ⛔ removido junto com o modelo local |
| Grafo de memória em Canvas 2D (força com decaimento `alpha`) | ✅ `GraphPanel` |
| Biblioteca de prompts (`/prompts` CRUD, clique insere no composer) | ✅ — removido em 01/10 (dados na tabela `prompt`) |
| `SecretRef` (`utils/secrets.py`: env/file/exec + máscara) | ✅ criado; nunca usado — removido em 01/10 |
| Upload de documento → base de conhecimento com progresso | ⏳ |
| Pareamento de dispositivo por QR/token | ⏸ adiado (YAGNI — não há cliente que consuma) |
| Personas por modelo, notas, calendário, automações, branching de resposta, citações inline, execução de código, tags/pastas, editor de tools in-app | ⏳ aguardando priorização do usuário |

### 9.5 Frontend v3 — SvelteKit (removido em 30/09/2026)

Reescrita completa (não port do React): bundle menor, menos boilerplate. Nasceu com
login/onboarding, chat streaming, sessões (renomear/excluir/agrupar por data),
busca de memória (`Ctrl+K`), configurações em 7 abas, composer multi-linha com
parar/tentar de novo/copiar, seletor de modelo, voz live, esfera (Orb) no estado
vazio, e todos os itens da §9.4. Paleta mais contida que o v1: `--bg #0b0b0d`,
`--accent #4fc3d9`, peso 300 (decisão do usuário 11/08: base no visual atual,
priorizando minimalismo). Detalhes técnicos: §7.2.

### 9.6 Casca desktop — Tauri (removida em 30/09/2026)

Tauri é a casca "só assistente" que carrega o SvelteKit. O frontend nunca sabe onde
está: interface fina de casca (`getSecret`, `setSetting`, `startBackend`)
implementada por `BrowserShell` e `TauriShell` (`window.__TAURI_INTERNALS__` → 3
comandos Rust). A casca Theia (IDE) foi excluída em 30/09/2026.

### 9.7 Regras de transição

- Mudança de schema sempre **aditiva** — nunca remover/renomear campo que código antigo lê.
- Snapshot SurrealDB + Qdrant antes de qualquer mudança de schema real.
- ~~**Critério de corte do v2:** o React continua em `/ui` até o v3 ter paridade
  testada; só sai após alguns dias de uso real sem bug crítico.~~ Obsoleto: v2 e v3
  removidos juntos em 30/09/2026.
- Instalador **não** carrega os ~3M vetores: conecta nos bancos existentes; "começar
  vazio" ou "importar snapshot" (~2-3GB) só se pedido.
- Footprint (backend + bancos + casca) ainda não medido — relevante agora que o
  hardware muda.

### 9.8 Status das fases (12/08/2026)

| Fase | Status |
|---|---|
| 1 — Backend `routers/models/utils` | ✅ 7/7 grupos de endpoint extraídos |
| 2 — Auth mínimo | ✅ backend; gating pendente (§9.2) — rotas desmontadas em 01/10 |
| 3 — Gateway WS | ✅ camada aditiva de leitura (encerrada) — removido em 01/10 |
| 4 — Frontend SvelteKit | ✅ núcleo + paridade com v2 auditada; servido em `/ui-novo` — removido em 30/09 |
| 5 — Casca desktop | ✅ contrato + `TauriShell`; `cargo check`/`clippy` limpos — removida em 30/09 (a casca Theia também) |

Em 12/08, nada bloqueado; restava sequenciamento de produto (cutover, gating) e
empacotamento/teste ao vivo — superado pela reescrita. Verificação feita:
compile/import + self-checks no backend, `svelte-check` + build + Playwright contra backend real no frontend.
**Não verificado:** cascata fim-a-fim com todas as nuvens e áudio real de voz.
