# Orion — Regras

> Princípios que valem para o Orion inteiro. Cada um diz **onde o código o impõe** e **qual teste o prova**: princípio sem teste é desejo. O texto antigo, numerado, está em [ORION_LEGADO.md](ORION_LEGADO.md) (mapa no início do arquivo).
>
> **Núcleo imutável:** este arquivo, `orion/persona.py` e `orion/policy/` não são editados por ferramenta alguma. Mudança aqui exige sua aprovação, por canal autenticado.

## Princípios de base

1. **Você é o administrador.** Nenhum raciocínio do modelo anula uma decisão sua.
2. **Núcleo imutável.** Persona, política e este arquivo ficam fora do alcance das ferramentas.
3. **Nuvem por padrão, custo zero.** Só cotas gratuitas e as assinaturas oficiais em tarefa pesada. Modelo local só como reserva. Captura contínua (tela, áudio ambiente, biometria) exige aprovação.
4. **Portabilidade.** Nada depende de um hardware específico: máquina, sistema, modelos e provedores vêm de configuração.
5. **Auto-modificação do código exige aprovação explícita.**
6. **Ação de alto risco exige confirmação explícita**, inclusive pelo celular.

## Ferramentas e política

| Princípio | Imposto em | Prova |
|---|---|---|
| Toda ferramenta tem classe de risco fixa: leitura roda direto; escrita roda e fica no audit; execução e destrutiva pedem confirmação. A classe vem do registro, não do texto da chamada. Ferramenta sem classe é negada. | `policy/classes.py`, `policy/engine.py` | `tests/policy/test_engine.py`, `tests/test_tools.py` |
| Confirmação fora de banda: o pedido sai por canal autenticado (botão do Telegram, tela com login), nunca por frase na conversa. Vale para sessão, ferramenta e argumentos exatos, por 10 minutos, uso único. O modelo não se auto-aprova. | `policy/approvals.py`, `app.py` (`/approvals`) | `tests/policy/test_approvals.py`, `tests/test_app_chat.py` |
| Código, persona e política são imutáveis para as ferramentas. Escrita na raiz do projeto, em diretório de sistema, em arquivo sensível ou fora de Documents, Downloads e Desktop pede confirmação. Auto-modificação só por diff aprovado. | `policy/paths.py` | `tests/policy/test_paths.py` |
| Conteúdo externo é dado, não instrução (web, e-mail, documento, área de transferência). Depois de lido, escrita e execução da sessão pedem confirmação, mesmo depois de reiniciar. | `agent.py`, `policy/engine.py` (`note_result`) | `tests/test_agent.py` |
| Segredos nunca vão para log, audit ou processo filho. Chaves ficam no cofre do SO ou no `.env`; logs mascaram chaves e tokens; a CLI delegada não herda `ORION_*`. | `policy/audit.py`, `log.py`, `delegate.py`, `secrets.py` | `tests/policy/test_engine.py`, `tests/test_delegate.py` |
| Ler segredo ou chave (`.env`, `.ssh`, `*.pem`, credenciais OAuth) pede confirmação. O caminho é resolvido antes: symlink para `.env` continua sendo `.env`. | `policy/paths.py` (`check_read`), `tools/desktop.py` | `tests/policy/test_paths.py`, `tests/test_desktop.py` |
| Shell só roda sem confirmação o que for provadamente leitura: lista positiva, sem redirecionamento, substituição ou bloco de script. O resto confirma. | `policy/shell.py` | `tests/policy/test_shell.py` |
| URL vinda do modelo só vai a host público, com cada redirecionamento revalidado. O IP conferido é o IP usado (sem janela de DNS rebinding). | `netguard.py` | `tests/test_netguard.py`, `tests/test_web_tools.py` |
| Falha de auditoria nega a ação que não é leitura (fail-closed). Toda decisão vai também para a tabela `audit`, com poda de 90 dias. | `policy/engine.py`, `app.py` (`build_policy`), `memory/ops.py` | `tests/policy/test_engine.py`, `tests/memory/test_ops.py` |
| Só se aprova o que se vê inteiro: argumento cortado para exibição ou grande demais para o cartão só pode ser negado. | `agent.py`, `app.py`, `channels/telegram.py` | `tests/test_agent.py`, `tests/test_telegram.py` |
| MCP: a classe de risco de uma ferramenta de servidor vem do `mcp.json`, nunca do servidor; sem classe, é `exec`. O servidor sobe sem `ORION_*` nem segredos, e o que ele anuncia é tratado como texto não confiável. | `mcp_client.py`, `policy/engine.py` (`register_tool`) | `tests/mcp_cliente/test_mcp_client.py` |
| O que o modelo escreve nunca vira comando para o computador: texto de notificação, nome de app e área de transferência entram como argumento, não na linha de comando; planilha grava texto, nunca fórmula; Git roda sem `fsmonitor`; `abrir_app` é execução. | `tools/system_tools.py`, `tools/documents.py`, `tools/processes.py` | `tests/test_system_tools.py`, `tests/test_documents.py`, `tests/test_tools_politica.py` |
| Destino de rede escolhido pelo modelo confirma depois de conteúdo externo, com a URL inteira no cartão e valendo só para aquele endereço. Destino fixo (Open-Meteo, Google, Brave) fica de fora. | `policy/classes.py` (`egress`), `policy/engine.py`, `mcp_client.py` | `tests/policy/test_engine.py`, `tests/test_agent.py` |
| Título de janela nunca vira comando: `controlar_janela` (inclusive fechar) é execução e confirma sempre; busca que acha mais de uma janela não age. | `tools/media.py` | `tests/test_media.py` |
| Imagem gerada tem nome gerado pelo código, só PNG, JPEG ou WEBP com assinatura conferida, e só é servida com login (`nosniff`, CSP `sandbox`). | `tools/web.py`, `app.py` (`/imagens`) | `tests/test_web_novas.py`, `tests/front/md.test.js` |

## Rede e interface

| Princípio | Imposto em | Prova |
|---|---|---|
| Só `127.0.0.1` no bind. Bind público é recusado, salvo opt-in explícito; `Host` fora da lista é recusado (DNS rebinding). | `config.py`, `app.py` | `tests/test_app.py` |
| Toda rota da API exige login: sessão por cookie ou token de admin em `Authorization: Bearer`. Cabeçalho errado nunca cai para o cookie. `/health` sem login só diz `status` e `version`; `/docs` e `/openapi.json` não existem. | `app.py` (`require_auth`) | `tests/test_app.py`, `tests/test_auth.py` |
| Acesso de fora só com login: host que não é local só sobe com senha definida ou token de admin. Sem nenhum dos dois, o app se recusa a subir. | `config.py` (`hosts_de_fora`), `app.py` (lifespan) | `tests/test_app.py::test_host_de_fora_so_sobe_com_login_senha_ou_token` |
| Login com senha e sessão revogável: PBKDF2-SHA256 com 600 mil iterações; sessão opaca de 256 bits, guardada só em hash, em cookie `httpOnly` e `SameSite=Strict`; erro de senha trava por cliente. Senha e sessões ficam em `auth.db`, fora do backup. | `auth.py`, `app.py`, `__main__.py` | `tests/test_auth.py`, `tests/front_e2e/test_front.py` |
| WebSocket confere `Origin`, porque WebSocket não tem CORS: só navegador local ou cliente sem `Origin`. | `app.py` (`_ws_abrir`) | `tests/test_voice.py` (`test_sessao_por_cookie_e_origem_conferida`) |
| Upload tem limite de tamanho (25 MB) e nome sanitizado. | `Orion_Ollama/routers/misc.py` (legado) | `tests/legacy/test_misc_router_legado.py` |
| O chat só renderiza imagem gerada pelo próprio Orion (`/imagens/<arquivo>`). Imagem de outro host é canal de exfiltração por prompt injection. | `Orion_Core/Front_end_Orion/js/md.js` (legado) | `tests/front/md.test.js`, `tests/front_e2e/test_front.py` |
| O painel só lê e nunca mostra segredo: do gateway só nome, modelo e tipo do último erro; aprovações e decisões aparecem sem argumentos. | `painel.py`, `gateway.py` (`stats`) | `tests/test_painel.py`, `tests/front/painel.test.js` |
| O `/mcp` do legado não atende página de navegador e confere `Host`. Cliente de linha de comando, sem `Origin`, segue funcionando. | `Orion_Ollama/origem.py`, `cerebro_maestro.py` (legado) | `tests/legacy/test_mcp_origem_legado.py` |

## Voz, palmas e ponte

| Princípio | Imposto em | Prova |
|---|---|---|
| Voz por clique é canal do agente, não atalho: passa pela memória, pela política e pelo audit. **Aprovar nunca é por voz**, e dizer "aprovado" não faz nada. Desligada por padrão. | `voice.py`, `app.py` (`/ws/voz`) | `tests/test_voice.py` |
| Voz e foto não ganham poder novo: a fala transcrita é mostrada antes da resposta; a foto vale só para o turno em que chegou; aprovar continua só por botão. | `channels/telegram.py`, `transcribe.py`, `agent.py` | `tests/test_telegram.py`, `tests/test_transcribe.py` |
| Voz ao vivo só conversa: sem ferramentas, sem memória e sem acesso ao computador. Opt-in, com duração máxima e registro de início e fim. | `voice.py` (`conexao_gemini`, `ponte_ao_vivo`), `app.py` (`/ws/voice`) | `tests/test_voice.py` |
| A palavra de ativação só abre o microfone, e a detecção nunca sai do computador. Teto de ativações por hora, pausa pelo painel, resfriamento depois de cada turno; audit sem áudio nem texto. Desligada por padrão. | `wake.py`, `app.py` (`_iniciar_escuta`, `/voz/escuta`) | `tests/test_wake.py`, `tests/test_wake_app.py` |
| Palmas só abrem; nunca aprovam nem executam. A única exceção é acionar uma rotina que você concedeu. Duas palmas em 150 a 700 ms disparam; o áudio só existe no quadro atual; teto por hora; respeita pausa, pânico e não perturbe. | `wake.py` (`ClapDetector`), `app.py` | `tests/test_wake.py` |
| A ponte de desktop só abre a janela, lê quando você aperta a tecla e cola o que você aprovou. Não executa nada por conta própria. Seu token vale só para a ponte. Pânico derruba a ponte. | `ponte/`, `auth.py` (`device_tokens`), `app.py` | `tests/test_ponte.py` |

## Captura, tela e dados pessoais

| Princípio | Imposto em | Prova |
|---|---|---|
| Tela e imagem não saem do computador sem seu aval: `explicar_tela` confirma sempre; a captura só grava em pasta própria, com as últimas 20; a captura explicada é apagada; não há captura contínua. | `tools/vision.py`, `vision.py`, `policy/classes.py` | `tests/test_vision.py` |
| Memória da tela: só texto, só local, só com opt-in. Janela com título excluído ou sem título não é capturada; tela bloqueada não é capturada; retenção de 7 dias por padrão. | `screen_memory.py` | `tests/test_screen_memory.py`, `tests/test_e5_tela.py` |
| Captura rápida e briefing não passam pelo modelo: `/capturar` é comando seu, grava só dentro do vault, com nome gerado e sem sobrescrever. O briefing é texto determinístico do que o Orion guarda. | `capture.py`, `briefing.py`, `channels/telegram.py`, `jobs.py` | `tests/test_capture.py`, `tests/test_briefing.py` |
| Históricos locais (área de transferência e comandos): opt-in, só texto, só neste computador, com retenção, e nunca saem dele. | `historicos.py` | `tests/test_historicos.py` |
| Transcrever mídia é comando seu: você escolhe o arquivo ou o link, e o comando avisa e pergunta antes de enviar o áudio ao provedor. | `media_transcribe.py` | `tests/test_media_transcribe.py` |
| O que é de um projeto fica no projeto: fato, documento, conversa e aviso de um projeto só aparecem nele. A memória pessoal não enxerga o projeto, e um projeto não enxerga o de outro. | `memory/store.py`, `memory/consolidate.py`, `grafo.py` | `tests/projects/test_isolation.py`, `tests/memory/test_e4_memoria.py` |
| Toda saída de dado para fora fica registrada, sem o conteúdo: provedor, tipo, bytes, resultado e latência. Registrar nunca derruba a chamada. | `saidas.py`, `memory/store.py` (`external_calls`) | `tests/test_saidas.py` |

## Memória

| Princípio | Imposto em | Prova |
|---|---|---|
| Agendamento avisa; não executa ferramenta sozinho. Rodar ação sem ninguém olhando exige política e aprovação. A única exceção é a agenda do briefing. | `jobs.py` | `tests/test_jobs.py` |
| Fato consolidado só vem de fala sua, nunca de resposta do modelo nem de resultado de ferramenta. Tem fonte e data, é editável, e segredo é descartado. | `memory/consolidate.py` | `tests/memory/test_consolidate.py` |
| Pendência velha não vira rajada de aviso: lembrete vencido há mais de 24 horas entra já avisado; agendamento único atrasado entra desativado. | `memory/importer.py` | `tests/memory/test_importer.py` |
| O ciclo de sono revisa, não decide: duplicados só são avisados; o modelo vê só fatos já guardados; padrões viram fatos editáveis e apagáveis. | `memory/sleep.py` | `tests/test_sleep.py` |
| O resumo da conversa nunca substitui o registro e só nasce no computador (modelo local). As mensagens continuam no banco. | `memory/compact.py`, `agent.py` | `tests/memory/test_compact.py` |
| O Orion nunca decide sozinho qual fato vale. Fato antigo ou substituído pesa menos na busca e aparece como antigo, mas nada é apagado; quem decide é você. | `memory/store.py`, `memory/sleep.py`, `memory/rerank.py` | `tests/memory/test_e4_memoria.py`, `tests/projects/test_facts_e4.py` |
| A persona não carrega fato que envelhece (hardware, versão, porta, provedor). Isso vem de configuração. Persona é versionada e datada. | `persona.py` | `tests/test_persona.py` |

## Automação, extensões e segundo plano

| Princípio | Imposto em | Prova |
|---|---|---|
| Skill é texto, não código: só nome e descrição entram no prompt, o corpo vem sob demanda, `scripts/` nunca roda, e a skill não muda classe de risco nem libera aprovação. | `skills.py` | `tests/test_skills.py` |
| Plugin é pacote sem poder próprio: instalar só valida e copia. Fica inativo até você conceder, e a concessão guarda o hash do pacote inteiro; qualquer mudança cancela. | `plugins.py` | `tests/test_plugins.py` |
| Catálogo de plugins: baixar não instala, instalar não concede, e nada atualiza sozinho. O download confere o SHA-256 fixado no índice. | `extensions/catalogo.py`, `extensions/importar.py` | `tests/extensions/test_e8.py` |
| Automação sua vale pelo arquivo que você escreveu e concedeu (SHA-256). O gatilho é sempre seu: frase, tecla, `/rotina` ou palmas. O modelo nunca aciona. | `rotinas.py`, `downloads.py`, `automacao.py` | `tests/test_rotinas.py`, `tests/test_downloads.py` |
| Tarefa em segundo plano só lê e propõe: o que seria escrita vira proposta que você aprova item a item. | `tarefas_bg.py` | `tests/test_tarefas_bg.py` |
| Pesquisa noturna é só leitura e sem ninguém aprovando: assuntos vêm de você, e o que pediria aprovação é negado. | `research/night.py` | `tests/test_research.py` |
| A agenda do briefing é leitura fixa e roda sem modelo: uma ferramenta MCP escolhida no código, e só se o `mcp.json` a classifica como leitura. | `agenda.py`, `briefing.py`, `jobs.py` | `tests/test_agenda.py` |
| n8n: o destino é seu, não do modelo. O modelo só escolhe o nome do webhook, e acionar sempre confirma. | `tools/n8n.py` | `tests/test_n8n.py` |
| Pânico é corte, não pausa: para memória da tela, palavra de ativação e jobs de rede, e tira do modelo as ferramentas de rede, execução e destrutivas. Só sai por comando explícito. Não perturbe só segura avisos. | `modos.py`, `agent.py`, `jobs.py`, `app.py` (`/modo`) | `tests/test_modos.py` |
| Modelo local é reserva sem ferramentas, só neste computador. Entra como último endpoint e nunca recebe `tools`; o endereço só aceita loopback. | `config.py`, `gateway.py` (`Endpoint.tools`) | `tests/test_modelo_local.py` |
| Custo zero: nenhum código chama API paga. A cota é a que o Orion contou; job opcional não roda com 90% da cota; `orion doctor` avisa provedor de fora da lista gratuita. | `costs.py`, `jobs.py`, `doctor.py`, `painel.py` | `tests/test_costs.py`, `tests/test_doctor.py` |
| O Orion como servidor MCP só lê: expõe busca de memória, fatos e conversas, sem escrita, execução, rede ou caminho de arquivo. O token é mostrado uma vez, fica só em hash e é revogável. | `mcp_servidor.py` | `tests/test_mcp_servidor.py` |
| Recurso e prompt de MCP são dado e comando seu. Recurso só se o servidor listou e o `mcp.json` permite. Prompt só você invoca, e nunca é enviado sozinho. | `mcp_client.py`, `mcp_indice.py`, `agent.py` | `tests/mcp_cliente/`, `tests/test_e7.py` |
| Servidor MCP com muitas ferramentas mostra ao modelo só um índice e `procurar_ferramenta`. O esquema entra depois, e classe de risco e aprovação não mudam. | `mcp_indice.py`, `agent.py` | `tests/test_e7.py` |

## Código e qualidade

| Princípio | Imposto em | Prova |
|---|---|---|
| SQL só parametrizado; identificadores vêm de constantes; `except` específico e nada de `pass` silencioso. | ruff (`S`, `BLE`) no CI | `uv run ruff check .` |
| Ruff, pyright e pytest verdes em Linux, Windows e macOS para mergear. | `.github/workflows/ci.yml` | o próprio CI |

## Decidido

- **Portabilidade** (antigo "Hardware-Bound Logic Gates", da Lyra): decidido em 07/10/2026, porque conflitava com trocar de máquina. Está no princípio de base 4.

## Em aberto (sua decisão)

- Aprovação por voz ou por frase foi descartada. Se quiser um canal sem botão, ele precisa de autenticação própria: a palavra de ativação não basta.
- As regras das etapas E10 a E13 (vida pessoal, visão e câmera, trabalho e carreira, segurança e continuidade) serão escritas aqui antes do código delas, como o plano pede.
