# Orion — arquitetura

> Produto: app nativo Rust de um só executável (`orion.exe`, núcleo e interface iced no mesmo processo, sem porta e sem Python). O pacote Python `orion/` fica só como referência executável até a paridade e a venda do PC (auditoria do PR #30, atualizada em 20/10/2026). Princípios e onde cada um é imposto: [REGRAS.md](REGRAS.md). Estado e plano: [ORION.md](ORION.md). Como rodar: [README](../README.md). Detalhes do legado: [ORION_LEGADO.md](ORION_LEGADO.md).

## 1. Referência executável: pacote `orion/` (Python)

**Referência histórica (o produto é Rust).** Um processo FastAPI em `127.0.0.1`, com política de ferramentas, memória em SQLite e canais (navegador, Telegram, ponte de desktop).

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
| Groq (Whisper) | Transcrição de voz e de mídia | Python: `ORION_TRANSCRIBE_API_KEY`; Rust: exige também `ORION_TRANSCRIBE_GROQ_FALLBACK=true` |
| Gemini | Embeddings (`gemini-embedding-001`), visão, voz ao vivo, pesquisa com Google Search | Chave do Google AI Studio; voz ao vivo é opt-in |
| Brave Search | `pesquisar_internet` | Opt-in (`ORION_WEB_TOOLS`) |
| Microsoft (edge-tts) | Fala das respostas | Corte com `ORION_VOICE_SPEAK=false`; só o texto da resposta sai |
| Telegram | Canal do celular | Token e IDs permitidos; default-deny |
| Servidores MCP (Google Workspace, navegador, outros) | Ferramentas externas | Conforme `mcp.json`; classe de risco do arquivo |
| n8n | Webhooks que você cadastra | `ORION_N8N_WEBHOOKS`; acionar sempre confirma |
| CLIs delegadas (`claude -p`, `codex exec`, `gemini -p`) | Tarefa pesada, com as assinaturas oficiais | Tempo-limite, pasta de trabalho definida, contador diário |
| Modelo local embarcado (`orion-llama.exe`) ou Ollama opcional | Reserva, sem ferramentas e sem saída de dados | `ORION_LOCAL_MODEL_PATH` para GGUF instalado; legado `ORION_LOCAL_MODEL`/loopback |
| Hugging Face (pesos) | Instalar GGUF e Whisper | Só seleção explícita no orion-setup; catálogo com revisão, tamanho e SHA-256 fixados |
| Gateway de visão e CLIs de assinatura (`ORION_VISION_ORDER`) | Foto da câmera (`ver_camera`, `/camera`) e quadros da voz ao vivo, quando ligados | `ORION_CAMERA=true`; `ver_camera` sempre confirma; cada uso vai ao registro de saídas |
| Pesquisa na web (Brave ou Google, conforme o provedor) | Só o nome do produto cadastrado no monitor de preço | Opt-in (`ORION_WEB_TOOLS`) e cota gratuita; o preço que volta é guardado |

Não saem do computador: finanças, rosto, rótulos de objetos, presença, detecção local e legenda ao vivo.

## 4. Fluxos

- **Mensagem.** Canal (Telegram, navegador, voz) → login → contexto (persona, fatos, memória relevante, histórico, e resumo se a conversa for longa) → agente chama o modelo com as ferramentas → cada pedido passa pela política: leitura roda direto, escrita roda e fica no audit, execução e destrutiva pedem aprovação no próprio canal → resposta em streaming → gravação. Consolidação e resumos rodam depois, em job.
- **Aprovação.** Cartão no canal, com botão. Vale por 10 minutos, uso único, e só para os argumentos exatos. Voz e frase nunca aprovam.
- **Voz.** Transcrição (Groq) → texto mostrado antes da resposta → turno comum, com memória, política e audit. A palavra de ativação e as palmas só abrem o microfone ou a janela, e a detecção é local.
- **Conteúdo externo.** Marcado como dado. Depois de lido, a sessão pede confirmação para escrita, execução e rede com destino escolhido pelo modelo.
- **Câmera.** Só com pedido seu (`ver_camera`, `/camera`, a paleta ou o botão da voz ao vivo). A foto vai ao modelo de visão, é descartada logo depois e cada uso fica registrado. Vigília e presença são locais: só rótulo, sem imagem, e a tela só é bloqueada com a sua confirmação.

## 5. Código preservado durante a migração

O backend `Orion_Ollama/`, os serviços em `bin/startup/`, os motores de voz antigos e a distribuição PyInstaller foram removidos. O histórico e as instruções antigas continuam no Git e em [ORION_LEGADO.md](ORION_LEGADO.md).

`Orion_Core/Front_end_Orion/` guarda o front HTML/CSS/JS antigo e seus assets; **o app Rust não o serve mais** (a interface é o `orion-app` em iced) e a pasta só serve de protótipo/referência. `orion/` e `tests/` permanecem como referência executável para a paridade incompleta. O sidecar (`orion-sidecar`, `Orion_Rust/python/`) foi removido em 20/10/2026: leitura de páginas e navegador guiado são Rust. Scripts de treino/avaliação de voz, benchmark, fixtures MCP e edge-tts ainda usam Python, fora do produto.

## 6. Interface

- **Agora:** a interface é o `orion-app` (iced), rodando dentro de `orion.exe` com o núcleo no mesmo processo; a UI chama as rotas por `Api::local`, sem socket. O front HTML (`/ui/`) só existe em `orion serve` (`--features http`) para desenvolvimento. Mini barra, bandeja e atalhos globais (Ctrl+Alt+Espaço, Ctrl+Alt+P) vêm de `orion-platform`. Os launchers pywebview antigos foram removidos.
- **Decisão:** só desktop, sem versão para celular. O celular fica no Telegram.
- **Destino:** iced (Rust), tema único Grafite escuro, conforme [ORION_UI_ESPECIFICACAO.md](ORION_UI_ESPECIFICACAO.md). As três opções antigas de tema estão superadas; o grafo nativo wgpu ainda está pendente.

A tela Vida (E10) e a legenda ao vivo (`/ui/legenda.html`, E11) fazem parte do front de desktop.

## 7. Destino em Rust

O alvo está atingido na estrutura: núcleo, API e UI em Rust, um executável, sem servidor HTTP no uso normal e sem Python no produto. Falta prova no Windows real e as fases F08–F27 do plano.

| Crate existente | Papel real no PR | Limite atual |
|---|---|---|
| `orion-policy` | Classes, aprovações, audit, shell, caminhos e PII | 25 testes portados; `engine` e `injecao_invariantes` ainda sem teste Rust |
| `orion-store` | SQLite v23, FTS5, vetores e backup | Sem SQLCipher/sqlite-vec; 60 testes de integração |
| `orion-ai` | Gateway, APIs, CLIs diretas, router, embeddings remotos, cotas | Sem ACP/e5/BGE local; 84 testes de integração |
| `orion-core` | Agente, ferramentas, auth, jobs, projetos, canais, visão, extensões e rotinas | Portes agrupados; 167 testes de integração, muitos contratos ainda sem paridade |
| `orion-server` | CLI/API Axum, montagem dos serviços e `appmode` (núcleo no processo do app) | HTTP só com `--features http`; rotas sem testes próprios (há `appmode::tests`) |
| `orion-mcp` | JSON-RPC stdio/HTTP próprio, cliente/servidor | Sem rmcp; 23 declarações de testes de integração |
| `orion-platform` | Bandeja e hotkeys Windows | Sem trait de plataforma completo, nem backend macOS; outras chamadas de SO ainda no core/setup |
| `orion-voice` | cpal, energia, palmas, oww-rs, Whisper, AEC3/playback | Sem Silero; wake word final/hardware pendentes |
| `orion-local` | Catálogo, downloads, protocolo, bundle wake e Job Object | Modelos de visão e gerenciador completo ainda pendentes |
| `orion-llama` | Worker llama-cpp-2 CPU/Vulkan separado | Vulkan ainda sem build final validado; CPU real comprovada no CI |

| `orion-vault` | Detecção/parser/scanner/busca em cache/Git e API explícita | Sem gravação automática, conflito seguro ou índice incremental local completo |
| `orion-app` | Interface iced (47 cenas, `orion-snap`), bandeja/mini barra, cliente `Api` (HTTP ou local) | Grafo 2D (3D wgpu pendente); sem Fluent; nunca rodou no Windows real |
| `orion-updater` | Feed minisign + SHA-256, `baixar_release`, `.bak`, rollback (`orion update`) | Falta chave, feed publicado e rollback automático |
| `orion-testkit` | HTTP/embedder/relógio de teste | Somente dev-dependency |

**Ainda não existem como crates:** tema/i18n compartilhados (Fluent), grafo wgpu, evals, sync e WASM. Telegram usa cliente HTTP próprio no core, sem teloxide, e o MCP também é próprio (sem rmcp), por decisão. Fases e bloqueios completos estão em [ORION.md](ORION.md), seção Auditoria consolidada do PR #30.

**Implementado em `Orion_Rust/` (PR #30, atualizado em 20/10/2026):** `orion.exe` abre o app iced com o núcleo no processo (`orion-server/src/appmode.rs`, `Router::oneshot`); `--background` fica escondido na bandeja (autostart opt-in) e a instância é única (`orion.lock` + `orion.mostrar`). `orion-platform` mantém bandeja e atalhos no laço de eventos da UI Windows. Fechar a janela esconde; Sair encerra núcleo e escuta. O atualizador é o subcomando `orion update`.

`orion-voice` fornece captura cpal, reamostragem para PCM mono 16 kHz, VAD por energia, teto por hora, openWakeWord (`oww-rs`, modelo ONNX treinado para Orion) e Whisper (`whisper-rs`/whisper.cpp). O adaptador `orion-core::native_audio` liga os backends ao listener e ao mesmo transcritor usado pelo canal Telegram; áudio comprimido é convertido localmente pelo ffmpeg em arquivos temporários removidos ao concluir. Pausa/pânico fecham a captura. Durante transcrição o microfone é fechado; durante TTS, com barge-in opt-in, cpal reproduz a fala e reabre o microfone para o AEC3 cancelar o eco usando a própria saída como referência. A fala residual sustentada interrompe o TTS e volta como turno comum; nunca aprova. Os primeiros ~2 s calibram o ruído; palmas só abrem a interface. CPU é o padrão; Vulkan exige `cargo build -p orion-server --features local-vulkan` e SDK no build. Modelos são caminhos configurados ou instalados após seleção explícita no orion-setup, nunca baixados na inicialização do servidor. O runtime exige bundle Orion validado (`orion.onnx`/`orion.json`, hash, limiar e métricas reais); o candidato sintético fica só para calibração explícita por `orion wake-test`.

`orion-local` mantém catálogo fixado e downloads com retomada, SHA-256 e cancelamento. `orion-setup` executa downloads fora da thread da UI, instala os executáveis do subdiretório cpu/ ou vulkan/ escolhido (CPU não depende do loader Vulkan) e grava `orion-local.json` no diretório de dados. O runtime descobre esse arquivo pelo manifesto ao lado do executável; ambiente e .env têm precedência. Voz e modelos não vêm selecionados. Um LLM e um STT ficam ativos por instalação. O instalador recebe opcionalmente a pasta do bundle Orion validado; sem ela, não liga escuta da palavra.

`orion-llama` embarca llama-cpp-2, com Vulkan opcional. É worker nativo separado para não ligar duas versões de GGML no mesmo executável e para impor o teto real pelo Job Object do Windows. O servidor serializa inferências, confere tamanho dos pesos + margem de contexto e RAM disponível, inicia o worker sem segredos, aplica limite de 30% da RAM e só então envia mensagens por stdin. Saída JSON por stdout, sem HTTP; 2.048 tokens de contexto, até 512 de resposta, timeout de 180 s e encerramento ao concluir/cancelar. mmap fica desligado para que os pesos entrem no limite de memória comprometida. Não há cache residente do LLM. O gateway injeta o aviso de reserva e recusa ferramentas; jobs locais e finanças usam o mesmo backend sem nuvem. O Whisper carrega e descarrega por transcrição. No pacote Windows, compilar ambos com `--features orion-server/local-vulkan,orion-llama/vulkan`; GPU requer driver Vulkan compatível e seleção explícita no instalador.

`orion-ai::budget` persiste reservas de chamadas no SQLite antes do envio, inclusive tentativas falhas. O gateway e as CLIs oficiais consultam os limites configurados (`ORION_WEEKLY_BUDGETS` ou painel): 95% por semana, 20% por tarefa, janela de 5 h opcional. Todos os modelos de um mesmo provedor compartilham a cota. A UI identifica a origem como estimativa local em chamadas; `GET/PUT /budget` e `POST /budget/calibrate` exigem login. Cotas não configuradas aparecem como pendentes de configuração, sem inventar saldo oficial.

O código do 2FA é guardado só em hash, em memória, vinculado ao desafio e cliente, expira em 5 min e é descartado após cinco erros ou um acerto. `POST /auth/login` com `ORION_TELEGRAM_2FA=true` só envia o desafio ao primeiro ID permitido; `POST /auth/2fa` cria a sessão após validar o código. Falha de entrega não cria sessão. A janela nativa consome esse fluxo. O bot continua default-deny e só decide pelo botão do cartão que o próprio Orion enviou; o retorno usa o canal original do pedido. Áudio de resposta usa TTS e `sendAudio`, com registro de saída sem conteúdo, apenas se `ORION_TELEGRAM_VOICE_REPLY=true`.

**Vault (F06 parcial):** as rotas autenticadas `/vault/status`, `/vault/pull`, `/vault/search` e `/vault/save` criam `VaultManager`. A gravação recebe o resumo do cliente; não é disparada automaticamente no turno. Parser identifica tags/arquivo/privado, scanner bloqueia padrões de credenciais, Git roda como subprocesso. Ainda faltam privacidade aplicada em todo o fluxo de envio, validação estrita de links/frontmatter, preservação das versões em conflito, isolamento do índice Git e pull nos eventos previstos. O índice existente da memória usa o store; o cache de `orion-vault` não é o e5/sqlite-vec incremental planejado.

**Visão/E11 parcial:** captura pontual via nokhwa e fallback ffmpeg; extrator facial local por contrastes Haar heurísticos + LBPH, sem validação biométrica real. YOLO tem pós-processamento/avistamentos, mas `Detector::detectar_jpeg` sempre retorna vazio; o reranker também não tem runtime ONNX vinculado. A legenda depende de ffmpeg/whisper-cli externos, não é ainda o overlay/loopback/tradução nativo completo.

**Distribuição/validação:** o instalador (`orion-setup`) baixa `latest-<variante>.json` (+ `.minisig`) e `orion.exe`/`orion-llama.exe` do GitHub Releases, conferindo minisign e SHA-256 (`orion_updater::baixar_release`); sem rede, usa as pastas `cpu`/`vulkan` ao lado do setup. Release por tag `v*` em `.github/workflows/release.yml`. Falta o par de chaves e o primeiro release (você). CPU e GGUF passaram no CI; o build Vulkan passou no run `39097948560`, sem prova de GPU. A UI do céu produz RGBA na CPU (`sky.rs`), sem renderer shader compartilhado. Nada disso foi provado num Windows real nem contra as metas de desempenho.

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

Esta tabela é uma pesquisa de candidatos, não uma lista de modelos instalados. O catálogo distribuído está em `orion-local/src/catalog.json`; o candidato Orion sintético e um smoke GGUF CPU já existem. WER e velocidade dos demais modelos continuam sendo números dos autores, sem validação no notebook.

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
