# Validação por ambiente — C46

Referências: [[ORION_EXTENSOES]], [[ORION_FRONT]], [[ORION_MCP]], [[ORION_PLUGINS]].

| Ambiente/integração | Evidência | Estado |
| --- | --- | --- |
| Linux, Python 3.12, backend local | Suite final: 653 testes; Node SDK incluído | Verificado com fixtures |
| Chromium 151, desktop/tela 700 px | Upload e cópias, revisão/rejeição/reload/axe; suites anteriores por etapa | Verificado nos fluxos registrados |
| Contexto, concessões e recursos | Canários entre projetos, revogação, orçamento de skills, SDK e processos nos testes C11–45 | Verificado com fixtures |
| Windows e pywebview | Ambiente atual não oferece desktop Windows nem ponte real | Pendente |
| Google Calendar com conta real | Discovery do pacote publicado e contratos simulados; escopo Google amplo no upstream | Pendente; não declarar mínima permissão real |
| OAuth/cofre reais do usuário | PKCE/refresh/revogação simulados, sem login pessoal | Pendente |
| CLIs pessoais Claude/Codex/Gemini | Comando oficial, confirmação, timeout/erro com scripts de ensaio | Assinaturas reais pendentes |

A bateria completa final do front passou: 138 testes em 471,59 s. Um resultado simulado
não comprova conta, cliente desktop ou comportamento de Windows. Não liberar scripts
locais de Windows com base nos testes Linux. Não executar convites, login ou ações
reais apenas para preencher checklist. O avanço da exportação local de leitura é
independente dessas provas externas e continua sujeito aos próprios testes.

## Bateria do front e correções C46a

A suite completa executou 131 casos em 479,50 s: 128 passaram, 3 falharam e 8
teardowns relataram erro. Corrigidos: capacidades novas anunciadas indevidamente no
backend legado (404), Escape do grafo não delegado pela tela de fatos e rota Fontes
faltando na matriz de navegação. A busca da conversa não reproduziu a falha na
reexecução, sem mudança de implementação. Os 10 casos afetados passaram em 31,18 s.
Isso registra uma suite completa com falhas corrigidas e regressões direcionadas;
não declara uma nova suite completa toda verde nem Windows/contas reais validados.

## Fechamento da rodada

Suite completa final: **138 passou em 471,59 s**, Chromium 151, sem falha/erro.
Backend: 653 passou em 39,30 s, ensaio Node SDK incluído; JavaScript 91 passou.
Ruff/format/Pyright passaram. Correção adicional de flags opcionais de leitura
(`9e23dfa`): 3 testes de arquivos passou em 1,33 s. Windows e contas reais pendentes.
