---
type: recurso
projeto: "[[Orion]]"
tags: [orion, auditoria, pendencias, checklist]
date: 2026-10-20
revisao: 2026-10-24 (backend no PR 55, visual 7ae8efd da main e evidências FE-VIS conciliadas)
resumo: "Pendências e decisões vigentes do Orion, com implementação, testes e aceite Windows separados; substitui os diagnósticos intermediários desta auditoria."
---

# ORION - AUDITORIA DE PENDÊNCIAS (21/10/2026)

> Base: branch `claude/wizardly-bell-ynlfwv` (PR #30). Esta consolidação substitui as conclusões intermediárias da auditoria; o histórico permanece no Git. A fonte do estado é [ORION.md](ORION.md); os IDs e critérios ficam no [plano](ORION_PLANO_IMPLEMENTACAO.md). `ORION_LEGADO.md` é referência histórica.
> Legenda: **[V]** depende do Antônio · **[W]** exige Windows/hardware real · **[C]** implementação · **[D]** decisão.

## Configurações e instalador — 26/10/2026

[PR #58](https://github.com/antoniossalomao/Orion/pull/58): Painel movido para Configurações, sem item de Mais/paleta e com consulta autenticada e Atualizar; Fontes permanece só nas configurações e Resultados permanece cortado, sem apagar artefatos. Setup solicita UAC antes de instalar/desinstalar; prévia e notas de versão não solicitam elevação. Módulo checado para Windows MSVC; execução real do UAC ainda depende do PC do Antônio. Os 550 testes existentes passaram no Linux (Rust 1.97.0), sem testes novos/CI automático.

Pedido posterior: [build manual somente do instalador](https://github.com/antoniossalomao/Orion/actions/runs/40416699527), sem recompilar app/worker nem baixar modelos. O artefato substitui Setup no pacote já baixado e requer a pasta `cpu` ao lado. Fonte do build: `03d0b5fac93b943ee1ec9981f8ff6008627223ed`; resultado e artefato podem ser acompanhados na execução. Aceite Windows continua pendente.

## Rodada de backend — 24/10/2026

Base `efb2cbc`, branch `codex/orion-backend-notion`. O backend foi separado do refinamento visual do Antigravity. Código de F19.13 foi reaproveitado e validado; sua tela de credenciais continua pendente, portanto o card não representa uma entrega completa. A revisão detalhada reabriu F04.09 ao encontrar aprovação truncada aceita pela API; a falha recebeu correção no servidor e na retomada, sem editar o núcleo imutável.

| Card | Backend entregue ou comprovado | Restante do critério completo |
|---|---|---|
| F02.04 | VACUUM semanal em repouso, manutenção explícita e reconstrução de FTS sem expurgo | Recuperação integral a partir do vault e aceite operacional |
| F03.21 | Avaliação -1/+1 removível, SQLite local e sem treino | Ligar os controles reais da UI |
| F04.07 | Só falas elegíveis entram na consolidação; origem revalidada após a chamada | Sem dependência de hardware para esta regra |
| F04.09 | Auditoria fail-closed existente; API/retomada recusam argumentos truncados; encerra tentativa sem repetição | Sem dependência de hardware para esta regra |
| F04.19 | Resumo local por dia/escopo, deduplicado, aviso das 23:59 e recuperação de no máximo um dia | Exibição nativa e recibos reais de notas |
| F04.24 | Fixadas sobrevivem à janela do histórico e ao resumo, inclusive na retomada | Cache de dez conversas e experiência completa de compactação |
| F05.03 | CRUD de prompts por escopo e expansão `{{args}}` no chat | Editor/autocomplete da UI |
| F05.14 | API de fixar/desafixar, teto de contexto e isolamento | Edição/ramos/fixação ligados aos controles nativos |
| F05.18 | Metadados do projeto, validação do nome e mudança segura de escopo | Fluxo nativo completo, título automático conforme critério e exclusão com confirmação |
| F05.19 | Exportação existente preservada, com título correto e histórico consistente | Ação nativa de salvar/exportar e aceite |
| F05.20 | Retrato autenticado de contexto, fontes fixas, anotações e resumo | Painel nativo consumindo o contrato |
| F05.22 | `compacted`/resumo disponíveis na API de contexto | Avisos na UI e estado de descarregamento de imagens |
| F08.05 | Endpoint remoto padrão persistido por projeto, com fallback/cotas preservados | Seletores nativos e integração com preferências de CLIs |
| F08.06 | Até 8 referências do projeto relidas como dados externos | Seleção e gestão nativas |
| F08.07 | Modelo/passos sobrepõem o padrão; `null` herda; revisão invalida aprovações | Demais preferências e marca visual de herança |
| F08.11 | Arquiva conversas e restaura só as automaticamente arquivadas; jobs já ignoram projetos arquivados | Mover nota no vault, rotinas concedidas e fluxo completo de Arquivadas |
| F08.12 | Mudança simples de projeto validada; recusa ramos/artefatos/propostas; nome do projeto nos metadados | UI e tratamento explícito dos vínculos complexos |
| F09.05 | API diária de conversas/tarefas/autorizações por escopo | Painel, recibos reais de notas e aceite visual |
| F19.13 | Credenciais de leitura com hash, validade, permissões, revogação e escopo já existiam | Tela de emissão nativa |
| F19.14 | Janela de 60 leituras/min por cliente, além do teto global; registro sem conteúdo | Sem dependência de hardware para esta regra |

**Validação:** 550 testes existentes passaram; 61 checagens temporárias da API com banco descartável e provedor simulado; verificação temporária do domínio cobriu relógio/deduplicação, VACUUM sem perda e contaminação durante consolidação. Não foram criados testes no repositório nem reativado o CI automático. A janela/API de produção não foi aberta para fora, nenhum segredo real foi usado e o aceite Windows não foi declarado.

**Distribuição:** Antônio autorizou app + instalador sem modelos após as duas frentes terminarem. Visual `7ae8efd` da main integrado sem conflitos ao [PR #55](https://github.com/antoniossalomao/Orion/pull/55), ainda aberto. Workflow manual preparado com `perfil=release`, `cifra=true`, `llama=false`, `instalador=true`; suíte conjunta em `f0082f1` aprovada (550 testes, 0 falhas, Rust 1.97.0/Linux) antes do disparo. Isso substitui a pausa de builds para este pedido específico, sem autorizar publicação de release/tag ou novos testes.

**Resultado do pacote combinado:** Build manual [40128437375](https://github.com/antoniossalomao/Orion/actions/runs/40128437375) concluído com sucesso (35 min 50 s), fonte `222531ae26c0fe7d79ca1d07313d26eaf9f52bf7`. [Artefato Orion-Windows-App](https://github.com/antoniossalomao/Orion/actions/runs/40128437375/artifacts/12575110465), ZIP de 80.061.946 bytes, release/SQLCipher: `orion.exe`, `Orion Setup.exe`, `cpu/orion.exe`, `LEIA-ME.txt` e `SHA256SUMS.txt`. Download e conferência local: ZIP SHA-256 `9771977e03faaab8620a649c8d518c22881efa61c09c060f7b3435c2bb5b1442`, quatro hashes do manifesto correspondem aos arquivos, executáveis PE válidos e as duas cópias do app são idênticas. Sem worker ou pesos de modelos locais. Isso comprova compilação/estrutura do pacote, sem substituir instalação e uso no Windows.

## Conciliação visual da main — 24/10/2026

Fonte: [commit direto `7ae8efd` do Antigravity](https://github.com/antoniossalomao/Orion/commit/7ae8efd43bafa691711447e504ed80e79a9105de), 10 arquivos, integrado na branch do backend em `f0082f1`. A main contém o visual; o backend aguarda revisão do PR #55.

| Frente | Alteração confirmada no diff | Limite da evidência |
|---|---|---|
| FE-VIS 02/03 — grafo | Forças multiescala, cache de coordenadas, colisões, rótulos, arestas por grau, vizinhança de 1/2 saltos e enquadramento com painel | Canvas 2D; renderer/layout 3D, persistência incremental e desempenho com 10 mil notas não entregues por esse commit |
| FE-VIS 04/05 — casca | Badges sem quebra interna, divisória da gaveta, busca e controles de janela na topbar estreita | Tokens e controles iniciais do R1 reaproveitados; faixa branca/DWM e DPI Windows pendentes |
| FE-VIS 06 — chat | Selo de mensagem salva ganha texto/check visíveis | Markdown/LaTeX/Mermaid e minimapa não foram alterados nesse commit; revisão de trabalho anterior |
| FE-VIS 07 — editor/painel | Terminal ocupa altura disponível sem rolagem externa duplicada | Editor e preview não foram alterados nesse commit; revisão de trabalho anterior |
| FE-VIS 08 — gestão | Projetos quebra a linha de badges e aumenta a altura dos cards | Não liga os novos contratos de preferências/referências de projetos à UI |
| FE-VIS 09 — repouso | Tick suspenso em segundo plano; hover vazio evita invalidar o grafo | Cache de geometria e animação finita do login já existiam; CPU/GPU no notebook ainda sem medição desta rodada |
| FE-VIS 01/10/11 — revisão | Antigravity registrou 48 capturas tiny-skia e 33 testes de orion-app | Capturas não incluídas no commit; resultados atribuídos ao executor, sem alegar reprodução ou aceite Windows pelo Codex |

Notion: 9 cards FE-VIS concluídos, 05 e 11 em revisão; planejamento em revisão. Estados do executor preservados, com links para o commit e ressalvas de evidência. Notas de FE-VIS 06/07 citavam telas de outro escopo; a conciliação registra chat em 06, editor/painel em 07 e gestão em 08, preservando o histórico. Os 17 cards de backend ainda em revisão não foram concluídos pelo polimento. R1.09 continua bloqueado por reprodução no Windows; R1.08/R1.15 continuam dependentes de aceite/medição.

## 1. Estado vigente

**A v1 ainda não está pronta.** Além da validação Windows e da distribuição assinada, faltam funcionalidades relevantes de F01–F10: Projetos, aceite do onboarding e dos backups, integração segura do vault e partes do chat, configurações e segurança.

- **Arquitetura:** `orion.exe` reúne interface iced e núcleo no mesmo processo, sem servidor HTTP do app. `orion-llama.exe` é um worker separado; o instalador é outro executável de distribuição. O navegador CDP usa porta de depuração efêmera em loopback; portanto, "sem porta" descreve a ligação UI–núcleo, não todos os processos auxiliares.
- **Produto sem Python:** pacote de referência e sidecar removidos em 20/10. Restam scripts de treino/smoke e a fixture MCP; o front HTML antigo e a rota `/ui` foram removidos em 21/10. Código removido permanece no Git; dados pessoais e instalações no PC não foram descartados.
- **Escopo:** v1 = M1, F01–F10, com saída em F10. F11–F27 são posteriores. F27 é fechamento do roadmap; a expressão histórica "release 1.0" nessa fase não amplia o escopo da v1 nem define a tag do primeiro release. A numeração semver deve ser alinhada ao publicar.
- **Checklists recontados em 21/10 contra o plano:** 458 itens F00–F27: **28 `[x]`, 282 `[~]`, 148 `[ ]`** (eram 20, 286 e 152 em 20/10: a F00 de 21/10, a verificação dos termos e as skills fecharam 8 itens, e o cancelamento de testes/CI e da tela Vida passou 5 itens de `[~]` para `[ ]` cancelado). Em F01–F10: **14 `[x]`, 173 `[~]`, 50 `[ ]`** (237 itens; F03.26 fechou). Não é percentual de conclusão: os itens têm tamanhos diferentes e `[~]` mistura trabalho parcial com aceite pendente.

## 2. Como interpretar a evidência

Mantida a legenda do plano: `[x]` subentrega feita sem depender de hardware; `[~]` parcial ou sem aceite Windows; `[ ]` não iniciado. Item explicitamente adiado/cancelado não bloqueia o marco de que foi retirado. Nenhuma marca isolada aceita uma fase inteira.

Ao atualizar cada item, registrar:

| Campo | Evidência esperada |
|---|---|
| Implementação | Ausente, parcial ou completa; arquivo e comportamento entregue |
| Testes | Não executados ou comando, commit e resultado; compilação sozinha não prova comportamento |
| Windows | Não aplicável, pendente ou cenário e resultado do aceite real |
| Próximo passo | Falta concreta, dependência e responsável |

Esta revisão corrige documentação; não reexecuta testes nem promove funcionalidades a prontas.

## 3. Decisões consolidadas

| Tema | Decisão vigente | Pendência |
|---|---|---|
| Atalhos | Ctrl+Alt+Espaço mini barra; P pânico; O captura rápida; I "Isso?" | Implementados; aceite Windows e fluxos completos F20 pendentes |
| Som do instalador | Sempre ligado, sem botão de mudo | Botão removido; aceite Windows pendente |
| Modo autônomo | Exige senha ao ligar; política de aprovação permanece | Código aplicado; teste da rota com senha e aceite pendentes |
| Autostart | Com banco cifrado, abre a tela de senha | Implementado; aceite Windows pendente |
| Nuvem | Segundo plano local **e** nuvem Claude/Codex pelas CLIs oficiais | Implementação parcial; testes e contas reais pendentes |
| Navegador | Edge/Chrome headless guiado por CDP; sem WebView2 e sem aba no painel | Teste real com Edge pendente; aba não é requisito faltante |
| MCP Apps | Só texto, pós-1.0 | Fora do aceite da v1 |
| Telegram/MCP | Clientes próprios; sem obrigação de teloxide/rmcp | Aceite das integrações conforme fases |
| Fontes | App Geist/Geist Mono/Newsreader; instalador Inter | Aceite visual |
| Busca do vault | BM25 + LSA; embeddings locais só se a busca falhar | Índice e integração completos pendentes |
| Sync | Log de eventos próprio | Aceite multi-PC pendente |
| Canais | Stable no começo; beta quando o notebook entrar | Publicar primeiro release |
| Ordem pós-v1 | F25.06 (Word/Excel/PowerPoint), depois F20 | Respeitar dependências das demais fases |
| Palavra "Orion" | Desligada na v1; ficam palmas e atalho | Modelo final e corpus real são trabalho futuro |
| TTS | edge-tts por ora; alternativas atrás de interface trocável | Antônio testa alternativas |
| Grafo | wgpu puro; Bevy/three-d só plano B | F16 |
| Licença | Uso pessoal | Revisar se mudar a distribuição |
| Legado | Código Python removido; vetores Qdrant/SurrealDB não migram | Exportar e verificar tabelas pessoais antes do descarte do PC |
| Casa/Alexa | Mantidos, adiados sem data | D10 |
| Automelhoria | Depois de F11 e F13 | F26; interruptor não comprova execução |
| macOS/extensão | Depois da 1.0; macOS após versão estável | Fora do primeiro lançamento |

**D03 está decidida quanto ao produto:** implementar `/usage` e `/status` pelas CLIs, com queda para estimativa e aviso se o formato mudar; Gemini usa estimativa. **F13.01/F13.02 seguem não implementados.** A verificação dos termos (F00.29/F03.26) foi feita em 21/10/2026 e está em `docs/termos-provedores.md`; ela não implementa a leitura de uso, que continua pendente. Não descrever toda F13 como bloqueada por decisão pendente.

**Decisões ainda abertas no grupo D01–D11:** só D08, na parte de Planos de arquivos (proposta: sem tela própria, cartão de aprovação no chat, depois da v1; aguarda confirmação). A tela Vida foi cortada e a D10 (Alexa) foi adiada para depois da v1, ambas em 21/10/2026. D06 (token do GitHub com acesso total; Drive só `drive.file`) e D09 (manter os 8 modelos) foram decididas em 21/10/2026; D01–D05, D07 e D11 já tinham decisão. Outros pedidos de confirmação específicos estão na seção de conflitos do plano.

## 4. Implementações recentes e seus limites

**Onboarding em tela cheia e céu compartilhado (21/10):** o onboarding ocupa a janela inteira como o instalador (seis etapas e tela final, 7 telas), com fontes, campos e grade de chaves maiores e "Iniciar com o Windows" na etapa Conta. O renderizador do céu do instalador virou o crate `orion-sky`, usado pelo instalador e pelo onboarding; no onboarding uma estrela acende por tela (7 telas, 7 estrelas) e o instalador renderiza no mesmo quadro ampliado (60% da resolução). Testes de `orion-app`, `orion-sky` e `orion-setup` passaram no Linux; capturas das 7 telas conferidas. **Pendentes:** custo de CPU do céu num PC real, janela real do instalador, "reduzir movimento" do Windows no onboarding e leitura/escrita real da chave de início do Windows (não rodou no Linux). As demais telas do app ainda usam o céu estático de `orion-app/src/sky.rs`.

**F00 (21/10):** feitos `rust-toolchain.toml` (1.97.0), `deny.toml`, `.cargo/config.toml` (atalhos), `docs/` (índice, arquitetura, contribuição, glossário, 7 ADRs, `docs/spikes/`), `Orion_Rust/README.md` (compilar no Windows), Dependabot semanal, modelos de PR e de issue, skills de auditoria (`.claude/skills/auditoria-seguranca` e `auditoria-interface`) e a feature `dev` do iced (`cargo dev`). **Cancelados por decisão de 21/10 (testes novos e CI pausados):** F00.07 (Actions), F00.08 (harness headless), F00.09 (snapshots automáticos) e F00.12 (`orion-evals`). **Feito depois (21/10):** termos dos provedores verificados e registrados em `docs/termos-provedores.md` (F00.29, F03.26), lista oficial do OWASP Agentic na skill de segurança e **21 skills de auditoria** (F00.13) no Orion (extensão embutida `orion-auditoria`) e no Claude Code (`.claude/skills`), em textos originais (`docs/skills.md`). **Não foi possível sozinho:** os 13 spikes (exigem hardware, GPU, modelos, contas ou Windows), a exportação do SurrealDB (F00.28), (F00.30 concluído: D05, D06 e D09 decididas e D10 adiada para depois da v1) e a branch protegida na `main` (configuração do GitHub). `cargo deny`, o hot reload e a viagem no tempo não foram executados.

**Limpeza do repositório (21/10):** removidos `.vscode/settings.json` (ajuste de Python de uma máquina) e `Orion_Rust/.gitignore` (redundante); `.gitignore` da raiz reduzido ao que existe hoje; cabeçalho do `.env.example` corrigido (apontava para o pacote Python removido). Todas as 131 variáveis do `.env.example` continuam lidas pelo código. Também removidos, a pedido: o front HTML antigo (`Orion_Core/Front_end_Orion/`, 75 arquivos), a rota `/ui` do `orion-server` (e a dependência `tower-http`), a opção `ORION_SERVE_UI` e o `scripts/wake_eval.py` (já quebrado: dependia de `pyproject.toml`). `orion legenda` sem segundos agora avisa que a janela da legenda fica no app. **Conferido no código do app iced:** a tela **Vida** foi **cortada por decisão sua em 21/10/2026** (removida do app, da barra lateral, da paleta e das capturas; o núcleo mantém os dados e rotas) e a **legenda ao vivo** é uma faixa estática de demonstração (`overlays::caption`), sem ler `GET /legenda`. Os backends existem (rotas `/vida/*`, `/foco`, `/legenda` e `orion legenda <segundos>`); a janela real da legenda era a página do front antigo, removida, então ficou sem tela até F15.14. Planos de arquivos nunca ganhou tela no app (D08/F25.08). O plano foi atualizado nesses pontos. Não há módulos Rust órfãos; `scripts/` (2 scripts) e `tests/mcp_cliente/` seguem em uso.

O alinhamento de 20/10 registra: remoção do mudo, senha do autônomo, senha no autostart cifrado, Ctrl+Alt+O/I, nuvem pelas CLIs e novo login com usuário padrão `antoniosalomao` e migração de `admin`.

**Verificação dessa rodada:** compilação (`cargo check`) e captura de tela; testes recentes de `orion-core` (nuvem), `orion-app` e `appmode` ainda não executados no registro consultado. Sem aceite Windows/contas reais.

**Proteção do vault (20/10):** preservação sem sobrescrita, propostas de conflito e isolamento dos commits implementados; a suíte exigida pelo AGENTS.md passou com 550 testes no Linux. Comparação no chat e aceite Windows/Obsidian permanecem pendentes. Detalhes e limites em ORION.md.

**Nuvem parcial:** descartar remove da lista local, sem cancelar remotamente; faltam log ao vivo e aviso de término no Telegram; Codex não recebe mensagem. Não confundir com `/tarefas-bg`, que continua existindo.

**Entregas parciais não são fases completas:**

- F08 tem APIs e isolamento F08.10 marcado `[x]`; telas e fluxo de Projetos continuam pendentes. Não é uma fase "sem nada feito".
- F06 tem gravação explícita por API; falta gravação automática por turno. **F06.14 está `[~]`: `save_conversation` preserva a original e salva proposta marcada como conflito**, com ambos os caminhos na API e sem commit/push do conflito. Comparação/escolha no chat e aceite Obsidian/Windows ainda pendentes.
- F21.04 entrega leitura/busca; F21.03, pesquisa profunda com relatório em nota, continua parcial.
- Modo autônomo (F04.17) e corrigir fato (F06.20) não comprovam automelhoria F26 nem leitura de uso F13.
- F27.06 é parcial: remoção de código feita; exportação, verificação e descarte de dados/PC pendentes.

## 5. Pendências para a v1 (F01–F10)

| Fase | Trabalho restante relevante |
|---|---|
| F01 | Em 26/10/2026 (PR #56): atalhos personalizáveis, `/config`, prefixo `>`, Ctrl +/−, escala/densidade persistentes e projetos recentes entraram sem aceite Windows; Resultados e Fontes deixaram de ser telas; Painel fica só em Configurações (PR #58). Céu: o motor CPU já é compartilhado entre instalador e onboarding (`orion-sky`); falta usá-lo nas demais telas do app e, se necessário, o shader wgpu. Fluent pt/en, comparação de snapshots, teclado/foco/contraste; Narrador fica fora da v1 |
| F02 | PIN crítico; aceite de Hello, recuperação, login, bloqueio e banco cifrado no Windows |
| F03 | ACP, catálogo de modelos/cotas completo e requisitos de RAM/VRAM |
| F04 | Classificador de injeção e cobertura de engine/invariantes; testar senha ao ligar autônomo |
| F05 | Diff por bloco, editar/reenviar, LaTeX/Mermaid, anexos e comandos `/`/`@`; solução sem WebView2 |
| F06 | Gravação automática, resumo diário, comparação/escolha de conflitos no chat e índice BM25 + LSA completo; preservação das versões e isolamento do commit implementados |
| F07 | Ligar todas as seções exigidas para a v1; separar controles futuros de funções operacionais |
| F08 | Cartões, clonar GitHub, pastas/referências, modelo/passos/herança, arquivar e mover conversa |
| F09 | Métricas reais do Painel (agora em Configurações), Integrações, "Você fez" e estados vazios; Resultados foi cortado |
| F10 | Onboarding (7 telas, céu com uma estrela por tela) e backup local/Drive implementados, com importação/exportação e recuperação; faltam aceite Windows/contas reais, integração Git completa do vault, atualização, rollback automático, modelos/GPU e primeiro release |

O onboarding decidido reúne usuário/senha, nome/idioma, recuperação em arquivo, vault, APIs, Telegram/2FA quando configurado, backup, projetos/GitHub, importação e Hello. Essa configuração não torna concluídas as fases posteriores correspondentes.

**Prioridade de dados:** preservar edições do vault e verificar backup/restauração antes de ampliar gravações automáticas. Exportar tabelas pessoais do SurrealDB e verificar F00.28/F23.13 antes de vender/formatar o PC.

## 6. Windows e distribuição

**[W] Aceite real pendente:** instalação/desinstalação em máquina limpa e notebook; bandeja, mini barra, Ctrl+Alt+Espaço/P/O/I, instância única, console, autostart e senha; Win+L e Hello quando implementado; Edge/CDP; nuvem com contas reais; Vulkan/Whisper, AEC3/barge-in, palmas com música e Telegram em dois aparelhos, conforme o escopo de cada marco.

**Metas da v1:** RAM parada <300 MB, janela <1 s, primeira resposta visível <100 ms; uma semana de uso sem perda de dados e backup restaurado. É necessário definir cenário e medir, sem confundir resposta visual com geração completa do modelo.

**[V] Distribuição:** confirmar a posse da chave privada correspondente à pública minisign já preenchida em `crates/orion-setup/release/minisign.pub` (não gerar outra por rotina), cadastrar/verificar os segredos `MINISIGN_SECRET_KEY` e `MINISIGN_PASSWORD` e publicar o primeiro release. Instalador usa feeds `latest-<variante>.json` CPU/Vulkan e fallback offline; updater é `orion update`, sem `orion-updater.exe`.

**Assinaturas distintas:** minisign autentica o feed; SHA-256 verifica os arquivos referenciados. A identidade MSIX autoassinada não equivale a Authenticode dos executáveis nem elimina SmartScreen. O `release.yml` atual assina feeds com minisign; não tem etapa Authenticode. Certificado comercial não é requisito decidido para o primeiro lançamento.

## 7. CI e documentação

**O `rust-windows.yml` está desativado nos PRs desde 21/10/2026** (roda ~30 min e falha; só `workflow_dispatch`; o gatilho está comentado no arquivo). Nenhum CI valida os PRs por ora. Quando reativado, ele configura testes dos crates (agora incluindo `orion-sky`), testes SQLCipher, builds CPU/Vulkan e smoke CPU com GGUF real; **não executa fmt/clippy**, nem prova hardware, aceite visual ou uso integrado. Runs antigos não comprovam sucesso do commit atual.

Pendências: descobrir por que o run Windows falha e reativar o gatilho; executar testes das mudanças recentes; adicionar fmt/clippy; comparar snapshots; vincular resultados ao commit e aos cenários executados.

As referências Python em "Imposto em"/"Prova" de `REGRAS.md` são históricas, conforme a nota de equivalência Rust. Ainda precisam ser reapontadas; esta correção documental não altera princípios protegidos nem inventa evidência Rust.

Repositório é origem; documentos alterados devem ser espelhados no vault na mesma sessão. Espelho atualizado não equivale a aceite do produto.

> **Atualização de código (20/10/2026):** onboarding nativo, cofre, recuperação em arquivo, Hello/Telegram, importações com prévia e backup autenticado local/Drive receberam implementação. Detalhes e limites em ORION.md, seção Onboarding nativo; todos os aceites Windows/contas reais continuam pendentes.

## 8. Pós-v1

- F11–F13: ferramentas de código/ConPTY/GitHub, rotinas completas, monitores/proatividade, limites da nuvem e leitura real de cotas.
- F14–F15: PIN/fila/Wake-on-LAN, UI de voz, Silero e modelo "Orion" final. Corpus real (≥50 positivos e ≥1 h negativos) fica para a futura ativação da palavra, sem bloquear a v1.
- F16–F19: grafo/efeitos, câmera e visão reais, WASM/WIT, conectores e proteção MCP completa. MCP Apps em texto ficam pós-1.0.
- F20–F25: captura/OCR/reuniões, pesquisa profunda, multi-PC/casa/iPhone, Google/finanças, estudo e utilidades. **F25.06 vem antes de F20**, logo após a v1.
- F26–F27: automelhoria por PR, auditorias do roadmap completo, offline/desempenho/documentação e fechamento posterior. Extensão de navegador e macOS permanecem fora do primeiro lançamento.

## 9. Ordem de trabalho recomendada

1. Reativar o CI Windows (hoje desativado), verificar testes das mudanças recentes e registrar resultados por commit.
2. Validar no Obsidian/Windows a preservação do vault e backup/restauração; fechar comparação de conflitos no chat; preservar/exportar dados do legado.
3. Gerar chave/feed e validar instalação, senha, atualização e rollback no Windows.
4. Fechar Projetos, onboarding e os demais fluxos F01–F10; executar o aceite F10.F limitado à v1.
5. Após a v1: F25.06, depois F20; demais fases conforme dependências.

A consolidação documental não conclui nenhuma dessas etapas.
