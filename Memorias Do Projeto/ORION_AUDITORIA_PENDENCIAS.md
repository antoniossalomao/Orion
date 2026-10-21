---
type: recurso
projeto: "[[Orion]]"
tags: [orion, auditoria, pendencias, checklist]
date: 2026-10-20
revisao: 2026-10-21 (onboarding em tela cheia, céu compartilhado, CI Windows desativado, limpeza do repositório, front antigo removido e checklists recontados)
resumo: "Pendências e decisões vigentes do Orion, com implementação, testes e aceite Windows separados; substitui os diagnósticos intermediários desta auditoria."
---

# ORION - AUDITORIA DE PENDÊNCIAS (21/10/2026)

> Base: branch `claude/wizardly-bell-ynlfwv` (PR #30). Esta consolidação substitui as conclusões intermediárias da auditoria; o histórico permanece no Git. A fonte do estado é [ORION.md](ORION.md); os IDs e critérios ficam no [plano](ORION_PLANO_IMPLEMENTACAO.md). `ORION_LEGADO.md` é referência histórica.
> Legenda: **[V]** depende do Antônio · **[W]** exige Windows/hardware real · **[C]** implementação · **[D]** decisão.

## 1. Estado vigente

**A v1 ainda não está pronta.** Além da validação Windows e da distribuição assinada, faltam funcionalidades relevantes de F01–F10: Projetos, aceite do onboarding e dos backups, integração segura do vault e partes do chat, configurações e segurança.

- **Arquitetura:** `orion.exe` reúne interface iced e núcleo no mesmo processo, sem servidor HTTP do app. `orion-llama.exe` é um worker separado; o instalador é outro executável de distribuição. O navegador CDP usa porta de depuração efêmera em loopback; portanto, "sem porta" descreve a ligação UI–núcleo, não todos os processos auxiliares.
- **Produto sem Python:** pacote de referência e sidecar removidos em 20/10. Restam scripts de treino/smoke e a fixture MCP; o front HTML antigo e a rota `/ui` foram removidos em 21/10. Código removido permanece no Git; dados pessoais e instalações no PC não foram descartados.
- **Escopo:** v1 = M1, F01–F10, com saída em F10. F11–F27 são posteriores. F27 é fechamento do roadmap; a expressão histórica "release 1.0" nessa fase não amplia o escopo da v1 nem define a tag do primeiro release. A numeração semver deve ser alinhada ao publicar.
- **Checklists recontados em 21/10 contra o plano (mesmos números de 20/10; nenhuma marca mudou):** 458 itens F00–F27: **20 `[x]`, 286 `[~]`, 152 `[ ]`**. Em F01–F10: **13 `[x]`, 173 `[~]`, 51 `[ ]`** (237 itens). Não é percentual de conclusão: os itens têm tamanhos diferentes e `[~]` mistura trabalho parcial com aceite pendente.

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

**D03 está decidida quanto ao produto:** implementar `/usage` e `/status` pelas CLIs, com queda para estimativa e aviso se o formato mudar; Gemini usa estimativa. **F13.01/F13.02 seguem não implementados.** A decisão não comprova verificação dos termos: F00.29/F03.26 continuam abertos. Não descrever toda F13 como bloqueada por decisão pendente.

**Decisões ainda abertas no grupo D01–D11:** D06 (escopos GitHub/Drive), D08 (Planos de arquivos e detalhes de Vida), D09 (lista/requisitos dos modelos) e D10 (Alexa). D01–D05, D07 e D11 já têm decisão registrada. Outros pedidos de confirmação específicos estão na seção de conflitos do plano.

## 4. Implementações recentes e seus limites

**Onboarding em tela cheia e céu compartilhado (21/10):** o onboarding ocupa a janela inteira como o instalador (seis etapas e tela final, 7 telas), com fontes, campos e grade de chaves maiores e "Iniciar com o Windows" na etapa Conta. O renderizador do céu do instalador virou o crate `orion-sky`, usado pelo instalador e pelo onboarding; no onboarding uma estrela acende por tela (7 telas, 7 estrelas) e o instalador renderiza no mesmo quadro ampliado (60% da resolução). Testes de `orion-app`, `orion-sky` e `orion-setup` passaram no Linux; capturas das 7 telas conferidas. **Pendentes:** custo de CPU do céu num PC real, janela real do instalador, "reduzir movimento" do Windows no onboarding e leitura/escrita real da chave de início do Windows (não rodou no Linux). As demais telas do app ainda usam o céu estático de `orion-app/src/sky.rs`.

**Limpeza do repositório (21/10):** removidos `.vscode/settings.json` (ajuste de Python de uma máquina) e `Orion_Rust/.gitignore` (redundante); `.gitignore` da raiz reduzido ao que existe hoje; cabeçalho do `.env.example` corrigido (apontava para o pacote Python removido). Todas as 131 variáveis do `.env.example` continuam lidas pelo código. Também removidos, a pedido: o front HTML antigo (`Orion_Core/Front_end_Orion/`, 75 arquivos), a rota `/ui` do `orion-server` (e a dependência `tower-http`), a opção `ORION_SERVE_UI` e o `scripts/wake_eval.py` (já quebrado: dependia de `pyproject.toml`). `orion legenda` sem segundos agora avisa que a janela da legenda fica no app. **Conferido no código do app iced:** a tela **Vida** é só um aviso "depois da v1" (`views/pages.rs`) e a **legenda ao vivo** é uma faixa estática de demonstração (`overlays::caption`), sem ler `GET /legenda`. Os backends existem (rotas `/vida/*`, `/foco`, `/legenda` e `orion legenda <segundos>`); a janela real da legenda era a página do front antigo, removida, então ficou sem tela até F15.14. Planos de arquivos nunca ganhou tela no app (D08/F25.08). O plano foi atualizado nesses pontos. Não há módulos Rust órfãos; `scripts/` (2 scripts) e `tests/mcp_cliente/` seguem em uso.

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
| F01 | Céu: o motor CPU já é compartilhado entre instalador e onboarding (`orion-sky`); falta usá-lo nas demais telas do app e, se necessário, o shader wgpu. Fluent pt/en, comparação de snapshots, teclado/foco/contraste; Narrador fica fora da v1 |
| F02 | PIN crítico; aceite de Hello, recuperação, login, bloqueio e banco cifrado no Windows |
| F03 | ACP, catálogo de modelos/cotas completo e requisitos de RAM/VRAM |
| F04 | Classificador de injeção e cobertura de engine/invariantes; testar senha ao ligar autônomo |
| F05 | Diff por bloco, editar/reenviar, LaTeX/Mermaid, anexos e comandos `/`/`@`; solução sem WebView2 |
| F06 | Gravação automática, resumo diário, comparação/escolha de conflitos no chat e índice BM25 + LSA completo; preservação das versões e isolamento do commit implementados |
| F07 | Ligar todas as seções exigidas para a v1; separar controles futuros de funções operacionais |
| F08 | Cartões, clonar GitHub, pastas/referências, modelo/passos/herança, arquivar e mover conversa |
| F09 | Painel, métricas, Resultados, Integrações, "Você fez" e estados vazios |
| F10 | Onboarding (7 telas, céu com uma estrela por tela) e backup local/Drive implementados, com importação/exportação e recuperação; faltam aceite Windows/contas reais, integração Git completa do vault, atualização, rollback automático, modelos/GPU e primeiro release |

O onboarding decidido reúne usuário/senha, nome/idioma, recuperação em arquivo, vault, APIs, Telegram/2FA quando configurado, backup, projetos/GitHub, importação e Hello. Essa configuração não torna concluídas as fases posteriores correspondentes.

**Prioridade de dados:** preservar edições do vault e verificar backup/restauração antes de ampliar gravações automáticas. Exportar tabelas pessoais do SurrealDB e verificar F00.28/F23.13 antes de vender/formatar o PC.

## 6. Windows e distribuição

**[W] Aceite real pendente:** instalação/desinstalação em máquina limpa e notebook; bandeja, mini barra, Ctrl+Alt+Espaço/P/O/I, instância única, console, autostart e senha; Win+L e Hello quando implementado; Edge/CDP; nuvem com contas reais; Vulkan/Whisper, AEC3/barge-in, palmas com música e Telegram em dois aparelhos, conforme o escopo de cada marco.

**Metas da v1:** RAM parada <300 MB, janela <1 s, primeira resposta visível <100 ms; uma semana de uso sem perda de dados e backup restaurado. É necessário definir cenário e medir, sem confundir resposta visual com geração completa do modelo.

**[V] Distribuição:** gerar minisign, cadastrar segredos, incorporar a chave pública e publicar o primeiro release. Instalador usa feeds `latest-<variante>.json` CPU/Vulkan e fallback offline; updater é `orion update`, sem `orion-updater.exe`.

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
