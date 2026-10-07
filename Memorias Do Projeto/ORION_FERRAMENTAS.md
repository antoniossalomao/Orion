# ORION — Triagem das 55 ferramentas do legado

> Para onde vai cada ferramenta de `Orion_Ollama/tools/` (NUCLEO §5, fase 4). Criado em 03/10/2026.
> **Proposta para você revisar**: o destino é recomendação, não decisão tomada. A coluna "Risco" vem
> de `orion/policy/classes.py` (regra 1 do [ORION_REGRAS.md](ORION_REGRAS.md)); o teste
> `tests/test_ferramentas_doc.py` falha se esta tabela e a política divergirem.

## Resumo

| Situação | Qtd | O que significa |
|---|---|---|
| portada | 10 | Já existe em `orion/` com teste (nomes e argumentos do legado; ids agora inteiros) |
| substituida | 3 | O objetivo continua, por outro caminho (CLI delegada, canal Telegram, backup diário) |
| a-portar | 34 | Falta escrever: servidor MCP pronto ou `orion-desktop` próprio (fase 4) |
| descartar | 8 | Proposta de apagar: o agente ou o modelo já fazem, ou dependia de peça que saiu |

Já existe um `orion-desktop` v0 (`orion/tools/desktop.py`, desligado por padrão) com as 3 ferramentas de maior uso.

Servidores MCP candidatos (a validar na fase 4, cada um com classe de risco antes de ligar): filesystem,
Google Workspace, busca, fetch, navegador (Playwright), git. O que não tem servidor pronto e não é
multiplataforma de graça (`controlar_janela`, `organizar_pasta`) vai para `orion-desktop` ou é adiado.

## Tabela

| Ferramenta | Módulo no legado | Risco | Situação | Destino | Fase |
|---|---|---|---|---|---|
| `ler_arquivo` | `fs.py` | read | portada | orion.tools.desktop (opt-in `ORION_DESKTOP_TOOLS`); segredo e chave pedem confirmação | 4 |
| `listar_arquivos` | `fs.py` | read | portada | orion.tools.desktop (opt-in `ORION_DESKTOP_TOOLS`) | 4 |
| `buscar_memoria` | `memory.py` | read | portada | orion.tools.builtin (SQLite + FTS5 + vetores) | 3 |
| `salvar_memoria` | `memory.py` | write | portada | orion.tools.builtin (fatos com fonte e data) | 3 |
| `listar_numeros` | `numbers.py` | read | portada | orion.tools.ops_tools | 3 |
| `registrar_numero` | `numbers.py` | write | portada | orion.tools.ops_tools | 3 |
| `executar_comando` | `os_tools.py` | exec | portada | orion.tools.desktop (opt-in); só leitura provada roda direto (`orion.policy.shell`); o caminho do Windows só tem o argv testado | 4 |
| `gerenciar_agendamentos` | `reminders.py` | write | portada | orion.tools.ops_tools; o disparo avisa, não executa a ferramenta | 3 |
| `gerenciar_lembretes` | `reminders.py` | write | portada | orion.tools.ops_tools (SQLite); aviso pela fila `/notifications` | 3 |
| `gerenciar_tarefas` | `reminders.py` | write | portada | orion.tools.ops_tools; entram no contexto como "em aberto" | 3 |
| `backup_memoria` | `memory.py` | write | substituida | `orion backup` + job de backup diário + `orion verify-export` | 3 |
| `notificar_celular` | `notifications.py` | write | substituida | canal do Telegram lendo a fila `/notifications` (sem ntfy.sh) | 5 |
| `consultar_especialista` | `specialist.py` | exec | substituida | `delegar` (claude, codex, gemini CLI) | 2 |
| `escrever_clipboard` | `clipboard.py` | write | a-portar | orion-desktop | 4 |
| `ler_clipboard` | `clipboard.py` | read (externo) | a-portar | orion-desktop (multiplataforma); saída é conteúdo externo | 4 |
| `gerar_documento` | `documents.py` | write | a-portar | orion-desktop (python-docx, openpyxl, reportlab) | 4 |
| `ler_documento` | `documents.py` | read (externo) | a-portar | orion-desktop (pypdf, python-docx); saída é conteúdo externo | 4 |
| `transcrever_audio` | `documents.py` | read | a-portar | API de transcrição gratuita (Whisper no Groq) pelo gateway | 6 |
| `criar_evento` | `email_cal.py` | write | a-portar | servidor MCP do Google Workspace | 4 |
| `criar_rascunho_email` | `email_cal.py` | write | a-portar | servidor MCP do Google Workspace (só rascunho, nunca envia) | 4 |
| `ler_email` | `email_cal.py` | read (externo) | a-portar | servidor MCP do Google Workspace | 4 |
| `ler_emails` | `email_cal.py` | read (externo) | a-portar | servidor MCP do Google Workspace; saída é conteúdo externo | 4 |
| `listar_eventos` | `email_cal.py` | read (externo) | a-portar | servidor MCP do Google Workspace | 4 |
| `escrever_arquivo` | `fs.py` | write | a-portar | orion.tools.desktop (escrita fora das pastas seguras confirma); ou servidor MCP de filesystem | 4 |
| `organizar_pasta` | `fs.py` | write | a-portar | orion-desktop (próprio; sem equivalente pronto) | 4 |
| `consultar_git` | `git_tools.py` | read | a-portar | orion-desktop (git somente leitura) ou servidor MCP de git | 4 |
| `notificar_usuario` | `notifications.py` | write | a-portar | orion-desktop (toast no Windows, notificação no macOS) | 4 |
| `abrir_app` | `os_tools.py` | write | a-portar | orion-desktop (Windows e macOS) | 4 |
| `controlar_janela` | `os_tools.py` | exec | a-portar | orion-desktop só Windows (UI Automation); sem equivalente no Mac, adiar | 4 |
| `controlar_midia` | `os_tools.py` | write | a-portar | orion-desktop; baixa prioridade | 4 |
| `iniciar_processo_bg` | `processes.py` | exec | a-portar | orion-desktop (processo em segundo plano com log), sob a política | 4 |
| `iniciar_vigilancia_pasta` | `processes.py` | write | a-portar | job do agendador (watchdog); baixa prioridade | 4 |
| `listar_processos_bg` | `processes.py` | read | a-portar | orion-desktop | 4 |
| `listar_vigilancias` | `processes.py` | read | a-portar | idem | 4 |
| `parar_vigilancia_pasta` | `processes.py` | write | a-portar | idem | 4 |
| `status_processo_bg` | `processes.py` | read | a-portar | orion-desktop | 4 |
| `consultar_audit_log` | `security_tools.py` | read | a-portar | orion-desktop: ler o audit do `orion.policy` (hoje vai para o log `orion.audit`) | 4 |
| `checar_saude_sistema` | `system.py` | read | a-portar | orion-desktop (psutil; sem GPU) | 4 |
| `analisar_imagem` | `vision.py` | read | a-portar | gateway multimodal (sem ferramenta própria se o modelo aceitar imagem) | 4 |
| `capturar_tela` | `vision.py` | read | a-portar | orion-desktop (mss); captura contínua exige aprovação (Ring 0 #3) | 4 |
| `explicar_tela` | `vision.py` | read | a-portar | orion-desktop + modelo de visão do gateway | 4 |
| `gerar_imagem` | `vision.py` | write | a-portar | API gratuita de imagem; depende de cota (decisão #7) | 6 |
| `buscar_url` | `web.py` | read (externo) | a-portar | servidor MCP de fetch + `url_guard` (regra 7) portado para `orion/` | 4 |
| `consultar_clima` | `web.py` | read | a-portar | API pública sem chave (Open-Meteo) | 4 |
| `navegar_web` | `web.py` | exec (externo) | a-portar | servidor MCP de navegador (Playwright); sempre confirma | 4 |
| `pesquisar_com_ia` | `web.py` | read (externo) | a-portar | Gemini com Google Search pelo gateway (decisão #7) | 4 |
| `pesquisar_internet` | `web.py` | read (externo) | a-portar | servidor MCP de busca (decisão #7, a pesquisar na fase 4) | 4 |
| `analisar_clipboard_com_ia` | `clipboard.py` | read (externo) | descartar | `ler_clipboard` + o modelo | — |
| `resumir_documento` | `documents.py` | read (externo) | descartar | `ler_documento` + o modelo; sem ferramenta própria | — |
| `traduzir_texto` | `documents.py` | read | descartar | o modelo traduz direto (era o modelo local) | — |
| `obter_topico_celular` | `notifications.py` | read | descartar | só existia por causa do ntfy.sh | — |
| `consolidar_enxame` | `specialist.py` | write | descartar | idem | — |
| `criar_enxame` | `specialist.py` | write | descartar | o agente é o núcleo; tarefa grande vai para `delegar` | — |
| `status_enxame` | `specialist.py` | read | descartar | idem | — |
| `checar_servicos_orion` | `system.py` | read | descartar | não há mais serviços (SurrealDB, Qdrant, embed); `/health` cobre | — |

"(externo)" = devolve conteúdo não confiável (web, e-mail, arquivo): chega marcado como dado e, depois
de lido, escrita e execução da sessão passam a pedir confirmação (regra 4).

## Fora desta tabela

`listar_fatos`, `esquecer_fato`, `editar_fato` e `delegar` já nasceram na reescrita (fase 2/3; edição adicionada em C36) e não existiam no legado.
