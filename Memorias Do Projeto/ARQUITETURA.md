# Orion — arquitetura

> Como o sistema é organizado hoje (Python) e para onde vai (Rust). Princípios e onde cada um é imposto: [REGRAS.md](REGRAS.md). Estado e plano: [ORION.md](ORION.md). Como rodar: [README](../README.md). Detalhes do legado: [ORION_LEGADO.md](ORION_LEGADO.md).

## 1. Hoje: pacote `orion/` (Python)

Um processo FastAPI em `127.0.0.1`, com política de ferramentas, memória em SQLite e canais (navegador, Telegram, ponte de desktop).

| Área | Módulos | O que faz |
|---|---|---|
| Entrada e API | `app.py`, `config.py`, `__main__.py`, `log.py`, `secrets.py`, `doctor.py` | Rotas, WebSocket, configuração (opt-ins e limites), CLI `orion ...`, logs sem segredo, cofre do SO, diagnóstico |
| Acesso | `auth.py`, `netguard.py` | Senha (PBKDF2), sessão opaca, tokens de dispositivo; barreira de rede para URLs que vêm do modelo |
| Modelos | `gateway.py`, `provedores.py`, `router.py`, `costs.py`, `saidas.py`, `delegate.py`, `persona.py` | Chat em streaming com fallback e quarentena; provedores diretos; roteamento por tipo de tarefa; cota gratuita; registro de saídas; CLIs delegadas (`claude`, `codex`, `gemini`); identidade versionada |
| Agente | `agent.py`, `memory/compact.py` | Loop próprio, ferramentas sob política, marcação de conteúdo externo; resumo local de conversas longas |
| Política | `policy/` (`classes.py`, `engine.py`, `paths.py`, `shell.py`, `approvals.py`, `audit.py`) | Classes de risco, aprovações fora de banda, caminhos protegidos, shell com lista positiva, audit |
| Memória | `memory/` (`store.py`, `ops.py`, `consolidate.py`, `sleep.py`, `importer.py`, `embedders.py`, `rerank.py`, `eval.py`), `grafo.py`, `fact_routes.py`, `grafo_routes.py` | SQLite com FTS5 e vetores em numpy, fusão RRF; fatos com fonte e data; consolidação, ciclo de sono, importação, reranker desligado, grafo |
| Ferramentas | `tools/` (`desktop.py`, `system_tools.py`, `documents.py`, `processes.py`, `media.py`, `vision.py`, `web.py`, `n8n.py`) | Arquivos, comandos, documentos, processos, mídia, janelas, visão, web e n8n, todos sob política |
| Extensões | `skills.py`, `plugins.py`, `extensions/` (`catalogo.py`, `importar.py`, `host.py`, `bundled/`), `mcp_client.py`, `mcp_servidor.py`, `mcp_indice.py` | Skills em texto, plugins com concessão por hash, catálogo local e remoto, cliente e servidor MCP |
| Canais | `channels/telegram.py`, `ponte/`, `painel.py` | Telegram (texto, voz, foto, botões); ponte de desktop (teclas, captura rápida, colar o aprovado); painel de leitura |
| Voz e sentidos | `voice.py`, `wake.py` (palavra de ativação e `ClapDetector`), `transcribe.py`, `vision.py`, `screen_memory.py`, `capture.py` | Voz por clique e ao vivo; palavra de ativação e palmas (detecção local); transcrição; OCR local da tela; captura rápida |
| Visão e presença (E11) | `camera.py`, `visao_cli.py`, `visao_local.py`, `rosto.py`, `presenca.py`, `vigia.py`, `legenda.py`, `tools/camera.py` | Foto sob pedido, visão pelas assinaturas ou local (YOLO ONNX, rótulos sem imagem), rosto local do dono, presença (pausa e cumprimento; bloqueio só confirmado) e legenda ao vivo (Vosk) |
| Vida pessoal (E10) | `vida.py`, `vida_routes.py` | Finanças locais (gastos, contas, preço), cápsula do tempo, linha do tempo, diário, revisão semanal, escudo de foco, estado pela voz, sonhos e cenários |
| Rotina e segundo plano | `jobs.py`, `briefing.py`, `agenda.py`, `rotinas.py`, `downloads.py`, `automacao.py`, `tarefas_bg.py`, `research/night.py`, `historicos.py`, `modos.py`, `media_transcribe.py`, `autostart.py` | Lembretes, agendamentos, consolidação, backup; briefing; agenda; rotinas e regras de Downloads concedidas; pesquisa noturna; pânico e não perturbe |

A ordem de leitura do código, para quem entra agora: `app.py` (rotas), `policy/engine.py` (decisões), `agent.py` (turno), `memory/store.py` (dados).

## 2. Dados

| Onde | O que guarda |
|---|---|
| `orion.db` (SQLite, pasta de dados do usuário ou `ORION_DATA_DIR`) | Sessões e mensagens; fatos; documentos e trechos com FTS5 e vetores; arestas do grafo; lembretes, prompts e usuários; `audit` (esquema v3, poda de 90 dias); `external_calls` (esquema v11, sem conteúdo); resumos de sessão; `screen_log` |
| `auth.db` | Senha em hash, sessões e tokens de dispositivo. Fica **fora** do backup |
| `orion.db`, tabelas de vida e visão (esquema v23) | `transactions`, `bills`, `price_watch` (finanças, só local); `object_sightings` (rótulos de objetos, 7 dias, sem imagem); `faces` (vetores cifrados, chave no cofre) |
| `<dados>/backups/` | Backup diário da memória, mantém 7 (`ORION_BACKUP_DIR` aponta para iCloud ou OneDrive) |
| `<dados>/` (pastas) | `mcp.json`, `plugins/`, `catalogo/`, `rotinas/`, `regras_downloads.yaml`, `capturas/`, `imagens/` |
| Vault do Obsidian (`ORION_VAULT_DIR`) | Indexado de hora em hora. Escrita só em `00 Inbox` e na pasta de captura |

## 3. Saídas para fora do computador

Toda chamada a um destino de fora grava uma linha em `external_calls` (sem o conteúdo). Os provedores gratuitos estão listados em `orion/costs.py`.

| Destino | Para quê | Condição |
|---|---|---|
| Provedores de modelo (Gemini, Groq, Cerebras, OpenRouter, Mistral, GitHub Models, NVIDIA, Z.ai ou endpoint com `url`) | Chat | Ordem em `ORION_PROVEDORES`; cota gratuita; chave com `orion chave <id>` |
| Groq (Whisper) | Transcrição de voz e de mídia | `ORION_TRANSCRIBE_API_KEY` |
| Gemini | Embeddings (`gemini-embedding-001`), visão, voz ao vivo, pesquisa com Google Search | Chave do Google AI Studio; voz ao vivo é opt-in |
| Brave Search | `pesquisar_internet` | Opt-in (`ORION_WEB_TOOLS`) |
| Microsoft (edge-tts) | Fala das respostas | Corte com `ORION_VOICE_SPEAK=false`; só o texto da resposta sai |
| Telegram | Canal do celular | Token e IDs permitidos; default-deny |
| Servidores MCP (Google Workspace, navegador, outros) | Ferramentas externas | Conforme `mcp.json`; classe de risco do arquivo |
| n8n | Webhooks que você cadastra | `ORION_N8N_WEBHOOKS`; acionar sempre confirma |
| CLIs delegadas (`claude -p`, `codex exec`, `gemini -p`) | Tarefa pesada, com as assinaturas oficiais | Tempo-limite, pasta de trabalho definida, contador diário |
| Modelo local (Ollama em `127.0.0.1`) | Reserva, sem ferramentas | `ORION_LOCAL_MODEL`; só loopback |
| Gateway de visão e CLIs de assinatura (`ORION_VISION_ORDER`) | Foto da câmera (`ver_camera`, `/camera`) e quadros da voz ao vivo, quando ligados | `ORION_CAMERA=true`; `ver_camera` sempre confirma; cada uso vai ao registro de saídas |
| Pesquisa na web (Brave ou Google, conforme o provedor) | Só o nome do produto cadastrado no monitor de preço | Opt-in (`ORION_WEB_TOOLS`) e cota gratuita; o preço que volta é guardado |

Não saem do computador: finanças, rosto, rótulos de objetos, presença, detecção local e legenda ao vivo.

## 4. Fluxos

- **Mensagem.** Canal (Telegram, navegador, voz) → login → contexto (persona, fatos, memória relevante, histórico, e resumo se a conversa for longa) → agente chama o modelo com as ferramentas → cada pedido passa pela política: leitura roda direto, escrita roda e fica no audit, execução e destrutiva pedem aprovação no próprio canal → resposta em streaming → gravação. Consolidação e resumos rodam depois, em job.
- **Aprovação.** Cartão no canal, com botão. Vale por 10 minutos, uso único, e só para os argumentos exatos. Voz e frase nunca aprovam.
- **Voz.** Transcrição (Groq) → texto mostrado antes da resposta → turno comum, com memória, política e audit. A palavra de ativação e as palmas só abrem o microfone ou a janela, e a detecção é local.
- **Conteúdo externo.** Marcado como dado. Depois de lido, a sessão pede confirmação para escrita, execução e rede com destino escolhido pelo modelo.
- **Câmera.** Só com pedido seu (`ver_camera`, `/camera`, a paleta ou o botão da voz ao vivo). A foto vai ao modelo de visão, é descartada logo depois e cada uso fica registrado. Vigília e presença são locais: só rótulo, sem imagem, e a tela só é bloqueada com a sua confirmação.

## 5. Legado (sai na fase 7)

- `Orion_Ollama/`: backend FastAPI antigo (`cerebro_maestro.py`, `routers/`), com Qdrant e SurrealDB.
- `Orion_Core/`: front v1 (pywebview, Three.js, hub WebSocket na porta 8765), voz e sentidos antigos.
- `bin/`: `.bat` dos serviços antigos.

Continuam no repositório como referência e rodando no PC atual até a venda. O conteúdo deles está em [ORION_LEGADO.md](ORION_LEGADO.md).

## 6. Interface

- **Hoje:** navegador local, servido em `/ui/` pelo `orion.app`, com tela de entrada. O front v1 do legado usa pywebview.
- **Decisão:** só desktop, sem versão para celular. O celular fica no Telegram.
- **Destino:** iced (Rust), com o tema "Observatório noturno": paleta noturna, constelação de Órion, três temas. O design e a engenharia do front atual estão em [ORION_LEGADO.md](ORION_LEGADO.md) (documento ORION_FRONT).

A tela Vida (E10) e a legenda ao vivo (`/ui/legenda.html`, E11) fazem parte do front de desktop.

## 7. Destino em Rust

Núcleo e UI em Rust. Python só para browser-use e crawl4ai, em processo separado, sem porta aberta. Crates previstos:

| Crate | Papel |
|---|---|
| `orion-ui` | Telas em iced (render com wgpu); grafo nativo |
| `orion-core` | Agente, persona, política de risco, rotinas, projetos |
| `orion-store` | rusqlite com FTS5, mesmo esquema do Python durante a transição |
| `orion-ai` | Provedores (reqwest com SSE), embeddings, rerank, router |
| `orion-mcp` | Cliente e servidor MCP (rmcp) |
| `orion-channels` | Telegram (teloxide) |
| `orion-voice` | Captura (cpal), STT (whisper-rs), VAD, palavra de ativação, palmas, TTS |
| `orion-platform` | Trait comum; implementações `win/` (primeiro marco) e `mac/` (fase 9) |
| `orion-sidecar` | Ponte JSON (stdin e stdout) para o processo Python residual |

Equivalências de plataforma (Windows | macOS): início automático (registro ou Task Scheduler | LaunchAgent); bandeja (`tray-icon`); teclas globais (`global-hotkey`); área de transferência (`arboard`); notificação (toast | `notify-rust`); cofre (Credential Manager | Keychain); captura de tela (`windows-capture` ou `xcap` | `xcap` ou ScreenCaptureKit); OCR (`Windows.Media.Ocr` | Vision); microfone (`cpal`). Permissões do macOS (Acessibilidade, Gravação de Tela, Microfone) exigem onboarding próprio.

## 8. Referências

### 8.1 Onde estão as pesquisas (no vault, `03 Recursos/Pesquisa Técnica/`)

- **Orion - matriz de portabilidade para Rust:** módulo a módulo, com classes F (fácil), M (médio), D (difícil) e X (sem equivalente).
- **Repositórios de referência para o Orion em Rust (2026-10-14):** 19 repositórios, com bibliotecas candidatas e referências de arquitetura.
- **Pesquisa ampla de projetos e bibliotecas em Rust para o Orion (2026-10-14):** organizada por módulo.
- **Pesquisa ampla II - links de referência em Rust para o Orion (2026-10-14):** mais de 150 links.
- **Pesquisa ampla III - Hugging Face para o Orion (2026-10-14):** 23 páginas verificadas quanto à licença e ao idioma; cerca de 100 links.

### 8.2 Candidatos em Rust, por área

| Área | Candidatos | Observação |
|---|---|---|
| UI | iced (escolhido), Slint (plano B), egui (referência) | egui descartado para a UI final; PhotoCraft e EffectCraft mostram que egui sustenta app grande com GPU |
| Grafo | wgpu | Canvas nativo |
| Memória | rusqlite com FTS5; cel-memory-sqlite (sqlite-vec e FTS) | sqlite-vec não carrega no Python do macOS; validar antes |
| Embeddings e rerank | fastembed-rs (BGE-M3 denso, esparso e ColBERT, via `ort`) | Comparar com o Python na mesma base |
| Palavra de ativação | oww-rs (openWakeWord em tract) | Modelo oficial é não comercial (ver 8.3) |
| VAD | silero (ONNX) | MIT nos espelhos |
| STT | whisper-rs (whisper.cpp) | Pesos ggml, não GGUF |
| Áudio | cpal, rubato (reamostragem para 16 kHz) | |
| Provedores | rust-genai, rig | Licença do rig a confirmar |
| MCP | rmcp (SDK oficial) | |
| Telegram | teloxide | |
| Plataforma | tray-icon, global-hotkey, arboard, notify-rust (mac), windows (toast), keyring | |
| Captura e automação de janelas | xcap, windows-capture, uiautomation, winocr | |
| Documentos | pdf-extract, rust_xlsxwriter, calamine | |
| Código de referência | huggingface/candle (Safetensors, GGUF, GGML), mistral.rs (Candle, API compatível com OpenAI e Anthropic) | kalosm, rust-bert e oxidized-transformers citados, sem análise |

### 8.3 Modelos no Hugging Face (verificados quanto à licença, salvo indicação)

Nada foi baixado nem medido. Os números de WER e velocidade são dos autores.

| Área | Modelo | Licença | Nota |
|---|---|---|---|
| STT PT-BR | `nvidia/parakeet-tdt-0.6b-v3` | CC-BY-4.0 | Português incluído, com treino em PT europeu; detecta o idioma; GGUF só via NeMo-Speech.cpp |
| STT em Rust | `ggerganov/whisper.cpp` (ggml, large-v3-turbo) | MIT | Para `whisper-rs` |
| STT streaming PT-BR | `ottema/nemotron-3.5-asr-ptbr` | OpenMDW-1.1 | Chunks de 80 ms a 1,12 s; WER normalizado CORAA 20,96 (Whisper large-v3: 25,15) |
| TTS PT-BR | `hexgrad/Kokoro-82M` (vozes `pf_dora`, `pm_alex`, `pm_santa`) | Apache-2.0 | ONNX só por terceiros |
| TTS com clonagem | `ResembleAI/Chatterbox-Multilingual-pt-br` | MIT | Clonar só a própria voz ou de quem autorizou |
| TTS leve | `edwixx/rapido-tts` | Apache-2.0 | 21M parâmetros, uma voz, pesos PyTorch |
| TTS Piper | `rhasspy/piper-voices`, voz `pt_BR-faber` | MIT (repo); dados CC0 | Biblioteca Piper é GPL (busca) |
| Palavra de ativação | `davidscripka/openwakeword` | **CC-BY-NC-SA-4.0** | Não comercial; a página liga a licença a datasets de termos restritivos |
| VAD | `TEN-framework/ten-vad` | Apache-2.0 com condições extras | ONNX, bibliotecas C e WASM; entrada em 16 kHz |
| VAD (espelhos do Silero) | `deepghs/silero-vad-onnx`, `mijuanlo/silero-vad-onnx` | MIT | Upstream do snakers4 não aberto |
| Palmas | `andrelgomes/yamnet-onnx` (classe 58, `Clapping`) | Apache-2.0 | Janela de cerca de 1 s (de memória); só confirma uma janela |
| Teste de palmas | `ashraq/esc50` (classe `clapping`) | CC BY-NC | Só teste local |
| Embeddings | `BAAI/bge-m3` | MIT | Mais de 100 idiomas; 8192 tokens; denso, esparso e multi-vetor |
| Embeddings (alternativa) | `Qwen/Qwen3-Embedding-0.6B` | Apache-2.0 (tag) | O cartão GGUF usa "proprietary" nos destaques: conferir |
| Embeddings não comerciais | `jinaai/jina-embeddings-v3` | CC-BY-NC-4.0 | Fora se houver distribuição |
| OCR | `ibm-granite/granite-docling-258M` | Apache-2.0 | Saídas DocTags, Markdown e HTML |
| OCR pesado | `PaddlePaddle/PaddleOCR-VL` | Apache-2.0 (busca) | 0,9B; relato de crash em CPU |
| LLM local | `HuggingFaceTB/SmolLM3-3B` | Apache-2.0 | Português entre as seis línguas nativas |
| LLM em português | `TucanoBR/Tucano-2b4` | Apache-2.0 (busca) | |
| PII antes da nuvem | `jugaadsrl/gliner2-privacy-filter-PII-multi-onnx` | Apache-2.0 | 7 idiomas, incluindo PT; ONNX |
| Tela (GUI) | `microsoft/OmniParser-v2.0` | AGPL-3.0 no detector padrão; MIT em `icon_detect_v3` e `icon_caption` | Para evitar AGPL, usar o caminho v3 |
| NER | `urchade/gliner_multi-v2.1` | Apache-2.0 | Sem ONNX oficial na página |

**Licença-alvo ainda não decidida.** Se o Orion for só pessoal, os itens não comerciais não bloqueiam. Se for vendido ou distribuído, saem: openWakeWord oficial, ESC-50, Jina v3, OmniParser com o detector padrão e modelos de pesquisa, como o Sabiá-7B.
