---
type: recurso
projeto: "[[Orion]]"
tags: [orion, implementacao, plano, checklist, rust, iced, roadmap]
date: 2026-10-16
resumo: "Plano de implementação em etapas do Orion em Rust, cobrindo todas as decisões de ORION - PLANEJAMENTO ARQUITETURA sem exceção: marcos M0 a M9, 28 fases com checklist, dependências, testes, critérios de aceite, riscos, resolução das decisões conflitantes e matriz de cobertura."
---

# ORION - PLANO DE IMPLEMENTAÇÃO

> Projeto: [[Orion]] · Fonte das decisões: [[ORION - PLANEJAMENTO ARQUITETURA]] · Instalador: [[ORION - INSTALADOR (ESPECIFICAÇÃO)]] · Interface: [[ORION - UI (ESPECIFICAÇÃO)]] · Referências: [[ORION - REFERÊNCIAS DE PESQUISA]] · Regras do projeto: [[Orion - Regras]] · Estado do legado: [[Orion - Estado e plano]] · Conceito: [[Rust]]
> **Como usar:** cada fase tem objetivo, dependências, tamanho, checklist (`- [ ]`), testes e critério de aceite. Marque o item só quando o critério de aceite da fase correspondente passar. O repositório do código é `antoniossalomao/Orion` (PRs com revisão cruzada, ver F00).

---

> **Legenda dos itens (20/10/2026):** `[x]` feito no código, sem depender de prova em hardware ou Windows real; `[~]` parcial, ou feito mas só compilado/testado no Linux, sem prova num Windows real; `[ ]` não iniciado; `[-]` cancelado por decisão (o motivo está no próprio item). Conferido item a item contra o código em 20/10/2026, com evidência de arquivo por item.

> **Evidência de status:** `[~]` não distingue código incompleto de código aguardando aceite. Ao atualizar um item, informe **Implementação**, **Testes** (com comando/commit/resultado) e **Windows** (aceite ou pendência), além do próximo passo. Não reclassifique itens por esta revisão documental.

## 0. Visão geral

### 0.1 Marcos
| Marco | Nome | Fases | Resultado |
|---|---|---|---|
| **M0** | Fundação | F00 | Repositório, CI, spikes com decisão de ir/não ir, riscos medidos |
| **M1** | **v1** | F01 a F10 | Chat completo, aprovações e modo autônomo, nota no vault com commit, Configurações completas, Projetos, login e instalador |
| **M2** | v1.1: trabalho e controle | F11 a F13 | Ferramentas de código, rotinas e automações, orçamento de cotas |
| **M3** | Telegram | F14 | Celular pelo Telegram, aprovações remotas, comandos do PC |
| **M4** | Voz | F15 | Palavra "Orion", palmas, voz ao vivo, mini barra, TTS |
| **M5** | Grafo e efeitos | F16, F17 | Grafo 3D da memória, bloom e blur |
| **M6** | Câmera e visão | F18 | Dois modos de câmera, "onde eu deixei?", rosto, legenda |
| **M7** | Extensões | F19 | MCP completo, skills, plugins WASM, MCP Apps, importações, clientes externos |
| **M8** | Funções novas e integrações | F20 a F25 | Memória de tela, reuniões, Click to Do, Alexa e casa, iPhone, finanças, estudo, utilidades, documentos |
| **M9** | Automelhoria e fechamento | F26, F27 | Automelhoria, auditorias finais, release 1.0 |

> A ordem vigente após a v1 é F25.06 (documentos), depois F20, conforme a rodada de 20/10/2026. Os demais marcos respeitam as dependências; a sequência antiga Telegram → Voz → Grafo → Câmera não é mais a ordem global.

> **Terminologia:** "v1" é a entrega M1 (F01–F10), com saída em F10. F27 é o fechamento posterior do roadmap; a expressão histórica "release 1.0" em F27 não amplia o escopo da v1 nem define a tag do primeiro release. Itens explicitamente pós-1.0 ficam fora desse primeiro lançamento.

### 0.2 Tamanhos (referência, sem datas: "prazo do primeiro marco: sem data")
**S** = até 3 dias · **M** = ~1 semana · **L** = 2 a 3 semanas · **XL** = 1 mês ou mais. Um desenvolvedor, com Claude e Codex ajudando.

### 0.3 Mapa de dependências (resumo)
```
F00 ─► F01 ─► F02 ─► F03 ─► F04 ─► F05 ─► F06 ─► F07 ─► F08 ─► F09 ─► F10 ═► v1
        └──────────────► (F01 também alimenta tudo que tem tela)
F10 ─► F11 ─► F12 ─► F13
F10 ─► F14 (Telegram; liga o 2FA) ─► F15 (Voz) ─► F16 (Grafo) ─► F17 (Efeitos) ─► F18 (Câmera)
F10 ─► F19 (Extensões) ; F12 ─► F20..F25 ; F13 + F19 ─► F26 (Automelhoria) ─► F27
```


### 0.4 Resumo das fases
| Fase | Nome | Marco | Tamanho | Depende de |
|---|---|---|---|---|
| F00 | Preparação, repositório e spikes | M0 | L | nenhuma |
| F01 | Janela, tema e base da interface | M1 | L | F00 |
| F02 | Dados, segurança e login | M1 | L | F00, F01 |
| F03 | Provedores, roteador e modelos locais | M1 | XL | F00, F02 |
| F04 | Agente, ferramentas, permissões e aprovações | M1 | XL | F02, F03 |
| F05 | Chat completo | M1 | XL | F01, F03, F04 |
| F06 | Vault e memória (v1) | M1 | XL | F02 a F05 |
| F07 | Configurações completas | M1 | L | F01 a F06 |
| F08 | Projetos | M1 | L | F06, F07 |
| F09 | Painel e telas auxiliares | M1 | M | F03, F04, F06 |
| F10 | Instalador, atualizador, onboarding e release v1 | M1 | XL | F01 a F09 |
| F11 | Ferramentas de código | M2 | L | F05, F08 |
| F12 | Rotinas, automações, nuvem e monitores | M2 | XL | F04 a F08 |
| F13 | Orçamento de uso | M2 | L | F03, F09 (D03 decidido) |
| F14 | Telegram | M3 | L | F04, F12 |
| F15 | Voz | M4 | XL | F03 a F05 |
| F16 | Grafo 3D da memória | M5 | XL | F06, F01.29 |
| F17 | Efeitos visuais | M5 | M | F16 |
| F18 | Câmera e visão | M6 | XL | F04, F06, F15 |
| F19 | Extensões | M7 | XL | F04, F05, F07 |
| F20 | Tela, captura e reuniões | M8 | XL | F04, F06, F15 |
| F21 | Web e pesquisa | M8 | L | F04, F05, F12 |
| F22 | Vários PCs, rede, casa e celular | M8 | XL | F02, F10, F12, F14 |
| F23 | Vida, Google e finanças | M8 | XL | F06, F12, F22 |
| F24 | Estudo e conhecimento | M8 | L | F06, F12 |
| F25 | Sistema, utilidades e documentos | M8 | L | F04, F12, F15 |
| F26 | Automelhoria | M9 | XL | F11, F12, F13, D03 |
| F27 | Auditoria final, documentação e release 1.0 | M9 | L | todas |

**Paralelismo possível (um desenvolvedor + Claude/Codex):** F01 e F02 andam juntas; F07 e F09 podem começar quando F03 e F04 estiverem estáveis; F16 e F15 não dependem uma da outra (a ordem Telegram → Voz → Grafo → Câmera é preferência, não dependência técnica).

---

### 0.5 Auditoria de implementação do PR #30 — 17/10/2026

Referência: `2020015`, base `main` em `a6a27d6`. Inventário, evidências e bloqueios detalhados estão na seção **Auditoria consolidada do PR #30** de [ORION.md](ORION.md), fonte única do estado. Esta seção distingue o plano desejado do código existente; não muda as decisões de produto por inferência.

**Como ler as marcações:** os checkboxes originais F/R/K/W continuam sendo **aceite completo**, incluindo testes e integrações exigidos. Item aberto com anotação **Parcial** já tem trabalho implementado. Os checkboxes abaixo registram somente subentregas comprovadas; não significam que a fase ou o marco inteiro foi aceito. Nenhuma das 28 fases demonstrou todo o seu aceite. A v1 não está pronta.

- [x] Workspace de 14 crates, lockfile e iced 0.14.0 fixado.
- [x] Portes do domínio/store/AI/MCP/core/API/CLI e 334 declarações de testes de integração de paridade.
- [x] Controle iced auxiliar Windows, bandeja, hotkeys, pânico e autostart opt-in.
- [x] Orçamento persistido, calibragem/reset/projeção e tetos 95% semanal/20% por tarefa em chamadas estimadas.
- [x] Telegram default-deny, aprovações inline, 2FA por código e áudio.
- [x] Captura cpal, palmas, openWakeWord configurável, Whisper local, TTS, AEC3 e barge-in opt-in.
- [x] Worker llama.cpp sob demanda com teto Windows de 30% da RAM e instalação do modo local pelo setup.
- [x] Visual do instalador, modelos com hash/retomada/cancelamento e rollback de arquivos novos.
- [x] Crate vault com parser/scanner/Git e API explícita; gravação automática/conflitos ainda pendentes.
- [x] Câmera nokhwa e extrator Haar heurístico/LBPH; reconhecimento físico ainda não validado.
- [x] Ponte sidecar Python testada isoladamente; integração ao agente ainda pendente.
- [x] Benchmark comparativo e limpeza de 79 arquivos obsoletos/PyInstaller.

| Fase | Estado auditado | Entregue e restante |
|---|---|---|
| F00 | **Parcial** | Workspace, lock, iced e CI Windows existem; faltam spikes/medidas, export real, toolchain/deny/audit/evals/snapshots. |
| F01 | **Parcial** | Controle auxiliar com bandeja/atalhos e céu do instalador; app principal, sidebar, paleta, Fluent, Mica e aceite visual ainda faltam. |
| F02 | **Parcial** | PBKDF2, sessão revogável, cofre, 2FA Telegram e auditoria portados; SQLCipher/Argon2id/Hello/recuperação/PII/log cifrado não entregues. |
| F03 | **Parcial** | Gateway/CLIs diretas/router e worker llama.cpp instalável feitos; ACP, UX completa, embeddings/visão locais e build/medição Vulkan pendentes. |
| F04 | **Parcial** | Agente, ferramentas e aprovações portados; paridade de política/agente, autônomo do plano, checkpoints, subagentes e notificações Windows com botões pendentes. |
| F05 | **Parcial** | Chat SSE, ramos e artefatos existem no núcleo; o chat do app iced (`orion-app`) está ligado a ele. O front HTML antigo foi removido em 21/10/2026. Faltam os itens F05 listados, E2E e snapshots comparados ao protótipo. |
| F06 | **Parcial** | Crate vault e rotas explícitas existem; não há gravação automática por turno, resolução de conflito sem sobrescrever ou índice local completo. Não marcar F06 inteira concluída. |
| F07 | **Parcial** | Há configuração por ambiente/.env/JSON e painel de orçamento; não há modal completo, TOML com recarga ou todas as seções nativas. |
| F08 | **Parcial** | Projetos/documentos/ramos e APIs portados; telas iced, GitHub, nota do projeto e herança/arquivamento coordenados pendentes. |
| F09 | **Parcial** | Dados do painel e telas web de resultados/arquivadas existem; telas iced, métricas e resumo diário do plano pendentes. |
| F10 | **Parcial** | Setup visual, cópia real CPU/Vulkan, modelos e rollback de arquivos existem; pacote/certificado/atalhos/serviço ainda são placeholders; updater, assinatura, onboarding e desinstalador faltam. |
| F11 | **Pendente como entrega** | Shell/Git/processos genéricos não concluem esta fase: diff por bloco, ConPTY, GitHub/CI, ACP e extensão VS Code ainda faltam. |
| F12 | **Parcial** | Jobs, rotinas concedidas, Downloads, briefing e pesquisa portados; agendador completo, nuvem, monitores e proatividade ainda faltam. |
| F13 | **Parcial** | Registro/calibragem/reset/projeção e tetos 95%/20% feitos em chamadas estimadas; leitura oficial, precisão, fila e preempção pendentes. |
| F14 | **Parcial** | Bot HTTP próprio, default-deny, cartões inline, 2FA e áudio feitos; não usa teloxide. PIN/Hello, nuvem, fila, energia/WoL e dois aparelhos reais pendentes. |
| F15 | **Parcial** | cpal, VAD por energia, palmas, Whisper, TTS, AEC3 e barge-in integrados; Silero, wake word final, testes acústicos/GPU e UI completa pendentes. |
| F16 | **Parcial só nos dados** | Backend de grafo/parser e grafo WebGL existem; nenhum renderer/layout/compute nativo wgpu ou aceite de desempenho entregue. |
| F17 | **Pendente** | Céu do instalador não implementa bloom/blur do grafo nem qualidade/bateria automáticas desta fase. |
| F18 | **Parcial** | Câmera nokhwa e extrator Haar heurístico/LBPH adicionados em 975e635; detector YOLO ainda retorna vazio. Indicadores, reconhecimento real, UI e aceite pendentes. |
| F19 | **Parcial** | MCP próprio, OAuth, skills/plugins/importação e credenciais de leitura existem; rmcp não é usado. WASM/WIT, MCP Apps, proteção completa contra rug pull e UI final pendentes. |
| F20 | **Parcial** | Ponte/captura, memória de tela, OCR externo e legenda portados; seleção/overlays nativos, PII visual, UI Automation e reuniões pendentes. |
| F21 | **Parcial** | Busca/fetch e pesquisa noturna portados; sidecar tem testes isolados, sem consumidor no agente. Navegador visível/CDP e pesquisa completa pendentes. |
| F22 | **Pendente** | Sync multi-PC, ACL Tailscale/exit node, WoL, casa/Alexa, Siri, Phone Link e Saúde não entregues. |
| F23 | **Parcial** | Vida/finanças, OAuth/agenda/MCP e captura de gastos portados; integrações reais, importadores bancários completos e semana de uso pendentes. A tela Vida foi cortada em 21/10/2026. |
| F24 | **Pendente** | FSRS, tutor, progresso, conexões e fluxo de estudo ainda não implementados como entrega. |
| F25 | **Parcial** | Históricos, Downloads, apps/mídia/documentos/conversor/planos portados; UI, Lixeira/desfazer completo, Spotify, jogo/economia e PowerPoint pendentes. |
| F26 | **Pendente** | Orçamento e tarefas restritas não são automelhoria: faltam fila, termos, execução por PR/evals e fluxo de interrupção. |
| F27 | **Parcial documental** | Memórias/vault e remoção do código obsoleto feitos; auditorias finais, migração/restauração, descarte no PC e release 1.0 não realizados. |

**Validação:** em `d069b17`, 1.564 testes Python passaram (3 ignorados), 16 arquivos Node passaram e 388 testes Windows nos alvos do CI passaram; build e GGUF CPU reais passaram. O log Vulkan registra CMake/MSB8066 (`cmake_install.cmake` ausente) antes do cancelamento; sem pacote final validado. O run [39086573378](https://github.com/antoniossalomao/Orion/actions/runs/39086573378) do código `2020015` já passou em testes/build/smoke CPU e estava no build Vulkan na última consulta. Detalhes e links em ORION.md.

**Correção de identificação:** a linha de navegação que duplicava `F07.08` passa a `F07.24`; `F07.08` permanece a seção Geral.

**Divergências do plano:** SQLite atual não usa SQLCipher/sqlite-vec; login já usa Argon2id e o banco já tem SQLCipher (opt-in por `orion db-cifrar`); APIs/canais são HTTP/JSON próprios (sem ACP/rmcp/teloxide); céu do setup é imagem RGBA produzida na CPU; o instalador baixa o pacote do GitHub Releases (fallback offline com pastas CPU/Vulkan); o sidecar Python foi removido (C11 atingido em 20/10/2026), restando só scripts Python de treino/benchmark fora do produto. Essas diferenças não encerram os requisitos correspondentes.

---

## 1. Decisões conflitantes e como foram resolvidas
O planejamento acumulou decisões ao longo de várias rodadas de perguntas. Onde duas se contradizem, **vale a mais recente**, e o item fica marcado para você confirmar. Cada linha tem a fase afetada.

| # | Tema | Decisões em conflito | **Resolução usada neste plano** | Fase |
|---|---|---|---|---|
| C1 | Fonte da interface | Inter/JetBrains Mono × protótipo Grafite | App: **Geist, Geist Mono e Newsreader** (F01.02). Instalador: **Inter**. | F01, F10 |
| C2 | Transcrição de voz | Whisper na Groq, envia áudio (15/10) × Whisper local (16/10, rodadas 2 e 10) | **Whisper large-v3-turbo local** (Vulkan). Groq Whisper vira **opção** para PC sem GPU, desligada por padrão. | F15 |
| C3 | Voz ao vivo | Gemini Live (15/10) × pipeline STT → LLM → TTS, sem full-duplex local (rodada 10) | **Pipeline** é o padrão; **Gemini Live** entra como modo opcional no atalho Ctrl+Alt+V quando houver internet. | F15 |
| C4 | TTS | edge-tts voz masculina pt-BR (15/10) × "testar depois" (rodada 2) | **edge-tts masculina** por padrão, atrás de uma interface trocável; teste de alternativas na F15. Sem voz offline. | F15 |
| C5 | Busca do vault | BM25 + LSA × FTS5 + sqlite-vec | **BM25 + LSA**, por decisão de 20/10/2026. Embeddings locais só se a busca falhar. | F06 |
| C6 | Embeddings | API gratuita × modelo local | **Opcionais e locais**, se BM25 + LSA não atender. Não são requisito padrão da v1; eventual API exige opt-in e nunca recebe notas privadas ou pastas bloqueadas. | F06 |
| C7 | Verificação de atualização | SHA-256 × minisign × certificado | Feed com **minisign + SHA-256**. Certificado autoassinado é decisão de distribuição separada; não elimina o aviso do SmartScreen. O workflow atual assina o feed, não demonstra assinatura Authenticode dos executáveis. | F10 |
| C8 | Tentativas de senha | Bloqueio de 5 min após 5 erros (15/10) × atraso crescente 1, 5 e 15 min + aviso no Telegram (rodada 3) | **Atraso crescente** + aviso no Telegram quando configurado. | F02 |
| C9 | 2FA pelo Telegram × Telegram só depois da v1 | O 2FA exige o bot | Na v1: **senha + Windows Hello**. O **2FA por Telegram liga quando o Telegram for configurado** (F14). Hello continua como reserva. | F02, F14 |
| C10 | Ordem pós-v1 | "Marco 1 além do Chat: Memória com grafo" e F4+F5 juntas (15/10) × Telegram → Voz → Grafo → Câmera (16/10) | **Memória (vault + índice) na v1; grafo 3D só no M5**, depois de Telegram e Voz. | F06, F16 |
| C11 | Python no projeto | Sidecar × Rust | **Sem Python no produto**. Readability e CDP em Rust; sidecar removido em 20/10/2026. Scripts de treino/smoke e fixture MCP permanecem fora do runtime do produto. | F12, F21 |
| C12 | Modo foco | "Não entra" (15/10) × "só lembrete gentil" (rodada 9) | **Lembrete gentil**, sem bloquear nada. | F25 |
| C13 | Gravação de reunião | "Não na v1" (15/10) × detecta a chamada e grava (rodada 9) | Consistente: **depois da v1** (F20). | F20 |
| C14 | Bateria do notebook | "Sem mudança" (15/10) × modo economia automático (rodada 3) | **Modo economia automático.** | F01, F17 |
| C15 | Controle do PC | Aprovação a cada ação (15/10) × livre no modo autônomo (rodada 3) | Fora do autônomo: **aprovação por ação** (leve dispensada, ver F04); no autônomo: livre, **menos a linha "sempre pedir"**. | F04 |
| C16 | Sync PC↔notebook | cr-sqlite × log de eventos | **Log de eventos próprio**, decidido em 20/10/2026. | F00, F22 |
| C17 | Auditoria diária do vault | Corrige links e frontmatter óbvios (15/10) × `AGENTS.md` do vault diz "só lista" | Ajustar o `AGENTS.md` do vault **antes** de ligar a correção. | F26 |
| C18 | Teto de gasto por tarefa | 20% por tarefa × 95% semanal | **Dois tetos:** 95% semanal global e 20% por tarefa, limitado pela janela opcional de 5 h. D03 decidiu a leitura; a execução de automelhoria depende de F11/F13 (a verificação dos termos F00.29/F03.26 foi feita em 21/10/2026). | F13, F26 |
| C19 | Memória da tela | Retenção de 7 dias, opt-in (REGRAS.md) × ~1 GB/mês, só texto (rodada 9) | **Só texto, opt-in, retenção de 7 dias por padrão**, configurável. | F20 |
| C20 | Modo "voz ao vivo" e ferramentas | REGRAS.md: voz ao vivo só conversa, sem ferramentas × função de comandos por voz | **Voz ao vivo (Gemini Live) só conversa.** Comandos por voz passam pelo pipeline normal, com política. | F15 |
| C21 | Reconhecimento facial | Haar + LBPH, sem IA (15/10) × ArcFace via ONNX (pesquisa rodada 10, não decidida) | **Haar + LBPH** como decidido; ArcFace só se o spike da F18 mostrar falso positivo alto. | F18 |
| C22 | WebView | Exceção para conteúdo interativo × app nativo | **Sem WebView2 no produto**, decisão de 20/10/2026. Mermaid/HTML exigem solução compatível; MCP Apps só em texto, pós-1.0. | F05, F19 |
| C23 | macOS | Depois do corte do Windows (14/10) × "macOS fica para o futuro" (15/10) | **Fora deste plano.** Abstração de plataforma desde a F00; porte só depois da 1.0. | F00 |
| C24 | Notificação com botões × instalador | MSIX puro (rodada 10) × Setup.exe com pacote esparso (rodada 11) | **Setup.exe + pacote esparso** (identidade de pacote). | F10 |
| C25 | Backup no Drive | "Só banco e configurações", diário, 7 cópias × "Drive inteiro" como acesso | Backup **só de banco e configurações**; a integração com o Drive tem **escopo de leitura do Drive inteiro**, **separado** do backup. Reavaliar o escopo. | F10, F23 |
| C30 | Automelhoria | Ligada na v1 × dependências posteriores | **Execução pós-v1**, depois de F11 e F13, conforme decisão de 20/10/2026. Interruptor/tela não comprovam fila nem execução. | F13, F26 |
| C26 | Marca do instalador "menor possível" | Instalador mínimo × ~100 a 200 MB estimados | Mantém o corte: **só o essencial** no Setup; modelos sob demanda. | F10 |
| C27 | Documentos Word, Excel e PowerPoint | Na v1 × pós-v1 | **F25.06 logo depois da v1, antes de F20**, decidido em 20/10/2026. | F25 |
| C28 | Navegador agente | Aba WebView2 × CDP externo | **Edge/Chrome headless guiado por CDP**, sem WebView2 e sem aba no painel. Decidido em 20/10/2026; falta aceite com Edge no Windows real. | F21 |
| C29 | OCR | "Via API" (15/10) × "nada pessoal sai" (R19, rodada 4) | **OCR local primeiro**; API só se você ligar, com máscara de dados antes do envio. | F20 |

> **Confirmações ainda necessárias:** C3, C9, C10 (ordem relativa dos marcos restantes), C25 e C29, onde não houver decisão posterior explícita. C1, C5/C6, C16, C27, C28 e C30 seguem as decisões de 20/10/2026; C18/D02 foi confirmado em 17/10/2026.

---

## 2. Convenções que valem para todas as fases

### 2.1 Regras herdadas do Orion em Python ([[Orion - Regras]])
Todas continuam valendo na versão em Rust. Cada regra vira **teste** (princípio sem teste é desejo) e é citada pelo nome no código.
- [ ] **R01** Você é o administrador; nenhum raciocínio do modelo anula uma decisão sua.
- [ ] **R02** **Núcleo imutável:** o arquivo de regras, a persona e o crate `orion-policy` não são editados por nenhuma ferramenta; mudança só com aprovação por canal autenticado.
- [ ] **R03** **Classe de risco fixa por ferramenta**, vinda do registro e não do texto da chamada (leitura, escrita, execução, destrutiva). Ferramenta sem classe é negada.
- [ ] **R04** **Confirmação fora de banda** (botão no app ou no Telegram), nunca por frase na conversa; vale para a sessão, a ferramenta e os argumentos exatos, uso único, por 10 minutos.
- [ ] **R05** **Conteúdo externo é dado, não instrução** (web, e-mail, documento, área de transferência, descrição de MCP). Depois de lido, escrita e execução da sessão pedem confirmação, inclusive após reiniciar.
- [ ] **R06** **Segredos nunca em log, auditoria ou processo filho.** Logs mascaram chaves e tokens; CLIs delegadas não herdam variáveis `ORION_*`.
- [ ] **R07** Ler segredo ou chave (`.env`, `.ssh`, `*.pem`, credenciais OAuth) pede confirmação; o caminho é resolvido antes (symlink para `.env` continua `.env`).
- [ ] **R08** Shell só roda sem confirmação o que for **provadamente leitura** (lista positiva, sem redirecionamento, substituição ou bloco de script); o resto confirma.
- [ ] **R09** URL escolhida pelo modelo só vai a host público, com cada redirecionamento revalidado; o IP conferido é o IP usado (sem janela de DNS rebinding).
- [ ] **R10** **Falha de auditoria nega a ação** que não é leitura (fail-closed); toda decisão vai para a tabela de auditoria.
- [ ] **R11** **Só se aprova o que se vê inteiro**: argumento cortado ou grande demais para o cartão só pode ser negado.
- [ ] **R12** MCP: a classe de risco vem da configuração do Orion, nunca do servidor; sem classe, é "execução". O servidor sobe sem variáveis `ORION_*` nem segredos.
- [ ] **R13** O que o modelo escreve nunca vira comando: texto de notificação, nome de app e conteúdo da área de transferência entram como **argumento**, não na linha de comando.
- [ ] **R14** Destino de rede escolhido pelo modelo confirma depois de conteúdo externo, com a URL inteira no cartão; vale só para aquele endereço.
- [ ] **R15** Título de janela nunca vira comando; controlar ou fechar janela é execução e confirma sempre; busca que acha mais de uma janela não age.
- [ ] **R16** Imagem gerada tem nome criado pelo código e tipo conferido pela assinatura do arquivo; o chat só renderiza imagem gerada pelo próprio Orion.
- [ ] **R17** Painel só lê e nunca mostra segredo.
- [ ] **R18** **Voz:** aprovar nunca é por voz; a palavra de ativação só abre o microfone (detecção local); palmas só abrem; a fala transcrita aparece antes da resposta; voz ao vivo só conversa, sem ferramentas nem memória.
- [ ] **R19** Tela e imagem não saem do computador sem seu aval; não há captura contínua; memória da tela só texto, local, opt-in; janela excluída, sem título ou bloqueada não é capturada.
- [ ] **R20** **O que é de um projeto fica no projeto** (fatos, documentos, conversas, avisos); a memória pessoal não enxerga o projeto e um projeto não enxerga outro.
- [ ] **R21** Toda saída de dado para fora fica registrada **sem o conteúdo**: provedor, tipo, bytes, resultado, latência. Registrar nunca derruba a chamada.
- [ ] **R22** Telegram: default-deny, só o seu ID, conversa privada, aprovação só por botão.
- [ ] **R23** Custo zero: só cotas gratuitas e as assinaturas oficiais, pelas CLIs oficiais; nenhum token extraído.

### 2.2 Padrões de código
- [ ] **K01** Workspace Cargo com um crate por responsabilidade (lista em F00.02); `thiserror` nas bibliotecas, `anyhow` no binário; `tracing` com arquivo rotativo, spans por conversa e por ferramenta.
- [ ] **K02** `rusqlite` com migrações versionadas; nenhuma consulta SQL fora de `orion-store`.
- [ ] **K03** Nomes de código em **inglês**, comentários em **português**; mensagens ao usuário nos arquivos **Fluent** (pt-BR e en), nunca fixas no código.
- [ ] **K04** Nada de `unwrap`/`expect` em caminho de produção; `clippy` limpo (recomendado, não bloqueante, conforme decisão de CI).
- [ ] **K05** Toda ferramenta nova declara: nome, descrição, esquema de argumentos, **classe de risco**, escopo de dados, tempo limite, como desfazer.
- [ ] **K06** Toda função nova com UI tem: estado vazio (ilustração do cinturão pequeno + 1 ação), estado de erro (cartão no chat ou aviso), foco de teclado, atalho (quando fizer sentido), texto em pt e en.
- [ ] **K07** Módulos pesados (voz, grafo, visão, índice) carregam **sob demanda** e descarregam ao sair do uso.
- [ ] **K08** Documentação do código em **markdowns bem estruturados e organizados** (pasta `docs/`), mais `rustdoc` nas APIs públicas dos crates.
- [ ] **K10** Runtime assíncrono **Tokio** em todo o núcleo (o iced integra); nada de bloqueio na thread da interface.
- [ ] **K09** Atualização de dependências: **PR semanal automático**; `cargo deny` e `cargo audit` em cada PR (bloqueia vulnerabilidade alta).

### 2.3 Fluxo de trabalho e CI
- [ ] **W01** Branch por fase, **PR obrigatório** no repositório do Orion, **revisão cruzada** (Claude Code Review + Codex) e a sua; nada de merge automático.
- [ ] **W02** CI obrigatório no PR: build Windows, testes, **testes headless/E2E do iced 0.14**, **snapshots PNG só das telas alteradas**, `cargo deny`/`cargo audit`.
- [ ] **W03** Releases: `main` + tags semver; **stable no começo**, beta quando o notebook entrar (decisão de 20/10/2026).
- [ ] **W04** Evals do agente rodam no CI (harness próprio em Rust + casos YAML com respostas gravadas).
- [ ] **W05** Cada PR de fase atualiza: este plano (checklist), o planejamento (se mudou decisão), `docs/` e as strings pt/en.

### 2.4 Orçamentos de desempenho (valem sempre)
| Métrica | Meta |
|---|---|
| RAM com a interface aberta e parada | **< 300 MB**, quanto menos melhor |
| Janela pronta para digitar | **< 1 s** |
| Primeira resposta visível ("Pensando") | **< 100 ms** |
| Céu e grafo | 60 fps na interação, 20 fps parado, pausam fora de vista |
| Modelos locais | **Descarregados por padrão**; carregam ao usar; descarregam após X min; teto de 30% da RAM |
| Cache de imagens | 1 GB, WebP, 1600 px, sem duplicatas; descarrega imagens antigas de conversas pesadas |
| Conversas na memória de trabalho | dez |

### 2.5 Definição de pronto (vale para toda tarefa)
Uma tarefa só está pronta quando: **(1)** funciona de ponta a ponta, **(2)** tem teste automatizado (unidade, E2E ou snapshot), **(3)** tem textos pt/en, **(4)** respeita as regras da §2.1, **(5)** tem estado vazio e de erro, **(6)** está documentada, **(7)** passou na revisão cruzada.

---

## 3. Itens a decidir (carregados do planejamento)
- [x] **D01** Ordem pós-v1 decidida em 20/10/2026: **F25.06 (Word/Excel/PowerPoint), depois F20**. As demais funções seguem suas dependências.
- [x] **D02** Teto de **20% por tarefa**, limitado por 95% semanal e janela opcional de 5 h, confirmado pelo pedido explícito de 17/10/2026. A automelhoria ainda não foi implementada (C18/D03).
- [x] **D03** Decisão de produto em 20/10/2026: ler `/usage` (Claude) e `/status` (Codex) pelas CLIs, com queda para estimativa local e aviso se o formato mudar; Gemini só estimativa. **Libera implementar F13.01/F13.02. Termos verificados em 21/10/2026**, registrados em `docs/termos-provedores.md` (F00.29/F03.26); a implementação da leitura de uso permanece pendente.
- [x] **D04** Provedor de TTS definitivo (testar na F15). **Decidido em 20/10/2026: edge-tts pt-BR por enquanto, atrás de interface trocável; o Antônio vai testar ElevenLabs e outras vozes depois.**
- [x] **D05** O que mais um Projeto deve ter além de nota, repositório, conversas, instruções, pastas e arquivos de referência. **Decidido em 20/10/2026: o Projeto também tem arquivos de referência fixos, modelo e passos próprios (com marca "herdado") e rotinas do projeto; tarefas e prazos ficam na nota do vault (F08.08).**
- [x] **D06** Escopo do token do GitHub e do acesso ao Drive. **Decidido em 21/10/2026: token do GitHub com acesso total (clássico, `repo`), escolha do Antônio mesmo com o risco de um vazamento dar acesso a todos os repositórios privados da conta; o Drive continua só com `drive.file` (arquivos criados pelo Orion), que é o que o código usa.**
- [x] **D07** Licença-alvo: **uso pessoal** (decidido); revisar se virar distribuição.
- [ ] **D08** Telas do front antigo (removido em 21/10/2026) sem versão no app iced. **Decidido em 21/10/2026: a tela Vida foi cortada** (sai do app, da barra lateral, da paleta e das capturas; as rotas do núcleo ficam). **Planos de arquivos: em aberto**; proposta para o Antônio confirmar: sem tela própria, o plano aparece como cartão de aprovação no chat (origem → destino e tamanho) e fica listado em Aprovações, depois da v1 (F25.08).
- [x] **D09** Modelos locais do instalador. **Decidido em 21/10/2026: manter os 8 modelos (Whisper large-v3-turbo, Qwen 3 8B, Gemma 4 E4B, Granite 4.1 8B, Phi-4-mini 3.8B, Moondream 2, MiniCPM-V 4.5 e Mistral Small 3.2 24B), com as marcações atuais de recomendado e pré-selecionado.** Os requisitos de RAM/VRAM da especificação (`~2`, `~6`, `~5`, `~6`, `~3`, `~2`, `~6` e `~16 GB`) são estimativas e continuam sem medição no PC (F00.22/F00.25).
- [x] **D10** Alexa. **Decidido em 21/10/2026: adiada para depois da v1**; aparelhos e caminhos (oficial × não oficial) serão decididos então.
- [x] **D11** Confirmar se a **extensão de navegador** (Chrome/Edge, decidida "na v1") fica na 1.0 ou vira M7 (ela aumenta muito o escopo da v1). **Decidido em 20/10/2026: extensão de navegador só depois da 1.0 (F19.15 e F21.06 saem da v1); o navegador guiado por CDP basta por ora.**

---

### Decisões da rodada de perguntas (20/10/2026)
<!-- rodada-20-10 -->
- **Canais de release:** só **stable** no começo (um feed); o beta nasce quando o notebook entrar (F10.16, F22.01).
- **Sync entre PCs (F22.01):** log de eventos próprio (não `cr-sqlite`).
- **Busca do vault:** mantém BM25 + LSA; embeddings locais só se a busca falhar.
- **Documentos Word/Excel/PowerPoint (F25.06):** logo depois da v1 (antes de F20).
- **Palavra de ativação "Orion" (F15.05):** desligada na v1 (sem gravações para validar o modelo); ficam as duas palmas e o atalho (Ctrl+Alt+Espaço). O modelo candidato fica no repositório, sem escuta contínua.
- **Reconhecimento facial (F18.05):** mantido, com confirmação (falso positivo não age).
- **Automelhoria (F26):** depois de F11 (ferramentas de código) e F13 (orçamento confiável).
- **Assinatura:** certificado autoassinado + minisign; explicar o aviso do SmartScreen no instalador.
- **Relatório de falhas:** só local, com dados mascarados; nada sai do computador.
- **macOS:** depois da 1.0 estável (F27.08).
- **Grafo (F16):** wgpu puro; Bevy/`three-d` fica só como plano B registrado (F17.05).
- **Escopo da v1:** F01–F10. Os critérios de saída da F10.F valem para F01–F10; F11–F27 são pós-v1.
- **Casa e Alexa (F22.04–F22.07):** ficam no plano e vão para depois da v1 (D10 adiada em 21/10/2026; aparelhos e caminhos são decididos então).
- **Estudo (F24):** mantido, depois da v1 e de F20–F23.

## 4. Marco M0: Fundação

### F00: Preparação, repositório e spikes · tamanho **L**

> **Auditoria PR #30 (`2020015`), atualizada em 21/10/2026: Parcial.** Workspace, lock, iced, toolchain fixo, `deny.toml`, `docs/`, README do Rust, templates, Dependabot, skills de auditoria e ferramentas de desenvolvimento existem. Itens de testes e CI (F00.07, F00.08, F00.09, F00.12) foram cancelados por ora. Faltam os spikes/medidas, o export real do SurrealDB (você), os termos dos provedores e a branch protegida na `main` (GitHub).

**Objetivo:** ter a base do projeto e **medir os riscos** antes de construir. Cada spike termina com uma decisão escrita (ir, não ir, plano B).
**Dependências:** nenhuma.

#### F00.A Repositório e organização
- [x] **F00.01** Criar a árvore do código no repositório `antoniossalomao/Orion` (**privado**, licença de uso pessoal): pasta `rust/` (workspace) ao lado do legado Python, sem mexer no Python em uso.
- [~] **F00.02** Criar o workspace Cargo com os crates (testes de fumaça pausados por decisão de 21/10/2026):
  - [ ] `orion-core` (tipos, eventos, erros, configuração TOML)
  - [ ] `orion-policy` (núcleo imutável: classes de risco, aprovação, shell, caminhos, rede)
  - [ ] `orion-store` (SQLite, SQLCipher, migrações, FTS5, vetores)
  - [ ] `orion-vault` (leitura, escrita, git, frontmatter, wikilinks)
  - [ ] `orion-providers` (OpenAI-compatível, catálogo, ACP, CLIs, llama.cpp, cotas, roteador)
  - [ ] `orion-agent` (loop, ferramentas, subagentes, compactação, checkpoints)
  - [ ] `orion-tools` (arquivos, shell, git, web, vault, sistema)
  - [ ] `orion-ui` (iced: telas e widgets) e `orion-theme` (tokens, fontes, animações)
  - [ ] `orion-i18n` (Fluent pt-BR e en)
  - [ ] `orion-security` (senha, cofre, 2FA, PII, segredos, classificador de injection)
  - [ ] `orion-mcp` (cliente MCP com `rmcp`, hash das ferramentas)
  - [ ] `orion-voice`, `orion-vision`, `orion-graph`, `orion-telegram`, `orion-sync`, `orion-routines`, `orion-integrations` (esqueletos)
  - [ ] `orion-platform` (abstração de Windows: bandeja, atalhos, notificações, janela, autostart, cofre) com a implementação Windows e um *trait* para macOS futuro
  - [ ] `orion-setup` (instalador), `orion-updater` (serviço de atualização), `orion-evals` (harness), `orion-wasm` (plugins, depois)
- [x] **F00.03** `rust-toolchain.toml` fixo (1.97.0, com rustfmt, clippy e alvo `x86_64-pc-windows-msvc`), `Cargo.lock` versionado, **versão do iced fixada** (0.14.x), `deny.toml` (licenças tiradas do `Cargo.lock`) e `.cargo/config.toml` (atalhos `cargo app/snap/dev/verificar`; o alvo Windows não é o padrão, para não quebrar Linux). **Estado 21/10/2026:** arquivos criados; `cargo deny` não foi rodado (instalação pendente e testes/CI pausados).
- [x] **F00.04** Pasta `docs/` com índice, arquitetura, decisões (ADR curtos), guia de contribuição, glossário e um `docs/regras/` espelhando [[Orion - Regras]]; **o repositório é a origem e o vault espelha**. **Feito em 21/10/2026:** `docs/regras/` só aponta para `Memorias Do Projeto/REGRAS.md` (sem segunda cópia, para não divergir); também `docs/FERRAMENTAS-DEV.md` e `docs/spikes/`.
- [x] **F00.05** Arquivo de regras e persona (núcleo imutável) com **trava**: o `orion-policy` recusa escrita em `REGRAS.md`, persona e no próprio crate.
- [x] **F00.06** `README` do Rust: como compilar, rodar e testar no Windows. **Feito em 21/10/2026:** `Orion_Rust/README.md` (requisitos, comandos, pacote Vulkan); a parte "testar" registra que testes e CI estão pausados.

#### F00.B CI, qualidade e revisão
- [-] **F00.07** GitHub Actions: build Windows, cache, testes, `cargo deny`, `cargo audit`. **Cancelado por decisão de 21/10/2026** (testes novos e CI pausados; ver `docs/decisoes/0007-testes-e-ci-pausados.md`). O workflow `rust-windows.yml` fica só com `workflow_dispatch`.
- [-] **F00.08** Harness de testes headless e E2E do iced 0.14. **Cancelado por decisão de 21/10/2026** (testes novos e CI pausados; ver `docs/decisoes/0007-testes-e-ci-pausados.md`).
- [-] **F00.09** Pipeline de snapshots PNG com comparação automática. **Cancelado por decisão de 21/10/2026** (testes novos e CI pausados; ver `docs/decisoes/0007-testes-e-ci-pausados.md`). As fotos manuais (`cargo snap`) continuam.
- [x] **F00.10** Workflow semanal de **atualização de dependências** (PR automático): `.github/dependabot.yml` (cargo em `/Orion_Rust` e GitHub Actions, toda segunda, agrupando minor/patch).
- [~] **F00.11** Configurar a **revisão cruzada**: Claude Code Review e Codex no PR; modelos de PR e de issue; regra de branch protegida na `main`. **Estado 21/10/2026:** Claude Code Review e Codex já comentam nos PRs; criados `.github/pull_request_template.md` e `ISSUE_TEMPLATE/` (defeito e tarefa). **Falta:** a branch protegida na `main` é configuração do GitHub (Settings › Branches) e só o dono do repositório faz.
- [-] **F00.12** Harness de evals (`orion-evals`). **Cancelado por decisão de 21/10/2026** (testes novos e CI pausados; ver `docs/decisoes/0007-testes-e-ci-pausados.md`).
- [x] **F00.13** Skills de auditoria preparadas para as revisões: OWASP LLM + Agentic Top 10, Trail of Bits (Semgrep, cadeia de dependências), design critique e acessibilidade (ver [[ORION - REFERÊNCIAS DE PESQUISA]]). **Feito em 21/10/2026:** `.claude/skills/auditoria-seguranca/` (OWASP LLM 2025 e Agentic ASI01–ASI10) e `.claude/skills/auditoria-interface/`; **21 skills escolhidas pelo Antônio** a partir dos temas da Trail of Bits, em textos originais, disponíveis no Orion (extensão embutida `orion-auditoria`) e no Claude Code (`.claude/skills`). Lista, origem e limites em `docs/skills.md`. A extensão compila; instalação e uso no Orion não foram exercitados.

#### F00.C Spikes (cada um: protótipo mínimo, medição, decisão escrita em `docs/spikes/`)
> **21/10/2026:** `docs/spikes/README.md` lista o que falta de cada um; nenhum foi concluído porque todos exigem hardware, GPU, modelos baixados, contas externas ou um Windows real.
- [~] **F00.14** **Spike iced:** janela sem moldura, barra de título própria, Mica, barra lateral, lista virtualizada de 1000 mensagens, streaming de texto, campo de mensagem. Medir RAM, tempo até a janela pronta, fps. **Decisão:** iced × plano B **Slint**.
- [~] **F00.15** **Spike wgpu no iced:** widget `shader` desenhando o céu do instalador (sprites aditivos, linhas com gradiente) a 60 fps. Medir CPU e GPU.
- [ ] **F00.16** **Spike do grafo:** layout de forças 3D em **compute shader** com 1000, 5000 e 10000 nós; comparar com Barnes-Hut na CPU; **fallback na CPU** para GPU integrada. Referências: GraphWaGu, graphpu.
- [~] **F00.17** **Spike de banco:** `rusqlite` + **SQLCipher** + **FTS5** + **sqlite-vec** + **log de eventos próprio** no mesmo arquivo. **Decisão de 20/10/2026:** cr-sqlite saiu da solução escolhida (C16); sua compatibilidade não bloqueia mais o sync.
- [ ] **F00.18** **Spike de identidade de pacote:** pacote esparso registrado por um executável comum; testar **toast com botões Aprovar/Negar**, **OCR do Windows** (`Windows.Media.Ocr`) e nome/ícone do app. **Decisão:** viabilidade do Setup.exe (C24).
- [-] **F00.19** **Spike WebView2 cancelado por decisão de 20/10/2026** (C22). Fora dos critérios da v1; Mermaid/HTML precisam de solução sem WebView2 (F05.08).
- [~] **F00.20** **Spike ACP e CLIs:** conversar com Claude Code, Codex e Gemini/Antigravity por **ACP** (adaptadores) e por `stream-json`; medir latência e confiabilidade; levantar o que cada um expõe (aprovações, ferramentas, sessão).
- [~] **F00.21** **Spike de roteador:** regras + heurística, medidas contra um conjunto de 100 pedidos reais (simples, médio, difícil).
- [~] **F00.22** **Spike de áudio:** `cpal` + `aec3` + Silero VAD + `oww-rs` + `whisper-rs` (Vulkan); latência fim a fim; **teste de eco** com alto-falante. **Estado PR #30: cpal/AEC3/oww/Whisper integrados; Silero e medições faltam.**
- [ ] **F00.23** **Spike do modelo de injection:** rodar Shield Small, ProtectAI DeBERTa e ModernBERT (ONNX, CPU) em **português** com 200 exemplos; medir falso positivo e negativo.
- [ ] **F00.24** **Spike de embeddings:** multilingual-e5-small int8 em português; qualidade de busca no vault real (895 notas) contra FTS5 puro.
- [~] **F00.25** **Spike do `llama-cpp-2`:** carga, descarga, uso de RAM, Vulkan; troca de modelo em tempo de execução. **Estado PR #30: Worker CPU executou GGUF no CI; Vulkan/medições finais pendentes.**
- [ ] **F00.26** **Spike de gravação no vault:** `gix`/`git2` fazendo commit e push com credencial do Credential Manager; tratar conflito com edição no Obsidian.
- [~] **F00.31** **Ferramentas de desenvolvimento do iced 0.14:** feature `dev` em `orion-app` (`iced/debug`, `iced/time-travel`, `iced/hot`) e `docs/FERRAMENTAS-DEV.md`; `cargo dev` roda o app com elas. **Estado 21/10/2026:** compila; não foi provado numa janela real, e o hot reload exige o `cargo-hot` instalado à parte.
- [ ] **F00.27** **Spike do escaneamento de segredos e PII:** `cloakrs` + regras próprias (RG, banco, PIX); medir falso positivo em notas reais.

#### F00.D Pendências que só você faz (bloqueiam o descarte do legado)
- [~] **F00.28** **Exportar as tabelas pessoais do SurrealDB** (eventos, lembretes, tarefas, contatos) conforme `ORION_LEGADO.md` e rodar `orion verify-export` até dar `PRONTO` — **antes da venda do PC (fim do ano)**.
- [x] **F00.29** Confirmar os **termos de uso** dos provedores para automação em segundo plano e leitura de `/usage` (D03). **Feito em 21/10/2026** a partir de páginas oficiais da Anthropic, OpenAI e Google, registrado em `docs/termos-provedores.md` (não é parecer jurídico). Conclusões: usar as CLIs oficiais sem modificá-las e nunca ler, copiar ou guardar tokens OAuth; `/usage` e `/status` pela CLI estão no caminho documentado; automação contínua pede chave de API; o OAuth do Gemini CLI não pode ser usado por software de terceiros (Gemini fica só com chave de API e estimativa).
- [x] **F00.30** Decidir os itens D05, D06, D09 e D10 da §3. **Feito em 21/10/2026:** D05, D06 e D09 decididos; **D10 (Alexa) adiada por decisão para depois da v1.**

**Critério de aceite F00:** repositório com CI verde; um relatório por spike com número medido e decisão; a decisão iced × Slint registrada; export do SurrealDB verificado; lista de riscos atualizada.
**Riscos:** o iced não atender o visual (plano B Slint); cr-sqlite incompatível com SQLCipher (log de eventos); identidade de pacote inviável (OCR por ONNX e toast sem botões).

---

## 5. Marco M1: v1

> **Escopo da v1:** chat completo + aprovações e modo autônomo + nota no vault com commit + Configurações completas + Projetos + login + instalador. O que não está nesta lista entra nos marcos seguintes.

### F01: Janela, tema e base da interface · tamanho **L**

> **Auditoria PR #30 (`2020015`), atualizada em 20/10/2026: Parcial.** O app principal `orion-app` (iced, 47 cenas) existe e roda dentro de `orion.exe` com o núcleo no processo, com bandeja, mini barra e atalhos (Ctrl+Alt+Espaço, Ctrl+Alt+P); nunca rodou num Windows real. Faltam Fluent pt/en, motor de céu compartilhado (shader), Mica, snapshots comparados ao protótipo e aceite visual.

**Objetivo:** a casca do app, idêntica ao protótipo aprovado, com todos os elementos de janela, tokens, tipografia e navegação. **Especificação exata de cada tela, medida e estado em [[ORION - UI (ESPECIFICAÇÃO)]] (protótipo: `Protótipos/orion-ui-mockup.html`); em divergência, vale o protótipo, exceto nos itens U1–U15 daquela nota.**
**Dependências:** F00 (decisão iced × Slint).

#### F01.A Tema e tipografia
- [x] **F01.01** Tokens Grafite no `orion-theme`: `bg #151618`, `side #111214`, `raise #1b1c1f`, `fg/fg-2/fg-3 #ececee/#a9aab0/#75767d`, `accent (Rigel) #a4b3de`, `signal (Betelgeuse) #e9b467`, `primary #b9c6ea` (texto `#0f1320`), `ok #74cfae`, `danger #f07c88`, `line #25272c`. **Só tema escuro, sem seletor.**
- [x] **F01.02** Fontes embutidas no binário: **Geist** (400/500/600), **Geist Mono** (400/500) e **Newsreader** (respostas do chat; 400/500/600 + itálico). Opção em Configurações: Newsreader, Geist ou fonte do sistema. *(Corrigido em 17/10: segue o protótipo de UI, [[ORION - UI (ESPECIFICAÇÃO)]] U2.)*
- [~] **F01.03** Escala de tipo: texto base **14 px**; h1 24; eyebrow mono 11 com espaçamento; rótulos 12; legenda 11.
- [~] **F01.04** Formas: cantos **médios (10 a 14 px)**, bordas **finas e sutis**, bolhas do usuário com **fundo sutil**, botões principais em **Rigel pálido**, destaque (foco, seleção, switches) em **azul de Rigel**.
- [~] **F01.05** Componentes base reutilizáveis: botão principal/secundário/fantasma, campo, seletor, switch, caixa, cartão, pílula, selo, toast, modal sobreposto com fundo escurecido, menu de contexto, tooltip, barra de progresso, lista virtualizada, ícones **Lucide** (SVG embutidos).
- [~] **F01.06** Movimento: transições entre telas **fade de 150 ms**; painéis e abas **suaves de 200 ms**; indicador "Pensando" com ponto pulsante; **reduzir movimento** (sistema ou configuração) desliga tudo; **redução automática** com pouca RAM e **modo economia** na bateria.
- [~] **F01.07** Densidade **Confortável** (~32 px, padrão) e **Compacta** (~26 px); escala **90–150%** (padrão **100%**) com Ctrl +/−.
- [~] **F01.08** Espaçamento entre mensagens **médio**; **cursor piscando** na digitação; **toast curto** ao copiar; ilustração de estado vazio: **cinturão de Órion, pequeno**.

#### F01.B Janela e sistema
- [~] **F01.09** Janela sem moldura com **barra de título própria**: título da conversa (com o nome do projeto), **pílula de estado** (Pensando, Ouvindo, Aguardando você, ponto colorido âmbar/girando/verde), **"Buscar ou comandar" (Ctrl+K)**, botão do painel de contexto, minimizar, tela cheia, fechar.
- [ ] **F01.10** **Mica/Acrylic** com `window-vibrancy` (só Windows 11), com fundo sólido como reserva; evitar blur ao arrastar/redimensionar.
- [ ] **F01.11** Lembrar **tamanho, posição e monitor** (onde estava da última vez); janela pronta em **< 1 s**.
- [~] **F01.12** **Fechar vai para a bandeja** (`tray-icon`): se há tarefa rodando, **pergunta** e pode seguir na bandeja; destruir a interface e manter o **núcleo vivo** para rotinas e Telegram. **Estado PR #30: Fechar mantém o controle na bandeja; não existe ainda a casca principal.**
- [~] **F01.13** Menu da bandeja: **Nova conversa / captura**, **Pausar voz / câmera**, **Modo autônomo on/off**, status de cotas, sair. **Ponto no ícone** para aviso pendente e **ponto vermelho** quando a câmera estiver ativa.
- [~] **F01.14** **Atalho global Win+Shift+Espaço** (`global-hotkey`) abre a janela com o campo de mensagem focado; **instância única** (segunda execução foca a primeira). **Estado PR #30: Atalho entregue é Ctrl+Alt+Espaço, conforme pedido posterior; instância única pendente.**
- [~] **F01.15** **Inicialização ao ligar o PC:** abre **minimizado na bandeja**, só o núcleo; interface, voz e grafo carregam sob demanda. **Estado PR #30: Autostart desktop --background implementado; prova no notebook pendente.**
- [ ] **F01.16** **Splash com o cinturão** curto (sem atrasar a janela).
- [~] **F01.17** **Pausar o Orion inteiro** (Ctrl+Alt+P): pausa voz, rotinas e automações; indicador visível. **Estado PR #30: Ctrl+Alt+P aciona pânico; falta aceite global de rotinas/automação.**
- [~] **F01.18** Abstração de plataforma em `orion-platform` (trait) e implementação Windows; nada de chamadas Win32 fora dela.

#### F01.C Estrutura, navegação e atalhos
- [x] **F01.19** **Barra lateral de 272 px**, recolhível com **Ctrl+B** (recolhe para ícones): voltar/avançar, busca, **Nova conversa, Projetos, Memória, Aprovações, Rotinas**, "Mais" (Resultados, Nuvem, Fontes, Conhecimento, Painel, Arquivadas; Integrações ficam em Configurações > Conectores), com a busca como campo no topo e Ctrl+K *(ordem e itens do protótipo de UI, U6)*; rodapé com perfil e engrenagem.
- [~] **F01.20** Lista de conversas: fixadas no topo, **Hoje, Ontem, 7 dias, Mais antigos**, mais recente primeiro; título com projeto embaixo; indicadores (âmbar aguardando aprovação, girando trabalhando, azul resposta nova); ícones de origem (relógio = rotina, avião = Telegram); **arquivar só manual**.
- [~] **F01.21** **Paleta única Ctrl+K**: busca em conversas e notas do vault + ações com prefixo `>`; navegação por teclado.
- [~] **F01.22** **Atalhos personalizáveis** (tela em Configurações) com **detecção de conflito**; tabela padrão: Ctrl+K, Ctrl+B, Ctrl+/− (escala), Win+Shift+Espaço, Ctrl+Alt+P, Ctrl+Alt+V (voz ao vivo), Ctrl+Alt+T (tradução de tela), Win+Shift+O (captura rápida), Win+Shift+I ("Isso?").
- [~] **F01.23** Roteamento de telas (Início, Chat, Projetos, Memória, Aprovações, Rotinas, Painel, Resultados, Fontes, Conhecimento, Integrações, Arquivadas, Configurações) com **links profundos** (`/config <seção>`).

#### F01.D Tela inicial, idioma e acessibilidade
- [~] **F01.24** **Tela inicial:** constelação de Órion ao fundo (animada, sutil, usando o motor do céu da F01.29), saudação por **hora do dia + nome**, campo de mensagem central, projeto, seletor de modelo, linha de estado, **sugestões (poucas e relevantes: pendências, cotas, falhas)**, **projetos recentes** e **pendências**. **Ao abrir: conversa nova** (com o último projeto usado).
- [ ] **F01.25** `orion-i18n` com **Fluent** (pt-BR e en) desde o começo; **respostas sempre em português**; títulos de conversa em português.
- [ ] **F01.26** **Aviso de novidades** discreto na primeira abertura após atualizar.
- [ ] **F01.27** Acessibilidade: navegação completa por teclado, foco visível, ordem de tab lógica, rótulos para leitor de tela nos controles (Narrador **fora da v1**), contraste AA.
- [~] **F01.28** Painel de **"Mais"** vazio por padrão apenas com as telas que existem na versão (itens futuros aparecem quando a fase correspondente chegar).
- [~] **F01.29** **Motor do céu** compartilhado (widget `shader` do iced, pipeline wgpu com sprites aditivos, linhas com gradiente, câmera com respiração, gerador Park–Miller com semente 7): usado na **tela inicial**, no **login e bloqueio**, no **instalador**, no **onboarding** e na **mini tela de atualização**. **Estado 21/10/2026:** o motor CPU (imagem RGBA) virou o crate `orion-sky`, compartilhado por instalador e onboarding; login e tela inicial ainda usam o céu estático de `orion-app/src/sky.rs`; o widget `shader` wgpu não existe. Especificação exata (constantes, ordem de desenho, fórmulas) em [[ORION - INSTALADOR (ESPECIFICAÇÃO)]], §8. **Pausa fora de vista**, 20 fps parado, 60 fps na interação, versão simplificada na GPU integrada.
- [~] **F01.30** **Ícone do app: cinturão de Órion** (três pontos em diagonal) em todos os tamanhos (barra de tarefas, bandeja, atalho, instalador, desinstalador), mais a **marca ao lado do nome** na saudação; versão com **ponto de aviso** e **ponto vermelho** (câmera).
- [~] **F01.31** **Conformidade com o protótipo de UI:** snapshot PNG de cada tela e estado (dados de exemplo da §28 de [[ORION - UI (ESPECIFICAÇÃO)]]) comparado ao `Protótipos/orion-ui-mockup.html`; registrar toda divergência intencional na tabela U1–U15 daquela nota.

**Testes:** snapshots de cada estado da casca (sidebar aberta/recolhida, escalas 90/100/150, densidades); E2E de Ctrl+K, Ctrl+B, bandeja e atalho global; medição de RAM e tempo da janela pronta contra o orçamento.
**Critério de aceite:** casca idêntica ao protótipo aprovado; **< 1 s** até digitar; **< 300 MB** parado; todas as ações acessíveis pelo teclado.
**Riscos:** Mica instável em algumas GPUs (fundo sólido); API do iced mudando (versão fixada).

---

### F02: Dados, segurança e login · tamanho **L**

> **Auditoria PR #30 (`2020015`): Parcial.** PBKDF2, sessão revogável, cofre, 2FA Telegram e auditoria portados; SQLCipher/Argon2id/Hello/recuperação/PII/log cifrado não entregues.

**Objetivo:** banco cifrado, cofre, login com avatar, recuperação, filtros de segredo e dados pessoais, e a trilha de auditoria.
**Dependências:** F00 (spikes de banco e Hello), F01.

#### F02.A Banco de dados (`orion-store`)
- [~] **F02.01** SQLite com **SQLCipher**; chave derivada da senha com **Argon2id**; chave só em memória, **zeroizada** ao bloquear. **Estado PR #30: Parcial (19/10/2026): SQLCipher com chave mestra aleatória embrulhada pela senha (Argon2id) e pela chave de recuperação, senha pedida na subida, migração `orion db-cifrar` com cópia conferida, backups cifrados e chave zeroizada ao bloquear; feature `sqlcipher` fora do build padrão e o `orion.db` atual segue em claro até a migração.**
- [~] **F02.02** Esquema v1 com migrações: `conversas`, `mensagens`, `ramos`, `anexos`, `aprovacoes`, `atividade`, `auditoria`, `fatos` (com validade/temporal), `chunks_notas` (FTS5), `vetores` (sqlite-vec), `rotinas`, `execucoes`, `cotas`, `chamadas` (saídas sem conteúdo), `tarefas_estado`, `configuracoes`, `eventos_sync` (se o log de eventos for escolhido).
- [~] **F02.03** **Transcrição bruta** das conversas guardada para sempre (histórico "para sempre"); histórico de atividade **1 ano**, depois resumo mensal; desfazer por **7 dias**.
- [ ] **F02.04** Banco **sem limite prático**; compactação (`VACUUM`) agendada; **recuperação do índice** a partir do vault se o banco se perder.
- [ ] **F02.05** Configurações em **TOML** (`settings.toml`) com botão "Abrir arquivo" e **recarga automática** ao mudar o arquivo (resolve edição manual × salvar sozinho); **segredos nunca no TOML**.
- [~] **F02.06** **Backup local:** cópia criptografada do banco e das configurações na pasta do Orion (a cópia no Drive é F10).

#### F02.B Cofre, senha e login
- [~] **F02.07** **Cofre do Windows** (Credential Manager via `keyring`) para chaves de API, token do Telegram, token do GitHub; nada vai ao vault nem ao git.
- [ ] **F02.08** **Proteção por Windows Hello** (`UserConsentVerifier`) da chave do banco, seguindo o desenho "chave de conteúdo embrulhada por chave do Hello"; senha continua como reserva.
- [~] **F02.09** **Criação do perfil local:** nome, **avatar**, **senha forte** (mínimo 12, com letras, números e símbolos), medidor de força.
- [~] **F02.10** **Recuperação:** código gerado uma vez e salvo em arquivo novo escolhido no onboarding, sem sobrescrever; CLI `conta-recuperar`, restauração de backup autenticado e reembrulho da chave mestra. Impressão e experiência completa de recuperação pela janela, além do aceite Windows, ainda pendentes.
- [~] **F02.11** **Tela de login** (céu animado + Orion surgindo + avatar no centro): campo de senha, entrar com Hello, mensagem de erro, contagem de tentativas.
- [~] **F02.12** **Tentativas erradas:** atraso crescente (**1, 5 e 15 min** após 5 erros), registrado na auditoria, aviso no Telegram quando houver (C8).
- [~] **F02.13** **Bloqueio:** a senha é pedida ao abrir e **quando o Windows é bloqueado (Win+L)**; **sem timer próprio**; tela de bloqueio mostra só a **contagem de pendências**, sem conteúdo. **Estado PR #30 (19/10/2026): Parcial: no Windows, Win+L encerra as sessões e apaga a chave da memória (`ORION_LOCK_EXIT=true` encerra o processo); sem tela de bloqueio com contagem de pendências e nunca testado num Windows real.**
- [~] **F02.14** **Segundo fator:** senha e Telegram na entrada nativa, com desafio de uso único e prazo; Windows Hello opt-in como atalho de login e reserva do 2FA, via UserConsentVerifier e credencial no cofre. Teste com hardware/contas reais no Windows pendente.
- [~] **F02.15** **Sair da conta** (logout): apaga a chave da memória, fecha sessões e exige senha + 2FA para voltar.
- [~] **F02.16** **Sessões em outros PCs/apps:** lista, **notifica** quando uma nova abre e permite **encerrar**.
- [~] **F02.17** **Mudanças sensíveis pedem a senha de novo:** desligar a senha, ligar o modo autônomo, apagar dados, exportar dados, trocar chave de recuperação.
- [~] **F02.18** **Exportação de dados** só com senha e confirmação; **apagar histórico** por período, com confirmação.

#### F02.C Auditoria, logs e privacidade (`orion-security`)
- [x] **F02.19** **Auditoria fail-closed (R10):** toda decisão da política grava em `auditoria`; se gravar falhar, a ação que não é leitura é **negada**.
- [~] **F02.20** **Logs** com `tracing`: arquivo rotativo, **30 dias**, **cifrados**, **máscara sempre** de chaves, senhas e dados pessoais, **com contexto para depuração**; **só você** vê, pelo app, com senha.
- [~] **F02.21** **Filtro de PII e segredos** antes de qualquer envio: `cloakrs` (CPF/CNPJ, chaves de API, JWT, AWS) + **regras próprias** (RG, dados bancários, PIX); detecta e **mascara**; **transparência**: ícone discreto na resposta que usou provedor externo, e o clique mostra **o que saiu e para qual provedor**.
- [~] **F02.22** **Registro de saídas** sem conteúdo (R21): provedor, tipo, bytes, resultado, latência; tela em Privacidade.
- [ ] **F02.23** **Pastas bloqueadas:** você marca; **nada sai** delas para provedor externo (padrão: nenhuma marcada); pastas sensíveis do sistema (navegador, cookies, área de transferência) **só com aprovação caso a caso**.
- [~] **F02.24** **Escaneamento de segredos antes de gravar no vault** (bloqueia o commit) e checagem de frontmatter e links.
- [ ] **F02.25** **Área de transferência:** limpeza **30 s** depois de copiar um segredo.
- [ ] **F02.26** **Verificação de treino por provedor:** campo "usa dados para treino" no catálogo; aviso se estiver ligado; **código e vault privado nunca vão a esses provedores**.
- [~] **F02.27** **Telemetria só local**, métricas na tela Painel; **relatórios de erro só locais**.

**Testes:** testes de política (regras R03 a R11), fuzzing do filtro de PII, teste de recuperação com chave perdida/banco apagado, teste de atraso de tentativas, testes de zeroização, E2E de login/logout/Win+L.
**Critério de aceite:** sem a senha nada abre; banco ilegível sem a chave; segredos nunca aparecem em log; recuperação com a chave impressa funciona.

---

### F03: Provedores, roteador e modelos locais · tamanho **XL**

> **Auditoria PR #30 (`2020015`): Parcial.** Gateway/CLIs diretas/router e worker llama.cpp instalável feitos; ACP, UX completa, embeddings/visão locais e build/medição Vulkan pendentes.

**Objetivo:** falar com qualquer provedor (gratuito, CLI oficial ou local), com fallback, cotas e roteamento automático.
**Dependências:** F00 (spikes ACP, roteador, llama.cpp), F02.

#### F03.A Camada de provedores (`orion-providers`)
- [~] **F03.01** Cliente **OpenAI-compatível próprio** (SSE, ferramentas, visão, JSON), sem crate de terceiros; **formato interno único** de mensagens.
- [~] **F03.02** **Catálogo pronto** (URL, link para pegar a chave, modelos, limites, "usa dados para treino"): Groq, Gemini API (Flash/Flash-Lite), OpenRouter `:free`, Cerebras, Mistral, GitHub Models, Cloudflare Workers AI e **todos os gratuitos que forem achados**, mais **"Personalizado"** (URL + chave + modelo). **Uma conta por provedor.**
- [~] **F03.03** Por provedor: **estado e latência (testar conexão)**, **cota restante e reset**, **modelos disponíveis** (lidos da API), **prioridade por arrastar**; **chave inválida** é desativada, marcada em vermelho, com aviso; **alerta 14 dias** antes de a chave expirar.
- [~] **F03.04** **Ordem por tarefa:** código → Groq; conversa e resumo → Gemini API; configurável por tipo (chat, código, resumo, visão, imagem).
- [~] **F03.05** **Erros e retentativas:** timeout **60 s**, **2 retentativas**, depois **troca de provedor**; **faixa discreta de uma linha** ("Groq sem cota, usando Gemini"); cartão no chat só quando nada funciona; cota estourada no meio da tarefa **troca de provedor** sozinha.
- [ ] **F03.06** **Troca de provedor no meio da conversa:** **resumo ao trocar** (não reenvia o histórico inteiro).
- [ ] **F03.07** **Cache:** cache de prompt do provedor + **respostas idênticas por 24 h**.
- [~] **F03.08** **Origem da resposta** sempre registrada e mostrada de forma discreta; **custo estimado por conversa** discreto no painel (zero para gratuitas).
- [~] **F03.09** **Nunca API paga** (R23): bloqueio por padrão; nada pago sem aprovação.

#### F03.B CLIs oficiais e ACP
- [~] **F03.10** Integração com **Claude Code, Codex e Google (Gemini CLI/Antigravity)** por **ACP** como padrão e **CLI direta (`stream-json`)** como reserva; provedor "Google" abstrato que troca de CLI (a gratuita do Gemini CLI será substituída). **Estado PR #30: Somente CLIs diretas; sem ACP.**
- [~] **F03.11** Seleção de **modelo + esforço** (padrão **Sonnet**, esforço **médio**) por CLI; CLIs **atualizam sozinhas**; processo filho **sem variáveis `ORION_*`** (R06).
- [~] **F03.12** Ciclo de vida do subprocesso: iniciar, ler eventos, cancelar, retomar sessão, matar órfãos ao sair.
- [~] **F03.13** **Seletor de modelo na interface** com 5 opções: **Auto, Claude, Codex, Gemini e Orion** ("Orion" = APIs gratuitas, ordem e cotas nas configurações).

#### F03.C Modelos locais (`llama-cpp-2`)
- [~] **F03.14** llama.cpp embutido, **Vulkan** quando houver GPU; **descarregado por padrão**, carrega ao usar e **descarrega após X min** sem uso; **teto de 30% da RAM**; GPU ou CPU por configuração. **Estado PR #30: Código CPU/Vulkan e limite real feitos; descarrega ao terminar cada inferência, em vez de reter por X min. Build Vulkan ainda bloqueado.**
- [~] **F03.15** **Gerenciador de modelos:** download com **verificação de hash**, **pausa e retomada**, **avisa antes de baixar atualização**, quantização **Q4** (escolha Q5/Q8 opcional), **teste rápido** (prompt + tokens/s), remover. **Estado PR #30: Hash/retomada/cancelamento e local-test existem; gestão completa ainda falta.**
- [~] **F03.16** **Fallback offline:** sem internet cai para o modelo local e avisa; modelo escolhido **pela GPU detectada**; **família Qwen** como padrão.
- [~] **F03.17** Lista de modelos do instalador e seus **requisitos por modelo** (RAM/VRAM): Whisper large-v3-turbo, Qwen 3 8B, Gemma 4 E4B, Granite 4.1 8B, Phi-4-mini, Moondream 2, MiniCPM-V 4.5, Mistral Small 3.2 24B.

#### F03.D Roteador "Auto"
- [~] **F03.18** Roteador por **regras e heurística** (palavras-chave, tamanho, anexos, projeto, modelo padrão do projeto), classificando **simples, médio, difícil**.
- [ ] **F03.19** **Escala de nível** quando a resposta falha em checagens (formato, testes, confiança baixa); pergunta simples vai para o mais barato que atenda (gratuitas, depois local); difícil/código vai para Claude ou Codex pela assinatura quando há cota.
- [~] **F03.27** **Anexo de imagem para modelo sem visão:** o roteador **envia para um provedor com visão** (ex.: Gemini gratuito) em vez de falhar; o modelo local de visão (Moondream/MiniCPM-V) é a alternativa offline.
- [~] **F03.20** **Botão "Por quê?"** nas decisões do Auto; registro local das decisões.
- [ ] **F03.21** **👍/👎** das respostas guardados **só local**; depois da v1, treino de um classificador com esses dados (estilo RouteLLM) — gancho previsto.

#### F03.E Cotas e extras
- [~] **F03.22** **Registro local de cada chamada** por provedor (tokens, janela, reset), base de todas as estimativas; **calibragem manual**; dia e hora do **reset semanal** por provedor; **alerta em 80%**.
- [~] **F03.23** **Painel de cotas** atualizado a cada **5 min**.
- [~] **F03.24** **Imagens (`/imagem`):** Gemini por padrão e outros gratuitos como reserva; salvas em `03 Recursos/Orion/Imagens/` e linkadas na nota da conversa; nome gerado pelo código, tipo conferido (R16); **sem marca de IA** nos metadados.
- [~] **F03.25** **Busca na web:** Gemini com Google Search primeiro, **Brave** quando a cota acabar; a consulta é "pergunta com contexto" que **passa pela filtragem**; pastas bloqueadas ficam de fora.
- [x] **F03.26** **Verificação de termos** (D03) registrada antes de ligar qualquer uso automático de assinatura. **Registrada em 21/10/2026** em `docs/termos-provedores.md`; refazer se os termos mudarem ou antes de ligar um uso automático novo.

**Testes:** provedores falsos e gravados (cassetes), erro/timeout/cota, troca no meio da conversa, roteador contra o conjunto da F00.21, descarga de modelo, medição de RAM.
**Critério de aceite:** uma pergunta simples e uma difícil chegam ao provedor certo; queda de provedor troca sozinha; modelo local carrega e descarrega medidos; nenhuma chamada paga possível.

---

### F04: Agente, ferramentas, permissões e aprovações · tamanho **XL**

> **Auditoria PR #30 (`2020015`): Parcial.** Agente, ferramentas e aprovações portados; paridade de política/agente, autônomo do plano, checkpoints, subagentes e notificações Windows com botões pendentes.

**Objetivo:** o cérebro que age, com política que o modelo não consegue contornar.
**Dependências:** F02, F03.

#### F04.A Política (`orion-policy`, núcleo imutável)
- [x] **F04.01** Registro de ferramentas com **classe de risco fixa** (leitura, escrita, execução, destrutiva) e metadados (K05).
- [~] **F04.02** Tabela de permissões implementada:
  - [ ] ler arquivos em **pastas confiáveis** (Documentos, Área de trabalho, pasta dos projetos): sem aprovação;
  - [ ] gravar notas no vault + commit na `main`: sem aprovação;
  - [ ] busca na web sem dados pessoais: sem aprovação;
  - [ ] comandos de leitura (`git status`, `cargo check`, listar processos): sem aprovação;
  - [ ] escrever arquivo de projeto, enviar para fora, comandos que alteram algo: **com aprovação** (dispensada no autônomo);
  - [ ] apagar, formatar, mexer em sistema, `force-push`, reescrever histórico, desinstalar: **aprovação + digitar o texto do comando, sempre**, mesmo no autônomo.
- [x] **F04.03** **Shell (PowerShell):** lista positiva de leitura provada (R08); sem sandbox (risco aceito); mesmo usuário.
- [x] **F04.04** **Arquivos:** resolver caminho antes (R07); leitura de segredo pede confirmação; escrita fora das pastas confiáveis pede aprovação.
- [x] **F04.05** **Rede:** `netguard` (R09/R14), **qualquer site**, sem lista de domínios; URL inteira no cartão após conteúdo externo.
- [~] **F04.06** **Conteúdo externo é dado** (R05): marcação de origem em tudo que vem de web, e-mail, documento, área de transferência e MCP; instrução vinda dele **nunca executa**, pede aprovação; **classificador local de prompt injection** (Shield Small / ProtectAI / ModernBERT conforme o spike em português) como **camada extra** (não substitui a aprovação).
- [~] **F04.07** **Fatos aprendidos** só se gravam por fonte confiável: conteúdo externo não grava fatos.
- [~] **F04.08** **Controle de apps do PC** na ordem: **1** API/MCP/extensão do app; **2** controle de mídia do sistema; **3** **UI Automation**; **4** teclado/mouse simulados (último recurso). **Qualquer app, aprovação por ação** (clique, digitação, tecla), com sugestão de apps pelo Orion e OK por app; **ações leves** (abrir app, tocar música) sem pedir; apps elevados e jogos só se você autorizar, com confirmação; título de janela nunca vira comando (R15).
- [~] **F04.09** Falha de auditoria nega (R10); cartão só aprova o que se vê inteiro (R11).

#### F04.B Aprovações
- [~] **F04.10** **Cartão de aprovação** no chat (âmbar): ação, argumentos completos, risco, botões **Aprovar/Negar**, "sempre permitir isto **neste projeto**" (vira regra editável em Privacidade).
- [~] **F04.11** **Tela Aprovações:** fila única de todas as conversas e rotinas no topo, **histórico de atividade** embaixo (filtro todas/aguardando/negadas), **aprovar/negar em lote** (menos a linha "sempre pedir").
- [~] **F04.12** **Expiração:** sem resposta em **24 h**, cancela e registra. (A "contagem regressiva para cancelar" **não foi escolhida** na rodada 3 e não entra.)
- [~] **F04.13** **Recibo de ação** (o que mudou + desfazer) em cada item; **desfazer por 7 dias** (commit no vault, arquivo editado).
- [ ] **F04.14** **Checkpoints nomeados** antes de tarefas longas (restaurar arquivos).
- [ ] **F04.15** **Confirmação extra** de comando destrutivo: digitar o texto do comando.
- [x] **F04.16** **Confirmação fora de banda** (R04): vale 10 min, uso único, para a sessão, a ferramenta e os argumentos exatos. **Estado PR #30: Aprovação de sessão/ferramenta/argumentos e uso único portados; paridade da política pendente.**

#### F04.C Modo autônomo
- [x] **F04.17** Interruptor em **Configurações › Agente** e na **pílula de estado**; **desligado ao instalar**; ligar pede a senha de novo. **Estado 20/10/2026: o código liga o modo autônomo (`POST /modo/autonomo`, 4× passos) sem pedir a senha de novo. Feito em 20/10/2026: ligar pede a senha (`/modo/autonomo` exige `senha`; janela de senha na UI).**
- [ ] **F04.18** Ligado: age sem pedir aprovação, **exceto a linha "sempre pedir"**; **para só quando precisa de uma decisão sua**.
- [~] **F04.19** **Resumo ao fim do dia** (no Painel e na notificação das 23:59), não a cada ação.
- [ ] **F04.20** **Retomar do ponto salvo** se o PC cair no meio da tarefa (progresso salvo a cada passo).
- [ ] **F04.21** **Tarefas em paralelo até 3**, fila para o resto; **limite de passos configurável por projeto** (padrão 25, depois pergunta).

#### F04.D Loop do agente
- [~] **F04.22** **Híbrido:** loop próprio para APIs gratuitas; código pesado delega à CLI (ACP), que usa as ferramentas dela; ferramentas nativas na v1: **arquivos + shell + git, web (busca + fetch), vault, sistema** (apps, janelas, volume, área de transferência).
- [~] **F04.23** **Subagentes** dentro do limite de paralelos.
- [~] **F04.24** **Compactação automática** de conversa longa (resume o início, mantém fixadas e a nota do vault) e **avisa**; **dez conversas** na memória de trabalho; imagens antigas descarregadas.
- [ ] **F04.25** **Conflito entre pedido, vault e banco: perguntar sempre.**
- [ ] **F04.26** **Correção de erro de sintaxe** em código gerado: corrige e **mostra o que mudou**, passando pela política.
- [~] **F04.27** **Personalidade:** informal e direto, **sem emojis**, diz quando **não tem certeza** (com nível de confiança e fonte), respostas longas geradas direto, código com **nomes em inglês e comentários em português**; seletor **Conciso/Normal/Detalhado** + campo livre; sem avatar.
- [~] **F04.28** **Relatório de sessão** gerado ao fim do dia no vault.

#### F04.E Avisos e sons
- [ ] **F04.29** **Notificações do Windows com botões** (Aprovar/Negar) via pacote esparso: aprovação pendente, resposta pronta (janela fora de foco), rotina concluída, tarefa na nuvem terminou, tarefa falhou, cota no limite, automelhoria pronta.
- [~] **F04.30** **Sons sutis** (ligados por padrão) e **não perturbe só manual**; sem horário de silêncio automático.
- [ ] **F04.31** **Matriz evento × canal** (Windows, Telegram, som, bandeja) — Telegram liga na F14.

**Testes:** suíte de política (cada regra R01–R23), injeção de conteúdo externo, tentativas de contornar classe de risco, evals do agente (roteamento, comandos, ferramentas), simulação de queda no meio da tarefa.
**Critério de aceite:** nenhum caminho de teste consegue executar ação de risco sem a aprovação correta; destrutivo sempre pede o texto do comando; conteúdo externo nunca aciona ferramenta; retomada pós-queda funciona.

---

### F05: Chat completo · tamanho **XL**

> **Auditoria PR #30 (`2020015`), atualizada em 21/10/2026: Parcial.** Chat SSE, ramos e artefatos existem no núcleo e o chat do app iced (`orion-app`) está ligado a ele; o front HTML antigo foi removido em 21/10/2026. Faltam os itens abaixo, E2E e snapshots comparados ao protótipo.

**Objetivo:** a tela principal, idêntica ao protótipo, com tudo que foi decidido para o chat.
**Dependências:** F01, F03, F04.

#### F05.A Campo de mensagem
- [~] **F05.01** Enter envia, Shift+Enter quebra linha, **↑ repete a última mensagem**; **digitar enquanto o Orion responde enfileira** e entra no próximo passo.
- [~] **F05.02** **`/` comandos** (menu com busca): `/nota`, `/vault <busca>`, `/rotina`, `/modelo`, `/projeto`, `/autonomo`, `/limpar`, `/resumo`, `/aprovar`, `/voz`, `/imagem`, `/ajuda`, `/config <seção>`; os de fases futuras entram ao serem implementados (`/nuvem`, `/mcp`, `/skill`, `/pesquisa`, `/aprendizado`, `/gravar`, `/rotina rodar`).
- [ ] **F05.03** **Comandos `/` personalizados** (prompt salvo, opcionalmente ligado a um projeto).
- [~] **F05.04** **`@` cita uma nota do vault** como contexto (autocompletar por título e caminho).
- [~] **F05.05** **Anexos:** imagens e prints (colar ou arrastar), texto e código, **PDF** (`pdf_oxide` → Markdown; **OCR** se escaneado), **áudio** (transcreve antes); **limite de 20 MB**, acima disso **oferece referenciar o caminho** (pasta confiável); **miniatura pequena** ao lado do campo; metadados limpos. **Rodada Codex 21/10:** anexos continuam pendentes; no modo real o botão avisa indisponibilidade e não insere arquivos de exemplo.
- [~] **F05.06** Seletor de **projeto**, **modelo** (5 opções + modelo/esforço) e **linha de estado** sob o campo.

#### F05.B Mensagens e respostas
- [~] **F05.07** **Streaming** token a token com cursor piscando, botão **Parar** e **Esc** (mantém o texto parcial); **"Pensando"** com ponto pulsante em **< 100 ms**; raciocínio do modelo **recolhido**, expande ao clicar.
- [~] **F05.08** **Markdown** com `pulldown-cmark`: texto, listas, tabelas, citações, links, código com realce e copiar; **LaTeX complexo e prévia HTML pendentes de solução sem WebView2** (C22). A interface permanece iced. **Estado 22/10/2026:** itálico, links http(s) clicáveis, tabelas, régua, citação com barra e botão Copiar nos blocos de código; Fórmulas LaTeX simples em linha (`$...$`: letras gregas, operadores, frações, raízes, expoentes e índices) viram texto Unicode; Fórmulas LaTeX em bloco (`$$…$$` e blocos `latex`) são diagramadas no próprio app (`math.rs`, canvas): frações empilhadas, raízes com índice, expoentes e índices, ∑ ∏ lim com limites empilhados, integrais, `\left…\right`, matrizes (`pmatrix`, `bmatrix`, `vmatrix`), `cases`, `align`, acentos, `\text`; larguras estimadas por caractere, e fórmula que não fecha volta ao texto Unicode. Fórmulas em linha (`$...$`) continuam texto Unicode (23/10/2026, validado sob Xvfb); Mermaid (fluxogramas `graph`/`flowchart`: nós `[ ] ( ) { } (( ))`, setas `-->`, `---`, `-.->`, `==>` com rótulo, cadeias; arestas que pulam camadas contornam pelo lado) é desenhado no próprio app (`mermaid.rs`, canvas, sem WebView); outros tipos de diagrama (sequência, classe, gantt) continuam como código com aviso (23/10/2026, validado sob Xvfb); fórmulas complexas ficam como vieram.
- [~] **F05.09** **Linha de progresso** resumida ("Leu 4 arquivos, executou 1 comando") que **expande** passo a passo.
- [~] **F05.10** **Cartão de aprovação** inline e **cartão de erro de API** no chat.
- [~] **F05.11** **Ações da resposta** (aparecem ao passar o mouse): copiar (toast), gerar de novo, **gerar com outro modelo**, 👍/👎, salvar no vault; **ouvir** entra na F15. **Rodada Codex 21/10:** cópia de mensagens ligada à área de transferência; regeneração e ouvir desabilitados no modo real até implementação. Testes desta rodada registrados em ORION.md; aceite Windows pendente.
- [~] **F05.12** **Hover da hora:** provedor que respondeu, tokens e % da cota usada; **ícone de dados externos** (clique mostra o que saiu).
- [~] **F05.13** **Links e caminhos clicáveis:** arquivo abre no **VS Code** (`code -g arquivo:linha`), URL no navegador. **Estado 22/10/2026 (Linux, sob Xvfb):** URL abre no navegador só se for http/https; caminho absoluto de arquivo que existe abre no VS Code; caminho relativo só avisa. Falta resolver caminho relativo pela pasta do projeto e provar no Windows.
- [~] **F05.14** **Suas mensagens:** **editar e reenviar cria um ramo**, navegação **< 2/3 >** na mensagem; copiar; **fixar** (contexto permanente, **até ~2k tokens**, avisa se passar). **Rodada Codex 21/10:** cópia real da mensagem do usuário; editar/reenviar e fixar contexto continuam pendentes e desabilitados no modo real.
- [~] **F05.15** **Navegação em respostas longas:** botão **"ir ao fim"** e **minimapa** de mensagens na barra de rolagem. **Estado 22/10/2026:** botão "Ir ao fim" entregue (aparece ao rolar para cima); **Estado 23/10/2026:** minimapa entregue (faixa fina à direita com um bloco por mensagem, trecho visível realçado, clique salta para o ponto; aparece com 8+ mensagens; validado em Linux). Corrigido também o botão "Ir ao fim", que usava o deslocamento invertido.
- [~] **F05.16** **Artefatos** (código, HTML, documento longo): **aba no painel da direita**, com versões, copiar e salvar em arquivo. **Estado 22/10/2026:** aba "Artefatos" no painel direito lista os blocos de código de 5 linhas ou mais (e html/md/json/mermaid/latex) da conversa, com Copiar e Salvar em Documentos/Orion/Artefatos; **23/10/2026:** artefatos HTML ganharam botão Prévia: o HTML vira blocos (títulos, parágrafos, listas, tabelas, citações, código, links http(s); `html.rs`), sem CSS, JavaScript, imagens nem rede, e nada é executado ou aberto sozinho; faltam versões, documento longo como artefato e provar no Windows.
- [~] **F05.17** **Lista virtualizada + cache de layout** para conversas de 1000+ mensagens; **imagens antigas descarregadas** e recarregadas ao abrir; **cache de imagens 1 GB** (WebP, 1600 px, sem duplicatas). **Estado 22/10/2026:** só as 60 mensagens mais recentes são montadas; **23/10/2026:** janela virtual no topo da lista: monta só as mensagens que cobrem a distância até o fim + a tela + 2400 px de folga (mínimo 60), cresce ao rolar para cima e encolhe ao voltar (histerese de 20; a âncora no fim mantém a posição); mensagens abaixo da tela seguem montadas e a altura é estimada pela média, sem medição por mensagem; **cache de imagens 23/10/2026** (`imgcache.rs`): imagem numa linha `![alt](caminho)` é reduzida a 1600 px (caminho local só vale dentro de `Documentos/Orion`; caminho de rede/UNC, de dispositivo, com `..` ou fora dela vira o texto alternativo, sem tocar no disco; imagens geradas vêm do núcleo por `/imagens/<nome>`, baixadas com login), guardada em WebP sem perdas com nome pelo conteúdo (sem duplicatas) em `%LOCALAPPDATA%\Orion\imagens`, e a pasta é aparada a 1 GB tirando os menos usados (provado: de 5 arquivos de 1 MB com limite de 2,5 MB, saem os 3 mais antigos); a tela só segura a imagem enquanto a mensagem está montada (ao sair da janela virtual o renderizador a descarta e ela volta do WebP); faltam imagens anexadas pelo usuário e geradas (`/imagem`) chegarem ao chat, e a medição real de altura por mensagem.

#### F05.C Histórico, projeto e exportação
- [~] **F05.18** **Busca no histórico em texto completo**; **títulos automáticos em português, editáveis**; mover a conversa para outro projeto (a nota do vault é religada); arquivar manual; excluir pede aprovação. **Rodada Codex 21/10:** renomear por formulário ligado ao PATCH autenticado da sessão, com validação, foco, envio único e confirmação do núcleo. Não conclui busca/movimentação nem aceite Windows. **Estado 22/10/2026:** o título automático é a primeira linha do primeiro pedido (até 48 caracteres, sem chamar modelo).
- [~] **F05.19** **Exportar a conversa em Markdown**; compartilhar nota ou conversa **fora da v1**.
- [~] **F05.20** **Painel de contexto** (direita, **fechado por padrão**, abre sozinho com alteração de arquivos ou aprovação, **empurra** a conversa): aba **Contexto** (ambiente: projeto, branch, modelo; contexto usado; fontes ligadas; memória usada; alterações) — abas **Arquivos/Diff/Terminal** na F11.
- [~] **F05.21** **Bloco "nota do vault"** no painel: nome da nota da conversa, última atualização, **abrir no Obsidian** (`obsidian://`), último **commit** (hash e hora) — ligado na F06.
- [ ] **F05.22** **Aviso de conversa longa** (compactação feita) e de **conversa pesada** (imagens descarregadas).

**Testes:** E2E de todo o fluxo (enviar, parar, editar, ramo, anexar, aprovar), snapshots de cada tipo de mensagem, medição de scroll com 1000 mensagens, cobertura de acessibilidade.
**Critério de aceite:** o chat completo roda com um provedor gratuito e com o modelo local; 1000 mensagens rolam sem travar; nenhuma ação de risco sem cartão.

---

### F06: Vault e memória (v1) · tamanho **XL**

> **Auditoria PR #30 (`2020015`): Parcial.** Crate vault e rotas explícitas existem; não há gravação automática por turno, resolução de conflito sem sobrescrever ou índice local completo. Não marcar F06 inteira concluída.

**Objetivo:** o vault é a memória. O Orion lê, busca, grava notas de cada conversa com commit na `main`, aprende fatos e mostra tudo no app.
**Dependências:** F02, F03, F04, F05.

#### F06.A Leitura, índice e busca
- [~] **F06.01** **Detectar o vault** (config do Obsidian) e confirmar o caminho; repositório do vault **privado**; credencial do push no **Credential Manager**.
- [~] **F06.02** **`git pull`** ao abrir o app, a cada conversa nova, antes de gravar uma nota e ao abrir a tela Memória; tratar falha de rede sem travar.
- [~] **F06.03** **Leitor do vault:** parser de frontmatter YAML, wikilinks, tags, `type`, pastas; respeita o [[claude.md]] do vault. **Estado PR #30: Parser existe; conformidade completa com regras/links do vault ainda não validada.**
- [~] **F06.04** **Índice incremental por hash** a cada commit/pull: **BM25 + LSA**, conforme decisão de 20/10/2026; indexação inicial em segundo plano, reindexar e exclusões nas configurações. Embeddings locais só se a busca falhar. **Aceite do índice completo pendente; infraestrutura existente não comprova a busca decidida.**
- [~] **F06.05** **Privacidade do índice:** notas com `#privado` e pastas bloqueadas **nunca** viram embedding por API nem saem do PC; API de embeddings só se você ligar (C6).
- [~] **F06.06** **Busca na paleta (Ctrl+K)** e na tela Memória; notas do **Arquivo (04)** aparecem **esmaecidas**.
- [~] **F06.07** **Tela Fontes:** o que o Orion pode ler, com switch por fonte.
- [~] **F06.08** Ao responder, **memória de longo prazo busca no histórico automaticamente**; mostra "memória usada" no painel de contexto.

#### F06.B Gravação das conversas
- [~] **F06.09** **Uma nota por conversa** em `03 Recursos/Orion/Memória/`, nome **`AAAA-MM-DD - agente - título`**, **atualizada a cada rodada** com seções novas (como as notas de conversa do Claude). **Estado PR #30: Gravação por API explícita existe; atualização automática por turno ausente.**
- [~] **F06.10** Frontmatter conforme o `claude.md`: `type: conversa`, `date`, `hora`, `fonte` (nova origem **orion**, a registrar nas regras do vault), `projeto: "[[Nome exato]]"`, `tags` (criadas direto pelo Orion), `resumo`; **link a um Projeto/Área existente** (regra "não criar ilhas"); conteúdo técnico linka/cria hub em `03 Recursos/Conceitos/`.
- [ ] **F06.11** **Todas as conversas** viram resumo, automático e sem perguntar; **resumo escrito por API gratuita** (não gasta assinatura); conteúdo forte (decisão, debug resolvido, fato novo, pesquisa com fontes) é **destacado e linkado ao Projeto/Área**.
- [ ] **F06.12** **Conversas triviais** ("que horas são") entram no **resumo diário único das 23:59**, em `03 Recursos/Orion/Memória/Diário/AAAA-MM-DD.md`, linkando as conversas do dia; **com o PC desligado, gerar na nuvem** (mesma filtragem de segredos e dados pessoais).
- [~] **F06.13** **Commit e push direto na `main`**, **um commit por nota** (mensagem = resumo da conversa), **sem PR**; antes: escaneamento de segredos (bloqueia), checagem de frontmatter e links (bloqueia nota malformada). **Estado 20/10/2026: árvore com apenas o blob validado da nota, índices temporários e lock, staged alheios preservados, só main, sem hooks/filtros/fsmonitor; push do hash exato exige pai já remoto. Valida campos/frontmatter, Projeto/Área e wikilinks de notas; integração completa com regras particulares do vault e aceite Windows ainda pendentes.**
- [~] **F06.14** **Conflito com edição no Obsidian:** se a mesma nota foi editada, **mantém as duas versões**, marca o conflito e **pergunta no chat** mostrando as duas. **Estado 20/10/2026: publicação sem sobrescrita, versão diferente preservada em proposta marcada como conflito; API retorna ambos os caminhos e não commita/publica o conflito. Comparação e escolha das versões no chat, além do aceite real no Obsidian/Windows, ainda pendentes.**
- [ ] **F06.15** **Relatório de sessão** no vault ao fim do dia e, quando o Orion alterar arquivos do vault, **um relatório em `03 Recursos/Relatórios de Mudanças/`** (regra do `claude.md`).
- [ ] **F06.16** **Agentes externos (Codex, Gemini) no vault:** leitura e escrita, com **registro no log do Orion**.
- [ ] **F06.17** **Backup manual do vault em zip**; imagens geradas em `03 Recursos/Orion/Imagens/` linkadas na nota.
- [~] **F06.18** **Comandos:** `/nota` (grava ou atualiza agora) e `/vault <busca>`. **Rodada Codex 21/10:** `/nota` deixa de anunciar gravação fictícia no app real; informa que ainda não está ligado. A rota explícita do núcleo não equivale a integração pelo comando.

#### F06.C Conhecimento (fatos)
- [~] **F06.19** **Fatos e preferências aprendidos são salvos sozinhos** no banco **e** numa nota do vault; o Orion usa o banco; em divergência **o vault vence** (reimporta no `git pull`).
- [x] **F06.20** **Histórico temporal:** fato antigo marcado como **substituído, não apagado** ("o que era verdade em tal data").
- [~] **F06.21** **Tela Conhecimento:** fatos com fonte; **corrigir** abre a nota no vault; **esquecer** apaga com aprovação; conteúdo externo **não grava fatos** (R05).
- [ ] **F06.22** **"Sobre você"** em Configurações: lista de fatos com editar e apagar, espelhando a nota.

#### F06.D Editor e tela Memória
- [~] **F06.23** **Abrir nota no Orion:** editor e prévia **lado a lado** (Markdown à esquerda, renderizado à direita), wikilinks clicáveis, **fontes mono para código e serifa para texto**, **autocompletar `[[wikilinks]]` e `#tags`**, **pedir ao Orion para reescrever um trecho**, botão **abrir no Obsidian** (`obsidian://`). **Rodada Codex 21/10:** editor permanece demonstração; abertura/salvamento bloqueados no modo real para não mostrar NOTE_SAMPLE nem anunciar commit inexistente. Leitura, edição segura e conflitos ainda pendentes.
- [~] **F06.24** **Tela Memória (v1):** busca, **notas recentes do Orion**, **propostas de reorganização** (mover, renomear, mesclar) com **lista em lote para aprovar**; o **grafo** entra no M5.
- [ ] **F06.25** **Reorganizar o vault** só com proposta e aprovação (seguindo o `claude.md`).
- [~] **F06.26** **Importadores de histórico no onboarding:** JSON/JSONL de Claude Code, Codex e ChatGPT; prévia/hash e resumo determinístico com trechos limitados no vault. Resumos por LLM, exports reais variados e conformidade completa com regras específicas de cada vault ainda pendentes.

**Testes:** E2E de gravação (nota, commit, push), simulação de conflito com edição externa, teste de bloqueio por segredo/frontmatter, qualidade da busca em 895 notas, teste de reindexação, teste "banco perdido, vault intacto", E2E de editor.
**Critério de aceite:** uma conversa real gera nota válida com commit; busca encontra por palavra e por sentido; fatos substituídos mantêm histórico; nada `#privado` sai do PC.

---

### F07: Configurações completas · tamanho **L**

> **Auditoria PR #30 (`2020015`): Parcial.** Há configuração por ambiente/.env/JSON e painel de orçamento; não há modal completo, TOML com recarga ou todas as seções nativas.

**Objetivo:** a tela de Configurações inteira, com a estrutura e as regras decididas.
**Dependências:** F01 a F06 (cada seção liga ao que já existe).

#### F07.A Estrutura e regras gerais
- [~] **F07.01** **Modal sobreposto** (fundo escurecido), **lista agrupada à esquerda** (App · IA · Memória · Entradas · Integrações · Sistema) e **busca no topo** que filtra opções de todas as seções; em janela estreita vira **lista → detalhe**.
- [~] **F07.24** **Navegação e conteúdo exatos do protótipo:** dois grupos (Configurações: Geral, Notificações e sons, Conta, Privacidade e autonomia, Uso, Modelos, Memória, Fontes, Conectores · Este computador: Sistema, Modelos locais, Voz, Câmera, Atalhos), cada linha, texto e comportamento da §18 de [[ORION - UI (ESPECIFICAÇÃO)]]; os grupos da F07.01 são reorganizados para esses dois (U12).
- [ ] **F07.02** **Salva sozinha ao mudar**, com aviso "Salvo" e **desfazer**; espelho em `settings.toml` (sem segredos), com recarga ao editar à mão.
- [~] **F07.03** **Descrição curta embaixo de cada opção**; **restaurar padrão por opção e por seção** (opção alterada mostra **ponto azul**).
- [ ] **F07.04** Opções avançadas e perigosas atrás de **"Mostrar avançado"** (logs de depuração, resetar o banco, etc.).
- [~] **F07.05** **Mudanças sensíveis pedem a senha de novo** (F02.17).
- [~] **F07.06** **Links diretos** (`/config <seção>`) e links nos avisos do chat (ex.: aviso de cota → Orçamento).
- [x] **F07.07** **Sem perfis** (Trabalho/Pessoal) na v1.

#### F07.B Seções do grupo **App**
- [~] **F07.08** **Geral:** nome, idioma (pt/en), **tom de resposta** (Conciso/Normal/Detalhado + instruções livres), iniciar com o Windows e comportamento da bandeja.
- [ ] **F07.09** **Sobre você:** lista de fatos com editar/apagar (F06.22).
- [~] **F07.10** **Aparência:** escala 90–150%, largura do chat (**ajustável**, padrão centralizada ~760 px), animações, bloom/blur, qualidade gráfica (automática pela GPU), Mica/Acrylic, fonte das respostas, densidade, tamanho do texto, movimento reduzido.
- [~] **F07.11** **Notificações:** **matriz evento × canal** (Windows, Telegram, som, bandeja) para cada evento (aprovação pendente, resposta pronta, rotina concluída, nuvem terminou, cota, sugestão proativa); **não perturbe** manual.
- [~] **F07.12** **Atalhos:** globais e do app, personalizáveis com **detecção de conflito**; **comandos `/` personalizados**.

#### F07.C Seções do grupo **IA**
- [~] **F07.13** **Modelos:** regras do Auto (editáveis), modelo + esforço padrão por tipo (chat, código, resumo, visão, imagem), modelo do resumo do vault e da compactação, **ordem de fallback global**.
- [~] **F07.14** **Provedores:** catálogo + "Personalizado", estado e latência, cota e reset, modelos, prioridade (arrastar), **uma conta por provedor**, campo "usa dados para treino". **Estado 22/10/2026:** cadastro de chave por provedor em Configurações › Modelos (cofre do sistema, vale na próxima abertura, `POST /config/chave`); cotas de exemplo removidas do uso real.
- [~] **F07.15** **Orçamento:** teto de 95%, dia e hora do reset semanal por provedor, calibrar uso real, **fila da automelhoria** (a lógica entra na F13/F26; a tela já existe).
- [~] **F07.16** **Agente:** modo autônomo, **limite de passos**, agentes em paralelo e subagentes, **proatividade** (sugestões por dia, o que observa), **pastas confiáveis**.
- [~] **F07.17** **Modelos locais:** adicionar e remover, teste rápido, quantização, **descarregar da RAM após X min**, GPU/CPU, pasta dos modelos.

#### F07.D Demais grupos
- [~] **F07.18** **Memória › Vault:** caminho, remoto git e branch; regras de gravação (o que vira nota, horário do diário); indexação (reindexar, embeddings, exclusões); pastas do Orion fixas.
- [~] **F07.19** **Entradas › Voz** e **Câmera e visão:** telas prontas, **ativadas quando as fases F15 e F18 chegarem** (até lá mostram "em breve" com o estado).
- [~] **F07.20** **Integrações › Contas e integrações** (Telegram, Google, GitHub), **Conectores** (abas Instalados/Descobrir/Skills, permissão por ferramenta), **Desenvolvimento** (terminal e shell, VS Code, CLIs, nuvem, revisão cruzada) — telas prontas e preenchidas fase a fase.
- [~] **F07.21** **Sistema › Privacidade e segurança:** senha, chave de recuperação, regras de permissão salvas, registro do que saiu, hosts contatados, retenção de logs, pausar tudo, apagar histórico, sessões abertas.
- [~] **F07.22** **Sistema › Sistema:** atualizações (**beta/stable**), backup/exportar/importar, **sincronização PC↔notebook**, desempenho, **bateria**, RAM por componente com gráfico, versão, pastas confiáveis, navegador.
- [ ] **F07.23** **Sistema › Sobre:** versão, **"O que mudou"**, licenças.

**Testes:** snapshot de cada seção, teste de busca, de restaurar padrão, de desfazer, E2E de edição do TOML.
**Critério de aceite:** toda opção tem efeito real (ou estado "em breve" explícito), é buscável, restaurável e persiste.

---

### F08: Projetos · tamanho **L**

> **Auditoria PR #30 (`2020015`): Parcial.** Projetos/documentos/ramos e APIs portados; telas iced, GitHub, nota do projeto e herança/arquivamento coordenados pendentes.

**Objetivo:** a tela e o modelo de projeto, com isolamento, arquivos de referência e painel próprio.
**Dependências:** F06, F07.

- [~] **F08.01** **Tela Projetos:** **cartões com estado** (CI, pendências, última atividade). **Rodada Codex 21/10:** botão Novo projeto abre formulário real; cartões refletem a resposta do núcleo. Estados CI/pendências e aceite Windows continuam pendentes.
- [~] **F08.02** **Criar projeto** a partir de **repositório GitHub** (clona e vincula), **pasta local** ou **do zero** (cria nota + repositório); **conversa nova abre com o último projeto usado**. **Rodada Codex 21/10:** criação básica por POST /projects com nome, instruções e pasta existente opcional; memória pessoal isolada. Falha mantém formulário; sucesso exige resposta com ID. Clonar GitHub, criar nota/repositório e último projeto persistente continuam pendentes.
- [~] **F08.03** **Nota do projeto no vault** (`01 Projetos/<Nome>/<Nome>.md`), de onde o Orion lê estado e instruções; **instruções próprias** (prompt do projeto).
- [~] **F08.04** **Repositório vinculado** (PRs, CI, branch) e **pastas do disco vinculadas** (dentro do projeto o Orion só mexe nelas).
- [ ] **F08.05** **Modelo padrão** por projeto (ex.: Orion usa Claude, Vault usa Gemini).
- [~] **F08.06** **Arquivos de referência** (documentos que o Orion sempre lê).
- [~] **F08.07** **Configuração por projeto sobrepõe a global** (modelo, passos, autônomo, pastas, instruções), com a marca **"herdado"**.
- [~] **F08.08** **Painel do projeto:** pendências e tarefas, **PRs e CI**, conversas e notas recentes; **listas de tarefas em notas do vault, uma por projeto**.
- [~] **F08.09** **Rotinas do projeto** (ex.: revisar PRs do Orion a cada 4 h) — gancho para F12.
- [x] **F08.10** **Isolamento (R20):** fatos, documentos, conversas e avisos ficam no projeto; a memória pessoal não enxerga o projeto; um projeto não enxerga o outro.
- [~] **F08.11** **Arquivar:** move a nota para `04 Arquivo`, esconde as conversas, **pausa as rotinas**; ficam **para sempre** na aba **Arquivadas** (desarquivar ou apagar).
- [~] **F08.12** **Mover conversa** entre projetos e **nome do projeto no título** da conversa.
- [ ] **F08.13** Decisão D05 (o que mais um Projeto deve ter) registrada e implementada.

**Testes:** isolamento (um projeto não vê dados do outro), clonar repositório, arquivar/desarquivar sem perder nada.
**Critério de aceite:** criar, usar e arquivar um projeto real (o próprio Orion) sem vazamento entre projetos.

---

### F09: Painel e telas auxiliares · tamanho **M**

> **Auditoria PR #30 (`2020015`): Parcial.** Dados do painel e telas web de resultados/arquivadas existem; telas iced, métricas e resumo diário do plano pendentes.

**Objetivo:** as telas de acompanhamento da v1.
**Dependências:** F03, F04, F06.

- [~] **F09.01** **Tela Painel**, em ordem: **cotas das APIs gratuitas** (quanto resta hoje, atualizadas a cada 5 min), **uso das assinaturas** (Claude, Codex, Gemini), **saúde do sistema** (RAM, CPU, GPU, disco, serviços), **atividade da semana** (conversas, ações por tipo), **tempo da primeira resposta**, modelos e CLIs, política; gráficos de **barras e linhas simples** com abas **Hoje / Semana / Mês**.
- [~] **F09.02** **Métricas de desempenho locais**, **custo estimado** por conversa e **transparência de saídas** (resumo).
- [~] **F09.03** **Tela Resultados:** saídas guardadas com tipo, versão e data.
- [ ] **F09.04** **Tela Integrações** (lista de serviços com estado: conectado, token expirado) — preenchida conforme as integrações chegarem.
- [ ] **F09.05** **Resumo do dia ("Você fez"):** conversas e tarefas, notas gravadas e aprovações; aparece no Painel e no aviso das 23:59.
- [~] **F09.06** **Tela Arquivadas** (conversas e projetos).
- [~] **F09.07** **Estados vazios** de todas as telas com a ilustração do cinturão e uma ação sugerida.

**Testes:** snapshots, dados falsos, desempenho da tela com um ano de atividade.
**Critério de aceite:** números batem com o banco; nenhum segredo aparece (R17).

---

### F10: Instalador, atualizador, onboarding e release da v1 · tamanho **XL**

> **Auditoria PR #30 (`2020015`), atualizada em 20/10/2026: Parcial.** Setup visual, cópia real CPU/Vulkan, download do pacote de Releases (minisign + SHA-256), modelos, rollback, registro/atalho/autostart, identidade MSIX, desinstalador e atualizador (`orion update`) existem no código; `release.yml` pronto. Faltam chave de assinatura e primeiro release (você), onboarding no app, prova num Windows real, mini tela "O que mudou" e canais.

**Objetivo:** entregar o Orion instalado, atualizável e configurado no primeiro uso. Especificação visual e técnica completa do instalador em [[ORION - INSTALADOR (ESPECIFICAÇÃO)]]; o protótipo aprovado é a fonte da verdade.
**Dependências:** F01 a F09, F00 (spike de identidade de pacote).

#### F10.A `Orion Setup.exe` (`orion-setup`)
- [~] **F10.01** Janela **960×600 fixa**, sem moldura, barra de título de 36 px, cantos de 12 px, **idêntica ao protótipo** (tokens, fontes, espaçamentos da especificação §2 a §5).
- [~] **F10.02** **Céu animado** (motor da F01.29) com a constelação acendendo **uma estrela por etapa** (Betelgeuse, Bellatrix, **Cinturão em sequência 0/0,35/0,70 s**, Saiph, Rigel), nebulosa M42, estrelas de fundo com a **semente 7**, linhas que crescem em 1,6 s, conclusão com brilho que respira, estrela cadente rara.
- [~] **F10.03** **Som ambiente sempre ligado**, sem botão (4 osciladores + passa-baixas 900 Hz, volume 0,028 em 4 s, fade-out ao fechar). **Feito em 20/10/2026: botão de mudo removido; o som é sempre ligado, como na spec.**
- [~] **F10.04** **Transição entre etapas** (saída 0,34 s subindo 8 px; entrada 0,34 s vindo de 10 px; barra de etapas 0,7 s; troca do nome da estrela com fade) e **janela que nunca muda de tamanho** (palco rola por dentro).
- [~] **F10.05** **Etapa 1 Boas-vindas:** idioma + **checagem do PC** (Windows 11, RAM, GPU/VRAM/Vulkan, espaço por disco); bloqueia no Windows 10.
- [~] **F10.06** **Etapa 2 Pastas:** app (Arquivos de Programas), **modelos no disco com mais espaço** (sugerido), dados em `%ProgramData%\Orion`.
- [~] **F10.07** **Etapa 3 Modelos:** **cartões** com tamanho, RAM, uso e selo **"recomendado para o seu PC"**, pré-marcação por RAM/VRAM, total em GB.
- [~] **F10.08** **Etapa 4 Opções:** iniciar com o Windows, voz, fixar na barra de tarefas, relatório de falhas; **UAC** ao instalar.
- [~] **F10.09** **Etapa 5 Instalando:** copiar o app, **registrar o pacote esparso**, instalar o **certificado autoassinado**, criar atalhos (**Menu Iniciar** e **barra de tarefas**, sem Área de Trabalho), **serviço de atualização**, agendar o **download dos modelos**, com **desfazer em ordem inversa** em erro ou cancelamento; **fechar pede confirmação**. **Estado PR #30 (19/10/2026): cópia, modelos, registro em Apps instalados, atalho do Menu Iniciar, início automático opt-in, identidade MSIX com certificado autoassinado (pulada e dita sem SDK ou administrador), lista SHA-256, tarefa diária do atualizador e rollback de tudo isso implementados no código e testados só por registrador falso e em Linux. Sem prova num Windows real.**
- [~] **F10.10** **Final:** "Pronto. O Orion está no céu." + **Abrir Orion** (abre o onboarding).
- [~] **F10.11** **Download dos modelos em segundo plano:** o app já abre e usa as APIs enquanto baixa; pausar/retomar; **verificação de hash**; avisa antes de atualizar modelo.
- [~] **F10.12** **Desinstalador** (Configurações › Aplicativos): **pergunta separadamente** app / modelos / dados; **o vault nunca é tocado**. **Estado PR #30 (19/10/2026): Implementado no código, sem prova no Windows: `Orion Setup.exe --uninstall <pasta>` abre a janela com três caixas (app; modelos; dados, desmarcados), remove o app, o atalho, o início automático, a identidade, a tarefa e o registro; só apaga modelos registrados e dados de pasta com nome Orion; o vault nunca é tocado.**
- [~] **F10.13** Teste de **snapshot por etapa** comparado ao protótipo (mesmo quadro, mesma semente) e checklist de aceite da especificação §13.

#### F10.B Atualizador (`orion-updater`)
- [~] **F10.14** **Serviço de atualização com permissão** (sem UAC a cada update), checando **uma vez por dia** em segundo plano. **Estado PR #30 (19/10/2026): Parcial: tarefa agendada diária (SYSTEM, sem UAC) chama `orion-updater --check`; sem `feed_url` e chave pública ele não faz nada. Nunca rodou no Windows.** **Atualização 20/10/2026: o atualizador agora é o subcomando `orion update --check [--apply]|--rollback` de `orion.exe` (a tarefa diária chama esse comando); não existe mais `orion-updater.exe` e `ALLOWED_FILES` é só `orion.exe` e `orion-llama.exe`.**
- [~] **F10.15** Verificação: **assinatura minisign + SHA-256** do release (C7); recusa binário não assinado; **rollback** para a versão anterior. **Estado PR #30 (19/10/2026): Parcial: feed e SHA-256 conferidos, só versão maior, `.bak` e `--rollback`, testado com chaves minisign geradas no teste. Falta o par de chaves de release, a publicação do feed assinado e o rollback automático se o app novo não subir.** **Atualização 20/10/2026: o instalador também baixa o pacote com a mesma verificação (`latest-<variante>.json` + `.minisig`, variantes `cpu`/`vulkan`); `release.yml` existe e nunca rodou, pois falta a chave.**
- [ ] **F10.16** **Instala ao fechar** o app; **mini tela de atualização** (céu + **"O que mudou"**); canais **beta** e **stable**. *(Decisão de 20/10/2026: só o canal **stable** no começo; o **beta** nasce quando o notebook entrar.)*
- [ ] **F10.17** **CLIs (Claude, Codex, Gemini) atualizam sozinhas** e o Orion mostra a versão.

#### F10.C Onboarding no app (primeira abertura)
> **Decisões de 20/10/2026 (respostas do Antônio):** uma tela só, rolável, com o céu ao fundo (F10.18); **substituída em 21/10/2026 por seis etapas e uma tela final em tela cheia, com uma estrela acesa por tela.** Entra: usuário e senha, chave de recuperação (mostrada uma vez), pasta do vault, chaves das APIs (com link e teste de conexão), Telegram + 2FA, backup (Drive + local, cifrado com a senha, 7 cópias, exportar/importar) e importar do Claude/Codex (MCPs, skills e histórico antigo). Banco cifrado: caixa **marcada por padrão**, com opção de desmarcar. Windows Hello **nos dois papéis**: atalho de login e reserva do 2FA; a senha continua existindo e abre o banco. Ficam em Configurações: pastas confiáveis/bloqueadas, rotinas padrão e início com o Windows. Windows só no notebook, depois: o código do Hello compila e tem teste de lógica, mas o item fica `[~]` até a prova lá.
> **Mais decisões (20/10/2026):** a chave de recuperação é **salva num arquivo escolhido** (ex.: pen drive), não só mostrada; o histórico antigo importado vira resumos em `03 Recursos/<fonte>/Conversas/AAAA-MM/` do vault, seguindo o `claude.md`; o backup no **Google Drive já entra no onboarding** (login Google/OAuth na própria tela, token no cofre), além da cópia local cifrada.
> **Fechamento da lista (20/10/2026):** o onboarding também pergunta **como o Orion deve chamar você + idioma** (vai para a persona) e **pasta dos projetos + token do GitHub no cofre** (para a F08 já nascer ligada). Ficam de fora, por ora: resumo do que sai do PC, modelo/provedores, permissões opt-in explicadas, não perturbar, teste final de saúde e acessibilidade/escala (seguem em Configurações).
- [~] **F10.18** **Onboarding em seis etapas e tela final, em tela cheia, com o céu do instalador (uma estrela por tela, 7 telas; antes: tela única rolável)** implementado em `orion-app/src/views/onboarding.rs`, com componentes/fontes Grafite, pt/en e fluxo nativo antes do núcleo. Captura headless adicionada; aceite visual/Windows pendente.
- [~] **F10.19** **Perfil local:** usuário, senha forte/Argon2id, nome/idioma, recuperação em arquivo sem sobrescrever e cifra SQLCipher opt-out. Windows Hello opt-in com UserConsentVerifier + Credential Manager, entrada e reserva do 2FA. Avatar e aceite Windows permanecem pendentes.
- [~] **F10.20** **Vault:** detecta o Obsidian e permite confirmar/escolher a pasta; projetos e token GitHub no cofre. Remoto e autenticação de push seguem a configuração Git existente; configuração completa/revalidação do push ainda pendentes.
- [~] **F10.21** **APIs no onboarding:** chaves opcionais dos provedores e Brave no cofre, links oficiais, teste por consulta sem geração e aviso de políticas de treino. Aceite com contas/cotas reais e equivalência com todas as configurações ainda pendentes.
- [~] **F10.22** **Importar com prévia/hash:** MCP JSON/TOML desligado (sem env/headers), plugins ZIP e pastas de skills inativos; sugestões locais de Claude/Codex. Histórico JSON/JSONL vira resumo determinístico com trechos limitados em `03 Recursos/<fonte>/Conversas/AAAA-MM/`, vinculado a projeto existente. Não executa hooks/scripts; resumir com LLM e aceite em exports reais permanecem pendentes.
- [ ] **F10.23** **Pastas confiáveis** (Documentos, Área de trabalho, pasta dos projetos) e **pastas bloqueadas** (nenhuma por padrão).
- [~] **F10.24** **Rotinas padrão** criadas: **briefing às 7h** e **resumo diário às 23:59** (executam a partir da F12).
- [~] **F10.25** **Modo autônomo desligado**; **aviso de novidades** discreto.

#### F10.D Backup e dados
- [~] **F10.26** **Backup de banco/perfil cifrado**, sem auth.db/tokens, local + Google Drive via OAuth/PKCE e cofre; retenção de 7 cópias. Exportar/importar por CLI, restauração em pasta nova e nova senha. Serviço diário ligado ao perfil após desbloqueio; prova de agendamento e OAuth/Drive reais no Windows ainda pendente.
- [~] **F10.27** **Autenticação do push do vault** pelo Credential Manager; token do GitHub no cofre.

#### F10.E Pipeline de release
- [~] **F10.28** CI gera `Orion-Setup.exe`, `orion.exe` e `orion-llama.exe`, publica GitHub Releases CPU/Vulkan e assina os feeds com **minisign + SHA-256**. Updater é `orion update`, sem serviço/executável próprio. **Stable no começo**. Pendentes chave/segredos, primeiro release e prova fim a fim; certificado autoassinado não equivale à assinatura minisign, e Authenticode não é etapa do workflow atual.
- [ ] **F10.29** **Pacote de aceite da v1:** checklist abaixo.

#### F10.F Checklist de saída da v1
- [ ] Todas as fases F01 a F10 com critério de aceite cumprido e testes verdes no CI. F11–F27 não são pré-requisitos da v1.
- [ ] RAM parada **< 300 MB**; janela pronta **< 1 s**; primeira resposta visível **< 100 ms**.
- [~] Nenhum caminho de teste executa ação de risco sem aprovação; auditoria fail-closed verificada.
- [ ] Auditoria de segurança do **escopo F01–F10** sem achado alto aberto; revisão de design e acessibilidade desse escopo feita. A auditoria de todas as funções fica em F27.
- [ ] Instalar do zero e desinstalar limpo numa máquina limpa (Windows 11) e no notebook.
- [ ] **Uma semana de uso real** sem perda de dados, com backup restaurado com sucesso.
- [ ] Documentação suficiente para instalar, recuperar, atualizar e usar **F01–F10**, com este plano atualizado. A documentação final de todo o roadmap fica em F27.05.

---

## 6. Marco M2: v1.1 (trabalho e controle)

### F11: Ferramentas de código · tamanho **L**

> **Auditoria PR #30 (`2020015`): Pendente como entrega.** Shell/Git/processos genéricos não concluem esta fase: diff por bloco, ConPTY, GitHub/CI, ACP e extensão VS Code ainda faltam.

**Objetivo:** o painel da direita completo (Contexto, Arquivos, Diff, Terminal), GitHub e a extensão do VS Code.
**Dependências:** F05, F08, F04 (aprovações), F03 (ACP).

#### F11.A Painel em abas
- [~] **F11.01** **Arquivos:** explorador do projeto (pastas vinculadas), abrir no VS Code, indicar o que o Orion leu/alterou.
- [~] **F11.02** **Diff:** mudanças **antes de aprovar**, **por arquivo e por bloco** (como `git add -p`), com aprovar/negar por bloco e "desfazer".
- [~] **F11.03** **Terminal completo** (PowerShell de verdade): avaliar o **`iced_term`**; senão `alacritty_terminal` + **ConPTY**; cores, cópia/cola, redimensionar, abas.
- [ ] **F11.04** **Sessão compartilhada** com o agente (decisão da rodada 7), com **proteções**: indicador de **quem está digitando**, **entrada travada enquanto um comando do agente roda**, senhas coladas **mascaradas no histórico visto pelo modelo**.
- [ ] **F11.05** **Erro de comando:** ao falhar, o Orion **explica o erro** e **oferece correção** (passando pela política).

#### F11.B GitHub e CI
- [ ] **F11.06** **PRs abertos e CI** do projeto, **abrir PR** e **merge** (com aprovação); **status de PR e CI**, **logs do CI**, **rodar testes**; token no cofre (D06: reavaliar escopo).
- [ ] **F11.07** **Repositório do Orion:** branch e PR com revisão antes do merge; **snapshots só das telas alteradas**.
- [ ] **F11.08** **Revisar PRs dos seus repositórios** sozinho: comentário **rascunho para você aprovar**.
- [ ] **F11.09** **Changelog e mensagem de commit automáticos**; **PR semanal de atualização de dependências** (revisão obrigatória).

#### F11.C ACP e CLIs no painel
- [ ] **F11.10** Eventos das CLIs (ferramentas, arquivos, aprovações) aparecem na **linha de progresso** e no **Diff**; aprovações das CLIs **mapeadas para a política do Orion**.
- [ ] **F11.11** **Revisão cruzada** (Claude + Codex) disparável do chat.

#### F11.D Extensão do VS Code
- [ ] **F11.12** **Extensão do Orion para VS Code** (a única parte fora do Rust): painel de chat dentro do editor, enviar seleção/arquivo ao Orion, aplicar diff, abrir arquivo:linha; **canal local autenticado** (named pipe) com o núcleo; **nenhuma chave** na extensão.
- [ ] **F11.13** Publicação privada (`.vsix`) e atualização junto com o app.

**Testes:** E2E do Diff (aprovar bloco), do terminal (ConPTY), de falha de comando, de merge com aprovação; teste de segurança da extensão (canal só local, sem segredos).
**Critério de aceite:** uma tarefa de código real do começo ao fim: pedir, ver diff, aprovar por bloco, rodar testes, abrir PR.

---

### F12: Rotinas, automações, nuvem e monitores · tamanho **XL**

> **Auditoria PR #30 (`2020015`): Parcial.** Jobs, rotinas concedidas, Downloads, briefing e pesquisa portados; agendador completo, nuvem, monitores e proatividade ainda faltam.

**Objetivo:** o Orion trabalhando sozinho, no horário certo ou por gatilho, e delegando à nuvem quando o PC está desligado.
**Dependências:** F04, F05, F06, F08.

#### F12.A Rotinas
- [~] **F12.01** **Agendador** no núcleo (`orion-routines`) com **linguagem natural + prévia das próximas 3 execuções** ("seg a sex 8h"); criar **pelo chat**, **pelo formulário** da tela Rotinas ou por **`/rotina`**.
- [~] **F12.02** **Tela Rotinas:** liga/desliga, próxima e última execução (ok, falhou, pulada), **histórico de execuções**, botão **Rodar agora**, item curto na barra lateral com relógio.
- [ ] **F12.03** **Falha:** **3 tentativas com backoff**, depois avisa; **duas rotinas no mesmo horário rodam uma depois da outra**.
- [ ] **F12.04** **PC desligado/suspenso:** rodar ao ligar **se o atraso for menor que 6 h**, senão marcar **pulada**; **acordar o PC** pelo **agendador do Windows** quando a rotina pedir.
- [~] **F12.05** **Resultado** numa **conversa própria da rotina** (uma mensagem por execução), numa **nota no vault** e no Telegram (F14).
- [ ] **F12.06** **Rotinas críticas na nuvem** (rotinas do Claude Code na nuvem) quando o PC estiver desligado; o resultado volta por **commit no vault + Telegram**, e **ao ligar o PC o Orion lê do vault** e cria a mensagem.
- [~] **F12.07** **Rotinas do projeto** (F08.09) e **rotinas padrão**: **briefing às 7h** e **resumo diário às 23:59**.
- [~] **F12.08** **Briefing:** **agenda + e-mails (só leitura)**, **PRs/CI dos projetos**, **tarefas do vault**; formatos **texto no chat + Telegram**, **nota diária no vault** e **áudio estilo podcast de 2 a 5 min** (a voz chega na F15).
- [~] **F12.09** **Retrospectiva semanal** (domingo, nota no vault: projetos, tempo, gastos, aprendizado); partes ligam quando as fontes existirem.
- [~] **F12.10** **Lembretes e timers** sobre o agendador (barra lateral e Telegram).

#### F12.B Automações por gatilho
- [~] **F12.11** **Gatilhos:** frase de voz (F15), horário, **evento do PC** (ligar, abrir app, conectar dispositivo), **mensagem do Telegram** (F14) e **duas palmas** (F15); **sequência de ações** em ordem.
- [~] **F12.12** **Criação por você**, pelo chat ou pela tela, **revisando os passos antes de ativar**.
- [~] **F12.13** **Falha numa automação:** **para e avisa**, mostrando o passo que falhou.
- [~] **F12.14** **Aprovação:** ações leves (abrir app, tocar música) **sem pedir**; escrever arquivo e comandos que alteram algo **pedem OK**; apagar, formatar e force-push **pedem confirmação extra**.

#### F12.C Nuvem: Claude e Codex
- [~] **F12.15** **`/nuvem claude <tarefa>`:** `claude --remote "<tarefa>"` cria sessão do Claude Code na nuvem, na assinatura; `claude --teleport` traz para o PC; só quando você pedir; só pelas CLIs oficiais. *(20/10/2026: implementado em `orion-core/src/nuvem.rs` (opt-in `ORION_NUVEM`), sem prova com contas reais; sem cancelamento remoto, sem log ao vivo, sem aviso no Telegram. "Segundo plano" do Orion (`/tarefas-bg`) fica ao lado.)*
- [~] **F12.16** **`/nuvem codex <tarefa>`:** `codex cloud exec`, `codex cloud list` (JSON) para acompanhar, `codex apply` para trazer o diff (flags a confirmar na doc oficial). *(20/10/2026: implementado em `orion-core/src/nuvem.rs` (opt-in `ORION_NUVEM`), sem prova com contas reais; sem cancelamento remoto, sem log ao vivo, sem aviso no Telegram. "Segundo plano" do Orion (`/tarefas-bg`) fica ao lado.)*
- [~] **F12.17** **Tela Nuvem:** tarefas em andamento nos dois serviços (estado, link, **log ao vivo**, **mandar mensagem para a sessão**, **cancelar**, **abrir no navegador**, botão **Trazer resultado**). *(20/10/2026: implementado em `orion-core/src/nuvem.rs` (opt-in `ORION_NUVEM`), sem prova com contas reais; sem cancelamento remoto, sem log ao vivo, sem aviso no Telegram. "Segundo plano" do Orion (`/tarefas-bg`) fica ao lado.)*
- [~] **F12.18** **Ao terminar:** **avisa e espera** (notificação e Telegram) com o link da sessão ou do PR; trazer o diff é decisão sua. *(20/10/2026: implementado em `orion-core/src/nuvem.rs` (opt-in `ORION_NUVEM`), sem prova com contas reais; sem cancelamento remoto, sem log ao vivo, sem aviso no Telegram. "Segundo plano" do Orion (`/tarefas-bg`) fica ao lado.)*
- [~] **F12.19** **Codex que falha:** avisa o erro e **pergunta** (tentar de novo, mandar para o Claude ou desistir); **não troca de serviço sozinho**. *(20/10/2026: implementado em `orion-core/src/nuvem.rs` (opt-in `ORION_NUVEM`), sem prova com contas reais; sem cancelamento remoto, sem log ao vivo, sem aviso no Telegram. "Segundo plano" do Orion (`/tarefas-bg`) fica ao lado.)*

#### F12.D Monitores
- [ ] **F12.20** **Monitor de página/site que mudou**, **releases das libs e ferramentas que você usa** (iced, rmcp, Claude Code) e **notícias de temas**; avisa pelo canal escolhido; guarda histórico no vault.
- [ ] **F12.21** Monitor de **preço de produto** (usa o mesmo motor da F23.08; a tela Vida foi cortada em 21/10/2026).

#### F12.E Proatividade
- [ ] **F12.22** **Motor de sugestões proativas:** avisos importantes **e** sugestões, **no máximo 3 por dia**, agrupadas, **nunca em modo foco/jogo ou com o "não perturbe"**; aparecem como chips na tela inicial e no canal escolhido (matriz de notificações).
- [ ] **F12.23** **O que observa** (cada fonte com interruptor em Configurações › Agente): **janela/app ativo** (só totais por dia), **histórico de conversas** (padrões repetidos), **git/CI/projetos** e **agenda**; **poucas e relevantes**: pendências, cotas e falhas.
- [ ] **F12.24** **Feedback:** dispensar, "não sugerir mais isso" e 👍/👎 por sugestão (registro local) para calibrar a frequência.

**Testes:** simulação de relógio (atrasos, suspensão, duas rotinas juntas), retentativas, E2E de automação com falha no meio, simulação de CLIs de nuvem.
**Critério de aceite:** o briefing das 7h e o resumo das 23:59 rodam por uma semana sem intervenção; uma rotina crítica roda na nuvem com o PC desligado e aparece depois no app.

---

### F13: Orçamento de uso (assinaturas e APIs) · tamanho **L**

> **Auditoria PR #30 (`2020015`): Parcial.** Registro/calibragem/reset/projeção e tetos 95%/20% feitos em chamadas estimadas; leitura oficial, precisão, fila e preempção pendentes.

**Objetivo:** economizar com qualidade e usar a cota semanal quase toda, medindo com honestidade.
**Dependências:** F03, F09. **D03 decidido em 20/10/2026:** a leitura de `/usage` e `/status` pelas CLIs está liberada, com queda para a estimativa local e aviso se o formato mudar; deixou de estar bloqueada.

- [ ] **F13.01** **Leitura de uso do Claude:** rodar `/usage` na CLI do Claude Code e ler a janela de 5 h e a da semana; se o formato mudar, **cai para a estimativa local e avisa**. *(20/10/2026: liberado pela D03; ainda não implementado.)*
- [ ] **F13.02** **Leitura do Codex:** `/status` da CLI (só depois de a sessão receber uma mensagem; pode estar desatualizado; divergências conhecidas); **Gemini:** só estimativa local. *(20/10/2026: liberado pela D03; ainda não implementado.)*
- [~] **F13.03** **Registro local de cada chamada** por provedor como base; **calibragem manual** quando a estimativa desviar; **dia e hora do reset** por provedor. **Estado PR #30: Registro/reset/calibragem feitos em chamadas estimadas; precisão real pendente.**
- [~] **F13.04** **Teto de 95%** da cota semanal com **5% de reserva**; **projeção diária** do que sobra no reset pelo ritmo de uso. **Estado PR #30: Tetos/projeção implementados e testados; aceite de precisão da fase pendente.**
- [ ] **F13.05** **Alerta em 80%** usado; aviso quando a cota está no limite.
- [~] **F13.06** **Teto por tarefa** (20% da cota semanal, respeitando a janela de 5 h; **20% confirmado em 17/10/2026**, C18/D02). **Estado PR #30: Teto confirmado e implementado; aceite completo da fase pendente.**
- [ ] **F13.07** **Tela Orçamento** (Configurações): fila de automelhoria escolhida por você (ferramentas, código, lógica, performance, segurança, auditoria e manutenção do vault, testes e cobertura, refatoração, pesquisa), gasto e projeção.
- [~] **F13.08** **Regra:** pedido seu **sempre tem prioridade**; tarefa de fundo **para** se você pedir algo.

**Testes:** leitura com saídas gravadas (formatos antigos e novos), falha de leitura, projeção com histórico simulado.
**Critério de aceite:** a projeção bate com o uso real dentro de uma margem registrada; formato inesperado nunca derruba o app.

**Entrega parcial no PR #30 (17/10/2026):** painel nativo/API e reservas persistidas em chamadas estimadas, calibragem manual, reset por provedor, projeção e tetos de 95% semanal / 20% por tarefa (valor de 20% confirmado pelo pedido desta sessão), com janela de 5 h configurável. Não lê `/usage` nem `/status` oficiais e não implementa fila de automelhoria. A unidade de estimativa é explícita na interface.

---

## 7. Marco M3: Telegram

### F14: Telegram · tamanho **L**

> **Auditoria PR #30 (`2020015`): Parcial.** Bot HTTP próprio, default-deny, cartões inline, 2FA e áudio feitos; não usa teloxide. PIN/Hello, nuvem, fila, energia/WoL e dois aparelhos reais pendentes.

**Objetivo:** o celular pelo Telegram, com segurança estrita e o 2FA.
**Dependências:** F04 (aprovações), F12 (rotinas), F02 (gancho do 2FA).

- [~] **F14.01** Cliente com `teloxide`; **só o seu ID** responde, **conversa privada**; o resto é **ignorado em silêncio**; token no cofre; **troca do token só quando você pedir**. **Estado PR #30: Bot HTTP próprio com default-deny e cofre; não usa teloxide.** *(Decisão confirmada 20/10/2026: manter o cliente próprio; `teloxide` descartado.)*
- [ ] **F14.02** **PIN próprio** além do ID para ações críticas (ligar, desligar, aprovar); **login novo ou IP desconhecido avisa e bloqueia** a ação.
- [~] **F14.03** **Conversar com o Orion:** as conversas aparecem no app do PC com a marca **"via Telegram"** (ícone do avião); a **resposta completa aparece no chat do PC**; no Telegram, resposta longa vira **resumo + botão "ver completo"** (abre a conversa no PC ou envia `.md`).
- [ ] **F14.04** **PC ocupado:** mensagens vão para uma **fila** e são respondidas quando terminar.
- [~] **F14.05** **Aprovar ou negar** com botões **Aprovar/Negar** (R22), só pelo clique do ID autorizado; **frase nunca decide**; cartão mostra o que se vê inteiro (R11). **Estado PR #30: Cartões remotos vinculados ao ID/cartão/sessão implementados; E2E real pendente.**
- [~] **F14.06** **Comandos do PC:** `/status` (ligado, RAM), `/rotina rodar`, `/nuvem codex <tarefa>`, `/nuvem claude <tarefa>`.
- [ ] **F14.07** **Ligar e desligar o PC:** desligar pede **confirmação por botão**; **ligar** por **Wake-on-LAN** enviado por um **roteador ou Raspberry** ligado na mesma rede (subnet router da Tailscale); BIOS e adaptador de rede configurados.
- [~] **F14.08** **Áudio:** mandar áudio (transcrição pelo Whisper; só o texto é guardado, o áudio é descartado) e **receber resposta em áudio** (F15); **arquivos grandes** pelo Bot API local (opcional). **Estado PR #30: Entrada e resposta em áudio implementadas; dois aparelhos/Bot API local pendentes.**
- [~] **F14.09** **Rotinas e avisos** (resultado de rotina, aprovação, cota, tarefa na nuvem) chegam pelo Telegram conforme a **matriz evento × canal**.
- [~] **F14.10** **2FA por código no Telegram**: ao entrar de novo depois de sair da conta, o código vai para o seu ID; Windows Hello como reserva (C9). **Estado PR #30: Código Telegram entregue; Hello de reserva ausente.**
- [ ] **F14.11** **Aviso** de login no Telegram a cada tentativa errada de senha (F02.12).
- [ ] **F14.12** **Segurança de rede:** Tailscale **só com dispositivos da sua conta** (ACL).

**Testes:** API falsa do Telegram (default-deny, ID errado, grupo, replay), E2E de aprovação por botão, fila com PC ocupado, teste real em dois aparelhos.
**Critério de aceite:** aprovar uma ação do PC pelo celular; ninguém além do seu ID consegue qualquer resposta.

**Entrega parcial no PR #30 (17/10/2026):** aprovações remotas do PC com botões vinculados ao cartão, sessão e argumentos originais; 2FA opt-in antes da emissão de sessão (código de uso único, 5 min, 5 erros); áudio de entrada local/Groq opt-in e resposta em MP3 com edge-tts. PIN próprio, Hello, energia/WoL, fila de nuvem e teste com dois aparelhos ainda pendem.

---

## 8. Marco M4: Voz

### F15: Voz · tamanho **XL**

> **Auditoria PR #30 (`2020015`): Parcial.** cpal, VAD por energia, palmas, Whisper, TTS, AEC3 e barge-in integrados; Silero, wake word final, testes acústicos/GPU e UI completa pendentes.

**Objetivo:** falar com o Orion: palavra de ativação, palmas, voz ao vivo, mini barra, respostas faladas e legenda.
**Dependências:** F00.22 (spike de áudio), F03, F04, F05.

**Entrega parcial no PR #30 (17/10/2026):** `orion-voice` + listener do core, captura cpal, mono 16 kHz, VAD por energia, palmas calibradas 150–700 ms só para abrir, teto por hora e corte da captura por pausa/pânico. openWakeWord exige modelo Orion local; Whisper CPU/Vulkan exige modelo ggml; Groq exige opt-in separado; edge-tts pt-BR masculino já integrado. Controle nativo mostra a transcrição. AEC3 e barge-in já estão integrados; faltam Silero, modelo Orion final, prova acústica/GPU e a UI completa de voz/legenda/constelação.

#### F15.A Áudio base (`orion-voice`)
- [~] **F15.01** Captura com `cpal` (**microfone padrão do Windows**), reamostragem, **Silero VAD**, **cancelamento de eco `aec3`** (referência: áudio que o Orion toca). **Estado PR #30: cpal/reamostragem/AEC3 feitos; Silero ausente.**
- [~] **F15.02** **Sob demanda:** só carrega quando a voz está ligada; **pílula de estado** (Ouvindo, Pensando, Falando) e **indicador de microfone**; **modo "não escutar" só pelo app**, sem atalho.
- [~] **F15.03** **Privacidade (R18):** só a **transcrição** é guardada, o **áudio é descartado**; teto de ativações por hora; pausa; registro sem áudio nem texto.

#### F15.B Entrada de voz
- [~] **F15.04** **STT local:** **Whisper large-v3-turbo** via `whisper-rs` com **Vulkan**; **só português**; **silêncio de 1 s** para parar; **Groq Whisper opcional** para PC sem GPU, desligado por padrão (C2).
- [~] **F15.05** **Palavra "Orion"** com `oww-rs` (openWakeWord, modelo treinado, local), **sensibilidade média**, ajustável; **só abre o microfone**; **traz a janela para o foco**. **Estado PR #30: Engine/treino candidato feitos; candidato reprovado para escuta contínua, modelo final pendente.** *(Decisão de 20/10/2026: a palavra "Orion" fica **desligada na v1**, sem gravações para validar o modelo; ficam as palmas e o atalho. Reativar quando houver amostras reais.)*
- [~] **F15.06** **Duas palmas** (150 a 700 ms entre elas), **só abrem** (nunca aprovam), calibração, teto por hora; **traz a janela para o foco**. **Estado PR #30: Detector/calibração/foco implementados; ruído/música reais pendentes.**
- [~] **F15.07** **A fala transcrita aparece antes da resposta**; comandos de voz passam pela **política normal** (**aprovar nunca é por voz**).
- [~] **F15.08** **`/voz`** liga e desliga; **Ctrl+Alt+V** para a voz ao vivo.

#### F15.C Saída de voz
- [~] **F15.09** **TTS:** **edge-tts pt-BR, voz masculina**, atrás de uma interface trocável; **só responde por voz quando o pedido foi por voz**; **respostas faladas curtas**, texto completo na tela; **sem voz offline**; teste de alternativas (D04).
- [ ] **F15.10** **Botão "ouvir"** nas respostas e **leitura em voz alta** de documentos.

#### F15.D Voz ao vivo e mini barra
- [~] **F15.11** **Voz ao vivo (Ctrl+Alt+V):** pipeline STT → LLM → TTS com **interrupção falando por cima** (barge-in via aec3); **Gemini Live** como modo opcional quando houver internet (C3); **só conversa, sem ferramentas nem memória** (R18), com **duração máxima** e registro de início e fim. **Estado PR #30: Barge-in do listener feito; não equivale ao modo completo sem ferramentas e seus atalhos.**
- [~] **F15.12** **Mini barra flutuante no topo, centralizada**: **constelação reagindo ao áudio** (o cinturão acende com o som), **transcrição ao vivo + resposta curta**; resposta longa abre a janela.
- [ ] **F15.13** **Constelação da tela inicial reage à voz** (pílula de estado + cinturão acendendo com o áudio).

#### F15.E Legenda e áudio do sistema
- [~] **F15.14** **Legenda ao vivo:** faixa flutuante na base da tela, **sobre qualquer janela**, com a transcrição do **áudio do PC** (loopback). **Estado 21/10/2026:** o backend existe (`GET /legenda` e `orion legenda <segundos>` imprimem as linhas); o app iced só tem uma faixa estática de demonstração ligada ao interruptor em Configurações › Câmera (`overlays::caption`), sem ler `/legenda`. A única janela real era a página HTML do front antigo, removida; falta a faixa nativa sobre qualquer janela.
- [ ] **F15.15** **Tradução ao vivo:** áudio em inglês → **legenda em português**.
- [ ] **F15.17** **Configurações › Voz** funcionando: sensibilidade da palavra e das palmas, microfone e alto-falante, voz e velocidade do TTS, teste de gravação, opção Groq (C2), duração máxima da voz ao vivo.
- [~] **F15.16** **Briefing em áudio** (podcast de 2 a 5 min) e **respostas por voz no Telegram** (F14).

**Testes:** taxa de erro de palavras em português (conjunto fixo), **ativações falsas por hora**, latência fim a fim, teste de eco com alto-falante, teste de ruído; E2E "Orion, que horas são".
**Critério de aceite:** pedir por voz e ouvir a resposta curta; interromper falando por cima sem eco; **nenhuma ativação falsa** numa hora de TV ligada.

---

## 9. Marco M5: Grafo e efeitos

### F16: Grafo 3D da memória · tamanho **XL**

> **Auditoria PR #30 (`2020015`): Parcial só nos dados.** Backend de grafo/parser e grafo WebGL existem; nenhum renderer/layout/compute nativo wgpu ou aceite de desempenho entregue.

**Objetivo:** a visualização do vault em 3D, como a do Obsidian, sobre o céu do Orion, com **wgpu puro**.
**Dependências:** F00.15/F00.16 (spikes), F06 (vault e índice), F01.29 (motor do céu).

#### F16.A Dados e índice
- [~] **F16.01** **Nós** = **todas** as notas `.md` (incluindo Arquivo (04), Relatórios de Mudanças e notas do Orion); notas do **Arquivo aparecem esmaecidas**; **arestas** = `[[wikilinks]]` resolvidos por nome do arquivo ou `aliases`, ignorando `#cabeçalho`, `^bloco` e `|apelido`, sem direção e sem duplicatas; **tags como nós (opção)**.
- [~] **F16.02** **Tipo e cor** pelo `type` (projeto âmbar/Betelgeuse, área laranja, conceito azul/Rigel, pesquisa verde, recurso cinza, conversa rosa, relatório cinza apagado e **escondido por padrão**) — **paleta da identidade**; **tamanho** pelo log do nº de links; **hubs (≥ 45 links) e projetos** com brilho e rótulo fixo.
- [ ] **F16.03** **Posições e arestas guardadas no índice** pelo **hash do commit** do vault; recalcula só as notas que mudaram.
- [ ] **F16.04** **Acima de 500 notas**, **agrupa por pasta** (clusters que abrem ao aproximar).

#### F16.B Layout e renderização
- [ ] **F16.05** **Layout 3D em compute shader (wgpu)** em segundo plano, com mola de força 1/min(grau), repulsão de longo alcance, gravidade leve e **casca esférica**; **fallback na CPU** (Barnes-Hut em octree) para GPU integrada; escala a **10 mil notas**.
- [ ] **F16.06** **Widget `shader` do iced** com **instancing**: um draw para os nós (sprites billboard, **blending aditivo**, núcleo branco + halo da cor do tipo), um para as arestas (linhas finas e quase transparentes) e um para o céu.
- [~] **F16.07** **Fundo espacial** do `sky.js`: estrelas com posição real (ascensão reta e declinação), profundidade proporcional à distância, **constelação de Órion ao fundo**, Betelgeuse âmbar e Rigel azul; atrás do grafo no modo "fundo" (≤ 20 fps, sem paralaxe).
- [ ] **F16.08** **Rótulos com `glyphon`:** só projetos, áreas, hubs e a nota em foco; os outros no hover ou com zoom; **sem sobreposição** (prioridade).
- [~] **F16.09** **Movimento:** rotação lenta quando parado (**abre com o vault inteiro girando devagar**); desligada com "movimento reduzido"; **60 fps só na interação, 20 parado**, **pausa fora de vista**.

#### F16.C Interação
- [~] **F16.10** **Girar (orbit), zoom, arrastar a câmera**; **arrastar um nó** o reposiciona.
- [~] **F16.11** **Clique:** a câmera **voa até a nota em ~900 ms** e abre no **painel do Orion** (tipo, caminho, tags, data, status, notas ligadas) com **vizinhos em destaque**; **duplo clique abre o editor**; ações secundárias: abrir no Obsidian, abrir no GitHub, copiar caminho; nota selecionada com **anel branco**.
- [~] **F16.12** **Grafo local** a partir de uma nota (profundidade 1 ou 2); **filtros** por tipo, pasta, tag e projeto; **grupos de cor configuráveis**.
- [~] **F16.13** **Busca** por título, tag ou pasta com lista navegável pelo teclado; **Enter voa até a nota**.
- [~] **F16.14** **Linha do tempo** (slider de data para ver o vault crescer) e **destaque das notas que o Orion leu** na última resposta (**anel branco pulsante**).
- [~] **F16.15** **Acessibilidade:** busca + lista + painel de detalhes como caminho equivalente ao canvas (WCAG 2.1.1).

**Testes:** snapshots do grafo com semente fixa, desempenho com 1k, 5k e 10k nós, teste de fallback na CPU, teste de atualização incremental, teste de teclado.
**Critério de aceite:** o vault real (≥ 895 notas, 3.804 links) abre girando a 60 fps na interação, com rótulos sem sobreposição e busca funcionando; a GPU integrada abre a versão simplificada.

---

### F17: Efeitos visuais (bloom e blur de vidro) · tamanho **M**

> **Auditoria PR #30 (`2020015`): Pendente.** Céu do instalador não implementa bloom/blur do grafo nem qualidade/bateria automáticas desta fase.

**Objetivo:** o acabamento do grafo e dos painéis, com qualidade automática.
**Dependências:** F16.

- [ ] **F17.01** **Bloom:** renderizar grafo e céu numa textura HDR (`Rgba16Float`), extrair acima de um limiar, **downsample/upsample (dual Kawase)**, somar por cima; **3 a 5 passes WGSL**.
- [ ] **F17.02** **Blur de vidro** dos painéis **dentro do grafo** (borrar o retângulo atrás do painel com a textura do frame); fora do grafo, **Mica/Acrylic** ou fundo sólido.
- [ ] **F17.03** **Qualidade automática (notebook):** detecta a GPU, mede o custo e **desliga bloom e blur** se pesar; **economia na bateria**; redução automática com pouca RAM.
- [ ] **F17.04** Opção manual em Aparência (bloom/blur on/off, qualidade).
- [x] **F17.05** **Plano B** registrado: Bevy/`three-d` só para o grafo, se o custo ficar alto.

**Critério de aceite:** bloom visível e estável; o notebook mantém 60 fps na interação ou cai para o modo simples sozinho.

---

## 10. Marco M6: Câmera e visão

### F18: Câmera e visão · tamanho **XL**

> **Auditoria PR #30 (`2020015`): Parcial.** Câmera nokhwa e extrator Haar heurístico/LBPH adicionados em 975e635; detector YOLO ainda retorna vazio. Indicadores, reconhecimento real, UI e aceite pendentes.

**Objetivo:** o Orion enxerga, com os dois modos, tudo local e com indicador sempre visível.
**Dependências:** F04, F15 (presença e voz), F06.

- [~] **F18.01** **Câmera com `nokhwa`** (Media Foundation), **dois modos com interruptores separados**: foto **sob pedido** ("olha isso") e **presença contínua local**; **ponto vermelho** na barra de título e no ícone da bandeja sempre que ativa; nada de imagem guardada na presença. **Estado PR #30: Captura nokhwa implementada; indicadores e aceite dos dois modos pendentes.**
- [~] **F18.02** **Ver e responder sobre uma foto** (modelo de visão: local — Moondream/MiniCPM-V — ou provedor com visão); a foto vale **só para o turno em que chegou**.
- [~] **F18.03** **Detecção de objetos local** (modelo pequeno **ONNX em CPU**, YOLO). **Estado PR #30: Não entregue: detectar_jpeg retorna sempre lista vazia.**
- [~] **F18.04** **"Onde eu deixei?":** lembra onde viu objetos; guarda **só rótulos, sem imagem** (`objeto, lugar, hora`), **retenção sem prazo**, último lugar + histórico compactado (uma linha por mudança); **lista inicial editável**: chaves, carteira, celular, fone de ouvido, óculos, controle remoto, carregador, notebook, mochila, documentos (RG/CNH), cartões, remédios, garrafa/copo, mouse e teclado sem fio, pendrive, relógio, guarda-chuva, caneta.
- [~] **F18.05** **Reconhecer o Antonio** (rosto): **clássico e local (Haar + LBPH)**, sem IA e sem nuvem; **falso positivo não age, pede confirmação**; cadastro guiado; ArcFace só se o spike mostrar necessidade (C21). **Estado PR #30: Extrator Haar heurístico/LBPH implementado; falta validação facial real e cadastro guiado.**
- [~] **F18.06** **Presença contínua:** ao **sair do PC** pausa a voz que estiver falando e **bloqueia o Orion** (pede senha na volta); **na volta** cumprimenta com o que aconteceu ("Voltou. A rotina das 14h terminou."); alimenta a **presença** do modo jogo e do bem-estar.
- [~] **F18.07** **Legenda ao vivo** com áudio (F15.14 e F15.15) integrada ao painel de câmera quando aplicável.
- [~] **F18.08** **Captura de tela "Isso?"** com **detecção e blur** de dados sensíveis antes do envio (ver F20).
- [~] **F18.09** **Tela Câmera e visão** nas Configurações: os dois modos, objetos lembrados, legenda ao vivo, retenção.

**Testes:** conjunto de imagens para objetos, taxa de falso positivo do rosto, teste de bloqueio ao sair, verificação de que **nenhuma imagem é gravada** na presença.
**Critério de aceite:** "onde eu deixei as chaves?" responde com lugar e hora a partir de rótulos; sair do PC bloqueia o Orion; ponto vermelho sempre acende com a câmera.

---

## 11. Marco M7: Extensões

### F19: Extensões (MCP, skills, plugins WASM, MCP Apps, importações, clientes externos) · tamanho **XL**

> **Auditoria PR #30 (`2020015`): Parcial.** MCP próprio, OAuth, skills/plugins/importação e credenciais de leitura existem; rmcp não é usado. WASM/WIT, MCP Apps, proteção completa contra rug pull e UI final pendentes.

**Objetivo:** suportar **tudo o que Claude e Codex têm e for possível no Orion**.
**Dependências:** F04, F05, F07.

#### F19.A MCP
- [~] **F19.01** **Cliente MCP** com `rmcp`: servidores **stdio** (comando local) e **HTTP/SSE** (URL), com OAuth quando exigido; ferramentas aparecem no chat **com o nome do servidor**. **Estado PR #30: Cliente próprio stdio/HTTP e OAuth implementados; sem rmcp, UI/servidores reais pendentes.** *(Decisão confirmada 20/10/2026: manter o cliente próprio; `rmcp` descartado.)*
- [x] **F19.02** **Classe de risco vem da configuração do Orion**, nunca do servidor; sem classe = **execução** (R12); o servidor sobe **sem variáveis `ORION_*` nem segredos**.
- [~] **F19.03** **Permissão por ferramenta** (sempre, perguntar, nunca; padrão **perguntar**); **leitura permitida, escrita pede aprovação** para plugins e MCPs de terceiros.
- [~] **F19.04** **Proteção contra tool poisoning e rug pull:** **hash** das ferramentas e das descrições na aprovação; **se mudar, desativa até você reaprovar**; **classificador de injection lê as descrições**; o que o servidor anuncia é **tratado como texto não confiável**.
- [~] **F19.05** **Tela Conectores** (abas **Instalados / Descobrir / Skills**), com estado da conexão, ligar/desligar; **Descobrir** usa o registro MCP público; **antes de instalar mostra**: ferramentas que expõe, **comando ou URL exato**, **origem (repositório)** e **permissões pedidas**.
- [~] **F19.06** **`/mcp`** e **`/skill`** para listar e ativar.

#### F19.B Skills e importação
- [x] **F19.07** **Skills em Markdown** (pastas com `SKILL.md`, formato das skills do Claude), carregadas **sob demanda**.
- [ ] **F19.08** **Skills automáticas:** o Orion **propõe** uma skill após uma tarefa de vários passos e **você aprova** (a pesquisa mostra que skill sem verificação piora o agente); verificação automática antes de propor.
- [~] **F19.09** **Importar do Claude e do Codex:** ler `mcp.json`/config e as skills existentes no PC e oferecer para ativar (também no onboarding, F10.22).

#### F19.C Plugins WASM
- [ ] **F19.10** **Plugins próprios em Rust/WASM** (Wasmtime + Component Model, alvo `wasm32-wasip2`), **sandbox**, **capacidades declaradas** (arquivos, rede, etc.), limites de memória e de tempo (fuel/epoch); instalação com **aprovação** mostrando as capacidades.
- [ ] **F19.11** Interface WIT estável, SDK e **plugin de exemplo**; atualização e remoção.

#### F19.D MCP Apps
- [ ] **F19.12** **MCP Apps só em texto, pós-1.0**, decisão de 20/10/2026. Sem WebView2 nem telas interativas embutidas; fora do aceite da v1.

#### F19.E Clientes externos
- [~] **F19.13** **Orion como servidor MCP** para outros programas (ex.: **Claude Code**): tela que gera **credenciais de leitura** com **nome, contexto, validade e permissões por leitura**; **não permite administrar o Orion**; revogáveis.
- [~] **F19.14** Registro do uso por cliente e limites de taxa.

#### F19.F Extensão de navegador (decisão D11)
- [ ] **F19.15** **Extensão para Chrome/Edge** para controlar apps web, com canal local autenticado; **aprovação por ação**; **Decidido em 20/10/2026: só depois da 1.0**; o navegador guiado por CDP basta por ora.

**Testes:** servidor MCP falso malicioso (descrição envenenada, mudança depois da aprovação, tentativa de ler segredos), plugin WASM tentando sair da sandbox, cliente externo tentando escrever, E2E de instalação com as 4 informações.
**Critério de aceite:** instalar um MCP real com aprovação informada; alterar a descrição dele desativa a ferramenta; um plugin sem a capacidade de rede **não consegue** abrir conexão.

---

## 12. Marco M8: funções novas e integrações
> Ordem decidida em 20/10/2026: F25.06 primeiro após a v1, depois F20. As demais funções respeitam suas dependências; casa/Alexa seguem adiados sem data.

### F20: Tela, captura e reuniões · tamanho **XL**

> **Auditoria PR #30 (`2020015`): Parcial.** Ponte/captura, memória de tela, OCR externo e legenda portados; seleção/overlays nativos, PII visual, UI Automation e reuniões pendentes.

**Objetivo:** o Orion entende o que está na tela e nas chamadas, sempre local e com consentimento.
**Dependências:** F04, F06, F15, F18.08.

#### F20.A Captura rápida e "Isso?" (a "Ponte de desktop")
- [~] **F20.01** **Captura rápida (Win+Shift+O):** barra **estilo Spotlight no centro** da tela, sobre qualquer app; uma linha vira **tarefa, lembrete, gasto ou nota** no `00 Inbox` do vault; **fecha com Esc** e mostra o **tipo detectado**; ambígua ("reunião amanhã com o Lucas") salva como **lembrete** e o tipo **troca com um clique**.
- [~] **F20.02** **"Isso?" (Win+Shift+I):** **seleção de região** (Enter captura a tela cheia) e abre a pergunta ao modelo de visão; **detecta e borra** CPF, cartão e senhas visíveis **antes do envio**; a captura só grava em **pasta própria (últimas 20)**; a explicada é **apagada** (R19).
- [ ] **F20.03** **Tradução de tela (Ctrl+Alt+T)** sobre a seleção.
- [ ] **F20.04** **Click to Do:** atalho próprio + clique em qualquer coisa na tela abre ações: **resumir, traduzir, explicar, salvar no vault, criar tarefa**.
- [~] **F20.05** **OCR de tela:** primeiro o **OCR local do Windows** (`Windows.Media.Ocr`, graças ao pacote esparso) ou ONNX; **API só se você ligar**, com **mascaramento de CPF e cartão antes do envio** (a decisão original diz "via API"; confirmar a preferência).

#### F20.B Memória da tela
- [~] **F20.06** **Texto da tela** via **acessibilidade (UI Automation)** com **OCR como reserva**, **só texto, sem imagem**, **local e opt-in**; **busca** ("o que eu vi ontem sobre X"); guardado no banco cifrado.
- [~] **F20.07** **Exclusões:** lista de apps/janelas excluídos; janela **sem título** ou **bloqueada** não é capturada; **retenção de 7 dias** (configurável); **pausa**; indicador visível.

#### F20.C Reuniões
- [ ] **F20.08** **Detecta a chamada** (Meet, Teams, Discord, Zoom) e **pergunta** se pode gravar; **sem bot** entrando na call.
- [ ] **F20.09** **Transcrição local** (Whisper) do áudio do PC + microfone; **resumo e tarefas** gravados no vault, linkados ao projeto; **só texto** é guardado.
- [ ] **F20.10** **`/gravar`** para iniciar/parar manualmente; aviso de consentimento dos participantes.

**Testes:** máscara de dados sensíveis em capturas de teste, exclusão de janelas, retenção, E2E da captura rápida (todos os tipos), consentimento antes de gravar.
**Critério de aceite:** "o que eu vi ontem sobre o iced?" responde só com texto local; captura com CPF visível sai borrada.

---

### F21: Web e pesquisa · tamanho **L**

> **Auditoria PR #30 (`2020015`), atualizada em 20/10/2026: Parcial.** Busca, leitura de páginas (Readability) e navegador guiado por CDP em Rust feitos; o sidecar Python foi removido. A aba de navegador no painel foi retirada por decisão; faltam pesquisa profunda `/pesquisa` com relatório em nota e monitores.

**Objetivo:** o Orion navega e pesquisa por você, com aprovação nas ações de risco.
**Dependências:** F04, F05, F12.

- [~] **F21.01** **Edge/Chrome headless guiado por CDP** (`chromiumoxide`), com `navegador_ler`/`navegador_agir`, `netguard` por requisição, sem downloads e recusa de senha/cartão. **Sem aba no painel e sem WebView2 por decisão**, não por implementação faltante. Falta validação com Edge no Windows real.
- [~] **F21.02** **Cada ação** (clicar, digitar, enviar) passa pela política; **pagamento sempre pede aprovação**; **conteúdo da página é dado** (R05); URL inteira no cartão.
- [~] **F21.03** **Pesquisa profunda (`/pesquisa`):** várias buscas em paralelo, leitura de fontes (`reqwest` + extração própria, `netguard`), **relatório com citações** salvo como **nota no vault**, com progresso e tempo limite.
- [x] **F21.04** **Busca na web** (ver F03.25) integrada ao agente, **pergunta com contexto** filtrada. *(Feito em 20/10/2026: `buscar_url` com Readability + Markdown, `links=true`, `netguard`.)*
- [ ] **F21.05** **Monitores** (F12.20) usando o mesmo navegador quando precisar renderizar.
- [ ] **F21.06** **Extensão de navegador** (F19.15), se mantida. *(Decidido em 20/10/2026: pós-1.0, junto de F19.15.)*

**Critério de aceite:** uma pesquisa profunda real gera nota com fontes; o navegador faz uma compra de teste parando para aprovar no pagamento.

---

### F22: Vários PCs, rede, casa e celular · tamanho **XL**

> **Auditoria PR #30 (`2020015`): Pendente.** Sync multi-PC, ACL Tailscale/exit node, WoL, casa/Alexa, Siri, Phone Link e Saúde não entregues.

**Objetivo:** notebook em sincronia, casa inteligente, Alexa e iPhone.
**Dependências:** F02, F10, F12, F14, F15.

#### F22.A Vários PCs e rede
- [~] **F22.01** **PC principal e notebook pela Tailscale**: sync por **log de eventos próprio**, reconciliação de edições offline e vault pelo git. Stable no começo; beta quando o notebook entrar. Aceite multi-PC ainda pendente.
- [ ] **F22.02** **Tailscale só com dispositivos da sua conta** (ACL); **exit node automático em rede não confiável** (sai pelo PC de casa; só funciona com o PC ligado); **Atalhos da Tailscale** no iPhone.
- [ ] **F22.03** **Roteador/Raspberry** como subnet router para o Wake-on-LAN (já usado na F14.07); guia de configuração em `docs/`.

#### F22.B Casa e Alexa
- [ ] **F22.04** **Home Assistant via MCP** (luzes, tomadas, ar), se existir.
- [ ] **F22.05** **Alexa (Echo Dot 2):** **Orion fala no Echo** (anúncios), **dispara rotinas e aparelhos** da Alexa, **"Alexa, pede pro Orion…"** (skill própria com endpoint no PC via Tailscale Funnel) e **Echo como alto-falante** do Orion.
- [ ] **F22.12** **Segurança da skill da Alexa:** o endpoint exposto pelo Tailscale Funnel é **público**, então só responde à rota da skill, **valida a assinatura e o certificado de cada requisição da Alexa** (cadeia do certificado, `SignatureCertChainUrl`, timestamp de até 150 s, ID da skill), tem limite de taxa e **nunca executa ação de risco sem a aprovação normal**.
- [ ] **F22.06** **Método:** **oficial onde der** (skill, interruptores virtuais, Voice Monkey para anúncios e rotinas) + **API não oficial como extra** (estilo AlexaPy, **pode quebrar sem aviso**; anúncios com **reserva** no Telegram ou no PC).
- [ ] **F22.07** Decidir **aparelhos e caminhos** (D10) e testar as limitações do Echo Dot 2.

#### F22.C iPhone
- [ ] **F22.08** **Siri/Atalhos → Orion:** atalho que envia a fala para um **webhook autenticado** (token, HTTPS) via Tailscale; resposta curta lida pela Siri; **automações por local e horário** (ex.: "cheguei em casa" → Wake-on-LAN + briefing).
- [ ] **F22.09** **Compartilhar do iPhone → Orion/Inbox** (folha de compartilhamento via Atalho).
- [ ] **F22.10** **Phone Link** (notificações e SMS no PC), que também **alimentam as finanças** (F23).
- [ ] **F22.11** **Saúde via Atalho** (sono e passos no briefing e no bem-estar).

**Testes:** sync com offline simulado (dois bancos divergindo e juntando), falha de rede no meio, teste do webhook (token errado, replay), anúncio na Alexa com a API oficial e a não oficial caída.
**Critério de aceite:** editar no notebook offline e ver no PC depois; "Alexa, pede pro Orion…" e "Siri, Orion…" respondem; anúncio chega ao Echo.

---

### F23: Vida, Google e finanças · tamanho **XL**

> **Auditoria PR #30 (`2020015`): Parcial.** Vida/finanças, OAuth/agenda/MCP e captura de gastos portados; integrações reais, importadores bancários completos, telas e semana de uso pendentes. **21/10/2026:** a tela Vida foi cortada (removida do app iced, da barra lateral e da paleta); as rotas (`/vida/simular`, `/foco`) e os dados existem no núcleo.

**Objetivo:** as integrações do Google e a vida pessoal (finanças, preços, linha do tempo, foco) sempre locais e sob regras de dados pessoais. A tela Vida foi cortada em 21/10/2026; os dados e rotas do núcleo ficam.
**Dependências:** F12, F22.10, F06.

#### F23.A Google
- [~] **F23.01** **OAuth de app instalado** (redirecionamento local), tokens no cofre, **alerta de expiração**.
- [~] **F23.02** **Agenda do Google e Gmail: só leitura**; **enviar ou criar pede aprovação**; **rascunhos no Gmail** (você envia, ou **envia com aprovação**).
- [ ] **F23.03** **Google Drive:** acesso de leitura conforme decidido (**Drive inteiro**, D06/C25: reavaliar para o mínimo); **backup do Drive é separado** (F10.26).
- [~] **F23.14** **Lembretes e tarefas em dois lugares (decisão "ambos")**: nota do vault (Tasks) **e** Google Agenda/Tasks; **criar no Google pede aprovação**.
- [~] **F23.04** **Calendário interno** (eventos guardados no vault) e **lembretes por voz** na barra lateral e no Telegram; **clima na barra lateral** (por cidade, Open-Meteo).

#### F23.B Finanças (tudo local)
- [~] **F23.05** **Importar OFX/CSV** do banco (`ofx-rs`, mapeadores para Nubank e outros), **notificações/SMS de compra** (Phone Link/iPhone) e **captura rápida** ("gastei 30 no almoço").
- [~] **F23.06** **Categorização:** **regras + LLM local** para o resto; **dados bancários nunca saem**.
- [-] **F23.07** **Telas:** gastos por categoria e mês, orçamento com alerta, assinaturas recorrentes detectadas, previsão do saldo. **Tela Vida cortada por decisão de 21/10/2026.** Os dados continuam no núcleo (`/vida/*`); sem tela por ora.

#### F23.C Outras áreas da Vida
- [~] **F23.08** **Monitor de preços** (motor da F12.21) e alertas; **Tela Vida cortada por decisão de 21/10/2026.** Sem tela própria.
- [ ] **F23.09** **Rastreio de tempo** (estilo ActivityWatch): apps e projetos por dia, **só totais, sem títulos de janela**, alimenta o Painel e as notas diárias.
- [~] **F23.10** **Linha do tempo** da vida (eventos, tarefas, gastos, conversas) e **modo foco como lembrete gentil**.
- [ ] **F23.11** **Bem-estar:** lembretes de **água** e **hora de dormir** (usa a presença da câmera quando ligada).
- [x] **F23.12** **Decidir o destino** dos itens do legado não decididos (revisão, diário, cápsula, cenários, sonhos, estado pela voz) — **D08**.
- [~] **F23.13** **Exportar as tabelas pessoais do SurrealDB** para o novo banco (F00.28), com verificação.

**Critério de aceite:** importar o extrato de um mês e ver categorias, assinaturas e previsão; Agenda e Gmail só leem sem aprovação; nenhum dado bancário em log ou chamada externa.

---

### F24: Estudo e conhecimento · tamanho **L**

> **Auditoria PR #30 (`2020015`): Pendente.** FSRS, tutor, progresso, conexões e fluxo de estudo ainda não implementados como entrega.

**Objetivo:** o Orion ajuda a aprender e a conectar o que você sabe, sem fazer por você.
**Dependências:** F06, F12.

- [ ] **F24.01** **Flashcards com revisão espaçada (FSRS, crate `fsrs`)** gerados das notas (pasta Estudo); **revisão diária no chat e no Telegram**; estatísticas; agendamento próprio.
- [ ] **F24.02** **`/aprendizado`:** o Orion **não faz por você, mostra como faz.** Dois modos escolhidos na hora: **tutor socrático** (dicas em níveis 1 → 2 → 3, **a solução só se pedir**) ou **passo a passo com você digitando** (ele revisa o que você fez).
- [ ] **F24.03** **Registro do progresso:** nota em `Estudo` + **flashcards do que foi aprendido**, ligados à revisão espaçada.
- [ ] **F24.04** **Conexões no vault** (estilo Smart Connections): painel **"relacionadas" no editor** e **sugestão de links** aceita com um clique (embeddings locais).
- [ ] **F24.05** **Resumo de PDF** e **leitura em voz alta** integrados ao estudo.

**Critério de aceite:** um tema novo vira nota + cartões + revisão no dia seguinte; o tutor não entrega a solução sem pedido.

---

### F25: Sistema, utilidades e documentos · tamanho **L**

> **Auditoria PR #30 (`2020015`): Parcial.** Históricos, Downloads, apps/mídia/documentos/conversor/planos portados; UI, Lixeira/desfazer completo, Spotify, jogo/economia e PowerPoint pendentes.

**Objetivo:** utilidades do dia a dia, controle do sistema e geração de documentos.
**Dependências:** F04, F12, F15.

- [~] **F25.01** **Área de transferência inteligente:** histórico de **7 dias, sem senhas**, **só local e opt-in**, **Ctrl+Shift+V** abre, com ações (traduzir, formatar JSON…); limpeza de segredos em **30 s**.
- [~] **F25.02** **Organizador de arquivos automático:** **move e renomeia**; **apagar vai para a Lixeira**, nunca definitivo; **log com desfazer** (7 dias); **sugestão: restringir a Downloads e Desktop** (a lixeira não protege arquivos de projeto ou do vault); rotina semanal.
- [~] **F25.03** **Mídia:** controles de mídia do Windows (SMTC) + **API do Spotify** (exige Premium, OAuth, escopos de leitura e controle de reprodução; tocar, pular, playlists por humor); **YouTube e Netflix só por link**.
- [~] **F25.04** **Sistema:** abrir apps e pastas, **controlar volume e mídia**, **controle de janelas** (UI Automation, com a regra R15).
- [ ] **F25.05** **Modo jogo automático** em tela cheia: **descarrega os modelos** (libera RAM e GPU), **silencia as sugestões**; a voz continua.
- [~] **F25.06** **Documentos:** gerar **Word, Excel e PowerPoint** (pelas skills de documento); **decisão "na v1" (15/10) não coube no escopo da v1** (ver C27). **Decidido em 20/10/2026: entra logo depois da v1, antes de F20.**
- [~] **F25.07** **Calculadora e conversões**, **tradução de texto**, **resumo de PDF**.
- [~] **F25.08** **Planos de arquivos** (existia no front antigo, removido em 21/10/2026; "depois da v1"): proposta em D08 (cartão de aprovação no chat, sem tela própria); aguardando confirmação do Antônio.
- [ ] **F25.09** **Bateria do notebook:** **modo economia automático** (desliga animações e efeitos; a wake word continua).

**Critério de aceite:** organizar uma pasta de Downloads real com desfazer funcionando; gerar um `.docx` e um `.pptx` a partir de uma conversa; modo jogo libera RAM medida.

---

## 13. Marco M9: automelhoria e fechamento

### F26: Automelhoria · tamanho **XL**

> **Auditoria PR #30 (`2020015`): Pendente.** Orçamento e tarefas restritas não são automelhoria: faltam fila, termos, execução por PR/evals e fluxo de interrupção.

**Objetivo:** o Orion melhora o próprio código com a cota que sobra, **sempre por PR**.
**Dependências:** F13, F11, F12, **D03 resolvido**, C17.

- [ ] **F26.01** **Fila escolhida por você** (ferramentas, código, lógica, performance, segurança, auditoria e manutenção do vault, testes e cobertura, refatoração, pesquisa sobre temas).
- [ ] **F26.02** **Disparo:** **no máximo 1 tarefa por dia**; se faltarem **12 h para o reset** e o uso estiver **abaixo de 90%**, dispara e repete até o teto de **95%**; **nunca acima do teto**; **teto por tarefa** (C18).
- [ ] **F26.03** **Mudanças de código saem sempre como branch e PR** para revisão (revisão cruzada); **nunca merge sozinho**; **repositórios alteráveis**: todos (reavaliar a lista, flag).
- [ ] **F26.04** **Pedido seu tem prioridade**: a tarefa de fundo **para**; **avisa quando fica pronta**.
- [ ] **F26.05** **Crash:** relatório **só local**, com contexto e dados mascarados; a automelhoria **gera um PR de correção com um teste que reproduz o erro**.
- [ ] **F26.06** **Auditoria diária do vault** com **correções simples** (links e frontmatter óbvios, cada mudança registrada) — **depois de ajustar a regra do `AGENTS.md` do vault** (C17).
- [ ] **F26.07** **Atualização de dependências** (PR automático, F11.09) e **vulnerabilidades em cada PR** (bloqueia alta).
- [ ] **F26.08** **Resumo** de cada execução no Painel e na nota do dia; **gate de evals** antes de abrir o PR.

**Critério de aceite:** uma semana com a automelhoria ligada gerando PRs revisáveis, sem exceder o teto e sem merge automático.

---

### F27: Auditoria final, documentação e release 1.0 · tamanho **L**

> **Auditoria PR #30 (`2020015`): Parcial documental.** Memórias/vault e remoção do código obsoleto feitos; auditorias finais, migração/restauração, descarte no PC e release 1.0 não realizados.

**Objetivo:** fechar o roadmap posterior ao M1, com itens concluídos ou explicitamente adiados. A numeração da versão desse fechamento será alinhada ao primeiro release; o aceite da v1 fica em F10.
**Dependências:** todas as anteriores.

- [~] **F27.01** **Auditoria de segurança completa:** OWASP LLM Top 10 2026 + Agentic Top 10, Trail of Bits (Semgrep, CodeQL, cadeia de dependências), MCP (tool poisoning), **revisão de todas as regras R01–R23 com teste**.
- [ ] **F27.02** **Auditoria de design e acessibilidade** (design critique + acessibilidade) de **todas** as telas; **Narrador fora da v1** (registrar como pendência futura).
- [ ] **F27.03** **Testes offline:** sem internet funcionam **memória e vault, chat com modelo local, rotinas locais e voz**.
- [ ] **F27.04** **Desempenho final:** RAM, janela pronta, primeira resposta, grafo, voz, bateria; comparação com o legado Python.
- [~] **F27.05** **Documentação final** em `docs/` (guia do usuário, arquitetura, regras, runbooks, ADRs) e **espelho no vault**.
- [~] **F27.06** **Descarte do legado:** código Python de referência e sidecar removidos em 20/10/2026, preservados no Git. **Dados pessoais e instalações no PC são separados dessa limpeza:** exportar/verificar F00.28/F23.13 antes de apagar Qdrant/SurrealDB e formatar o PC ao descartá-lo; 3,09 milhões de vetores não migram.
- [ ] **F27.07** **Fechamento posterior ao M1**, chamado de "release 1.0" no plano antigo. Não bloqueia a saída da v1 em F10. A numeração semver desse fechamento precisa ser alinhada com a primeira versão publicada; stable no começo, beta só quando adotado.
- [ ] **F27.08** **Plano pós-1.0:** macOS (abstração de plataforma já pronta), leitor de tela (Narrador), compartilhar nota/conversa, perfis, exportação em PDF.

**Critério de aceite:** todos os itens deste plano marcados ou explicitamente adiados com motivo; auditorias sem achado alto aberto.

---

## 14. Matriz de cobertura: toda seção do planejamento tem fase
| Seção de [[ORION - PLANEJAMENTO ARQUITETURA]] | Fase(s) |
|---|---|
| Objetivo, Decisões, Stack | F00, F01, F03 |
| Respostas 15/10: primeira versão, plataformas (Windows, notebook), tema, pasta das notas, janela, seletor de modelo, quem respondeu, tela inicial, voz na interface, aprovações, atividade, rotinas, painel de contexto, idioma, avisos, atalho global, Telegram, modo autônomo, projetos | F01, F03, F04, F05, F06, F08, F12, F14, F15 |
| Respostas 16/10 (núcleo, provedores, segurança, voz, sync) | F00, F02, F03, F04, F06, F10, F15, F22 |
| Rodada 3 (segurança, funcionalidades, performance, design, código, qualidade, lógica, ferramentas) | F01, F02, F03, F04, F05, F11, F12, F15, F21, F25 |
| Rodada 4 (onboarding, proatividade, rotinas, cache, crash, voz, Telegram, grafo, plataforma, docs) | F01, F04, F10, F12, F14, F15, F16, F26 |
| Rodada 5 (telas) e Configurações (estrutura) | F01, F07, F08, F09, F10 |
| Rodada 7 (chat, painel, captura, editor, terminal, Telegram) | F05, F06, F11, F14, F20 |
| Rodada 8 (login, 2FA, embeddings, atalho, classificador) e Pesquisa | F00, F02, F04, F06, F14 |
| Rodada 9 (funções novas) | F12, F20 a F25 |
| Rodada 10 (WebView, MSIX→Setup, ACP, MCP, PII, modelos, voz, WoL, cr-sqlite, evals, PDF, iced 0.14) | F00, F03, F05, F10, F15, F19, F22 |
| Rodada 11 e especificação do instalador | F10 |
| Gravação no vault | F06 |
| Permissões | F04 |
| Chat (detalhes da v1) | F05 |
| Extensões: MCP, skills e plugins | F19 |
| Nuvem: Claude e Codex | F12 |
| Rotinas | F12 |
| Primeiro uso, segurança e privacidade | F02, F10 |
| Erros e modo offline | F03, F27 |
| Painel | F09 |
| Voz | F15 |
| Telegram | F14 |
| Vault dentro do Orion | F06, F16 |
| App no Windows (instalador, atualização, backup, janela, desempenho, acessibilidade, testes) | F00, F01, F10, F27 |
| Personalidade e respostas | F04, F05 |
| Câmera e visão | F18 |
| Notificações, sons e não perturbe | F04 |
| Conversas na barra lateral | F01, F05 |
| Ferramentas de código | F11 |
| Modelos locais | F03, F10 |
| Vários PCs | F22 |
| Projetos (detalhe) | F08 |
| Escopo da v1, captura rápida e outras decisões | F10 (v1), F20, F23 |
| Orçamento de uso (assinaturas e APIs) | F13, F26 |
| Identidade visual, estrutura da janela, telas, configurações | F01, F05, F07, F09 |
| Memória: vault + banco | F02, F06 |
| Grafo da memória | F16 |
| Efeitos visuais | F17 |
| Acessibilidade e desempenho | F01, F27 |
| Fases (F1 a F9 do planejamento antigo) | substituídas por F01 a F27 deste plano |
| Respostas da rodada interativa (15/10): segurança, rede, dados, performance, design, utilidades, integrações, controle de apps, automações, memória, voz, modelos, projeto | F02 a F26 (itens citados nas fases) |
| Perguntas em aberto | §3 deste plano (D01 a D11) |

---

## 15. Registro de riscos
| # | Risco | Impacto | Mitigação | Fase |
|---|---|---|---|---|
| 1 | iced não atender o visual ou mudar de API | alto | spike F00.14, versão fixada, plano B Slint | F00, F01 |
| 2 | cr-sqlite incompatível com SQLCipher | médio | log de eventos próprio | F00, F22 |
| 3 | Identidade de pacote inviável | médio | OCR por ONNX; toast sem botões | F00, F10 |
| 4 | Termos dos provedores proíbem automação com assinatura | alto | D03 decidido em 20/10/2026 (ler `/usage`/`/status`, só CLIs oficiais); conferir os termos oficiais como cautela | F13, F26 |
| 5 | Controle de PC livre no autônomo + sem sandbox + injection | alto | R05, classificador, linha "sempre pedir", auditoria | F04 |
| 6 | Alexa sem API oficial estável | médio | caminho oficial primeiro; reserva Telegram | F22 |
| 7 | Escopo gigante (28 fases) | alto | marcos, definição de pronto, cortes por fase, v1 enxuta | todas |
| 8 | Perda de dados no legado | crítico | F00.28 antes da venda do PC; backup verificado | F00, F27 |
| 9 | Embeddings e dados pessoais saindo do PC | alto | e5 local; `#privado`; pastas bloqueadas; filtro | F02, F06 |
| 10 | RAM acima da meta com voz, grafo e modelos | médio | tudo sob demanda; teto de 30%; economia automática | todas |
| 11 | Free tiers mudando (Gemini perdeu o Pro) | médio | catálogo atualizável, fallback automático, abstração "Google" | F03 |
| 12 | Falso positivo do reconhecimento facial ou da voz | médio | não age, pede confirmação; sensibilidade ajustável | F15, F18 |
| 13 | Terminal compartilhado com o agente | médio | indicador de quem digita; entrada travada; máscara de senhas | F11 |
| 14 | Automelhoria gerando PRs ruins | médio | gate de evals; PR sempre; teste que reproduz o crash | F26 |
| 15 | Certificado autoassinado gera aviso do SmartScreen | baixo | explicar no instalador; reavaliar certificado comercial | F10 |


