---
type: recurso
projeto: "[[Orion]]"
tags: [orion, auditoria, pendencias, checklist]
date: 2026-10-20
revisao: 2026-10-20 (documentos corrigidos)
resumo: "Auditoria dos 7 documentos de Memorias Do Projeto contra o código da branch do PR #30: o que falta, checklists errados, ideias que não batem entre documentos."
---

# ORION - AUDITORIA DE PENDÊNCIAS (20/10/2026)

> Base: código da branch `claude/wizardly-bell-ynlfwv` (PR #30) em 20/10/2026. Lidos: ORION, REGRAS, ARQUITETURA, PLANO_IMPLEMENTACAO, PLANEJAMENTO_ARQUITETURA, UI_ESPECIFICACAO, INSTALADOR_ESPECIFICACAO. `ORION_LEGADO.md` não entra (referência).
> Legenda: **[V]** só você pode fazer · **[W]** precisa de Windows/hardware real · **[C]** código que eu posso fazer · **[D]** decisão sua.

---

## 1. Resumo

- **Arquitetura-alvo foi atingida e os documentos ainda descrevem a anterior.** Hoje: 1 exe (`orion.exe`), núcleo no processo, sem porta, sem Python, `orion-llama.exe` separado, updater como subcomando. Os docs ainda falam em `orion serve`, front HTML em `/ui/`, `orion-sidecar`, Python de referência e `Orion Setup.exe` com certificado/serviço.
- **Os checklists do PLANO (F00–F27) estão todos `[ ]`**, inclusive itens concluídos. Só os blocos "Estado PR #30" dizem a verdade, e vários estão velhos.
- **O que impede o 1.0 não é mais código base:** é (a) prova em Windows real, (b) chave minisign + 1º release, (c) ~15 fases de funcionalidade (F11–F27) que nem começaram.

---

## 2. Documentos desatualizados ou contraditórios — status da correção

> **Corrigidos em 20/10/2026:** 1, 2, 3, 4, 5 (nota de equivalência Rust em REGRAS.md; as colunas por princípio seguem citando Python e devem ser reapontadas ao tocar cada princípio), 6, 7, 8, 9, 10, 13, 14, 15, 17, 19, 20. **Decididos por você em 20/10/2026 (docs já refletem; código a ajustar):** 11 (Ctrl+Alt+Espaço/P/O/I), 12 (autônomo pede senha), 16 (remover mudo). **Ainda aberto:** 18 (leitura oficial de uso, bloqueada pela D03). Os textos dos itens 17 e 19 foram escritos como "decisão 20/10" e valem se você confirmar as perguntas de alinhamento.
> Também notado ao corrigir: o CI (`rust-windows.yml`) roda só `cargo test`, **sem `fmt` nem `clippy`**; o plano tem 40 itens agora marcados `[~]` (todos que já traziam "Estado PR #30").


| # | Onde | Problema | Correção |
|---|---|---|---|
| 1 | ORION.md, "Estado (17/10)" e lista de crates | "14 crates" incluindo `orion-sidecar`. Hoje são **15**: ai, app, core, llama, local, mcp, platform, policy, server, setup, store, testkit, updater, vault, voice. Sidecar não existe | Atualizar contagem e lista |
| 2 | ORION.md, últimos bullets (linhas ~281–292) | Bullets cronológicos se contradizem: "app sobe `orion serve` como processo filho 127.0.0.1" → "servidor dentro do processo" → "sem socket". Só o último vale | Fundir em **uma** descrição: `orion.exe` = app+núcleo, `Api::local`, sem porta; `orion serve` só para testes/CI |
| 3 | ORION.md / ARQUITETURA §7 | Tabela de crates ainda lista `orion-sidecar`, "Python residual", "ainda não existem como crates: app/UI, updater" (ambos existem) | Reescrever §7 |
| 4 | ARQUITETURA §1 e §8 | "Python só onde não há equivalente (browser-use e crawl4ai)". Resolvido em Rust (`buscar_url` com Readability, `navegador_*` CDP) | Trocar por "sem Python no produto" |
| 5 | REGRAS.md ("Imposto em"/"Prova") | Quase todos os princípios apontam para `orion/*.py` e `tests/*.py`; CI "Linux/Windows/macOS" | Reapontar para `crates/orion-policy`, `orion-core`, testes Rust. Enquanto o Python existir no repo, manter só como "referência" |
| 6 | AGENTS.md (raiz) | Manda rodar `uv`, `ruff`, `pyright`, `pytest`, `node --test`; diz que sidecar e fixtures MCP continuam necessários e que `Orion_Core/Front_end_Orion/` é servido pelo Rust | Trocar por `cargo fmt/clippy/test` do `Orion_Rust`; remover menção ao sidecar |
| 7 | INSTALADOR §"Estado" e F10.09 | Diz "setup exige pastas CPU/Vulkan ao lado" e que certificado/atalhos/serviço são placeholders. Hoje o instalador **baixa do GitHub Releases** (minisign + SHA-256) com fallback offline; atalho, autostart, identidade MSIX e tarefa do updater estão no código | Atualizar tabela do instalador |
| 8 | PLANO F10.14/F10.15 | "Updater serviço/tarefa SYSTEM chama `orion-updater --check`" — o updater virou **subcomando `orion update`** e `ALLOWED_FILES` já é só `orion.exe`+`orion-llama.exe` | Atualizar texto |
| 9 | PLANO F21 + auditoria | "Sidecar tem testes isolados, sem consumidor"; navegador visível/CDP pendente. Agora: leitura (F21.04) e navegador guiado via Edge/Chrome com CDP (F21.01 parcial) **feitos**; ainda não é "aba do painel" nem WebView2 | Marcar F21.01 parcial, F21.04 feito |
| 10 | PLANO F01 ("app principal iced pendente") e UI_ESPECIFICACAO "Estado" | Diz que `orion-app` principal **não existe** e que o front é HTML. Existe: 47 cenas, ligado ao núcleo, bandeja, mini barra, atalhos | Reescrever bloco "Estado" da UI e da F01 |
| 11 | UI_ESPECIFICACAO U3/U4 e F01 (atalhos) | Docs divergem: Win+Shift+Espaço, Win+Shift+O/I, Alt+Shift+O/I. **Código usa Ctrl+Alt+Espaço (janela) e Ctrl+Alt+P (pânico)** | Escolher e registrar (ver D-A) |
| 12 | PLANO F04.17 (modo autônomo exige senha para ligar) × código | Implementado como meta `modo:autonomo` (`max_iterations*4`) sem pedir senha na UI | Alinhar: ou pedir senha, ou mudar o plano |
| 13 | PLANO D07 (licença) × ORION pendência 10 | D07 diz "uso pessoal"; pendência 10 ainda trata licença como aberta; `Cargo.toml` usa `LicenseRef-Proprietario` | Fechar a pendência |
| 14 | PLANEJAMENTO "Decisões" (tabela) | Fontes Geist/Newsreader (UI) × Inter/JetBrains Mono (INSTALADOR, F01.02). UI_ESPECIFICACAO U2 já diz "vale o mockup" mas F01.02 não foi corrigido | Corrigir F01.02 (e dizer que o instalador usa Inter) |
| 15 | PLANEJAMENTO "Fases" (F1–F9) | Lista antiga ainda com `[ ]`; PLANO diz que foi substituída | Apagar ou marcar "substituída por F00–F27" |
| 16 | PLANO F10.03 × INSTALADOR §9 × código | Spec: som "sempre ligado, sem botão, 4 osciladores". Código: trilha instrumental com mute. A própria nota já avisa | Decidir (D-B) e fechar |
| 17 | PLANO conflitos C11/C28 (WebView2) × código | Spec prevê WebView2 para navegador visível e MCP Apps; implementado com Edge/Chrome externo via CDP | Registrar decisão: externo guiado vence |
| 18 | PLANO F13 (`/usage`, `/status`) × D03 | F13 "bloqueada pela D03" (termos dos provedores) mas há código estimado em produção | Registrar que leitura oficial segue bloqueada |
| 19 | PLANO F14.01 / F19.01 | Cliente próprio (Telegram HTTP, MCP) em vez de `teloxide`/`rmcp` | Decidir: manter próprio e **editar o plano** (mais honesto que deixar `[ ]`) |
| 20 | ORION.md "Validação" | Cita 1.637/1.639 testes **Python** e 149 do front; esse código não está mais no produto | Trocar pelos testes Rust atuais |

---

## 3. Checklists que estão errados

### 3.1 Itens que já estão feitos (ou quase) e continuam `[ ]`
| Item | Estado real |
|---|---|
| F01.xx casca iced (barra lateral, superior, Início, chat, composer, painel, Configurações 14 seções, paleta, popovers, toasts) | Feito (parcial: sem grafo wgpu, sem snapshots comparados) |
| F01 bandeja + atalho global | Feito no código; **[W]** nunca rodou no Windows |
| F02 login Argon2id + SQLCipher + `dbkey` + tela de senha do banco | Feito; falta Hello, PIN crítico |
| F02/F10 instância única | Feito (`orion.lock` + `orion.mostrar`) |
| F03 gateway, CLIs, roteador | Feito parcial; ACP ausente |
| F05 chat SSE, aprovações, painel direita | Feito (diff por bloco e terminal ConPTY não) |
| F10.09 cópia/registro/atalho/autostart/identidade/rollback | Feito no código; **[W]** |
| F10.11 download de modelos com hash/pausa | Feito parcial (`/modelos-locais`) |
| F10.12 desinstalador 3 caixas | Feito no código; **[W]** |
| F10.15 minisign + SHA-256 + `.bak` + `--rollback` | Feito; falta chave + feed publicado + rollback automático |
| F10.28 `release.yml` (build, assina, publica) | Existe; **[V]** nunca rodou sem a chave/segredos |
| F12 rotinas, jobs, Downloads, briefing, pesquisa | Parcial: faltam linguagem natural, tela completa, nuvem, monitores, proatividade |
| F12 "segundo plano" (tela Nuvem → `/tarefas-bg`) | Feito (decisão: sem CLI remota; tarefas em segundo plano do Orion) |
| F15 voz: cpal, VAD por energia, palmas, Whisper, TTS, AEC3, barge-in | Feito parcial; faltam Silero, wake word final, UI de voz |
| F18.01 nokhwa | Feito; indicadores faltam |
| F19 MCP próprio, OAuth, skills, plugins, importação, servidor MCP com credenciais | Feito; falta WASM/WIT, MCP Apps, rug-pull completo |
| F19.06 / skills ligar-desligar | Feito (`/skills/ligar`) |
| F21.03/F21.04 busca + leitura + pesquisa | Feito (Readability+Markdown, netguard) |
| F21.01 navegador guiado (CDP, Fetch.enable, recusa senha/cartão) | Parcial: sem aba no painel |
| F26/F13 "modo autônomo", "corrigir fato" | Feito |
| F27.06 "código obsoleto retirado" | Feito inclusive sidecar/python |

### 3.2 Itens `[ ]` corretos (nada feito)
F08.xx completo (Projetos: cartões, GitHub clonar, isolamento R20, arquivar) — só APIs existem; F09 telas iced de Painel/Resultados/Integrações com métricas; F11 inteiro; F16 grafo wgpu; F17; F20 (captura/OCR/reuniões); F22; F23; F24; F25 (parcial); F26; F27.01–05, 07, 08.

### 3.3 Itens com marcação ambígua
- Blocos "Entrega parcial" vivem **fora** da lista `- [ ]`. Padronizar: `[x]` feito, `[~]` parcial (com 1 linha do que falta), `[ ]` não iniciado.
- F10.F "Checklist de saída da v1" mistura critérios de fase (F01–F10) com os de F11–F27. Se a v1 = M1 (F01–F10), tirar F11–F27 dos critérios.
- INSTALADOR §13 e UI §32: todos `[ ]` por serem de aceite visual — corretos, mas dependem de snapshot comparado (não existe CI para isso hoje).

---

## 4. Ideias que não batem entre documentos

| # | Tema | Conflito | Sugestão |
|---|---|---|---|
| A | Atalho global | 3 combinações diferentes (Win+O, Win+Shift+Espaço, Ctrl+Alt+Espaço) | Fechar com **Ctrl+Alt+Espaço** (já no código; Win+ combinações são reservadas pelo Windows) e atualizar docs |
| B | Som do instalador | "Sempre ligado, sem botão" × código com mute | Manter mute (acessibilidade) e **editar a spec** |
| C | Fonte | Geist (UI) × Inter (instalador/plano) | Declarar: app = Geist/Newsreader; instalador = Inter. Corrigir F01.02 |
| D | WebView2 | Plano prevê WebView2 (navegador, MCP Apps) × "app nativo sem WebView" (PLANEJAMENTO objetivo) × Edge/Chrome via CDP | Registrar: **sem WebView2 no produto**; MCP Apps (F19.12) passa a pós-1.0 ou fica com a decisão pendente |
| E | Nuvem Claude/Codex (F12.15–F12.19) | Plano exige `claude --remote` / `codex cloud` × você escolheu "tarefas em segundo plano do Orion" | Reescrever F12.C, ou marcar F12.15–19 como "adiadas por D03" |
| F | Instalador | "Orion Setup.exe com pastas cpu/vulkan ao lado" × "instalador baixa do GitHub Releases" | Atualizar INSTALADOR §10 e F10 |
| G | Updater | "Serviço com permissão" × `orion update` subcomando + tarefa agendada | Atualizar F10.B |
| H | Duas variantes | `cpu`/`vulkan` no release × spec do instalador que não menciona variantes | Documentar o feed `latest-<variante>.json` |
| I | Licença | D07 "uso pessoal" × pendência 10 aberta | Fechar |
| J | Telegram/MCP | Plano "teloxide/rmcp" × cliente próprio | Manter próprio e atualizar plano |
| K | Autostart | Plano "janela de senha ao iniciar com o Windows" × `--background` sem senha do banco | Banco cifrado + `--background`: o app precisa esperar a senha ao primeiro abrir (hoje a tela de senha existe, mas confirmar o fluxo no Windows) |
| L | Modo foco/jogo F25.05 | Depende de F15/F18 presença | OK, só registrar a dependência |
| M | Legado | PLANEJAMENTO: "descartar Qdrant/SurrealDB, 3,09 mi vetores não migram" × ORION.md "paridade e migração verificada" | Já decidido (PLANEJAMENTO vence). Atualizar ORION.md |
| N | Python de referência | AGENTS.md manda preservar `orion/` e `tests/` | Decidir quando apagar (F27.06 só depois de F00.28/F23.13) — manter até exportar SurrealDB e vender o PC |

---

## 5. O que falta fazer

### 5.1 Depende de você **[V]** / **[D]**
1. **Chave minisign + segredos GitHub + 1º release** (`release.yml` pronto). Sem isso updater/instalador não rodam fim a fim.
2. **Testar no Windows real** tudo da seção 5.2.
3. **Exportar tabelas pessoais do SurrealDB** (F00.28/F23.13) **antes** de vender o PC (fim do ano).
4. **Gravar amostras da palavra "Orion"** (≥50 positivos, ≥1 h negativos) — hoje só existe candidato sintético reprovado.
5. **D01–D11** abertos no PLANO (ordem F20–F25, D03 termos dos provedores, D05 o que mais um Projeto tem, D06 escopo Drive/GitHub, D08 itens do legado, D10 aparelhos Alexa, D11 extensão de navegador).
6. Decisões desta auditoria: atalhos (A), som (B), nuvem (E), MCP Apps/WebView (D), `rmcp`/`teloxide` (J).
7. Certificado de assinatura de código real, se quiser evitar SmartScreen (risco 15).

### 5.2 Prova em Windows real **[W]** (nada disso rodou fora de Linux/CI)
- Instalar do zero e desinstalar limpo (máquina limpa + notebook); instalador baixando de Releases.
- Bandeja, mini barra, atalhos (Ctrl+Alt+Espaço/P), autostart `--background`, instância única, console sem piscar.
- Tela de senha do banco cifrado; Win+L; Hello (ainda sem código).
- Navegador guiado com Edge; leitura Readability em sites reais.
- Vulkan/Whisper na GPU; AEC3/barge-in; palmas com música; Telegram em 2 aparelhos.
- Metas: RAM parada < 300 MB, janela < 1 s, primeira resposta < 100 ms; 1 semana de uso com backup restaurado.

### 5.3 Código **[C]**, por fase (o que ainda não existe)
**M1 / v1 (F01–F10)**
- F01: motor de céu compartilhado (shader) para UI e instalador; Fluent pt/en; snapshots/headless comparados ao protótipo; acessibilidade (leitor de tela, foco).
- F02: Windows Hello (login + reserva do 2FA); PIN próprio para ações críticas; chave de recuperação no onboarding.
- F03: ACP (eventos das CLIs); catálogo de modelos/cotas atualizável completo.
- F04: classificador de injeção completo; revisão R01–R23 com teste; testes de `engine` e `injecao_invariantes` (hoje sem teste Rust).
- F05: diff por bloco, editar/reenviar, Mermaid/LaTeX, anexos 20 MB, `/` e `@` completos.
- F06: gravação automática no vault + resumo 23:59, conflito com Obsidian, índice e5/BM25+LSA, reranker ONNX.
- F07: Configurações — todas as 14 seções ligadas (algumas só leitura).
- **F08 Projetos (inteira):** cartões, clonar GitHub, pastas vinculadas, modelo por projeto, herança, arquivar, mover conversa.
- F09: Painel com gráficos e métricas, Resultados, Integrações, "Você fez", estados vazios.
- F10: onboarding de primeira abertura (perfil, vault, APIs, importações, pastas confiáveis, rotinas padrão), backup Google Drive (7 cópias, cifrado), mini tela "O que mudou", canais beta/stable, rollback automático se o app novo não sobe, CLIs atualizando sozinhas, lista de modelos "recomendado para o seu PC" por RAM/VRAM, checagem de GPU/Vulkan completa.

**M2 (F11–F13)**
- F11 inteira: abas Arquivos/Diff/Terminal (ConPTY), terminal compartilhado com agente, GitHub PRs/CI, revisão cruzada, extensão VS Code.
- F12: agendador em linguagem natural + prévia de 3 execuções, histórico, 3 tentativas/backoff, acordar o PC, nuvem (se mantida), monitores (página, releases, preço), proatividade (≤3/dia), retrospectiva semanal.
- F13: leitura oficial de `/usage` e `/status` (bloqueada por D03), fila de automelhoria, preempção.

**M3–M4 (F14, F15)**
- F14: PIN, fila com PC ocupado, Wake-on-LAN, botão "ver completo", `/nuvem`.
- F15: Silero VAD, modelo "Orion" final, Ctrl+Alt+V voz ao vivo, mini barra reagindo ao áudio, legenda/tradução ao vivo, botão "ouvir", Configurações › Voz completa.

**M5–M6 (F16–F18)**
- F16 grafo 3D wgpu (compute + fallback CPU), F17 bloom/blur, F18 indicadores da câmera, YOLO (hoje retorna lista vazia), "onde eu deixei", rosto validado, presença/bloqueio.

**M7–M9 (F19–F27)**
- F19: WASM/WIT, MCP Apps, rug-pull completo, UI Conectores/Descobrir, extensão de navegador (D11).
- F20 captura rápida/"Isso?"/Click to Do/OCR/memória de tela/reuniões; F21 aba de navegador no painel + pesquisa profunda `/pesquisa` com relatório em nota; F22 multi-PC/Tailscale/Alexa/iPhone; F23 Google/finanças/Vida; F24 FSRS/tutor; F25 clipboard, organizador, Spotify, modo jogo, geração de docx/pptx; F26 automelhoria; F27 auditorias OWASP/Trail of Bits, offline, desempenho, docs, release 1.0.

### 5.4 Limpezas pequenas **[C]**
- Remover do repo `Orion_Core/Front_end_Orion/`, `orion/`, `tests/`, `pyproject.toml`, `uv.lock`, `scripts/` Python quando decidir (N).
- CI: `rust-windows.yml` só roda `cargo test`; avisos de compilação já zerados — acrescentar `cargo fmt --check` e `clippy -D warnings` se quiser.
- Espelhar os documentos corrigidos no vault (`01 Projetos/Orion/`), conforme AGENTS.md (pendente: requer anexar o repo `vault`).

---

## 6. Ordem sugerida
1. ~~Corrigir os 20 documentos desatualizados~~ — feito em 20/10/2026; falta alinhar as ideias em conflito (seção 4) com as suas respostas.
2. Você: chave + release + teste Windows. **[V][W]**
3. F08 Projetos → F09 Painel → onboarding F10.C → backup.
4. F12 completa (rotinas) → F14/F15 pendências.
5. Só então F11, F16–F17 e M8.

---

## 7. Alinhamento das ideias em conflito (respostas de 20/10/2026)

| Tema (seção 4) | Decisão | Efeito |
|---|---|---|
| A. Atalhos | Manter Ctrl+Alt: Espaço = mini barra, P = pânico, **O = captura rápida, I = "Isso?"** | Docs corrigidos; **[C]** implementar Ctrl+Alt+O/I (F20) |
| B. Som do instalador | **Remover o botão de mudo** (sempre ligado) | **[C]** tirar o mute do `orion-setup` |
| Modo autônomo | **Pedir a senha** ao ligar (F04.17) | **[C]** pedir senha em `POST /modo/autonomo` e na UI |
| E. Nuvem Claude/Codex | **Implementar já** (`/nuvem claude\|codex`, F12.15–19), além do Segundo plano | **[C]** `claude --remote`/`--teleport`, `codex cloud exec/list`, `codex apply`, tela Nuvem real; só pelas CLIs oficiais |
| D. WebView2 / MCP Apps | **Sem WebView2**; navegador invisível guiado por CDP; MCP Apps só em texto, pós-1.0 | F19.12 e a aba do navegador (F21.01) saem da v1 |
| J. teloxide / rmcp | **Manter clientes próprios** | Plano editado |
| N. Python de referência | **Apagar agora** | Feito: `orion/`, `tests/` (menos `servidor_cru.py`), `pyproject.toml`, `uv.lock`, `scripts/benchmark_rust_python.py` e `gerar_icone.py` removidos; ficam no Git. Restam `scripts/{smoke_local_windows,treinar_orion_wake,wake_eval}.py` (CI e treino da palavra) e `Orion_Core/Front_end_Orion` (usado por `orion serve --features http`) |
| K. Autostart + banco cifrado | **Abrir a tela de senha** no início com o Windows | **[C]** `--background` com banco cifrado mostra a janela de senha |
| C, F, G, H, I, L, M | Resolvidos nos docs (fontes, instalador, updater, variantes, licença, legado) | — |

**Novas tarefas de código geradas por este alinhamento:** (1) tirar o mute do instalador; (2) senha ao ligar o autônomo; (3) tela de senha no autostart; (4) Ctrl+Alt+O / Ctrl+Alt+I; (5) F12.15–F12.19 (nuvem pelas CLIs oficiais). Ainda aberto: 18, leitura oficial de uso (D03).
