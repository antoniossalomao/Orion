# Validação por ambiente — C46

Referências: [[ORION_EXTENSOES]], [[ORION_FRONT]], [[ORION_MCP]], [[ORION_PLUGINS]].

| Ambiente/integração | Evidência | Estado |
| --- | --- | --- |
| Linux, Python 3.12, backend local | C44: 642 testes, C45: 18 Git/delegação | Verificado com fixtures |
| Chromium 151, desktop/tela 700 px | Upload e cópias, revisão/rejeição/reload/axe; suites anteriores por etapa | Verificado nos fluxos registrados |
| Contexto, concessões e recursos | Canários entre projetos, revogação, orçamento de skills, SDK e processos nos testes C11–45 | Verificado com fixtures |
| Windows e pywebview | Ambiente atual não oferece desktop Windows nem ponte real | Pendente |
| Google Calendar com conta real | Discovery do pacote publicado e contratos simulados; escopo Google amplo no upstream | Pendente; não declarar mínima permissão real |
| OAuth/cofre reais do usuário | PKCE/refresh/revogação simulados, sem login pessoal | Pendente |
| CLIs pessoais Claude/Codex/Gemini | Comando oficial, confirmação, timeout/erro com scripts de ensaio | Assinaturas reais pendentes |

A bateria completa do front está em execução nesta rodada. Um resultado simulado
não comprova conta, cliente desktop ou comportamento de Windows. Não liberar scripts
locais de Windows com base nos testes Linux. Não executar convites, login ou ações
reais apenas para preencher checklist. O avanço da exportação local de leitura é
independente dessas provas externas e continua sujeito aos próprios testes.
