# Orion — Regras

> Princípios que valem para o Orion inteiro. Cada um diz **onde o código o impõe** e **qual teste o prova**: princípio sem teste é desejo. O texto antigo, numerado, está em [ORION_LEGADO.md](ORION_LEGADO.md) (mapa no início do arquivo).
>
> **Núcleo imutável:** este arquivo, `crates/orion-policy/` e `crates/orion-core/src/persona.rs` não são editados por ferramenta alguma. Mudança aqui exige sua aprovação explícita, por canal autenticado.
>
> **Atualização arquitetural (produto nativo em Rust):** As colunas "Imposto em" e "Prova" apontam diretamente para o código Rust no workspace `Orion_Rust/crates/`. O produto é um executável único (`orion.exe`) que hospeda a interface e o núcleo no mesmo processo, sem porta de rede permanente. O login usa **Argon2id** (com migração de PBKDF2), o banco é cifrado por **SQLCipher** (ChaCha20-Poly1305), as chamadas web passam por `netguard` em Rust, e a documentação operacional e técnica é consolidada em [ORION.md](ORION.md), [ARQUITETURA.md](ARQUITETURA.md), [ORION_MELHORIAS_PRE_V1.md](ORION_MELHORIAS_PRE_V1.md) e [ORION_MELHORIAS_POS_V1.md](ORION_MELHORIAS_POS_V1.md).

## Princípios de base

1. **Você é o administrador.** Nenhum raciocínio do modelo anula uma decisão sua.
2. **Núcleo imutável.** Persona, política e este arquivo ficam fora do alcance das ferramentas.
3. **Nuvem por padrão, custo zero.** Só cotas gratuitas e as assinaturas oficiais em tarefa pesada. Modelo local só como reserva. Captura contínua (tela, áudio ambiente, biometria) exige aprovação.
4. **Portabilidade.** Nada depende de um hardware específico: máquina, sistema, modelos e provedores vêm de configuração.
5. **Auto-modificação do código exige aprovação explícita.**
6. **Ação de alto risco exige confirmação explícita**, inclusive pelo celular.

## Ferramentas e política

**Autorização persistente de ferramentas (decisão explícita do Antônio, 26/10/2026).** No primeiro uso nativo, a pessoa pode autorizar de uma vez as ferramentas do computador e da web sem confirmação por ação, inclusive execução, ações destrutivas, leitura de arquivos sensíveis e ações após conteúdo externo. A escolha começa desmarcada, fica somente neste computador (fora de backups) e pode ser alterada nas Configurações autenticadas. Enquanto concedida, substitui as exigências de confirmação por ação descritas abaixo; desligá-la restaura essas exigências. O modelo não concede essa autorização. Continuam valendo classes conhecidas, isolamento de projetos, concessões/revisões de extensões, limites, auditoria, proteção de rede, modo só leitura, pânico e confirmação específica para auto-modificação do Orion. Autorizar ferramentas não ativa captura contínua, voz, câmera, integrações sem credenciais ou plugins ainda não concedidos.

| Princípio | Imposto em | Prova |
|---|---|---|
| **autorização única, local e revogável.** O consentimento do primeiro uso e as Configurações autenticadas controlam a disponibilidade de computador/web e a dispensa de confirmações, com efeito imediato. O padrão segue sem consentimento; restauração não importa concessão. | `orion-core/src/tool_permissions.rs`, `orion-policy/src/engine.rs`, `orion-server/src/estado.rs`, `orion-server/src/rotas/geral.rs`, `orion-app/src/views/settings.rs` | Suíte existente de política, onboarding e app; verificação manual dos fluxos de concessão/revogação (testes novos pausados pela ADR 0007). |

| Princípio | Imposto em | Prova |
|---|---|---|
| **classe de risco fixa por ferramenta.** Toda ferramenta tem classe de risco fixa: leitura roda direto; escrita roda e fica no audit; execução e destrutiva pedem confirmação. A classe vem do registro, não do texto da chamada. Ferramenta sem classe é negada. | `orion-policy/src/classes.rs`, `orion-policy/src/engine.rs` | `orion-policy/tests/` |
| **confirmação fora de banda (canal autenticado).** Confirmação fora de banda: o pedido sai por canal autenticado (botão do Telegram, tela com login), nunca por frase na conversa. Vale para sessão, ferramenta e argumentos exatos, por 10 minutos, uso único. O modelo não se auto-aprova. | `orion-policy/src/approvals.rs`, `orion-server/src/rotas/` | `orion-policy/tests/`, `orion-server/tests/appmode.rs` |
| **código, persona e política imutáveis para as ferramentas.** Código, persona e política são imutáveis para as ferramentas. Escrita na raiz do projeto, em diretório de sistema, em arquivo sensível ou fora de Documents, Downloads e Desktop pede confirmação. Auto-modificação só por diff aprovado. | `orion-policy/src/paths.rs` | `orion-policy/tests/` |
| **conteúdo externo é dado, não instrução.** Conteúdo externo é dado, não instrução (web, e-mail, documento, área de transferência). Depois de lido, escrita e execução da sessão pedem confirmação, mesmo depois de reiniciar. | `orion-core/src/agent.rs`, `orion-policy/src/engine.rs` | `orion-core/tests/` |
| **segredos nunca em log, audit ou processo filho.** Segredos nunca vão para log, audit ou processo filho. Chaves ficam no cofre do SO (Keyring) ou no `.env`; logs mascaram chaves e tokens; a CLI delegada não herda `ORION_*`. | `orion-policy/src/audit.rs`, `orion-core/src/logcrypt.rs`, `orion-ai/src/gateway.rs` | `orion-policy/tests/`, `orion-core/tests/` |
| **ler segredo ou chave pede confirmação.** Ler segredo ou chave (`.env`, `.ssh`, `*.pem`, credenciais OAuth) pede confirmação. O caminho é resolvido antes: symlink para `.env` continua sendo `.env`. | `orion-policy/src/paths.rs`, `orion-core/src/tools/` | `orion-policy/tests/` |
| **shell só roda sem confirmação o que é leitura provada.** Shell só roda sem confirmação o que for provadamente leitura: lista positiva, sem redirecionamento, substituição ou bloco de script. O resto confirma. | `orion-policy/src/shell.rs` | `orion-policy/tests/` |
| **URL do modelo só para host público.** URL vinda do modelo só vai a host público, com cada redirecionamento revalidado. O IP conferido é o IP usado (sem janela de DNS rebinding). | `orion-ai/src/netguard.rs` | `orion-ai/tests/` |
| **falha de auditoria nega a ação.** Falha de auditoria nega a ação que não é leitura (fail-closed). Toda decisão vai também para a tabela `audit`, com poda de 90 dias. | `orion-policy/src/engine.rs`, `orion-store/src/` | `orion-policy/tests/` |
| **só se aprova o que se vê inteiro.** Só se aprova o que se vê inteiro: argumento cortado para exibição ou grande demais para o cartão só pode ser negado. | `orion-core/src/agent.rs`, `orion-server/src/rotas/`, `orion-core/src/channels/telegram.rs` | `orion-core/tests/` |
| **MCP: a classe de risco é do mcp.json, não do servidor.** MCP: a classe de risco de uma ferramenta de servidor vem do `mcp.json`, nunca do servidor; sem classe, é `exec`. O servidor sobe sem `ORION_*` nem segredos, e o que ele anuncia é tratado como texto não confiável. | `orion-mcp/src/client.rs`, `orion-policy/src/engine.rs` | `orion-mcp/tests/` |
| **o que o modelo escreve nunca vira comando para o computador.** O que o modelo escreve nunca vira comando para o computador: texto de notificação, nome de app e área de transferência entram como argumento, não na linha de comando; planilha grava texto, nunca fórmula; Git roda sem `fsmonitor`; `abrir_app` é execução. | `orion-core/src/tools/` | `orion-core/tests/` |
| **destino de rede escolhido pelo modelo confirma após conteúdo externo.** Destino de rede escolhido pelo modelo confirma depois de conteúdo externo, com a URL inteira no cartão e valendo só para aquele endereço. Destino fixo (Open-Meteo, Google, Brave) fica de fora. | `orion-policy/src/classes.rs`, `orion-mcp/src/` | `orion-policy/tests/` |
| **título de janela nunca vira comando.** Título de janela nunca vira comando: `controlar_janela` (inclusive fechar) é execução e confirma sempre; busca que acha mais de uma janela não age. | `orion-core/src/tools/media.rs` | `orion-core/tests/` |
| **imagem gerada com nome do código, tipo conferido e só com login.** Imagem gerada tem nome gerado pelo código, só PNG, JPEG ou WEBP com assinatura conferida, e só é servida com login (`nosniff`, CSP `sandbox`). | `orion-core/src/tools/web.rs`, `orion-server/` | `orion-server/tests/` |

## Rede e interface

| Princípio | Imposto em | Prova |
|---|---|---|
| **bind só em 127.0.0.1 (quando HTTP ativado).** O aplicativo opera no mesmo processo sem portas no modo normal. Com `--features http`, bind só em `127.0.0.1`. Bind público é recusado; `Host` fora da lista é recusado (DNS rebinding). | `orion-server/src/appmode.rs` | `orion-server/tests/appmode.rs` |
| **toda rota da API exige login.** Toda rota da API exige login: sessão por cookie ou token de admin em `Authorization: Bearer`. Cabeçalho errado nunca cai para o cookie. `/health` sem login só diz `status` e `version`; rotas de docs não existem em produção. | `orion-server/src/` | `orion-server/tests/` |
| **acesso de fora só com login.** Acesso de fora só com login: host que não é local só sobe com senha definida ou token de admin. Sem nenhum dos dois, o app se recusa a subir. | `orion-server/src/appmode.rs` | `orion-server/tests/` |
| **Telegram com default-deny e aprovação só por botão.** Só responde a ID da lista, em conversa privada; o resto é ignorado em silêncio. Só o clique de usuário permitido num cartão de aprovação decide; frase nunca. O token nunca aparece em log nem em erro. Sem token, lista e gateway, o canal não sobe. | `orion-core/src/channels/telegram.rs` | `orion-core/tests/` |
| **login com senha e sessão revogável.** Login com senha e sessão revogável: Argon2id (19 MiB, 2 passes; PBKDF2 só migra no primeiro login); sessão opaca de 256 bits guardada em hash; erro de senha trava temporariamente por cliente. Senha e sessões ficam em `auth.db`, fora do backup. | `orion-core/src/auth.rs` | `orion-core/tests/auth.rs` |
| **WebSocket confere Origin.** WebSocket confere `Origin` no modo HTTP: só navegador local ou cliente sem `Origin`. | `orion-server/src/` | `orion-server/tests/` |
| **upload com limite de tamanho e nome sanitizado.** Upload tem limite de tamanho (6 MB por anexo, até 8 arquivos) e nome sanitizado. | `orion-server/src/rotas/` | `orion-server/tests/` |
| **chat só renderiza imagem gerada pelo Orion.** O chat só renderiza imagem gerada pelo próprio Orion (`/imagens/<arquivo>`). Imagem de outro host é canal de exfiltração por prompt injection. | `orion-app/src/views/chat.rs` | `orion-app/tests/` |
| **painel só lê e nunca mostra segredo.** O painel só lê e nunca mostra segredo: do gateway só nome, modelo e tipo do último erro; aprovações e decisões aparecem sem argumentos. | `orion-core/src/painel.rs`, `orion-ai/src/gateway.rs` | `orion-core/tests/` |
| **/mcp não atende página de navegador.** O servidor `/mcp` não atende página de navegador e confere `Host`. Clientes nativos stdio e HTTP local funcionam sob concessão. | `orion-mcp/src/server.rs` | `orion-mcp/tests/` |

## Voz, palmas e ponte

| Princípio | Imposto em | Prova |
|---|---|---|
| **voz por clique é canal do agente; aprovar nunca é por voz.** Voz por clique é canal do agente, não atalho: passa pela memória, pela política e pelo audit. **Aprovar nunca é por voz**, e dizer "aprovado" não faz nada. Desligada por padrão. | `orion-voice/src/`, `orion-core/src/agent.rs` | `orion-voice/tests/` |
| **voz e foto não ganham poder novo.** Voz e foto não ganham poder novo: a fala transcrita é mostrada antes da resposta; a foto vale só para o turno em que chegou; aprovar continua só por botão. | `orion-core/src/channels/telegram.rs`, `orion-ai/src/transcribe.rs` | `orion-core/tests/` |
| **voz ao vivo só conversa, sem ferramentas nem memória.** Voz ao vivo só conversa: sem ferramentas, sem memória e sem acesso ao computador. Opt-in, com duração máxima e registro de início e fim. | `orion-voice/src/`, `orion-ai/src/gateway.rs` | `orion-voice/tests/` |
| **palavra de ativação só abre o microfone; detecção local.** A palavra de ativação só abre o microfone, e a detecção nunca sai do computador. Teto de ativações por hora, pausa pelo painel, resfriamento depois de cada turno; audit sem áudio nem texto. Desligada por padrão. | `orion-voice/src/wake.rs` | `orion-voice/tests/` |
| **palmas só abrem; nunca aprovam nem executam.** Palmas só abrem; nunca aprovam nem executam. A única exceção é acionar uma rotina que você concedeu. Duas palmas em 150 a 700 ms disparam; o áudio só existe no quadro atual; teto por hora; respeita pausa, pânico e não perturbe. | `orion-voice/src/clap.rs` | `orion-voice/tests/` |
| **ponte de desktop só abre e cola o que você aprovou.** A ponte de desktop só abre a janela, lê quando você aperta a tecla e cola o que você aprovou. Não executa nada por conta própria. Seu token vale só para a ponte. Pânico derruba a ponte. | `orion-platform/src/`, `orion-core/src/auth.rs` | `orion-platform/tests/` |

## Captura, tela e dados pessoais

| Princípio | Imposto em | Prova |
|---|---|---|
| **tela e imagem só saem do computador com aval.** Tela e imagem não saem do computador sem seu aval: captura de tela confirma sempre; gravação temporária em pasta própria com retenção restrita; descarte após uso. | `orion-core/src/tools/vision.rs`, `orion-policy/src/classes.rs` | `orion-policy/tests/` |
| **memória da tela: só texto, local e com opt-in.** Memória da tela: só texto, só local, só com opt-in. Janela com título excluído ou sem título não é capturada; tela bloqueada não é capturada; retenção de 7 dias por padrão. | `orion-core/src/tools/vision.rs` | `orion-core/tests/` |
| **captura rápida e briefing não passam pelo modelo.** Captura rápida e briefing não passam pelo modelo: `/capturar` é comando seu, grava só dentro do vault, com nome gerado e sem sobrescrever. O briefing é texto determinístico do que o Orion guarda. | `orion-core/src/capture.rs`, `orion-core/src/briefing.rs` | `orion-core/tests/` |
| **históricos locais: opt-in, só texto, só neste computador.** Históricos locais (área de transferência e comandos): opt-in, só texto, só neste computador, com retenção, e nunca saem dele. | `orion-core/src/historicos.rs` | `orion-core/tests/` |
| **transcrever mídia é comando seu.** Transcrever mídia é comando seu: você escolhe o arquivo ou o link, e o comando avisa e pergunta antes de enviar o áudio ao provedor. | `orion-ai/src/transcribe.rs` | `orion-ai/tests/` |
| **o que é de um projeto fica no projeto.** O que é de um projeto fica no projeto: fato, documento, conversa e aviso de um projeto só aparecem nele. A memória pessoal não enxerga o projeto, e um projeto não enxerga o de outro. | `orion-store/src/workspace.rs`, `orion-core/src/memory/` | `orion-store/tests/` |
| **toda saída de dado fica registrada, sem conteúdo.** Toda saída de dado para fora fica registrada, sem o conteúdo: provedor, tipo, bytes, resultado e latência na tabela `external_calls`. | `orion-store/src/`, `orion-ai/src/gateway.rs` | `orion-store/tests/` |

## Memória

| Princípio | Imposto em | Prova |
|---|---|---|
| **agendamento avisa; não executa ferramenta sozinho.** Agendamento avisa; não executa ferramenta sozinho. Rodar ação sem ninguém olhando exige política e aprovação. A única exceção é a agenda do briefing. | `orion-core/src/jobs.rs` | `orion-core/tests/` |
| **fato consolidado só vem de fala do Antônio.** Fato consolidado só vem de fala sua, nunca de resposta do modelo nem de resultado de ferramenta. Tem fonte e data, é editável, e segredo é descartado. | `orion-core/src/memory/consolidate.rs` | `orion-core/tests/` |
| **pendência velha não vira rajada de aviso.** Pendência velha não vira rajada de aviso: lembrete vencido há mais de 24 horas entra já avisado; agendamento único atrasado entra desativado. | `orion-core/src/memory/importer.rs` | `orion-core/tests/` |
| **ciclo de sono revisa, não decide.** O ciclo de sono revisa, não decide: duplicados só são avisados; o modelo vê só fatos já guardados; padrões viram fatos editáveis e apagáveis. | `orion-core/src/memory/sleep.rs` | `orion-core/tests/` |
| **resumo da conversa nunca substitui o registro.** O resumo da conversa nunca substitui o registro e só nasce no computador (modelo local). As mensagens continuam no banco. | `orion-core/src/memory/compact.rs`, `orion-core/src/agent.rs` | `orion-core/tests/` |
| **o Orion não decide sozinho qual fato vale.** O Orion nunca decide sozinho qual fato vale. Fato antigo ou substituído pesa menos na busca e aparece como antigo, mas nada é apagado; quem decide é você. | `orion-store/src/`, `orion-core/src/memory/` | `orion-store/tests/` |
| **persona sem fato que envelhece.** A persona não carrega fato que envelhece (hardware, versão, porta, provedor). Isso vem de configuração. Persona é versionada e datada. | `orion-core/src/persona.rs` | `orion-core/tests/` |

## Automação, extensões e segundo plano

| Princípio | Imposto em | Prova |
|---|---|---|
| **skill é texto, não código.** Skill é texto, não código: só nome e descrição entram no prompt, o corpo vem sob demanda, `scripts/` nunca roda, e a skill não muda classe de risco nem libera aprovação. | `orion-core/src/skills.rs` | `orion-core/tests/` |
| **plugin é pacote sem poder próprio.** Plugin é pacote sem poder próprio: instalar só valida e copia. Fica inativo até você conceder, e a concessão guarda o hash do pacote inteiro; qualquer mudança cancela. | `orion-core/src/plugins.rs` | `orion-core/tests/` |
| **catálogo de plugins: baixar não instala, nada atualiza sozinho.** Catálogo de plugins: baixar não instala, instalar não concede, e nada atualiza sozinho. O download confere o SHA-256 fixado no índice. | `orion-core/src/extensions/` | `orion-core/tests/` |
| **automação sua vale pelo arquivo concedido.** Automação sua vale pelo arquivo que você escreveu e concedeu (SHA-256). O gatilho é sempre seu: frase, tecla, `/rotina` ou palmas. O modelo nunca aciona. | `orion-core/src/rotinas.rs`, `orion-core/src/automacao.rs` | `orion-core/tests/` |
| **tarefa em segundo plano só lê e propõe.** Tarefa em segundo plano só lê e propõe: o que seria escrita vira proposta que você aprova item a item. | `orion-core/src/tarefas_bg.rs` | `orion-core/tests/` |
| **pesquisa noturna só lê, sem aprovação.** Pesquisa noturna é só leitura e sem ninguém aprovando: assuntos vêm de você, e o que pediria aprovação é negado. | `orion-core/src/research.rs` | `orion-core/tests/` |
| **agenda do briefing é leitura fixa, sem modelo.** A agenda do briefing é leitura fixa e roda sem modelo: uma ferramenta MCP escolhida no código, e só se o `mcp.json` a classifica como leitura. | `orion-core/src/agenda.rs`, `orion-core/src/briefing.rs` | `orion-core/tests/` |
| **n8n: o destino é seu, não do modelo.** n8n: o destino é seu, não do modelo. O modelo só escolhe o nome do webhook, e acionar sempre confirma. | `orion-core/src/tools/n8n.rs` | `orion-core/tests/` |
| **pânico é corte; não perturbe só segura.** Pânico é corte, não pausa: para memória da tela, palavra de ativação e jobs de rede, e tira do modelo as ferramentas de rede, execução e destrutivas. Só sai por comando explícito. Não perturbe só segura avisos. | `orion-core/src/modos.rs`, `orion-server/src/rotas/` | `orion-core/tests/` |
| **modelo local é reserva sem ferramentas.** Modelo local é reserva sem ferramentas, só neste computador. Entra como último endpoint e nunca recebe `tools`; a inferência roda no worker `orion-llama.exe` via pipes. | `orion-ai/src/gateway.rs`, `orion-llama/` | `orion-llama/tests/` |
| **custo zero (sem API paga).** Custo zero: nenhum código chama API paga. A cota é a que o Orion contou; job opcional não roda com 90% da cota; Configurações avisa provedor de fora da lista gratuita. | `orion-ai/src/budget.rs`, `orion-core/src/jobs.rs` | `orion-ai/tests/` |
| **o Orion como servidor MCP só lê.** O Orion como servidor MCP só lê: expõe busca de memória, fatos e conversas, sem escrita, execução, rede ou caminho de arquivo. O token é mostrado uma vez, fica só em hash e é revogável. | `orion-mcp/src/server.rs` | `orion-mcp/tests/` |
| **recurso e prompt de MCP são dado, não comando.** Recurso e prompt de MCP são dado e comando seu. Recurso só se o servidor listou e o `mcp.json` permite. Prompt só você invoca, e nunca é enviado sozinho. | `orion-mcp/src/client.rs`, `orion-core/src/agent.rs` | `orion-mcp/tests/` |
| **MCP com muitas ferramentas mostra só um índice.** Servidor MCP com muitas ferramentas mostra ao modelo só um índice e `procurar_ferramenta`. O esquema entra depois, e classe de risco e aprovação não mudam. | `orion-mcp/src/client.rs`, `orion-core/src/agent.rs` | `orion-mcp/tests/` |

## Código e qualidade

| Princípio | Imposto em | Prova |
|---|---|---|
| **SQL parametrizado e exceções específicas.** SQL só parametrizado via `rusqlite`; identificadores vêm de constantes; erros são tipados com `thiserror`/`anyhow` sem silenciamento indevido. | `orion-store/src/` | `cargo test -p orion-store` |
| **CI verde em Rust/Windows para mergear.** Testes unitários e de integração verdes no CI Windows (`rust-windows.yml`). | `.github/workflows/rust-windows.yml` | o próprio workflow |

## Vida pessoal e visão (E10 e E11)

| Princípio | Imposto em | Prova |
|---|---|---|
| **finanças são locais: valor, conta e extrato nunca saem do computador.** Gastos, contas e histórico de preço ficam no banco local. Texto de gasto que a regra não entende vai só ao modelo local, nunca à nuvem. Nenhuma ferramenta do modelo expõe valores, extrato ou simulação. Fatura entra só com a sua confirmação, e o monitor de preço manda à pesquisa só o nome do produto. | `orion-core/src/vida.rs`, `orion-store/src/` | `orion-core/tests/` |
| **o escudo de foco pergunta; nunca bloqueia, fecha nem segura nada.** O bloco de foco faz uma pergunta discreta, no máximo uma vez por bloco, quando o app ou site em foco casa com lista de distrações. Não fecha janela, não mata processo e não mexe no não perturbe nem na memória da tela. | `orion-core/src/vida.rs` | `orion-core/tests/` |
| **o estado pela voz só muda o estilo da resposta.** Ritmo e energia da sua fala são medidos localmente e comparados com a sua média; fala apressada ou baixa dá resposta mais curta. As médias ficam só em memória, não viram fato nem entram em decisão, e o áudio não é guardado. | `orion-core/src/vida.rs` | `orion-core/tests/` |
| **a câmera só liga por pedido seu, e cada uso fica registrado.** Uma foto sai só quando você pede (`ver_camera`, `/camera`, a paleta ou o botão de câmera da voz), e `ver_camera` sempre pede aprovação. Cada uso vai ao registro de saídas e a foto é descartada logo depois. O modo contínuo guarda só rótulos de objetos, por 7 dias, nunca a imagem nem a classe "pessoa", e só roda com presença. A legenda escuta o áudio do sistema só com a opção ligada e não grava nem envia nada. | `orion-core/src/tools/camera.rs`, `orion-voice/src/` | `orion-core/tests/` |
| **rosto é local, cadastrado só pelo dono, e nunca sai do computador.** Os vetores faciais ficam cifrados no banco local, com a chave no cofre do sistema. Não existe ferramenta do modelo para rosto: só o comando `orion rosto` cadastra, compara e apaga. Rosto nunca vai a provedor, nem dentro de foto. A comparação só roda com a tela desbloqueada e a câmera ligada. | `orion-core/src/rosto.rs` | `orion-core/tests/` |
| **presença só pausa e retoma, cumprimenta, e bloqueia só com a sua confirmação; nunca envia nada para fora.** Os sinais são locais (tela, teclado e mouse, Bluetooth, rosto). Ausente, a memória da tela é pausada e a vigília para; ao voltar, retoma. O bloqueio da tela ao sair só existe com opt-in e confirmação explícita, e cada bloqueio vai à auditoria. | `orion-core/src/presenca.rs` | `orion-core/tests/` |

## Nuvem e modo autônomo

| Princípio | Imposto em | Prova |
|---|---|---|
| **tarefa na nuvem só por pedido seu, pelas CLIs oficiais, e nunca é ferramenta do modelo.** `claude --cloud` e `codex cloud exec` só rodam por clique ou `/nuvem` seu, com `ORION_NUVEM=true` (opt-in); sem shell, sem `ORION_*` no processo filho, shim `.cmd/.bat` recusado, serviço só `claude` ou `codex`. Trazer o resultado muda a pasta local: só por clique; o Codex falho nunca troca de serviço sozinho. | `orion-core/src/nuvem.rs`, `orion-server/src/rotas/automacao.rs` | `orion-core/tests/` |
| **ligar o modo autônomo pede a senha de novo.** `POST /modo/autonomo` com `ativo=true` exige `senha` e confere com a senha de login; desligar não pede. A política de aprovação não muda com o modo. | `orion-server/src/rotas/geral.rs`, `orion-app/src/views/overlays.rs` | `orion-server/tests/` |

## Decidido

- **Portabilidade** (antigo "Hardware-Bound Logic Gates", da Lyra): decidido em 07/10/2026, porque conflitava com trocar de máquina. Está no princípio de base 4.
- **Licença:** uso pessoal proprietário (`LicenseRef-Proprietario`, D07).

## Em aberto (sua decisão)

- Aprovação por voz ou por frase foi descartada. Se quiser um canal sem botão, ele precisa de autenticação própria: a palavra de ativação não basta.
- As regras das etapas E12 e E13 (trabalho e carreira, segurança e continuidade) serão escritas aqui antes do código delas, como o plano pede.
