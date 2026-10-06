# ORION — Triagem das 55 ferramentas do legado

> Para onde vai cada ferramenta de `Orion_Ollama/tools/` (NUCLEO §5, fase 4). Criado em 03/10/2026.
> **Proposta para você revisar**: o destino é recomendação, não decisão tomada. A coluna "Risco" vem
> de `orion/policy/classes.py` (regra 1 do [ORION_REGRAS.md](ORION_REGRAS.md)); o teste
> `tests/test_ferramentas_doc.py` falha se esta tabela e a política divergirem.

## Resumo

| Situação | Qtd | O que significa |
|---|---|---|
| portada | 37 | Já existe em `orion/` com teste (nomes e argumentos do legado; ids agora inteiros) |
| substituida | 2 | O objetivo continua, por outro caminho (CLI delegada, backup diário) |
| a-portar | 8 | Falta: depende de **você** configurar um servidor MCP (e-mail, agenda, navegador, busca) ou de uma API gratuita de imagem (`gerar_imagem`) |
| descartar | 8 | Proposta de apagar: o agente ou o modelo já fazem, ou dependia de peça que saiu |

O `orion-desktop` (opt-in `ORION_DESKTOP_TOOLS=true`) tem hoje: `executar_comando`, `ler_arquivo`,
`listar_arquivos`, `escrever_arquivo`, `organizar_pasta`, documentos, área de transferência,
notificação local, abrir app, Git somente-leitura, saúde do sistema, processos em segundo plano e
vigilância de pastas (e, com a chave de transcrição, `transcrever_audio`). As de web (`buscar_url`, `consultar_clima`, `pesquisar_com_ia`) são outro
opt-in (`ORION_WEB_TOOLS=true`) por causa do canal de exfiltração por URL (ver `orion/tools/web.py`).

**Servidores MCP** (`orion/mcp_client.py`, exemplo em `mcp.example.json`, conferir com
`uv run orion mcp-check`): é por onde entram e-mail, agenda, navegador e busca. A classe de risco de
cada ferramenta vem do seu `mcp.json`, nunca do servidor; o que não for classificado pede confirmação.
Mídia e janelas (`controlar_midia`, `controlar_janela`) e visão (`capturar_tela`, `explicar_tela`,
`analisar_imagem`) entraram em 06/10/2026: nenhuma delas foi executada fora do Linux (só o argv), e
`controlar_janela` não existe no macOS. A visão tem opt-in próprio (`ORION_VISION_TOOLS`).

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
| `notificar_celular` | `notifications.py` | write | portada | orion.tools.ops_tools: põe o aviso na fila e o canal do Telegram entrega (sem ntfy.sh) | 5 |
| `consultar_especialista` | `specialist.py` | exec | substituida | `delegar` (claude, codex, gemini CLI) | 2 |
| `escrever_clipboard` | `clipboard.py` | write | portada | orion.tools.system_tools (opt-in; texto vai por variável de ambiente, nunca na linha de comando) | 4 |
| `ler_clipboard` | `clipboard.py` | read (externo) | portada | orion.tools.system_tools (opt-in; PowerShell, `pbpaste`, `wl-paste`/`xclip`); saída é conteúdo externo | 4 |
| `gerar_documento` | `documents.py` | write | portada | orion.tools.documents (opt-in): txt, md, html, csv, docx, xlsx e pdf (`uv sync --extra pdf`); planilha nunca grava fórmula | 4 |
| `ler_documento` | `documents.py` | read (externo) | portada | orion.tools.documents (opt-in): pdf, docx, xlsx, html, txt, md, csv, json; saída é conteúdo externo; segredo confirma | 4 |
| `transcrever_audio` | `documents.py` | read (externo) | portada | orion.tools.audio (opt-in; só com `ORION_TRANSCRIBE_API_KEY`): Whisper no Groq; a transcrição é conteúdo externo; segredo confirma. **Não validado contra a API real** | 6 |
| `criar_evento` | `email_cal.py` | write | a-portar | servidor MCP do Google Workspace em `mcp.json`; cliente pronto, falta você escolher e configurar o servidor | 4 |
| `criar_rascunho_email` | `email_cal.py` | write | a-portar | idem (só rascunho, nunca envia) | 4 |
| `ler_email` | `email_cal.py` | read (externo) | a-portar | idem | 4 |
| `ler_emails` | `email_cal.py` | read (externo) | a-portar | idem; saída é conteúdo externo | 4 |
| `listar_eventos` | `email_cal.py` | read (externo) | a-portar | idem | 4 |
| `escrever_arquivo` | `fs.py` | write | portada | orion.tools.fs_tools (opt-in `ORION_DESKTOP_TOOLS`); escrever fora das pastas seguras ou em arquivo sensível confirma | 4 |
| `organizar_pasta` | `fs.py` | write | portada | orion.tools.fs_tools (opt-in); ganhou `simular`; raiz de drive, pasta pessoal e código do Orion confirmam | 4 |
| `consultar_git` | `git_tools.py` | read | portada | orion.tools.system_tools (opt-in); git sem hooks, fsmonitor nem diff externo do repositório | 4 |
| `notificar_usuario` | `notifications.py` | write | portada | orion.tools.system_tools (opt-in): balão no Windows, `osascript` no macOS, `notify-send` no Linux | 4 |
| `abrir_app` | `os_tools.py` | exec | portada | orion.tools.system_tools (opt-in); **virou execução** (abre qualquer programa ou arquivo): sempre confirma | 4 |
| `controlar_janela` | `os_tools.py` | exec (externo) | portada | orion.tools.media (opt-in): Windows (PowerShell) e Linux X11 (`wmctrl`); **não existe no macOS**; confirma sempre; busca ambígua não age; o título é externo; só o argv foi testado | 4 |
| `controlar_midia` | `os_tools.py` | write | portada | orion.tools.media (opt-in): teclas de mídia no Windows, `osascript` (Spotify/Music) no macOS, `playerctl` + `wpctl`/`pactl`/`amixer` no Linux; só o argv foi testado | 4 |
| `iniciar_processo_bg` | `processes.py` | exec | portada | orion.tools.processes (opt-in); comando classificado como `executar_comando`; log só do dono; avisa ao terminar | 4 |
| `iniciar_vigilancia_pasta` | `processes.py` | write | portada | `Operations.watch_*` + job do agendador (sem watchdog: compara nomes a cada tick); a lista vive no banco | 4 |
| `listar_processos_bg` | `processes.py` | read | portada | orion.tools.processes (opt-in); a lista zera quando o Orion reinicia | 4 |
| `listar_vigilancias` | `processes.py` | read | portada | idem | 4 |
| `parar_vigilancia_pasta` | `processes.py` | write | portada | idem | 4 |
| `status_processo_bg` | `processes.py` | read | portada | orion.tools.processes (opt-in) | 4 |
| `consultar_audit_log` | `security_tools.py` | read | portada | orion.tools.ops_tools lendo a tabela `audit` (toda decisão da política é gravada; poda de 90 dias) | 4 |
| `checar_saude_sistema` | `system.py` | read | portada | orion.tools.system_tools (opt-in; psutil, sem GPU; inclui disco e bateria) | 4 |
| `analisar_imagem` | `vision.py` | read (externo) | portada | orion.tools.vision (opt-in `ORION_VISION_TOOLS`): manda o arquivo ao modelo do gateway (`orion.vision`); segredo confirma; a descrição é externa. **Só funciona se o modelo aceitar imagem; não validado contra um modelo real** | 4 |
| `capturar_tela` | `vision.py` | write | portada | orion.tools.vision (opt-in): comando nativo (`screencapture`, PowerShell, `grim`/`scrot`/`import`), grava em `<dados>/capturas` (guarda as últimas 20); **virou escrita com log**; não há captura contínua | 4 |
| `explicar_tela` | `vision.py` | exec (externo) | portada | orion.tools.vision (opt-in): captura e envia a tela inteira ao modelo; **virou execução: confirma sempre**; a captura é apagada depois | 4 |
| `gerar_imagem` | `vision.py` | write | a-portar | API gratuita de imagem; depende de cota (decisão #7) | 6 |
| `buscar_url` | `web.py` | read (externo) | portada | orion.tools.web (opt-in `ORION_WEB_TOOLS`) + `orion.netguard` (regra 7: IP conferido é o IP usado) | 4 |
| `consultar_clima` | `web.py` | read | portada | orion.tools.web (opt-in); Open-Meteo, sem chave (o wttr.in do legado saiu) | 4 |
| `navegar_web` | `web.py` | exec (externo) | a-portar | servidor MCP de navegador (Playwright) em `mcp.json`, `external: true`; o cliente MCP já existe (`orion mcp-check`) | 4 |
| `pesquisar_com_ia` | `web.py` | read (externo) | portada | orion.tools.web (opt-in); Gemini com Google Search; **não validado contra a API real** (decisão #7) | 4 |
| `pesquisar_internet` | `web.py` | read (externo) | a-portar | sem API de busca gratuita estável: `pesquisar_com_ia` cobre; ou um servidor MCP de busca em `mcp.json` (decisão #7) | 4 |
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

`listar_fatos`, `esquecer_fato` e `delegar` já nasceram na reescrita (fase 2/3) e não existiam no legado.
