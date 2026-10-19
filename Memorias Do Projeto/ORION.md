# Orion — estado, plano e decisões

> Fonte única do estado do projeto. Atualizado em 19/10/2026. Princípios e regras: [REGRAS.md](REGRAS.md). Arquitetura, módulos e referências: [ARQUITETURA.md](ARQUITETURA.md). Como rodar: [README](../README.md). Material antigo e detalhado: [ORION_LEGADO.md](ORION_LEGADO.md).

## Visão

Assistente pessoal com identidade masculina, técnico, direto, não-servil. Não é um chatbot: lembra do que importa, age no computador e responde de qualquer lugar.

*"Não é uma IA. É uma extensão do sistema nervoso."*

Antônio é o administrador: decide, aprova e pode desligar qualquer coisa. Nada que o modelo diga anula isso.

## Estado (17/10/2026)

- **Referência Python:** `orion/` e seus testes permanecem até completar a paridade Rust. O backend pré-reescrita (`Orion_Ollama/`), os serviços em `bin/startup/`, a voz antiga e os launchers pywebview antigos foram removidos do repositório. `Orion_Core/Front_end_Orion/` preserva somente os assets do front compartilhado, ainda servido pelo Rust. Isso não altera instalações ou dados no PC.
- **Rust (`Orion_Rust/`):** workspace Cargo em andamento na branch da PR #30 com 14 crates (`orion-policy`, `orion-store`, `orion-ai`, `orion-mcp`, `orion-core`, `orion-server`, `orion-setup`, `orion-sidecar`, `orion-platform`, `orion-voice`, `orion-vault`, `orion-local`, `orion-llama`, `orion-testkit` — este último só para testes) e o pacote Python isolado do sidecar em `Orion_Rust/python/orion_sidecar`. Instalador nativo parcial (`Orion Setup.exe` em `iced` + céu e áudio); ponte `orion-sidecar` implementada e testada isoladamente, ainda sem chamada pelo agente. A conclusão de uma fase exige o aceite completo, além da existência do crate.
- **M2/M3/M4 no PR #30 (17/10/2026):** controle Windows com `orion desktop` (iced), `orion-platform` (tray-icon e hotkeys), mini barra `Ctrl+Alt+Espaço`, pânico `Ctrl+Alt+P`, rotinas concedidas e início automático opt-in. Painel/API de orçamento com estimativa persistida em chamadas, reset semanal, calibragem, projeção, tetos de 95% semanal / 20% por tarefa e janela opcional de 5 h. Telegram recebe cartões do PC com vínculo ao cartão/sessão original, código 2FA de uso único (5 min, 5 tentativas) antes de criar sessão e resposta em áudio só para pedidos por voz. `orion-voice` captura com cpal, reamostra para mono 16 kHz, usa VAD por energia, palmas calibradas (150–700 ms), openWakeWord local com modelo Orion configurado e Whisper local (CPU ou build Vulkan); Groq só com opt-in. TTS existente: edge-tts masculino pt-BR.
- **Validação anterior (M2/M3/M4):** Python com 1.637 testes passando e 3 ignorados; Ruff, formatação, Pyright (ambiente virtual explícito) e 16 testes de front verdes. Checagem estática cruzada para Windows, CPU e Vulkan, inclui servidor/controle e testes de `orion-core`, `orion-ai`, `orion-voice` e `orion-platform`; a execução nativa fica no workflow Windows.
- **Áudio e modelos embarcados (17/10/2026):** `orion-local` fornece catálogo de oito artefatos com revisão/tamanho/SHA-256 fixados, download cancelável com retomada e validação de bundles de wake word. `orion-llama.exe` embarca llama-cpp-2 e só carrega depois de entrar num Job Object com limite real de 30% da RAM; uma inferência por vez, contexto de 2.048 tokens, encerramento por conclusão, cancelamento ou 180 s. O processo também separa o GGML do llama.cpp do GGML do Whisper. Gateway, resumos e classificação financeira local compartilham essa reserva sem ferramentas. O Ollama legado continua opcional quando não se configura GGUF embarcado.
- **Instalação do modo local:** pelo `orion-setup`, no pacote que inclui `Orion Setup.exe` e os runtimes `cpu/` e `vulkan/` (`orion.exe` + `orion-llama.exe`). A escolha CPU não exige driver Vulkan. Modelos, voz, autostart e Vulkan começam desmarcados. A seleção permite um LLM e um STT por vez e recusa LLM que ultrapasse a estimativa de 30%; o limite real é aplicado na execução. A UI baixa e verifica antes de concluir, grava `orion-local.json` sem chaves, preserva arquivos existentes no rollback e abre o Orion instalado pelo botão. `--preview` mantém prévia sem download. As etapas antigas de certificado, atalhos e serviço continuam sendo protótipos em JSON; esta mudança não implementa assinatura/registro de certificado ou serviço do Windows.
- **AEC3 e interrupção:** playback nativo cpal entrega a referência do TTS ao AEC3 em quadros de 10 ms; fala residual sustentada por 192 ms interrompe a reprodução e inicia outro turno comum, sem aprovar nada. Só com escuta ativada e `ORION_VOICE_BARGE_IN=true`; pânico/pausa descartam a gravação. `ORION_VOICE_AEC_DELAY_MS` calibra o atraso (40 ms padrão). Whisper carrega por transcrição e libera os pesos antes do LLM. O workflow compila o pacote com Whisper e llama.cpp Vulkan e testa inferência CPU com GGUF real; prova física de eco, barge-in e aceleração na GPU permanece necessária.
- **Validação áudio/F03/F15:** 1.639 testes Python passaram, 3 ignorados; 149 testes do front, Ruff, formatação e Pyright passaram. Checagem de tipos cruzada Windows/Vulkan inclui todos os crates de domínio, voz, instalador e worker; bibliotecas C++ são compiladas de verdade no CI Windows. O ONNX foi retreinado a partir do corpus compactado e reproduziu o mesmo SHA-256. Execução nativa do worker com GGUF real e build do pacote Vulkan estão no workflow Windows; não substituem a prova no notebook.
- **Modelo Orion treinado, ainda candidato:** `Orion_Rust/models/orion-wake-candidate/` inclui ONNX de 210 KB, corpus sintético reproduzível e métricas. Treinado com duas vozes pt-BR e testado com voz pt-PT separada: 3/3 positivos, mas 1 falso em 34,36 s negativos (~104,77/h nesta amostra curta). **Não é modelo final e não deve ligar escuta contínua.** Instalador/runtime exigem status validated, hash correto, recall >=95%, <=0,5 falso/h, >=50 clipes positivos e >=1 h negativos reais separados por locutor. `scripts/treinar_orion_wake.py` sintetiza, treina e avalia; `orion wake-test` permite testar explicitamente o candidato no microfone, sem agente/rede/gravação. Antônio informou que não tem gravações; a validação real segue pendente.
- **Limites desta entrega:** controle nativo é uma janela auxiliar, não substitui a interface principal `orion-app` pendente. Não conclui todas as funcionalidades F11–F15. Ainda precisam de prova no notebook: bandeja/hotkeys/autostart, calibração com palmas e música reais, modelo Orion final validado (o candidato sintético está no repositório), Whisper/GPU e Telegram em dois aparelhos. Silero e validação acústica do AEC3, Windows Hello de reserva, PIN crítico próprio, leitura oficial de `/usage` e `/status`, fila de nuvem e controle de energia/Wake-on-LAN continuam pendentes. Nenhum recurso sensível é ligado pela instalação ou pelo agente.

- **App principal em iced (19/10/2026, PR #30): `crates/orion-app`.** Reproduz o protótipo `orion-ui-mockup.html` (ORION - UI (ESPECIFICAÇÃO)): entrada, barra lateral, barra superior, Início, chat com aprovação e painel da direita (4 abas), 10 telas de página, Memória (grafo 2D do snapshot do vault, filtros, busca, detalhes, linha do tempo), Configurações (14 seções), editor de nota, paleta Ctrl K, captura rápida, "Isso?", mini barra de voz, legenda, popovers e toasts. Geist, Geist Mono e Newsreader embutidas. `orion-snap` renderiza 47 cenas sem janela; `tools/` tira as mesmas fotos do HTML e junta lado a lado. 23 testes do estado; entra no CI do Windows. **Estado: parcial.** Hover com cor, dicas (tooltips), animações do protótipo (entrada de página, popover, modais, gaveta, seta de "Mais", switch, toast) e bordas tracejadas já entram. Falta ligar ao núcleo (dados são de exemplo), grafo 3D em wgpu, login final com avatar/2FA/Hello, bandeja e atalhos globais, motor animado do céu do instalador, empacotar no instalador, e conferir no Windows real.
- **Continuação do PR #30 (19/10/2026): bloqueios 1, 4 e 7 da auditoria.**
  - **Build Vulkan (concluído):** os commits `1bfdf74`, `f835816` e `d78efeb` (Chocolatey com nova tentativa, `vulkan-shaders-gen` isolado e gerador Ninja) corrigiram o MSB8066. O [run 39097948560](https://github.com/antoniossalomao/Orion/actions/runs/39097948560) em `d78efeb` terminou com sucesso, com build Vulkan, montagem do pacote e upload do artefato `Orion-native-Windows-Vulkan`. Falta só a prova física de GPU.
  - **Login com Argon2id (concluído no código):** senha nova e de fábrica vão em Argon2id (19 MiB, 2 passes, PHC); hash PBKDF2 antigo ainda confere e é regravado em Argon2id no primeiro login correto; a cópia da senha em claro é zeroizada. `derive_db_key` deriva a chave de 256 bits do banco (Argon2id + sal próprio). 20 testes em `orion-core/tests/auth.rs`. Depois do primeiro login o Python de referência deixa de ler o `auth.db`.
  - **Banco cifrado (SQLCipher) — implementado, desligado até você rodar `orion db-cifrar`:**
    - Chave mestra aleatória de 256 bits em `orion.key`, embrulhada duas vezes (ChaCha20-Poly1305): pela senha (Argon2id) e pelo código de recuperação de 32 letras, mostrado uma vez. Trocar a senha só reembrulha a chave, sem recifrar o banco.
    - Comandos: `db-cifrar` (migra com cópia conferida; o original em claro vira `orion.db.plain.bak` até `db-apagar-claro`), `db-senha`, `db-recuperar`, `db-nova-recuperacao` e `logs`.
    - Na subida o `orion` pede a senha (prompt ou `ORION_DB_PASSWORD`). Todos os 17 pontos de abertura do banco no servidor passaram por `dbopen`; backups saem cifrados com a mesma chave.
    - Logs cifrados por dia (`logs/orion-AAAA-MM-DD.log.enc`) com retenção (`ORION_LOG_RETENTION_DAYS`, 30 por padrão); log e audit com máscara de PII (e-mail, CPF/CNPJ com dígito verificador, telefone, cartão com Luhn).
    - Win+L (`ORION_LOCK_ON_SCREEN_LOCK`, ligado): encerra as sessões e apaga a chave da memória; com `ORION_LOCK_EXIT=true` encerra também o processo, o único jeito de o SQLCipher soltar a chave.
    - **Limites:** feature `sqlcipher` fora do build padrão (CI liga nos testes e no pacote Vulkan); conferir e restaurar backup cifrado não é suportado; o Python de referência não lê o banco cifrado; o autostart não tem janela de senha (o `orion` precisa de terminal ou `ORION_DB_PASSWORD`); sem Windows Hello; `auth.db` continua em claro (só tem hashes); Win+L nunca rodou num Windows real.
  - **Instalador (implementado no código, sem execução num Windows real):**
    - `register.rs`: Apps instalados (`reg.exe`, HKCU), atalho `.lnk` no Menu Iniciar, início automático só com a caixa marcada, lista de SHA-256 e rollback que desfaz também o registro.
    - `identity.rs`: pacote esparso MSIX com certificado `CN=Orion` autoassinado, assinado com `makeappx`/`signtool` do SDK do Windows e confiado em `TrustedPeople` (exige administrador). Sem SDK ou sem administrador o passo é pulado com o motivo na tela, nunca fingido.
    - Desinstalador `Orion Setup.exe --uninstall <pasta>` com janela de três caixas (app sempre; modelos e dados desmarcados). Remove só o que o manifesto lista, mantém arquivo alterado, apaga modelos só os registrados e dados só de pasta com nome Orion; o vault nunca é tocado. `--silent --remove-models --remove-data` para uso sem janela.
    - `orion-updater` (crate novo): feed `latest.json` assinado com minisign + SHA-256 por arquivo, só HTTPS, só versão maior, só `orion.exe`/`orion-llama.exe`/`orion-updater.exe`, `.bak` e `--rollback`. Tarefa agendada diária (SYSTEM, sem UAC) criada no passo 5. **Fail-closed:** sem `feed_url` e `public_key` em `update.json` ele não faz nada; falta gerar o par de chaves e publicar releases assinadas.
    - Removidos o certificado de exemplo, o `AppxManifest.xml` falso e o "serviço" em JSON.
  - **Validação local (Linux, 19/10/2026):** `orion-core` (login 20, `dbkey` 6, `dbopen` 4 e `logcrypt` 4 com `--features sqlcipher`), `orion-store` 60 + 3 com `sqlcipher`, `orion-policy` 25 (shell, caminhos, aprovações e PII, portados de `tests/policy`), `orion-setup` 25, `orion-updater` 9; `cargo check` do `orion-server` com e sem `sqlcipher` e do `orion-setup`/`orion-platform` para `x86_64-pc-windows-gnu`. A suíte completa, o OpenSSL embutido no Windows e tudo que toca o sistema ficam com o CI desta branch. **Ainda sem testes Rust:** `engine` e `injecao_invariantes` da política, e as rotas de `orion-server`.

- **Python, por etapa da rodada 2:**

| Etapa | Entrega | Estado |
|---|---|---|
| E0 | Opt-ins documentados e tabela-resumo | Feita; PR #18 mergeado |
| E1 | Custo zero, registro de saídas, pânico e não perturbe, modelo local | Feita; PR #18 mergeado; provada com gateway falso |
| E2 | Ponte de desktop, duas palmas, captura rápida, "O que é isso?" | Código feito; falta testar teclas globais e microfone no notebook e calibrar as palmas |
| E3 | Conversas arquivadas, filtro por projeto, mover para projeto | Código feito; PR ainda não mergeado |
| E4 | Isolamento por projeto, OCR de PDF, auto-compact, validade de fatos, reranker | Referência Python feita (PR #24); reserva Rust pode usar GGUF pelo setup. Reranker Rust ainda sem runtime ONNX |
| E5 | Busca da tela, exclusão por programa, tela bloqueada, sono na interface, grafo 3D, busca global | Feita (PR #24) |
| E6 | Biblioteca, comparar versões, pesquisa noturna pela interface | Código feito; falta CI nos três sistemas |
| E7 | Cliente e servidor MCP, skills | Código feito; falta registrar o Orion no Claude Code |
| E8 | Plugins: instalar, atualizar, importar, catálogo | Código feito |
| E9 | Rotinas, tarefa em segundo plano, lembrete insistente, Downloads, conversor, voz no Telegram | Código feito; falta a prova com palmas e música reais |
| E10 | Vida pessoal: revisão, diário, gastos, contas, preço, cápsula, cenários, linha do tempo, sonhos, escudo de foco e estado pela voz | Código feito (main); falta navegador real e a prova de uma semana |
| E11 | Visão e câmera: ver e agir, visão pelas assinaturas, detecção local, "onde eu deixei", rosto, presença, tradutor, legenda ao vivo e Flow | Código feito (main); falta equipamento real (webcam, microfone e modelos) |
| E12–E13 | Trabalho e carreira, segurança e continuidade | Não começaram |

- **Fases gerais da referência Python (histórico do plano de 2026-10, fases 0 a 7; não são as F00–F27 do novo plano Rust):**
  - 0, exportar os dados da base antiga: pendente, é sua (PC).
  - 1, fundação Python: histórico de CI nos três sistemas; o workflow antigo foi removido. CI atual desta branch é Rust Windows.
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

## Auditoria consolidada do PR #30 (17/10/2026)

**Referência auditada:** `20200158ab1ac0b836c23d2efee8d9314c2fde02`, base `main` em `a6a27d6`; GitHub informa 70 commits e 369 arquivos alterados. Foram conferidos histórico/diff, inventário dos 14 crates, dependências, entradas CLI, rotas e ligações do servidor, fontes Python/front, testes e workflow. Esta é uma auditoria de escopo e conclusão, não a auditoria final de segurança F27. O PR está aberto, sem merge e sem conflitos na consulta; não havia comentários de revisão retornados pela API.

**Legenda:** **Implementado** = comportamento localizado no código; **Parcial** = existe parte da entrega, mas faltam requisitos ou ligação; **Pendente** = entrega planejada não encontrada; **Validado** sempre informa teste/commit/plataforma. Nenhum marco M0–M9 nem fase F00–F27 tem o aceite completo demonstrado. Código portado, teste unitário, teste com provedor falso e teste físico são evidências diferentes.

### Entregas implementadas no PR

- [x] Workspace Cargo com 14 crates, lockfile, iced fixado em 0.14.0, perfil de release e workflow Windows. A divisão atual agrupa agente/ferramentas/canais no core; não corresponde a todos os crates propostos no plano.
- [x] Política de ferramentas portada em `orion-policy`: classes, aprovações de uso único, caminhos, shell, redação e auditoria. **Implementação sem suíte própria de testes; não equivale a paridade aprovada.**
- [x] SQLite v1–v23, FTS5, vetores, busca híbrida, sessões, fatos, documentos, backups e importação; 60 casos de integração em `orion-store`.
- [x] Gateway compatível com OpenAI, provedores gratuitos, heurística Auto, delegação às CLIs oficiais, embeddings remotos, visão/transcrição e cotas em `orion-ai`; 84 casos de integração. ACP e embeddings locais não estão implementados.
- [x] Agente, autenticação PBKDF2/sessões, ferramentas, jobs, projetos, artefatos, biblioteca, captura, ponte, rotinas e extensões portados em `orion-core`, com rotas Axum e CLI em `orion-server`. O porte é amplo, mas a paridade do conjunto está incompleta.
- [x] MCP próprio via JSON-RPC (stdio/HTTP), host de conexões, OAuth, recursos/prompts/índice, skills e plugins por concessão. `rmcp`, plugins WASM e MCP Apps continuam planejados.
- [x] Controle auxiliar iced Windows, bandeja/hotkeys, mini barra, pânico, autostart opt-in, execução de rotinas concedidas e orçamento persistido. Não substitui o app principal.
- [x] Telegram com lista de IDs/conversa privada, texto/foto/áudio, aprovações remotas inline e 2FA por código. PIN crítico, Hello, WoL e fila de nuvem não entregues.
- [x] Captura cpal, VAD por energia, detector de palmas calibrado, interface openWakeWord, Whisper local, edge-tts, AEC3 e barge-in opt-in ligados ao listener. Faltam Silero, modelo Orion final e prova acústica/GPU.
- [x] `orion-local`/`orion-llama`: catálogo de oito artefatos, download com retomada/hash/cancelamento, worker llama.cpp CPU/Vulkan, carga sob demanda e teto Windows de 30% da RAM; instalação dos runtimes e configuração pelo setup.
- [x] Visual do setup: céu procedural, etapas/transições, ícone PE, cantos DWM, pt/en e áudio instrumental cpal com mute. A especificação de som original ainda diverge; etapas técnicas do sistema seguem parciais.
- [x] `orion-vault`: autodetecção, parser Markdown/YAML, marcadores de privacidade/arquivo, scanner de padrões de segredos, busca em cache e API explícita de gravação com git. **F06 não está concluída**: ver bloqueios abaixo.
- [x] Porte parcial E10/E11: finanças/revisão/diário/cápsula/foco, visão/presença/legenda; câmera nativa `nokhwa` e extrator facial Haar heurístico + LBPH acrescentados em `975e635` (lock ajustado em `2020015`). Testes matemáticos não validam reconhecimento em pessoas reais.
- [x] Ponte sidecar Rust/Python isolada por stdin/stdout, sanitização de ambiente e validação de URL; quatro testes no crate. Ainda não há consumidor no core/servidor.
- [x] Testkit, 334 declarações de testes de integração de paridade (store 60, AI 84, MCP 23, core 167), benchmark comparativo e correções de importação/captura/relógio/MCP; remoção de 79 arquivos de legado/PyInstaller.

### Situação de todas as fases do plano F00–F27

Os caminhos abaixo são relativos a `Orion_Rust/crates/`, salvo indicação. O detalhamento e os critérios de aceite permanecem em [ORION_PLANO_IMPLEMENTACAO.md](ORION_PLANO_IMPLEMENTACAO.md).

| Fase / marco | Estado | Evidência entregue | O que falta para concluir |
|---|---|---|---|
| F00 / M0 | Parcial | Workspace, lock, iced fixado, CI Windows, benchmark, spikes de áudio/local | SQLCipher/sqlite-vec/cr-sqlite, métricas no PC, export real, snapshots iced, evals, deny/audit, toolchain fixo, spikes restantes |
| F01 / M1 | Parcial | `orion-server/src/desktop.rs`, `orion-platform`, céu do setup | Casca principal conforme UI, sidebar/paleta, Mica, fontes, Fluent, instância única, hotkeys completas, acessibilidade e medição |
| F02 / M1 | Parcial | `orion-core/src/{auth,dbkey,dbopen,logcrypt}.rs`, `orion-store/src/crypto.rs`, `orion-policy` (máscara de PII, 25 testes), `two_factor.rs`, cofre e saídas | Rodar `db-cifrar` no banco real, Hello, janela de senha no autostart, restaurar backup cifrado, `auth.db`, PII fora de log e audit, Win+L provado no Windows e aceites de segurança |
| F03 / M1 | Parcial | `orion-ai`, `orion-local`, `orion-llama`, `orion-server/src/embedded.rs` | ACP/stream-json completo, esforço/modelo por CLI, cache, catálogo/UI finais, modelos de visão, medição Vulkan e gestão completa de modelos |
| F04 / M1 | Parcial | `orion-core/src/agent.rs`, ferramentas, aprovações, modos, compactação | Paridade de política/agente, autônomo conforme plano, classificador injection, confirmação destrutiva extra, checkpoints/retomada, paralelos/subagentes, toasts com botões |
| F05 / M1 | Parcial | API de chat SSE, ramos/anexos/artefatos/exportação e front web compartilhado | Chat nativo iced completo, Markdown/LaTeX/Mermaid/WebView2, virtualização, fila e testes E2E/snapshots do novo app |
| F06 / M1 | Parcial | `orion-vault`, rotas `/vault/*`, memória/store e importadores | Resumo/nota automática por turno, pull nos eventos, conflitos sem sobrescrever, índice local incremental/sqlite-vec, privacidade de ponta a ponta, editor e aceite Git real |
| F07 / M1 | Parcial | Settings por ambiente/.env/JSON local; API e controle de orçamento | Modal completo da especificação, TOML com recarga, busca/desfazer/restaurar, todas as seções e strings pt/en |
| F08 / M1 | Parcial | `orion-core/src/projects.rs`, documentos, ramos e rotas | UI iced, clonar/vincular GitHub, nota no vault, herança de configurações, arquivamento coordenado e suíte de isolamento completa |
| F09 / M1 | Parcial | `painel.rs`, rotas, resultados/arquivadas no front atual | Telas iced finais, uso oficial de assinaturas, métricas completas, resumos diários e snapshots |
| F10 / M1 | Parcial | `orion-setup` (registro, identidade MSIX autoassinada, desinstalador com janela) e `orion-updater` (minisign + SHA-256, tarefa diária) | Provar no Windows real (instalar, atualizar e desinstalar), UAC, certificado e chaves de release, publicar o feed assinado, fixar na barra, onboarding/Hello, backup Drive e instalação limpa |
| F11 / M2 | Pendente como entrega | Ferramentas shell/Git/processos existem no core | Painel de arquivos/diff por bloco/terminal ConPTY, ACP no painel, fluxo GitHub/CI, revisão cruzada acionável e extensão VS Code |
| F12 / M2 | Parcial | `jobs.rs`, `rotinas.rs`, `automacao.rs`, Downloads, pesquisa, briefing | Agendamento NL completo, recuperação/retentativas conforme plano, resultados no vault, nuvem Claude/Codex, monitores gerais e proatividade |
| F13 / M2 | Parcial | `orion-ai/src/budget.rs`, painel/API: reset/calibragem/projeção e tetos 95%/20% | `/usage`/`/status`, precisão aferida, alertas/canais completos, fila de automelhoria e preempção de tarefas |
| F14 / M3 | Parcial | `channels/telegram.rs`, 2FA e cartões remotos; áudio | PIN crítico, reserva Hello, comandos de nuvem, fila/ocupado, energia/WoL, ACL Tailscale e dois aparelhos reais; cliente é HTTP próprio, sem teloxide |
| F15 / M4 | Parcial | `orion-voice`, `native_audio.rs`, `wake.rs`, `voice.rs` | Silero, wake word final, qualidade/latência/eco medidos, visualização de áudio, atalhos completos, overlay de legenda/tradução e briefing em podcast |
| F16 / M5 | Parcial só nos dados | `grafo.rs`, endpoints e grafo WebGL do front; parser de wikilinks | Grafo nativo wgpu/compute, layout/fallback, aliases completos, clusters, timeline, navegação e desempenho 1k/5k/10k |
| F17 / M5 | Pendente | Céu do setup já tem render próprio; não é a entrega F17 | Bloom/blur do grafo em WGSL, qualidade automática, bateria e controles |
| F18 / M6 | Parcial | Câmera nokhwa, Haar/LBPH, presença, avistamentos/rotas | Runtime YOLO, prova facial real, indicadores de câmera, bloqueio/retorno conforme especificação, retenção/lugar dos objetos e UI final |
| F19 / M7 | Parcial | Cliente/servidor MCP, OAuth, host, catálogo, skills/plugins/importação e export credentials | WASM/Wasmtime/WIT, MCP Apps isolados, revisão anti-rug-pull completa, UI Conectores, extensão de navegador e validação com servidores reais |
| F20 / M8 | Parcial | Ponte/captura, `screen_memory.rs`, OCR externo e legenda | Spotlight/seleção nativos, UI Automation/OCR Windows, mascaramento visual, Click to Do, gravação/detecção de reuniões e banco cifrado |
| F21 / M8 | Parcial | Busca/fetch web e `research.rs`; sidecar separado | Navegador visível/CDP/chromiumoxide ou integração explícita do sidecar, política por ação, pesquisa com relatório/progresso e monitores completos |
| F22 / M8 | Pendente | Bind/login local não equivalem à integração de rede | Sync entre PCs, ACL/exit node, subnet/WoL, Home Assistant/Alexa, Siri/Atalhos, Phone Link e Saúde |
| F23 / M8 | Parcial | `vida.rs`, OAuth/agenda/MCP, captura de gastos e reserva LLM local | Integrações Google reais, OFX/CSV bancário completo, telas novas, origem SMS, tempo de uso/bem-estar, export pessoal e uma semana de validação |
| F24 / M8 | Pendente | Leitura de documentos/busca genéricas não concluem estudo | FSRS, tutor, progresso, conexões de notas e fluxo de estudo/PDF/TTS |
| F25 / M8 | Parcial | Histórico local, Downloads, ferramentas de apps/mídia/documentos, conversor e planos | UI/hotkeys finais, desfazer/Lixeira, Spotify oficial, modo jogo/economia, PowerPoint e conversões ainda ausentes (inclui comprimir PDF) |
| F26 / M9 | Pendente | Há contadores e tarefas de fundo restritas; não são automelhoria | Fila/termos, agendamento 90%/12h, PRs/testes/evals, interrupção por pedido, auditoria e dependências automáticas |
| F27 / M9 | Parcial documental | Memórias/README/vault, inventário e limpeza do código obsoleto | Auditorias finais, offline ponta a ponta, performance, export/migração/restauração, descarte no PC, release 1.0 e plano macOS |

### Bloqueios concretos encontrados na leitura do código

1. **F06 não pode ser marcada pronta.** `orion-vault/src/sync.rs::save_conversation` é chamado apenas por `orion-server/src/rotas/memoria.rs::vault_save`, sem gancho automático no chat. A escrita substitui conteúdo existente, não preserva duas versões; `GitSync::commit_file` usa `git commit -m` sem isolar outros arquivos já staged, nem garante que HEAD é `main`. `pull` é ação explícita, o cache é reconstruído nas rotas e o parser tolera YAML inválido. Faltam bloqueio de frontmatter/links inválidos, credencial comprovada e teste de conflito/privacidade. Scanner cobre padrões de chaves, não toda a PII prometida.
2. **YOLO sem inferência:** `orion-core/src/visao_local.rs::Detector::detectar_jpeg` sempre retorna `vec![]`, mesmo com arquivo de modelo válido. Decodificador/NMS/armazenamento de rótulos existem; detecção e "onde deixei" com novas imagens não estão entregues de ponta a ponta.
3. **Reranker sem backend:** `orion-core/src/memory/rerank.rs::LocalReranker::score` falha com "runtime ONNX não vinculado neste binário" para trechos não vazios. Embeddings e5/BGE locais e sqlite-vec também não constam no runtime atual.
4. **Sidecar isolado:** nenhum crate de produção declara dependência `orion-sidecar` e não há chamada a ele pelo agente. O alvo "sem Python" do plano C11 e a ponte residual existente ainda precisam ser conciliados; manter a ponte não significa que navegador/crawler estejam integrados.
5. **Instalação parcial (atualizado em 19/10/2026):** registro em Apps instalados, atalho, início automático opt-in, identidade MSIX autoassinada, atualizador minisign com tarefa diária e desinstalador com janela existem no código, mas nunca rodaram num Windows real. Continuam fora: certificado de assinatura de release (o autoassinado só dá identidade local), par de chaves e feed de releases do atualizador, fixar na barra (o Windows 11 não permite sem confirmação) e onboarding. O pacote precisa conter as pastas CPU/Vulkan ao lado do setup, além de dependências externas quando usadas (edge-tts/ffmpeg/git/CLIs). O workflow/instalador não copia o front HTML: `front_dir()` depende de `project_root()`, cujo padrão é o caminho do repositório na máquina de build (`CARGO_MANIFEST_DIR`). Fora do checkout, `/ui/` não está resolvido pela instalação; precisa de assets e caminho de runtime próprios.
6. **Segurança do novo plano parcial (atualizado em 19/10/2026):** login em Argon2id com migração e zeroização; banco cifrado por SQLCipher com chave mestra, recuperação, logs cifrados com retenção e máscara de PII no log e no audit, tudo implementado e testado em Linux, mas o banco só fica cifrado depois de você rodar `orion db-cifrar` (o `orion.db` atual segue em claro). Faltam Windows Hello, janela de senha no autostart, restaurar backup cifrado, cifrar o `auth.db`, PII além de log e audit (por exemplo na memória e nos arquivos) e o aceite de segurança F27. Não declarar proteção completa de PII. O detector facial novo usa contrastes Haar heurísticos + LBPH; não há conjunto real/ROC que demonstre taxa segura de reconhecimento.
7. **Falta o app principal nativo:** não existem crates `orion-ui`/`orion-app` nem `orion-graph`; `desktop.rs` é controle auxiliar e ainda há servidor HTTP/front HTML. O céu usa imagem RGBA renderizada na CPU via `orion-setup/src/sky.rs`, não um motor wgpu compartilhado por todas as telas.
8. **CI não cobre o aceite inteiro:** não há snapshots/headless iced, cargo deny/audit, evals YAML, auditoria final ou fluxo de release assinado. `orion-policy`, `orion-server` e `orion-platform` têm zero testes próprios; testes de outros crates não provam todos os contratos dessas áreas.

### Evidência de validação e próximos passos

- **Windows CPU em `d069b17`:** [run 39084684949](https://github.com/antoniossalomao/Orion/actions/runs/39084684949) registra 388 testes aprovados nos alvos executados, build dos executáveis CPU e smoke com GGUF real aprovados. O workflow não seleciona `orion-sidecar`; testes condicionais podem retornar sem exercer hardware. Não é prova de paridade de 100%.
- **Vulkan no mesmo run (histórico):** o job terminou cancelado, com erro CMake/MSB8066 do `llama-cpp-sys-2` por `vulkan-shaders-gen-build/cmake_install.cmake` ausente. **Corrigido depois:** em `d78efeb` o [run 39097948560](https://github.com/antoniossalomao/Orion/actions/runs/39097948560) concluiu com sucesso (build Vulkan, pacote e upload). Sem prova de GPU física.
- **Último código auditado `2020015`:** [run 39086573378](https://github.com/antoniossalomao/Orion/actions/runs/39086573378), em andamento no build Vulkan na última consulta; testes de domínio, build CPU e smoke GGUF CPU já passaram também nesse commit. Isso inclui compilação do porte de câmera/Haar/LBPH; reconhecimento real continua sem prova física.
- **Referência após limpeza, `d069b17`, e repetição nesta auditoria documental sobre `2020015`:** 1.564 testes Python passaram, três ignorados; 16 arquivos Node passaram; Ruff/formatação/Pyright com venv explícito e sync locked passaram. Na auditoria também foram conferidos os links locais, blocos de código, as 28 fases e 458 IDs únicos do plano. Contagens históricas maiores incluem testes removidos do legado; não são a contagem atual.
- **Prioridade técnica:** corrigir build Vulkan e obter artefato; portar testes de política/API/agente; fechar integração segura do vault; implementar app iced e segurança F02; completar modelos/voz/câmera; depois medir no notebook, migrar dados e executar os aceites de release. Nenhum desses bloqueios é resolvido apenas por atualizar o checklist.

## Pendências suas (só você faz)

1. Fase 0: rodar `orion verify-export` no PC até dar `PRONTO` (plano de corte no legado).
2. No notebook: `uv sync` e `uv run pytest`.
3. Chaves dos provedores (`orion chave <id>`) e teste de `POST /chat`.
4. Chave de embeddings (`ORION_EMBED_API_KEY`) e de 15 a 30 perguntas reais em `tests/eval_pessoal.local.json`.
5. Telegram: criar o bot, preencher token e IDs, parar o bot do legado e testar. Instalar o Tailscale e rodar `orion autostart`.
6. MCP: escolher os servidores (e-mail, agenda, navegador), copiar `mcp.example.json` para `mcp.json` e rodar `orion mcp-check`. Registrar o Orion no Claude Code com `claude mcp add orion -- orion mcp-servidor`.
7. E2: testar teclas globais e microfone; calibrar as palmas.
8. Reserva local: selecionar GGUF/Whisper no `orion-setup` e validar no notebook. Ollama é alternativa opcional da referência antiga, não requisito do modo local embarcado.
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
| 7, residual Python (1 a 2 semanas) | Sidecar com browser-use e crawl4ai, JSON por stdin e stdout | Ponte e pacote implementados; integração ao agente, dependências na instalação e navegação real pendentes |
| 8, corte (2 a 3 semanas) | Migração de dados verificada, instalador assinado no Windows, comparação de desempenho | Instalador parcial: cópia de runtimes/modelos real; identidade, certificado, atalhos, serviço, assinatura, desinstalador e corte ainda pendentes |
| 9, macOS (a definir) | Implementações de plataforma, permissões, `.dmg` assinado e notarizado | Começa depois do corte do Windows |

**Métricas:** tempo até a janela pronta, memória ociosa, latência da primeira resposta, tamanho do pacote e paridade dos testes (100% no primeiro marco).

**Riscos principais:** o toolkit não atende o visual; a API do iced muda entre versões; embeddings em Rust com qualidade menor; permissões do macOS; escopo que cresce; perda de dados (crítico, mitigado por backup verificado e esquema compartilhado); browser-use exige Python.

### Paridade de testes e linha de base (17/10/2026)

- **Testes portados do Python** (`cargo test --workspace --exclude orion-setup`): `orion-store` 60, `orion-ai` 84, `orion-mcp` 23, `orion-core` 167 (memória, importação, MCP, login, jobs, modos, biblioteca, captura, autostart). O sidecar tem 4. **`orion-policy` ainda não tem nenhum teste** (a lista de 14 não existe nesta branch): a paridade de `tests/policy/` continua pendente. Mudanças na implementação protegida continuam exigindo aprovação explícita; o porte de código não comprova a paridade dos testes.
- **Ainda sem testes de paridade portados no `orion-core`:** agente, Telegram, ponte, voz, palavra de ativação, palmas, visão/câmera, vida (E10), rotinas, downloads, briefing, plugins/extensões, projetos (isolamento), pesquisa, tela. As rotas HTTP (`orion-server`) também.
- **Divergências do Rust achadas e corrigidas pelos testes:** importador tratava registro sem `id` como `"null"` e aceitava `score` não numérico; captura criava a pasta fora do vault antes de recusar `..`; `JobRunner` ignorava o relógio do banco; cancelamentos de MCP nunca eram contados.
- **Benchmark histórico informado na integração do PR #33** (`scripts/benchmark_rust_python.py`, banco sintético pequeno, 4 CPUs Linux; **não** é o seu PC e não foi reexecutado nesta auditoria; alvo Rust atual continua Windows): busca FTS5 p50 2,5 ms (Python) × 1,9 ms (Rust); híbrida 3,1 × 2,9 ms; memória com a matriz vetorial 60 × 18 MB; servidor ocioso 128 × 34 MB, `/health` em 2,7 × 0,2 s, `/busca` HTTP p50 26 × 17 ms; resultados dos dois lados idênticos. Falta rodar no Windows com o banco real e em escala maior (o gerador sintético é lento acima de ~5 mil registros).

**Integração do PR #33 (17/10/2026):** base do PR #30 atualizada até `8f4c783`, com dependências de teste e voz/Vulkan preservadas no manifesto e lockfile. Os testes de autostart passam a esperar `desktop --background` no Windows. Ruff dos scripts novos corrigido; benchmark usa token efêmero e só faz HTTP em loopback. Validação da integração: 1.637 testes Python passaram, 3 ignorados; 149 testes do front, Ruff, formatação, Pyright e checagem estática Windows/Vulkan verdes. O cliente HTTP do benchmark foi conferido com servidor local autenticado e recusa de destinos externos. A execução nativa dos novos testes de paridade depende do CI desta branch.

## Backlog Rust

Os itens em aberto das etapas antigas foram reescritos aqui, com IDs novos.

| ID | Item | Dono |
|---|---|---|
| B-01 | Verificar o export real do SurrealDB (fase 0) | Você (PC) |
| B-02 | Medir a linha de base do Python: janela pronta, memória ociosa, latência e tamanho do pacote — **script pronto (`scripts/benchmark_rust_python.py`); falta rodar no PC com o banco real e o pacote** | Você e Claude |
| B-03 | Spike iced: chat, paleta `Ctrl K` e grafo em wgpu; medir memória e latência | Claude |
| B-04 | **Parcial:** detector/calibração e engine local implementados; candidato Orion sintético existe, mas não é final. Faltam corpus real, ruído/música e aceite no microfone (E2/E9) | Você e Claude |
| B-05 | Spike de busca: rusqlite com FTS5 lendo o banco real; BGE-M3 via `ort` ou `candle` comparado ao Python — **FTS5 + vetores medidos (`orion-store/examples/bench_busca.rs`, resultados idênticos ao Python); falta BGE-M3** | Claude |
| B-06 | **Parcial:** cpal/VAD energia/Whisper/AEC3/barge-in integrados. Faltam Silero, corrigir build Vulkan, medir latência/eco/qualidade no notebook (ver [ARQUITETURA.md](ARQUITETURA.md), 8.3) | Claude |
| B-07 | Portar `policy/` com os mesmos casos de teste (fase 2) — **Parcial: código portado; zero testes no crate, paridade pendente** | Claude |
| B-08 | Definir as telas do primeiro marco | Você |
| B-09 | Fixar a versão do iced no `Cargo.lock` — **Feito (`iced = "=0.14.0"`)** | Claude |
| B-10 | Decidir a licença-alvo (ver Pendências, item 10) | Você |
| B-11 | Sidecar Python com browser-use e crawl4ai (fase 7) — **Ponte e pacote feitos; ligação ao agente/instalação e teste real pendentes** | Claude |

**Fora do marco 1:** E10 a E13. E10 e E11 já têm portes parciais em Rust (`vida.rs`, câmera, presença, rosto e legenda), além da referência Python. A inferência YOLO e o aceite de hardware ainda faltam; E12 e E13 não começaram como entregas completas. Cada um começa pelas regras em [REGRAS.md](REGRAS.md).

### Limpeza do legado (17/10/2026)

Removidos 79 arquivos obsoletos: `Orion_Ollama/`, `bin/startup/`, motores de voz antigos em `Orion_Core/`, launchers Python antigos do front, `requirements.txt` do legado e cadeia PyInstaller (workflow, spec, entrada, smoke e instruções do pacote). Saíram também os testes exclusivos desses componentes e três verificações de compatibilidade com os arquivos removidos. O front HTML/CSS/JS e seus assets continuam no mesmo caminho usado por Rust/Python e pelos testes Node. O grupo `build` e as dependências exclusivas do PyInstaller saem de `pyproject.toml`/`uv.lock`.

`orion/` continua como especificação executável: vários contratos ainda não têm testes Rust, e o benchmark compara as duas implementações. Python também permanece para o sidecar, edge-tts, treino/avaliação de wake word e fixtures MCP. As proteções de caminhos antigos nos testes de política continuam válidas para dados de instalações anteriores. `ORION_LEGADO.md` e `REGRAS.md` preservam referências históricas; os fontes removidos podem ser consultados no histórico Git.

Validação da limpeza: `uv sync --python 3.12 --locked`, Ruff, formatação e Pyright (com o venv explicitado) passaram; 1.564 testes Python passaram, 3 ignorados; os 16 arquivos de testes Node passaram. O workflow Windows também passa a observar o front, o smoke de modelos e a fixture MCP compartilhados. A compilação nativa continua no CI Windows.

## Decisões

Registro das decisões que valem para o projeto hoje. O histórico completo, desde 25/06/2026, está em [ORION_LEGADO.md](ORION_LEGADO.md) (plano antigo, seção 8).

| Data | Decisão | Motivo |
|---|---|---|
| 19/10/2026 | Login em Argon2id com migração do PBKDF2; banco cifrado por SQLCipher com chave mestra aleatória embrulhada pela senha e por um código de recuperação; migração com cópia conferida e original em claro guardado até ele apagar | Trocar a senha sem recifrar o banco, recuperar se esquecer, e nunca perder dados numa migração interrompida |
| 19/10/2026 | Instalador registra o app com `reg.exe`/PowerShell (HKCU), identidade MSIX com certificado autoassinado (pulada e dita quando falta SDK ou administrador), atualizador `orion-updater` com tarefa diária SYSTEM e desinstalador com janela de três caixas | Respostas do Antônio em 19/10/2026; o que depende de certificado de assinatura real ou de chaves de release fica documentado como pendente |
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

- App iced ligado ao núcleo: cliente HTTP do `orion serve` (login/2FA, sessões, chat SSE, aprovações, rotinas, projetos, fatos, resultados, painel, orçamento, modo/pânico, grafo). Rodar: `orion serve` + `orion-app` (ORION_URL padrão http://127.0.0.1:8000, ORION_ADMIN_TOKEN opcional); `--demo` usa dados de exemplo. Testes live: ORION_TEST_URL + ORION_TEST_SENHA. Nuvem/MCP/Skills/modelos locais/atalhos seguem de exemplo.
- `orion-app` sobe o `orion serve` sozinho (processo filho, 127.0.0.1, porta livre, token aleatório; binário `orion` ao lado do app, ou `ORION_BIN`) e o encerra ao sair; com `ORION_URL`/`ORION_ADMIN_TOKEN` usa um servidor existente. Banco cifrado exige `ORION_DB_PASSWORD`. Se o app for morto à força o servidor filho pode ficar órfão (falta job object no Windows). Evolução planejada: embutir `orion-core` no processo (extrair `estado::montar`).
- Um só executável: `orion.exe app` abre a interface nova com o servidor DENTRO do mesmo processo (127.0.0.1, porta livre, token aleatório só na memória; `crates/orion-server/src/appmode.rs`; `orion-server` depende de `orion-app` como biblioteca). O instalador abre/atalha `orion.exe app`; o pacote não leva mais `orion-app.exe`. `orion desktop --background` (bandeja/atalhos globais, autostart) segue como estava. O binário `orion-app` fica só para desenvolvimento (`--demo`, ou sobe o `orion serve` como filho).
