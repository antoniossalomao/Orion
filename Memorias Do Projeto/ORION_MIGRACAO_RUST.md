
# Orion — plano de migração para Rust

Decisão do usuário (2026-10-14): migrar tudo o que for possível para Rust, com foco em app rápido e leve. O que não couber fica em Python ou outra linguagem. Alvo: **Windows primeiro, depois macOS**. Sem Linux e sem mobile, nem compilação nem suporte.

Base de análise: matriz de portabilidade módulo a módulo (classes F, M, D, X), mantida no Vault, e branch `claude/quirky-dijkstra-65d6gq`, commit `a50f7de`.

## 1. Princípios

1. **Rust no núcleo e na UI.** Python só onde não há equivalente (hoje: browser-use e crawl4ai), por processo separado, sem HTTP.
2. **Sem servidor local.** A UI chama o núcleo por chamada direta ou canal em memória. HTTP fica só para APIs de terceiros.
3. **Paridade antes de melhoria.** Os testes Python são a especificação. Cada módulo portado precisa passar nos mesmos casos.
4. **Dados não se perdem.** O SQLite do Orion é o mesmo nos dois lados durante a transição. A migração de esquema é versionada e reversível.
5. **Medir antes e depois.** Nenhuma medição de desempenho existe hoje. A fase 0 cria a linha de base.
6. **Windows primeiro, macOS depois.** Cada módulo de plataforma tem uma interface comum e duas implementações. Só a implementação de Windows entra no primeiro marco; a de macOS é escrita depois, sem reescrever o núcleo.
7. **Fora de escopo: Linux e mobile.** Não compilar, não testar e não dar suporte.

## 2. Equivalências de plataforma

| Função | Hoje (Windows) | Windows | macOS |
|---|---|---|---|
| Início automático | registro, `autostart.py` | registro (`windows`) ou Task Scheduler | LaunchAgent (plist) |
| Bandeja | `pystray` | `tray-icon` | `tray-icon` (menu bar) |
| Teclas globais | `pynput` | `global-hotkey` | `global-hotkey` (exige Acessibilidade) |
| Área de transferência | `pyperclip` | `arboard` | `arboard` |
| Notificações | `winotify` | `windows` (toast) | `notify-rust` ou `UNUserNotificationCenter` |
| Cofre de segredos | `keyring` | Credential Manager | Keychain (`keyring`) |
| Captura de tela | `mss` | `windows-capture` / `xcap` | `xcap` ou ScreenCaptureKit (exige Gravação de Tela) |
| OCR local | `easyocr` (Python) | `Windows.Media.Ocr` (`windows`) | Vision framework (`objc2`) |
| Controle de mídia | `SendInput`, SMTC | SMTC (`windows`) | MediaRemote (privado; avaliar alternativa) |
| Janelas e processos | `pywin32`, `psutil` | `windows`, `sysinfo` | `objc2`, `sysinfo` |
| Microfone | `sounddevice` | `cpal` | `cpal` (exige Microfone) |
| Empacotamento | PyInstaller (`orion.spec`) | MSI ou NSIS, assinatura | `.app`, `.dmg`, assinatura e notarização |

**Custos de plataforma não técnicos:** conta Apple Developer (anuidade), certificado de assinatura de código no Windows (evita o alerta do SmartScreen) e teste em hardware real dos dois sistemas. Permissões do macOS (Acessibilidade, Gravação de Tela, Microfone) precisam de fluxo próprio de onboarding.

## 3. Arquitetura alvo

```
orion (app)                       binário único, Rust
├─ orion-ui         iced (Rust puro, render com wgpu), telas, paleta, tema "Observatório noturno"
├─ orion-core       agente, persona, política de risco, rotinas, projetos
├─ orion-store      rusqlite + FTS5, mesmo esquema do Python durante a transição
├─ orion-ai         provedores (reqwest + SSE), embeddings, rerank, router
├─ orion-mcp        cliente e servidor MCP (rmcp)
├─ orion-channels   Telegram (teloxide)
├─ orion-voice      cpal, whisper-rs, VAD, TTS
├─ orion-platform   trait comum + impl mac/ e win/ (ver tabela acima)
└─ orion-sidecar    ponte JSON (stdin/stdout) para o processo Python residual

orion-py (residual)  browser-use e crawl4ai, processo separado, sem servidor
```

Crates pequenos e separados, para que `orion-policy` e `orion-store` possam ser testados sem UI. A política (`orion/policy/`, ~1.100 linhas) é pura e vai primeiro.

**Escolha de toolkit de UI: iced** (decidido em 2026-10-14). Motivos: Rust puro, render com wgpu (o grafo entra como canvas nativo), licença MIT. Riscos: a API muda entre versões, então fixar a versão no `Cargo.lock` e atualizar em janelas planejadas. Alternativa de plano B: Slint, com a ponte para wgpu e licença a confirmar. `egui` fica descartado para a UI final por visual de ferramenta.

**Evidência nova (2026-10-14, a reavaliar):** o PhotoCraft (`storytold/photocraft`) é um app de edição de imagem em Rust puro, com UI em egui/eframe sobre wgpu, sem webview. O `docs/architecture.md` do repo registra essa escolha como decidida e separa o motor da UI, para trocá-la se necessário. Tem cerca de 194 mil linhas de Rust só em UI e app (com testes), 24 crates, Windows, macOS, Linux e Web, e está em alpha. Isso mostra que egui sustenta um app grande com GPU. A decisão por iced segue em vigor até você decidir o contrário.

**Segunda evidência (EffectCraft, `storytold/effectcraft`):** app de efeitos e motion graphics, em Rust, com UI em egui/eframe sobre wgpu. Tem cerca de 378 mil linhas de Rust, o dobro do PhotoCraft, com compute shaders na GPU, pré-visualização com áudio sincronizado (cpal), menus nativos (muda) e diálogos nativos (rfd). A arquitetura é em camadas (L0 a L5) com regra de build: as camadas abaixo da UI não podem depender de egui, winit, rfd, cpal ou muda. A CLI e um servidor MCP usam a mesma sessão da UI. Isso é relevante para o Orion: separação estrita entre núcleo e UI, e MCP no mesmo núcleo.

## 4. Fases

Duração: estimativa de ordem de grandeza para uma pessoa com apoio de IA. Não é compromisso; será revisada no fim de cada fase.

### Fase 0 — Preparação (1–2 semanas)
- Cumprir a verificação de export da fase 0 do Python (`orion verify-export`), com backup restaurável. Pré-requisito antes de qualquer corte.
- Medir a linha de base do Python: tempo até a janela abrir, memória ociosa, latência de primeira resposta de chat, tamanho do pacote. Registrar em tabela.
- Congelar o Python como referência e manter a suíte atual rodando.
- Definir o contrato do banco: esquema, migrações e um teste que lê o mesmo arquivo nos dois lados.
- **Saída:** linha de base registrada; backup verificado; contrato do banco escrito.

### Fase 1 — Spikes de risco (2–3 semanas)
- UI: uma tela de chat com streaming, paleta `Ctrl K`, tema e o grafo em wgpu, em iced. Medir memória e latência.
- Núcleo: `rusqlite` com FTS5 lendo o banco real do Orion; uma busca de memória comparada à do Python.
- Embeddings: BGE-M3 via `ort` ou `candle`. Comparar vetores e resultados de busca com o Python na mesma base. Se a qualidade cair, a decisão muda.
- Plataforma (Windows): tray, teclas globais e notificação funcionando. Macos só compila no fim do spike, sem testes de runtime.
- Áudio: captura com `cpal` e VAD mínimo.
- **Saída:** cinco resultados com número, e uma decisão de toolkit de UI registrada no vault.

### Fase 2 — Núcleo de domínio (4–6 semanas)
- `orion-policy`: portar `policy/` com os mesmos casos de teste.
- `orion-store`: memória, fatos, sessões, projetos, artefatos, rotinas. Usar o mesmo esquema.
- `orion-ai`: gateway com streaming, provedores, router e custo zero.
- Agente: persona, ferramentas nativas de `tools/builtin.py`, política de confirmação.
- **Saída:** `/chat` equivalente funcionando sem UI (teste de linha de comando) com paridade nos testes de `tests/policy`, `tests/memory`, `tests/test_router`.

### Fase 3 — Interface (4–6 semanas)
- Telas do primeiro marco: Chat, Memória, Atividade, Configurações, paleta `Ctrl K`, aprovações.
- Replicar o padrão visual atual: tokens, três temas, céu de Órion, aviso âmbar só para atenção.
- Teclado e acessibilidade (o front atual tem teste de axe; manter o critério).
- Cortar telas do primeiro marco: `extensions`, `mcp`, integrações avançadas entram depois.
- **Saída:** primeiro marco usável no Windows, com a mesma operação do front atual nas telas incluídas.

### Fase 4 — Plataforma e rotinas (3–4 semanas)
- Início automático, bandeja, teclas globais, área de transferência, notificações, cofre, observador de pastas (Downloads) em `orion-platform`.
- Rotinas e agendador (`rotinas.py`, `jobs.py`), briefing matinal, backup.
- Captura de tela e OCR nativo por plataforma (tabela acima).
- **Saída:** testes de plataforma no Windows; rotinas executando com o app fechado na bandeja.

### Fase 5 — Voz (3–5 semanas, com ponto de decisão)
- Captura, VAD, STT com `whisper-rs`, voz ao vivo por websocket.
- **Decisão:** palavra de ativação ("Orion") e detector de palmas. Ambos são D na matriz. Opção: adiar para depois do corte.
- TTS: avaliar WinRT/AVSpeechSynthesizer nativos antes de reimplementar o protocolo do edge-tts.
- **Saída:** comando por voz funcionando nos dois sistemas; decisão registrada sobre palmas e palavra de ativação.

### Fase 6 — Integrações (3–4 semanas)
- MCP cliente e servidor (`rmcp`), extensões e plugins.
- Telegram (`teloxide`), Google Agenda e Gmail (REST + OAuth).
- Fontes, documentos, conversor (ffmpeg e LibreOffice continuam externos).
- **Saída:** integrações ativas com as mesmas credenciais do cofre.

### Fase 7 — Residual Python (1–2 semanas)
- `orion-py` com browser-use e crawl4ai, falando JSON por stdin/stdout, sem porta aberta.
- Mínimo de dependências e empacotamento embutido (ou exigir Python instalado; decidir).
- **Saída:** navegação com IA funcionando por sidecar; medir o custo de memória do processo extra.

### Fase 8 — Corte (2–3 semanas)
- Migração de dados verificada com o mesmo teste da fase 0.
- Pacotes: MSI ou NSIS assinado no Windows. O `.dmg` assinado e notarizado para macOS fica para a fase 9, depois deste corte.
- Repetir a medição da fase 0 e comparar com a linha de base.
- Manter o Python instalável por uma versão como rollback. Remover depois.
- **Saída:** versão 1.0 Rust no Windows; relatório de comparação de desempenho.

### Fase 9 — macOS (a definir)
- Implementações de plataforma para macOS (tabela da seção 2), permissões do sistema, `.dmg` assinado e notarizado.
- Começa só depois do corte de Windows. Duração decidida no fim da fase 8.

## 5. Métricas de sucesso

| Métrica | Como medir | Meta inicial |
|---|---|---|
| Tempo até janela pronta | cronômetro de inicialização, 10 execuções | menor que a linha de base; meta a definir na fase 0 |
| Memória ociosa | processo principal e sidecar somados | menor que a linha de base |
| Latência de primeira resposta | do envio até o primeiro token, com provedor fixo | igual ou melhor; a rede domina |
| Tamanho do pacote | `.dmg` e instalador | menor que o PyInstaller atual |
| Paridade | testes portados passando | 100% dos testes do primeiro marco |

## 6. Riscos

| Risco | Probabilidade | Impacto | Mitigação |
|---|---|---|---|
| Toolkit de UI não atende o visual | média | alto | spike na fase 1; plano B com Slint |
| API do iced muda entre versões e quebra o código | alta | médio | fixar versão; atualizar só em janelas planejadas |
| Embeddings em Rust com qualidade menor | média | alto | comparar na fase 1 na mesma base; manter Python para embeddings se preciso |
| Permissões do macOS bloqueiam captura, teclas ou microfone | alta | médio | onboarding de permissões; testar cedo |
| Escopo cresce (fases 4–6 ficam maiores que o estimado) | alta | alto | marco de corte; telas e integrações fora do primeiro marco |
| Perda de dados na migração | baixa | crítico | backup verificado; esquema compartilhado; rollback |
| Dependência de browser-use em Python | certa | médio | sidecar isolado; aceitar o runtime extra |
| Custo de conta Apple e certificado Windows | certa | baixo | orçar antes da fase 8 |
| Sem Linux, sem CI de Linux para o app | — | — | CI em mac e win; testes de núcleo podem rodar em qualquer máquina |

## 7. Decisões

### Registradas (2026-10-14)
- **Grafo 3D:** wgpu. Mantém o grafo em 3D, desenhado nativamente. Impacto: a fase 3 ganha um item de renderização própria; prazo maior.
- **Palmas e palavra de ativação:** entram no primeiro marco. Impacto: a fase 5 deixa de ser opcional e vira obrigatória no marco; os itens D da matriz precisam de spike cedo, na fase 1.
- **Navegação com IA (Python):** aceita Python instalado como dependência, mantendo o mínimo possível de código Python. Impacto: o sidecar fica restrito a browser-use e crawl4ai; nada mais migra para Python.
- **Plataforma:** Windows primeiro, macOS depois. Impacto: a abstração de plataforma continua obrigatória desde a fase 0, para que o mac não exija reescrita. Build, assinatura e teste no mac ficam para depois do marco de Windows.
- **Escopo:** Linux e mobile fora de escopo. Não compilar, não testar e não dar suporte para nenhum dos dois.

### Em aberto
1. **Telas do primeiro marco:** confirmar a lista da fase 3.

*Toolkit de UI decidido: iced (ver seção 3).*

## 8. Próximos passos

- [x] Decidir o toolkit de UI: iced
- [ ] Fase 0: verificar export e medir a linha de base do Python
- [ ] Fase 1: spike de UI em iced com chat, paleta e grafo em wgpu
- [ ] Fase 1: spike de palmas e palavra de ativação no Windows
- [ ] Fixar a versão do iced e registrar no `Cargo.lock`

## Fontes
- Repositório `antoniossalomao/Orion`, branch `claude/quirky-dijkstra-65d6gq`, commit `a50f7de`: `orion/`, `Orion_Core/Front_end_Orion/`, `requirements.txt`, `Memorias Do Projeto/ORION_CORTE.md`, `Memorias Do Projeto/ORION_TECNICO.md`.
