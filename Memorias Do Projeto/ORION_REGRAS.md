# ORION — Regras (Ring 0, revisão 2)

> Regras que valem para o Orion inteiro. Cada uma diz **onde o código a impõe** e
> **qual teste a prova**: regra sem teste é desejo. Complementa os princípios do
> [ORION_NUCLEO.md §2](ORION_NUCLEO.md). Criado em 02/10/2026.
>
> **Núcleo imutável:** este arquivo, `orion/persona.py` e `orion/policy/` ficam na raiz do
> projeto, que o `PathGuard` protege. Nenhuma ferramenta escreve aqui sem confirmação do
> Antônio, que chega por um canal autenticado (regra 3).

## Ferramentas

| # | Regra | Onde é imposta | Prova |
|---|---|---|---|
| 1 | Toda ferramenta tem **classe de risco fixa**: leitura roda direto; escrita roda e fica no audit; execução e destrutiva pedem confirmação. A classe vem do registro, não de regex no texto da chamada. Ferramenta sem classe é negada. | `orion/policy/classes.py`, `engine.py` | `tests/policy/test_engine.py`, `tests/test_tools.py` |
| 2 | **Confirmação fora de banda.** O pedido de aprovação sai por um canal autenticado (botão do Telegram, tela web com login); nunca por uma frase na conversa. Vale para (sessão, ferramenta, argumentos exatos), por 10 min, uso único. O modelo não tem como se auto-aprovar. | `orion/policy/approvals.py`, `orion/app.py` (`/approvals`) | `tests/policy/test_approvals.py`, `tests/test_app_chat.py` (fluxo ponta a ponta) |
| 3 | **Código, persona e política são imutáveis para as ferramentas.** Escrever na raiz do projeto, em diretório de sistema, em extensão/arquivo sensível (`.ps1`, `.env`, `.ssh`, autostart) ou fora de Documents/Downloads/Desktop pede confirmação. Auto-modificação só por diff aprovado. | `orion/policy/paths.py` | `tests/policy/test_paths.py` |
| 4 | **Conteúdo externo é dado, não instrução** (web, e-mail, documento, clipboard). Chega marcado; depois de lido, escrita e execução da sessão passam a pedir confirmação (vale também depois de reiniciar). | `orion/agent.py`, `engine.py` (`note_result`) | `tests/test_agent.py` (taint) |
| 5 | **Segredos nunca em log/audit/processo filho.** Chaves ficam no cofre do SO ou no `.env`; o audit e o log mascaram chaves e tokens; a CLI delegada não herda `ORION_*`; ler `.env`/credenciais por shell não é "leitura segura". | `orion/policy/audit.py` (`redact`), `orion/log.py`, `orion/delegate.py`, `orion/secrets.py`, `orion/memory/embedders.py` (chave da API em cabeçalho, nunca na URL) | `tests/policy/test_engine.py`, `tests/test_app.py`, `tests/test_delegate.py`, `tests/memory/test_embedders.py` |
| 6 | **Shell só roda sem confirmação o que for provadamente leitura.** Lista positiva de comandos (PowerShell e POSIX), sem redirecionamento, substituição, bloco de script, método ou `::`. O resto confirma. | `orion/policy/shell.py` | `tests/policy/test_shell.py` (inclui os bypasses da Câmara de Eco antiga) |
| 7 | **URL vinda do modelo só vai a host público** (http/https), com cada redirecionamento revalidado. Bloqueia loopback, LAN, Tailscale e metadados de nuvem. | `Orion_Ollama/url_guard.py` (legado) | `tests/legacy/test_url_guard.py` |
| 8 | **Falha de auditoria nega a ação** que não é leitura (fail-closed); erro de gravação nunca some em silêncio. | `orion/policy/engine.py` | `tests/policy/test_engine.py` |

## Rede e interface

| # | Regra | Onde é imposta | Prova |
|---|---|---|---|
| 9 | **Só `127.0.0.1`.** Bind público é recusado, salvo `ORION_ALLOW_PUBLIC_BIND=true`. Acesso de fora só pelo Tailscale. `Host` fora da lista é recusado (DNS rebinding). | `orion/config.py`, `orion/app.py` | `tests/test_app.py` |
| 10 | **WebSocket não passa por CORS**, então confere `Origin`: só navegador local (pywebview) ou cliente sem `Origin`. | `Orion_Ollama/origem.py`, `routers/misc.py`, hub em `orion_app.py` | `tests/legacy/test_misc_router_legado.py` |
| 11 | **Upload tem limite de tamanho** (25 MB) e nome sanitizado. | `Orion_Ollama/routers/misc.py` | `tests/legacy/test_misc_router_legado.py` |
| 12 | **O chat só renderiza imagem gerada pelo próprio Orion** (`/imagens/<arquivo>`). Imagem de outro host é um canal de exfiltração por prompt injection. | `Orion_Core/Front_end_Orion/js/md.js` | `tests/front/md.test.js`, `tests/front_e2e/test_front.py` (no navegador) |
| 13 | Endpoints que mudam estado exigem `Authorization: Bearer` (token de 16+ caracteres) até o login da fase 5. Sem CORS, o navegador de outra origem não consegue chamá-los. | `orion/app.py` | `tests/test_app.py` |

## Operação (jobs, memória)

| # | Regra | Onde é imposta | Prova |
|---|---|---|---|
| 17 | **Acesso de fora só com login.** Host que não seja local em `ORION_ALLOWED_HOSTS` (Tailscale, curinga `*`) exige `ORION_ADMIN_TOKEN`; sem o token a configuração se recusa a subir. | `orion/config.py` | `tests/test_app.py::test_host_de_fora_exige_token_de_admin` |
| 18 | **Agendamento avisa; não executa ferramenta sozinho.** A ferramenta informada fica registrada, e o disparo vira um aviso na fila. Rodar ação sem ninguém olhando exige política e aprovação (fase 4). | `orion/jobs.py` | `tests/test_jobs.py::test_agendamento_avisa_recalcula_e_nao_executa_a_ferramenta` |
| 19 | **Fato consolidado só vem de fala do Antônio**, nunca de resposta do modelo nem de resultado de ferramenta; tem fonte e data, é editável, segredo é descartado, e o histórico importado (canal `legado`) fica de fora. A marca de progresso só avança se o modelo respondeu e a resposta foi lida. | `orion/memory/consolidate.py` | `tests/memory/test_consolidate.py` |
| 20 | **Pendência velha não vira rajada de aviso.** Na importação, lembrete vencido há mais de 24 h entra já avisado e agendamento único atrasado entra desativado; recorrente atrasado recalcula o próximo horário. | `orion/memory/importer.py` | `tests/memory/test_importer.py::test_importa_operacao_com_regras_de_seguranca_dos_avisos` |

| 21 | **Telegram: default-deny e aprovação só por botão.** Só responde a ID da lista, em conversa privada; o resto é ignorado em silêncio. Só o clique de usuário permitido num cartão de aprovação do próprio canal decide (frase nunca). O token vai na URL da API: nenhum erro ou log leva a URL, e o logger do `httpx` fica em WARNING. Sem token + lista + gateway, o canal não sobe. | `orion/channels/telegram.py`, `orion/config.py`, `orion/log.py` | `tests/test_telegram.py` |
| 22 | **Ler segredo ou chave pede confirmação.** `.env`, `.ssh`, `.aws`, `*.pem`, `google_auth/`, `id_rsa`... em `ler_arquivo`/`listar_arquivos` viram pedido de aprovação (o caminho é resolvido antes: symlink para `.env` continua `.env`). O shell só roda direto o que for leitura provada (regra 6) e o filho não herda `ORION_*` nem variável com nome de segredo. | `orion/policy/paths.py` (`check_read`), `engine.py`, `orion/tools/desktop.py` | `tests/policy/test_paths.py`, `tests/policy/test_engine.py`, `tests/test_desktop.py` |

## Código

| # | Regra | Onde é imposta |
|---|---|---|
| 14 | SQL só parametrizado; identificadores vêm de constantes. `except` específico; nada de `pass` silencioso (as exceções têm comentário). | ruff (`S`, `BLE`) no CI |
| 15 | Ruff, pyright e pytest verdes em Linux, Windows e macOS para mergear. | `.github/workflows/ci.yml` |
| 16 | **A persona não carrega fato que envelhece**: hardware, versão de banco, porta, provedor. Isso vem de configuração. Persona é versionada e datada. | `orion/persona.py` + `tests/test_persona.py` |

## Em aberto (sua decisão)

- **Ring 0 #4 (Hardware-Bound Logic Gates)** conflita com trocar de máquina. Proposta: trocar por
  **"Portabilidade"** — nada depende de hardware específico; hardware e stack vêm de configuração
  (já é a regra 16). Aguarda seu OK; o NUCLEO §2 não foi alterado.
- **Aprovação por voz/por frase** foi descartada pela regra 2. Se quiser um canal sem botão
  (voz, por exemplo), ele precisa de autenticação própria (palavra de ativação não basta).
