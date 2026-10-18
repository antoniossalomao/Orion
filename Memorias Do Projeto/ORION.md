# Orion — estado, plano e decisões

> Fonte única do estado do projeto. Atualizado em 17/10/2026. Princípios e regras: [REGRAS.md](REGRAS.md). Arquitetura, módulos e referências: [ARQUITETURA.md](ARQUITETURA.md). Como rodar: [README](../README.md). Material antigo e detalhado: [ORION_LEGADO.md](ORION_LEGADO.md).

## Visão

Assistente pessoal com identidade masculina, técnico, direto, não-servil. Não é um chatbot: lembra do que importa, age no computador e responde de qualquer lugar.

*"Não é uma IA. É uma extensão do sistema nervoso."*

Antônio é o administrador: decide, aprova e pode desligar qualquer coisa. Nada que o modelo diga anula isso.

## Estado (17/10/2026)

- **Referência Python:** `orion/` e seus testes permanecem até completar a paridade Rust. O backend pré-reescrita (`Orion_Ollama/`), os serviços em `bin/startup/`, a voz antiga e os launchers pywebview antigos foram removidos do repositório. `Orion_Core/Front_end_Orion/` preserva somente os assets do front compartilhado, ainda servido pelo Rust. Isso não altera instalações ou dados no PC.
- **Rust (`Orion_Rust/`):** workspace Cargo em andamento na branch da PR #30 com 14 crates (`orion-policy`, `orion-store`, `orion-ai`, `orion-mcp`, `orion-core`, `orion-server`, `orion-setup`, `orion-sidecar`, `orion-platform`, `orion-voice`, `orion-vault`, `orion-local`, `orion-llama`, `orion-testkit` — este último só para testes) e o pacote Python isolado do sidecar em `Orion_Rust/python/orion_sidecar`. Instalador nativo (`Orion Setup.exe` em `iced` + céu estrelado + áudio ambiente) e Fase 7 (`orion-sidecar`) concluídos.
- **M2/M3/M4 no PR #30 (17/10/2026):** controle Windows com `orion desktop` (iced), `orion-platform` (tray-icon e hotkeys), mini barra `Ctrl+Alt+Espaço`, pânico `Ctrl+Alt+P`, rotinas concedidas e início automático opt-in. Painel/API de orçamento com estimativa persistida em chamadas, reset semanal, calibragem, projeção, tetos de 95% semanal / 20% por tarefa e janela opcional de 5 h. Telegram recebe cartões do PC com vínculo ao cartão/sessão original, código 2FA de uso único (5 min, 5 tentativas) antes de criar sessão e resposta em áudio só para pedidos por voz. `orion-voice` captura com cpal, reamostra para mono 16 kHz, usa VAD por energia, palmas calibradas (150–700 ms), openWakeWord local com modelo Orion configurado e Whisper local (CPU ou build Vulkan); Groq só com opt-in. TTS existente: edge-tts masculino pt-BR.
- **Validação anterior (M2/M3/M4):** Python com 1.637 testes passando e 3 ignorados; Ruff, formatação, Pyright (ambiente virtual explícito) e 16 testes de front verdes. Checagem estática cruzada para Windows, CPU e Vulkan, inclui servidor/controle e testes de `orion-core`, `orion-ai`, `orion-voice` e `orion-platform`; a execução nativa fica no workflow Windows.
- **Áudio e modelos embarcados (17/10/2026):** `orion-local` fornece catálogo de oito artefatos com revisão/tamanho/SHA-256 fixados, download cancelável com retomada e validação de bundles de wake word. `orion-llama.exe` embarca llama-cpp-2 e só carrega depois de entrar num Job Object com limite real de 30% da RAM; uma inferência por vez, contexto de 2.048 tokens, encerramento por conclusão, cancelamento ou 180 s. O processo também separa o GGML do llama.cpp do GGML do Whisper. Gateway, resumos e classificação financeira local compartilham essa reserva sem ferramentas. O Ollama legado continua opcional quando não se configura GGUF embarcado.
- **Instalação do modo local:** pelo `orion-setup`, no pacote que inclui `Orion Setup.exe` e os runtimes `cpu/` e `vulkan/` (`orion.exe` + `orion-llama.exe`). A escolha CPU não exige driver Vulkan. Modelos, voz, autostart e Vulkan começam desmarcados. A seleção permite um LLM e um STT por vez e recusa LLM que ultrapasse a estimativa de 30%; o limite real é aplicado na execução. A UI baixa e verifica antes de concluir, grava `orion-local.json` sem chaves, preserva arquivos existentes no rollback e abre o Orion instalado pelo botão. `--preview` mantém prévia sem download. As etapas antigas de certificado, atalhos e serviço continuam sendo protótipos em JSON; esta mudança não implementa assinatura/registro de certificado ou serviço do Windows.
- **AEC3 e interrupção:** playback nativo cpal entrega a referência do TTS ao AEC3 em quadros de 10 ms; fala residual sustentada por 192 ms interrompe a reprodução e inicia outro turno comum, sem aprovar nada. Só com escuta ativada e `ORION_VOICE_BARGE_IN=true`; pânico/pausa descartam a gravação. `ORION_VOICE_AEC_DELAY_MS` calibra o atraso (40 ms padrão). Whisper carrega por transcrição e libera os pesos antes do LLM. O workflow compila o pacote com Whisper e llama.cpp Vulkan e testa inferência CPU com GGUF real; prova física de eco, barge-in e aceleração na GPU permanece necessária.
- **Validação áudio/F03/F15:** 1.639 testes Python passaram, 3 ignorados; 149 testes do front, Ruff, formatação e Pyright passaram. Checagem de tipos cruzada Windows/Vulkan inclui todos os crates de domínio, voz, instalador e worker; bibliotecas C++ são compiladas de verdade no CI Windows. O ONNX foi retreinado a partir do corpus compactado e reproduziu o mesmo SHA-256. Execução nativa do worker com GGUF real e build do pacote Vulkan estão no workflow Windows; não substituem a prova no notebook.
- **Modelo Orion treinado, ainda candidato:** `Orion_Rust/models/orion-wake-candidate/` inclui ONNX de 210 KB, corpus sintético reproduzível e métricas. Treinado com duas vozes pt-BR e testado com voz pt-PT separada: 3/3 positivos, mas 1 falso em 34,36 s negativos (~104,77/h nesta amostra curta). **Não é modelo final e não deve ligar escuta contínua.** Instalador/runtime exigem status validated, hash correto, recall >=95%, <=0,5 falso/h, >=50 clipes positivos e >=1 h negativos reais separados por locutor. `scripts/treinar_orion_wake.py` sintetiza, treina e avalia; `orion wake-test` permite testar explicitamente o candidato no microfone, sem agente/rede/gravação. Antônio informou que não tem gravações; a validação real segue pendente.
- **Limites desta entrega:** controle nativo é uma janela auxiliar, não substitui a interface principal `orion-app` pendente. Não conclui todas as funcionalidades F11–F15. Ainda precisam de prova no notebook: bandeja/hotkeys/autostart, calibração com palmas e música reais, modelo Orion final validado (o candidato sintético está no repositório), Whisper/GPU e Telegram em dois aparelhos. Silero e validação acústica do AEC3, Windows Hello de reserva, PIN crítico próprio, leitura oficial de `/usage` e `/status`, fila de nuvem e controle de energia/Wake-on-LAN continuam pendentes. Nenhum recurso sensível é ligado pela instalação ou pelo agente.

- **Python, por etapa da rodada 2:**

| Etapa | Entrega | Estado |
|---|---|---|
| E0 | Opt-ins documentados e tabela-resumo | Feita; PR #18 mergeado |
| E1 | Custo zero, registro de saídas, pânico e não perturbe, modelo local | Feita; PR #18 mergeado; provada com gateway falso |
| E2 | Ponte de desktop, duas palmas, captura rápida, "O que é isso?" | Código feito; falta testar teclas globais e microfone no notebook e calibrar as palmas |
| E3 | Conversas arquivadas, filtro por projeto, mover para projeto | Código feito; PR ainda não mergeado |
| E4 | Isolamento por projeto, OCR de PDF, auto-compact, validade de fatos, reranker | Feita (PR #24); falta instalar o Ollama e `qwen3.5:4b` |
| E5 | Busca da tela, exclusão por programa, tela bloqueada, sono na interface, grafo 3D, busca global | Feita (PR #24) |
| E6 | Biblioteca, comparar versões, pesquisa noturna pela interface | Código feito; falta CI nos três sistemas |
| E7 | Cliente e servidor MCP, skills | Código feito; falta registrar o Orion no Claude Code |
| E8 | Plugins: instalar, atualizar, importar, catálogo | Código feito |
| E9 | Rotinas, tarefa em segundo plano, lembrete insistente, Downloads, conversor, voz no Telegram | Código feito; falta a prova com palmas e música reais |
| E10 | Vida pessoal: revisão, diário, gastos, contas, preço, cápsula, cenários, linha do tempo, sonhos, escudo de foco e estado pela voz | Código feito (main); falta navegador real e a prova de uma semana |
| E11 | Visão e câmera: ver e agir, visão pelas assinaturas, detecção local, "onde eu deixei", rosto, presença, tradutor, legenda ao vivo e Flow | Código feito (main); falta equipamento real (webcam, microfone e modelos) |
| E12–E13 | Trabalho e carreira, segurança e continuidade | Não começaram |

- **Fases gerais (plano de 2026-10, fases 0 a 7):**
  - 0, exportar os dados da base antiga: pendente, é sua (PC).
  - 1, fundação: feita, com CI nos três sistemas.
  - 2, cérebro: feita com gateway e CLIs falsos; faltam as chaves reais.
  - 3, memória: feita no código; faltam a chave de embeddings e as suas perguntas reais.
  - 4, ferramentas: feita no código; faltam os servidores MCP que você escolhe e o que depende de modelo multimodal.
  - 5, canais: feita no código (Telegram só com API falsa); faltam o Tailscale e o autostart no notebook.
  - 6, interface e voz: front só desktop; voz por clique, voz ao vivo e palavra de ativação no código; falta testar a palavra com seu microfone.
  - 7, limpeza: backend pré-reescrita, launchers antigos, dependências Qdrant/SurrealDB e empacotamento PyInstaller removidos. Python de referência, testes de paridade, sidecar e utilitários ativos preservados.

- **Não verificado** (detalhes em [ORION_LEGADO.md](ORION_LEGADO.md), documento ORION_MELHORIAS):
  - Telegram, voz e foto no Telegram: só contra API falsa.
  - Embeddings do Gemini, consolidação e pesquisa com Google Search: só com modelo ou API falsos.
  - Formato do export real do SurrealDB: nunca visto.
  - Autostart: gerado, não ativado no sistema.
  - `orion-desktop` no Windows e no macOS: só o argv testado.
  - MCP: testado com o servidor do SDK; os servidores reais não foram escolhidos.
  - E10 e E11: navegador da tela Vida, webcam, microfone, loopback, modelos ONNX e Vosk, Bluetooth, tesseract, CLIs de assinatura e a URL do Flow.

## Pendências suas (só você faz)

1. Fase 0: rodar `orion verify-export` no PC até dar `PRONTO` (plano de corte no legado).
2. No notebook: `uv sync` e `uv run pytest`.
3. Chaves dos provedores (`orion chave <id>`) e teste de `POST /chat`.
4. Chave de embeddings (`ORION_EMBED_API_KEY`) e de 15 a 30 perguntas reais em `tests/eval_pessoal.local.json`.
5. Telegram: criar o bot, preencher token e IDs, parar o bot do legado e testar. Instalar o Tailscale e rodar `orion autostart`.
6. MCP: escolher os servidores (e-mail, agenda, navegador), copiar `mcp.example.json` para `mcp.json` e rodar `orion mcp-check`. Registrar o Orion no Claude Code com `claude mcp add orion -- orion mcp-servidor`.
7. E2: testar teclas globais e microfone; calibrar as palmas.
8. E4: `ollama pull qwen3.5:4b`.
9. E9: a prova com palmas e música reais.
10. Decidir a licença-alvo: uso pessoal ou distribuição. Isso muda a escolha de modelos (ver [ARQUITETURA.md](ARQUITETURA.md), seção 8.3).
11. Decidir a lista de telas do primeiro marco em Rust.
12. Plano de corte: data da venda do PC e o que roda em paralelo.
13. E10: a prova de uma semana real (revisão, diário, gastos e contas).
14. E11: provas com equipamento: webcam, microfone, modelo de objetos (`ORION_OBJECT_MODEL`), rosto (`ORION_FACE_MODEL`, `ORION_FACE_KEY`, `orion rosto cadastrar`), legenda (`ORION_SUBTITLE_MODEL`) e Bluetooth (`ORION_PRESENCE_BT`).
15. E11.8: conferir a URL do Flow (`https://labs.google/fx/tools/flow`) no site real.

## Plano de migração para Rust (decidido em 14/10/2026)

- **Alvo:** Windows primeiro, depois macOS. Linux e mobile ficam fora de escopo: não compilar, não testar, sem suporte.
- **Rust no núcleo e na UI.** Python só onde não há equivalente (browser-use e crawl4ai), em processo separado e sem servidor HTTP.
- **UI em iced** (Rust puro, render com wgpu). Plano B: Slint. egui fica descartado para a UI final.
- **Grafo 3D em wgpu.**
- **Palmas e palavra de ativação entram no primeiro marco.**
- **Abstração de plataforma desde a fase 0.** Só a implementação de Windows entra no primeiro marco.
- **Paridade antes de melhoria.** Os testes Python são a especificação. O banco SQLite é o mesmo nos dois lados durante a transição.

| Fase | Entrega | Saída |
|---|---|---|
| 0, preparação (1 a 2 semanas) | `verify-export`, linha de base do Python, contrato do banco | Linha de base e backup verificados |
| 1, spikes de risco (2 a 3 semanas) | iced com chat, paleta e grafo; rusqlite com FTS5; BGE-M3 comparado ao Python; tray, teclas e notificação no Windows; cpal e VAD | Cinco números medidos e a decisão de toolkit registrada |
| 2, núcleo de domínio (4 a 6 semanas) | `orion-policy`, `orion-store`, `orion-ai` e o agente | `/chat` sem UI, com paridade nos testes de política, memória e router |
| 3, interface (4 a 6 semanas) | Chat, Memória, Atividade, Configurações, paleta, aprovações, grafo em wgpu | Primeiro marco usável no Windows |
| 4, plataforma e rotinas (3 a 4 semanas) | Início automático, bandeja, teclas, cofre, rotinas, backup, captura e OCR nativos | Rotinas rodando com o app na bandeja |
| 5, voz (3 a 5 semanas) | Captura, VAD, STT com whisper-rs, palavra de ativação e palmas, TTS | Comando por voz nos dois sistemas |
| 6, integrações (3 a 4 semanas) | MCP (rmcp), plugins, Telegram (teloxide), Google | Integrações com as mesmas credenciais |
| 7, residual Python (1 a 2 semanas) | Sidecar com browser-use e crawl4ai, JSON por stdin e stdout | Feita (`orion-sidecar` + `Orion_Rust/python/orion_sidecar`) |
| 8, corte (2 a 3 semanas) | Migração de dados verificada, instalador assinado no Windows, comparação de desempenho | Instalador nativo `orion-setup` pronto; falta corte final |
| 9, macOS (a definir) | Implementações de plataforma, permissões, `.dmg` assinado e notarizado | Começa depois do corte do Windows |

**Métricas:** tempo até a janela pronta, memória ociosa, latência da primeira resposta, tamanho do pacote e paridade dos testes (100% no primeiro marco).

**Riscos principais:** o toolkit não atende o visual; a API do iced muda entre versões; embeddings em Rust com qualidade menor; permissões do macOS; escopo que cresce; perda de dados (crítico, mitigado por backup verificado e esquema compartilhado); browser-use exige Python.

### Paridade de testes e linha de base (17/10/2026)

- **Testes portados do Python** (`cargo test --workspace --exclude orion-setup`): `orion-store` 60, `orion-ai` 84, `orion-mcp` 23, `orion-core` 167 (memória, importação, MCP, login, jobs, modos, biblioteca, captura, autostart). O sidecar tem 4. **`orion-policy` ainda não tem nenhum teste** (a lista de 14 não existe nesta branch): portar `tests/policy/` exige tocar no crate do núcleo imutável, então espera sua aprovação.
- **Ainda sem testes de paridade portados no `orion-core`:** agente, Telegram, ponte, voz, palavra de ativação, palmas, visão/câmera, vida (E10), rotinas, downloads, briefing, plugins/extensões, projetos (isolamento), pesquisa, tela. As rotas HTTP (`orion-server`) também.
- **Divergências do Rust achadas e corrigidas pelos testes:** importador tratava registro sem `id` como `"null"` e aceitava `score` não numérico; captura criava a pasta fora do vault antes de recusar `..`; `JobRunner` ignorava o relógio do banco; cancelamentos de MCP nunca eram contados.
- **Benchmark** (`scripts/benchmark_rust_python.py`, banco sintético pequeno, 4 CPUs Linux; **não** é o seu PC): busca FTS5 p50 2,5 ms (Python) × 1,9 ms (Rust); híbrida 3,1 × 2,9 ms; memória com a matriz vetorial 60 × 18 MB; servidor ocioso 128 × 34 MB, `/health` em 2,7 × 0,2 s, `/busca` HTTP p50 26 × 17 ms; resultados dos dois lados idênticos. Falta rodar no Windows com o banco real e em escala maior (o gerador sintético é lento acima de ~5 mil registros).

**Integração do PR #33 (17/10/2026):** base do PR #30 atualizada até `8f4c783`, com dependências de teste e voz/Vulkan preservadas no manifesto e lockfile. Os testes de autostart passam a esperar `desktop --background` no Windows. Ruff dos scripts novos corrigido; benchmark usa token efêmero e só faz HTTP em loopback. Validação da integração: 1.637 testes Python passaram, 3 ignorados; 149 testes do front, Ruff, formatação, Pyright e checagem estática Windows/Vulkan verdes. O cliente HTTP do benchmark foi conferido com servidor local autenticado e recusa de destinos externos. A execução nativa dos novos testes de paridade depende do CI desta branch.

## Backlog Rust

Os itens em aberto das etapas antigas foram reescritos aqui, com IDs novos.

| ID | Item | Dono |
|---|---|---|
| B-01 | Verificar o export real do SurrealDB (fase 0) | Você (PC) |
| B-02 | Medir a linha de base do Python: janela pronta, memória ociosa, latência e tamanho do pacote — **script pronto (`scripts/benchmark_rust_python.py`); falta rodar no PC com o banco real e o pacote** | Você e Claude |
| B-03 | Spike iced: chat, paleta `Ctrl K` e grafo em wgpu; medir memória e latência | Claude |
| B-04 | Spike de palmas e palavra de ativação no Windows, com calibração em microfone real (inclui a prova da E2 e da E9) | Você e Claude |
| B-05 | Spike de busca: rusqlite com FTS5 lendo o banco real; BGE-M3 via `ort` ou `candle` comparado ao Python — **FTS5 + vetores medidos (`orion-store/examples/bench_busca.rs`, resultados idênticos ao Python); falta BGE-M3** | Claude |
| B-06 | Spike de áudio: captura com cpal e VAD mínimo; testar um STT candidato (ver [ARQUITETURA.md](ARQUITETURA.md), 8.3) | Claude |
| B-07 | Portar `policy/` com os mesmos casos de teste (fase 2) — **Feito (`orion-policy`)** | Claude |
| B-08 | Definir as telas do primeiro marco | Você |
| B-09 | Fixar a versão do iced no `Cargo.lock` — **Feito (`iced = "=0.14.0"`)** | Claude |
| B-10 | Decidir a licença-alvo (ver Pendências, item 10) | Você |
| B-11 | Sidecar Python com browser-use e crawl4ai (fase 7) — **Feito (`orion-sidecar` + `Orion_Rust/python/orion_sidecar`)** | Claude |

**Fora do marco 1:** E10 a E13. E10 e E11 já têm código em Python (ver Estado), sem Rust; E12 e E13 não começaram. Cada um começa pelas regras em [REGRAS.md](REGRAS.md).

### Limpeza do legado (17/10/2026)

Removidos 79 arquivos obsoletos: `Orion_Ollama/`, `bin/startup/`, motores de voz antigos em `Orion_Core/`, launchers Python antigos do front, `requirements.txt` do legado e cadeia PyInstaller (workflow, spec, entrada, smoke e instruções do pacote). Saíram também os testes exclusivos desses componentes e três verificações de compatibilidade com os arquivos removidos. O front HTML/CSS/JS e seus assets continuam no mesmo caminho usado por Rust/Python e pelos testes Node. O grupo `build` e as dependências exclusivas do PyInstaller saem de `pyproject.toml`/`uv.lock`.

`orion/` continua como especificação executável: vários contratos ainda não têm testes Rust, e o benchmark compara as duas implementações. Python também permanece para o sidecar, edge-tts, treino/avaliação de wake word e fixtures MCP. As proteções de caminhos antigos nos testes de política continuam válidas para dados de instalações anteriores. `ORION_LEGADO.md` e `REGRAS.md` preservam referências históricas; os fontes removidos podem ser consultados no histórico Git.

Validação da limpeza: `uv sync --python 3.12 --locked`, Ruff, formatação e Pyright (com o venv explicitado) passaram; 1.564 testes Python passaram, 3 ignorados; os 16 arquivos de testes Node passaram. O workflow Windows também passa a observar o front, o smoke de modelos e a fixture MCP compartilhados. A compilação nativa continua no CI Windows.

## Decisões

Registro das decisões que valem para o projeto hoje. O histórico completo, desde 25/06/2026, está em [ORION_LEGADO.md](ORION_LEGADO.md) (plano antigo, seção 8).

| Data | Decisão | Motivo |
|---|---|---|
| 17/10/2026 | Câmera nativa com nokhwa (Media Foundation) e reconhecimento facial clássico local Haar+LBPH (F18; C21) sem dependência externa de ffmpeg nem ONNX | Captura 100% nativa em Rust no Windows e biometria local determinística, leve e sem IA pesada |
| 17/10/2026 | Remover backend pré-reescrita e distribuição PyInstaller; preservar front compartilhado e Python necessário para paridade, sidecar e utilitários | Pedido explícito do Antônio para limpar o repositório voltado ao Rust; paridade ainda incompleta |
| 17/10/2026 | Instalar modelos pelo orion-setup; embarcar llama.cpp em worker nativo descartável, limitado a 30% da RAM pelo Windows; separar seus símbolos GGML do Whisper | Pedido explícito do Antônio (F03.14/F15); dispensa Ollama e limita memória de verdade |
| 17/10/2026 | Tratar o wake word sintético como candidato até passar por avaliação real separada; bundle leva hash, limiar e métricas | Não há gravações do usuário e o candidato ainda dispara falsos positivos |
| 17/10/2026 | Implementar o teto por tarefa de 20% da cota semanal no controle Rust, conforme pedido explícito do Antônio; medir em chamadas estimadas e informar essa origem | Não existe leitura oficial confiável das cotas nesta entrega |
| 14/10/2026 | Migrar para Rust: Windows primeiro, depois macOS; Linux e mobile fora de escopo | Desempenho e leveza |
| 14/10/2026 | UI em iced (plano B: Slint); egui descartado para a UI final | Rust puro, wgpu, licença MIT |
| 14/10/2026 | Grafo 3D em wgpu | Desenho nativo |
| 14/10/2026 | Palmas e palavra de ativação entram no primeiro marco | Pedido seu |
| 14/10/2026 | Python só para browser-use e crawl4ai, em sidecar sem HTTP | Não há equivalente em Rust |
| 14/10/2026 | Regras por princípio, sem número; o código cita o nome do princípio | Pedido seu |
| 14/10/2026 | Documentação em quatro arquivos, mais o legado e o AGENTS.md; o repositório é a origem e o vault espelha | Pedido seu |
| 14/10/2026 | Etapas E0 a E13 viram backlog Rust | Pedido seu |
| 14/10/2026 | E10 e E11 entram no Python com as regras de vida pessoal e visão escritas antes do código; câmera e presença só com opt-in | Plano da rodada 2 |
| 09/10/2026 | Modelos direto nos provedores, sem OmniRoute | Um processo a menos; fallback no próprio `gateway.py` |
| 17/10/2026 | Paridade em testes de integração por crate (`crates/*/tests/`), com `orion-testkit` (servidor HTTP e relógio de mentira); o Python continua sendo a especificação | Teste cobre o contrato, não o código interno |
| 07/10/2026 | Front só desktop; o celular fica no Telegram | Pedido seu |
| 07/10/2026 | Palavra de ativação "Orion", com escuta local | Pedido seu |
| 07/10/2026 | "Portabilidade" no lugar de "Hardware-Bound" | Conflitava com trocar de máquina |
| 06/10/2026 | Login com senha e sessão opaca revogável, no lugar de JWT | JWT não se revoga; o backup não deve levar o hash |
| 03/10/2026 | Agendamento só avisa; não executa a ferramenta | Execução sem ninguém olhando exige política e aprovação |
| 03/10/2026 | Aprovação só do que se vê inteiro | O cartão cortava o argumento e escondia o fim do comando |
| 02/10/2026 | Loop de agente próprio, no lugar do PydanticAI | O fluxo de aprovação precisa controlar cada ferramenta |
| 02/10/2026 | Vetores em tabela comum com numpy, no lugar do sqlite-vec | O Python do macOS vem sem extensões |
| 01/10/2026 | Custo zero; assinaturas só pelos programas oficiais | Token extraído leva a banimento |
| 30/09/2026 | Nome Orion, reescrita em repositório próprio | Identidade própria |

## Referências

Pesquisas de Rust e de modelos do Hugging Face, com links e licenças: [ARQUITETURA.md](ARQUITETURA.md), seção 8.
