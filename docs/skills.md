# Skills de auditoria (F00.13)

Duas coleções, com os mesmos 21 textos:

- **Orion (produto):** extensão embutida `orion-auditoria` (`Orion_Rust/crates/orion-core/bundled/auditoria/`, id `orion-auditoria`). Fica na lista de perfis do Orion e só é instalada por ação administrativa, como as outras (`orion-pesquisa`, `orion-agenda` etc.). Skill é texto: só nome e descrição entram no prompt e o corpo vem sob demanda; nada executa. Ferramentas liberadas: `ler_arquivo`, `listar_arquivos`, `consultar_git_projeto`, `buscar_memoria` e, nas skills que pedem execução, `delegar` (com aprovação).
- **Claude Code (repositório):** `.claude/skills/<nome>/SKILL.md`, usadas por quem trabalha neste repositório com o Claude Code.

Além delas, `auditoria-seguranca` (OWASP LLM 2025 e Agentic ASI01–ASI10) e `auditoria-interface` (design e acessibilidade) existem só no Claude Code.

## As 21 skills
| Nome | O que faz | Origem do tema |
|---|---|---|
| `revisar-rust` | Revisar segurança de código Rust: unsafe, memória, concorrência, FFI e erros que viram falha. | tema de `rust-review` |
| `auditar-zeroize` | Achar segredos que não são apagados da memória (senhas, chaves, tokens) em Rust ou C/C++. | tema de `zeroize-audit` |
| `revisar-diferenca` | Revisar uma alteração (diff, PR ou commit) por risco de segurança e regressão, usando o histórico. | tema de `differential-review` |
| `checar-falso-positivo` | Verificar achados de segurança e descartar falsos positivos com evidência. | tema de `fp-check` |
| `achar-padroes-inseguros` | Achar padrões que 'falham aberto': padrões padrão inseguros, segredos de exemplo e verificações que passam quando deveriam bloquear. | tema de `insecure-defaults` |
| `achar-apis-perigosas` | Achar APIs e configurações fáceis de usar errado (armadilhas de design) e propor interface mais segura. | tema de `sharp-edges` |
| `conferir-especificacao` | Conferir o código contra o documento que o especifica e listar divergências. | tema de `spec-to-code-compliance` |
| `entender-codigo-antes-de-auditar` | Construir entendimento do código função por função antes de procurar falhas. | tema de `audit-context-building` |
| `buscar-variantes` | A partir de uma falha conhecida, procurar outras ocorrências do mesmo padrão no código. | tema de `variant-analysis` |
| `cacar-regressao` | Achar em qual commit ou PR um comportamento quebrou e validar a causa. | tema de `regression-hunt` |
| `corrigir-bug-com-patch-pequeno` | Reproduzir um bug conhecido e propor o menor patch que o corrige, com validação. | tema de `patching-bugs` |
| `validar-patch` | Depois de um patch, procurar variantes não cobertas e regressões que ele possa ter causado. | tema de `post-patch-validation` |
| `analise-estatica` | Planejar e interpretar análise estática (Semgrep, CodeQL, SARIF, cargo audit/deny) sobre o código. | tema de `static-analysis` |
| `criar-regra-semgrep` | Escrever uma regra Semgrep para um padrão de vulnerabilidade específico, com exemplos de acerto e erro. | tema de `semgrep-rule-creator` |
| `portar-regra-semgrep` | Portar uma regra Semgrep existente para outra linguagem, adaptando sintaxe e casos de teste. | tema de `semgrep-rule-variant-creator` |
| `auditar-github-actions` | Auditar workflows do GitHub Actions por falhas de segurança, inclusive as ligadas a agentes de IA. | tema de `agentic-actions-auditor` |
| `segunda-opiniao` | Pedir uma segunda revisão independente de uma alteração a outra ferramenta (Codex ou Claude) e confrontar os resultados. | tema de `second-opinion` |
| `passo-a-passo-da-mudanca` | Explicar uma alteração de branch em ordem de leitura, do porquê ao detalhe. | tema de `review-walkthrough` |
| `melhorar-codigo` | Revisar um trecho, skill ou branch e propor melhorias pequenas e seguras, uma de cada vez. | tema de `code-improver` |
| `triar-issues-e-prs` | Triar issues e PRs abertos: classificar, priorizar e propor próximo passo (sem fechar nem publicar nada). | tema de `github-triage` |
| `escrever-goal` | Escrever um objetivo (`/goal`) verificável para o Claude Code ou o Codex, com critérios de pronto testáveis. | tema de `goal-prompt` |

## Origem e licença
Os temas vêm da lista de plugins do repositório [Trail of Bits skills](https://github.com/trailofbits/skills) (CC-BY-SA-4.0), conforme a escolha do Antônio em 21/10/2026. **Os textos são originais**, escritos para o Orion: só as descrições de uma linha da Trail of Bits foram consultadas, nenhum `SKILL.md` delas foi lido nem copiado, então o conteúdo daqui não é obra derivada. Licença dos textos: `LicenseRef-Orion-Proprietary`. Quem quiser as skills originais pode instalá-las como plugin externo do marketplace da Trail of Bits.

## Limites
- No Orion, as skills guiam a leitura e a análise; ferramentas externas (Semgrep, CodeQL, `cargo audit`) só rodam por delegação aprovada ou pela própria pessoa.
- Não foi possível abrir o Orion e instalar o perfil nesta sessão (Linux, sem janela); o pacote compila (`cargo check -p orion-core`), mas a leitura e a instalação não foram exercitadas.
