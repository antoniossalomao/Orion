# ORION — Plano do Projeto

> Visão, princípios, arquitetura-alvo, inventário do legado, fases e
> decisões. Atualizado em 06/10/2026. A reescrita começou (`orion/`, ver
> status na §6); este documento continua sendo o planejamento. Como o código **atual** funciona:
> [ORION_TECNICO.md](ORION_TECNICO.md) · Operação do legado: [README.md](../README.md) ·
> Histórico completo do legado: histórico do git deste repositório. Plano de corte (venda do PC):
> [ORION_CORTE.md](ORION_CORTE.md) · Triagem das ferramentas: [ORION_FERRAMENTAS.md](ORION_FERRAMENTAS.md) ·
> Como ligar login, Tailscale, MCP e ferramentas: [ORION_OPERACAO.md](ORION_OPERACAO.md).

---

## 1. Quem e o quê

**Arquiteto:** Antônio — estudante de ADS na UNIMAR (Marília-SP), arquiteto único.
Prefere explicações diretas, sem enrolação.

**Orion:** assistente pessoal com identidade masculina, técnico, direto, não-servil.
Não é um chatbot: é uma extensão do dia a dia do Antônio — lembra do que importa,
age no computador dele e responde de qualquer lugar.

*"Não é uma IA. É uma extensão do sistema nervoso."*

## 2. Ring 0 — princípios invioláveis (revisados em 01/10/2026)

1. **Antônio é o Administrador Supremo.** O Protocolo de Sobrescrita anula
   qualquer raciocínio da máquina.
2. **Núcleo imutável.** Persona e Ring 0 ficam num arquivo que o próprio Orion não
   pode editar (o antigo `core.py`).
3. **Nuvem por padrão, custo zero.** Free tiers + as assinaturas que o Antônio já
   paga (Claude, Gemini, ChatGPT/Codex), estas só pelos programas oficiais. Modelo
   local pequeno só como último recurso. Memória pessoal pode ir para a nuvem como
   contexto. Captura contínua (tela, áudio ambiente, biometria) exige aprovação.
4. **Hardware-Bound Logic Gates** *(rever — o Orion vai trocar de máquina)*.
5. **Auto-modificação de código exige aprovação explícita.**
6. **Ação de alto risco exige confirmação explícita**, inclusive pelo celular.

**Regras de trabalho:** nada de download grande sem confirmação; código legível
sem contexto prévio (POO onde fizer sentido); cada fase termina apagando o que
substituiu.

> **Regras operacionais (02/10/2026):** o que o código impõe de cada princípio — e o teste que o
> prova — está em [ORION_REGRAS.md](ORION_REGRAS.md). Proposta para o #4 (Hardware-Bound) lá, aguardando seu OK.

## 3. Restrições

| Restrição | Consequência no desenho |
|---|---|
| **Notebook Lenovo IdeaPad Slim 3, i5, 8GB DDR5, Windows** — depois MacBook M2 16GB | Nada de servidores pesados rodando o tempo todo (Qdrant, SurrealDB, embedding local); sem Docker; tudo nativo |
| **Windows agora, macOS depois** | Código e ferramentas multiplataforma desde o primeiro commit; CI testando os dois |
| **Orçamento R$0** | Free tiers com fallback automático por cota; assinaturas só para tarefa pesada |
| **Acesso pelo celular** | Canal móvel (Telegram) + web acessível de fora de casa sem expor portas |
| **Um desenvolvedor** | Menos peças, mais coisa pronta: gateway, servidores MCP, banco embutido |
| **Notebook pode estar desligado** | Backend portátil, para migrar a uma máquina sempre ligada se precisar |

## 4. Arquitetura-alvo

### 4.1 Visão geral

```
Celular ── Telegram ─────────────┐
Celular ── web (Tailscale) ──────┤
Notebook ── web / casca desktop ─┤
                                 ▼
                     Orion (Python, FastAPI, 1 processo)
                     ├─ Agente: persona fixa + ferramentas + memória
                     ├─ Modelos ──► OmniRoute (local) ──► free tiers por chave de API
                     ├─ Tarefa pesada ──► CLIs oficiais: claude -p · codex exec · gemini -p
                     ├─ Ferramentas ──► servidores MCP (prontos + orion-desktop próprio)
                     │                   └─ política: leitura livre · escrita com log ·
                     │                      destrutiva só com confirmação
                     ├─ Memória ──► SQLite (FTS5 + vetores em tabela, numpy), um arquivo
                     │               └─ embeddings por API gratuita
                     └─ Jobs ──► lembretes · consolidação da memória · backup
```

### 4.2 Peças

| Peça | Escolha | Por quê |
|---|---|---|
| Linguagem/servidor | Python + FastAPI, pacote `orion/` com `pyproject` + uv | Ecossistema de IA; uv resolve Python e dependências igual no Windows e no Mac |
| Qualidade | ruff, pyright, pytest; CI no GitHub Actions em Windows e macOS | Garantia de que roda nas duas máquinas |
| Configuração/segredos | pydantic-settings + `.env`; chaves no cofre do SO (`keyring`) | Funciona no Credential Manager e no Keychain |
| Modelos (conversa) | **OmniRoute** rodando local, endpoint compatível com a API da OpenAI | Já rastreia cotas de free tiers e faz fallback; o Orion só fala um formato |
| Modelos (tarefa pesada) | Claude Code, Codex e Gemini CLI em modo sem interface | Usa as assinaturas pelo caminho oficial, sem token extraído |
| Último recurso | Nenhum por enquanto (sem modelo local) | Ollama saiu em 01/10/2026; reavaliar se faltar internet ou cota |
| Agente | PydanticAI | Ferramentas tipadas, MCP nativo, streaming, testável com modelo falso |
| Memória | SQLite + FTS5 (palavra-chave) + vetores em tabela comum com busca em numpy + fusão RRF | Arquivo único, zero servidor, multiplataforma, backup = copiar o arquivo |
| Embeddings | API gratuita (Gemini), com fila e retry | Sem GPU; se cair, a busca segue só por palavra-chave |
| Ferramentas | MCP: servidores prontos + `orion-desktop` próprio | Menos código; o mesmo servidor serve o Orion e o Claude Code |
| Celular | Bot do Telegram + web via Tailscale | Telegram funciona de qualquer lugar; Tailscale dá acesso à web sem expor portas |
| Jobs | Agendador dentro do processo | Lembretes, consolidação noturna, backup |

### 4.3 Caminho de uma mensagem

1. Chega por um canal (Telegram ou web) e passa pela autenticação.
2. O Orion monta o contexto: persona fixa + fatos sobre o Antônio + memória
   relevante (busca híbrida) + histórico da sessão.
3. O agente chama o modelo pelo OmniRoute com as ferramentas disponíveis.
4. Pedido de ferramenta passa pela política: leitura roda direto, escrita roda e
   fica no log, destrutiva pede confirmação no próprio canal (botão no Telegram).
5. Tarefa pesada vira a ferramenta `delegar(agente, tarefa, pasta)`, que roda a
   CLI oficial com tempo-limite, pasta de trabalho definida e contador de uso
   diário; se a cota de uma acabar, tenta a próxima.
6. A resposta volta em streaming; a conversa é gravada; a consolidação
   (fatos, resumos, deduplicação) roda depois, num job.

### 4.4 Memória (modelo de dados)

| Tabela | Conteúdo |
|---|---|
| `sessions`, `messages` | Conversas (episódico) |
| `facts` | Fatos e preferências do Antônio, com fonte e data; atualizados pela consolidação |
| `documents`, `chunks` (+ índice FTS5 e vetor) | Vault do Obsidian e arquivos enviados |
| `edges` | Relações (mensagem→tópico, fato→fonte) — substitui o grafo do SurrealDB |
| `reminders`, `prompts`, `users`, `audit`, `approvals` | Operação |

Sem datasets genéricos. Backup diário copiando o arquivo para a nuvem (iCloud ou
OneDrive). Herdado do legado: Goal Drift (objetivos em aberto no briefing) e
Response Provenance (de onde veio cada resposta).

### 4.5 Segurança

- Nada escuta fora de `127.0.0.1`; acesso de fora só pela rede privada do Tailscale.
- Login obrigatório em toda rota: senha em PBKDF2 e **sessão opaca** em cookie httpOnly
  (revogável; não é JWT, que não se revoga) — `orion/auth.py`; token de admin só para
  máquinas; bot do Telegram só responde ao ID do Antônio.
- OmniRoute com a senha padrão trocada e só provedores por chave de API (os
  marcados como risco ficam desligados).
- Política de ferramentas com rate limit, Câmara de Eco e audit log (portados do legado).

## 5. Inventário do legado

O que existe hoje e o destino de cada peça. "Fase" = quando é substituído e apagado.

| Legado | Destino | Fase |
|---|---|---|
| `cerebro_maestro.py`, `routers/`, `models/` | Reescrever como app FastAPI com lifespan e injeção de dependências; portar chat, sessões, prompts, logs, sistema | 1–2 |
| `config.py`, `logger.py`, `test_smoke.py`, `requirements.txt` | pydantic-settings + keyring, logging padrão, pytest, `pyproject` | 1 |
| `llm_cascade.py` (3 formatos de API) | Apagar — OmniRoute | 2 |
| Andar Claude CLI / `consultar_especialista` | Generalizar para Claude Code, Codex e Gemini CLI | 2 |
| Roteamento MoE por regex | Apagar | 2 |
| `orion_agentes.py` (enxame) | Apagar — o agente vira o núcleo; tarefa grande vai para as CLIs | 2 |
| `rag_engine.py`, `bm25_index.py`, `surreal_client.py`, `reconciliar_episodios.py`, `embed_service.py`, Qdrant, SurrealDB | Apagar — memória nova em SQLite | 3 |
| `session_manager.py` | Portar a lógica | 3 |
| `orion_shadow_thoughts.py` | Reescrever como consolidação da memória (job) | 3 — feito em `orion/memory/consolidate.py` (fatos a partir das falas do Antônio) |
| `proactive_loop.py` | Reescrever como agendador (lembretes, consolidação, backup); self-healing de bancos deixa de existir | 3–4 — feito em `orion/jobs.py` (lembretes, agendamentos, embeddings, vault, backup, consolidação); falta a vigilância de pastas |
| `tools/` (55 ferramentas) | Triagem: o que um servidor MCP pronto já faz sai; o resto vai para `orion-desktop` multiplataforma | 4 — feito: 32 portadas, 2 substituídas, 8 a descartar, 13 a portar (dependem de servidor MCP seu, modelo multimodal, API de imagem ou Windows). Ver [ORION_FERRAMENTAS.md](ORION_FERRAMENTAS.md) |
| `orion_seguranca.py` (rate limit, Câmara de Eco, audit) | Portar como política de ferramentas | 4 — feito em `orion/policy/` (audit também em banco, tabela `audit`) |
| `orion_browser.py`, `orion_google_workspace.py` | Servidores MCP (navegador, Google) | 4 — o cliente MCP existe (`orion/mcp_client.py`, `mcp.example.json`); falta **você** escolher e configurar os servidores |
| `orion_telegram.py` | Reescrever como canal principal do celular | 5 — feito em `orion/channels/telegram.py` (texto, **voz**, **foto**, botões de aprovar, avisos). O bot do legado **não** fala com o `/chat` novo (não envia o token) |
| Login (PBKDF2 + JWT, `utils/auth.py` no histórico do git) | Portar | 5 — feito em `orion/auth.py`: PBKDF2 + sessão opaca revogável (não JWT), tela de entrada no front |
| `bin/startup/*.bat`, `orion_boot.vbs` | Apagar — um comando só e autostart por SO | 5 — `orion autostart` gera o arquivo de cada SO (falta ligar no notebook); apagar na fase 7 |
| `orion_voice_live.py`, `audio_manager.py`, `mic_engine.py`, `commands.py` | Voz nova (masculina, palavra de ativação "Orion") | 6 |
| Frontend v1 (pywebview + Three.js), `dashboard.html` | Decidir na fase de interface | 6 |

**Já apagado (30/09–01/10):** IDE Theia, app desktop antigo, front-ends v2 (React) e
v3 (SvelteKit + Tauri), scripts de ingestão e
vetorização dos datasets, `ingest_webdocs.py`, `build_bm25_index.py`,
`validador_cortical.py`, `calibrar_pesos_rag.py`, `Modelfile`, `Modelfile_mini`,
`webcam.py`, launcher antigo, PDF e gerador do Gênesis, screenshots, mockups.
Em 01/10, o que estava sem cliente: rotas e routers `auth`, `gateway`, `tools`,
`logs`, `models_hub` e `prompts`, `utils/`, `orion_agent.py` (ReAct),
`tools/_registry.py`, `start_screenpipe.bat`, dependências sem import, e o que
nunca funcionava: keyring (pacote não instalado), auto-extensão
(`criar_ferramenta`, cujas ferramentas nunca eram executadas) e foto no Telegram
(chamava uma rota inexistente). Também em 01/10, tudo que dependia do Ollama:
andar local da cascata, draft do spec-decoding, watcher de VRAM, fallbacks
locais de tradução/visão/clipboard, especialista local e `start_ollama.bat`.
Tudo continua no histórico do git.

## 6. Fases

| Fase | Entrega | Pronto quando |
|---|---|---|
| **0 — Antes de vender o PC** *(urgente)* | Exportar do SurrealDB as tabelas pessoais (`evento`, `sessao`, `prompt`, `lembrete`, `agendamento`, `tarefa`, `numero`) para JSON; copiar `.env` e `Orion_Core/google_auth/`; conferir o vault do Obsidian no iCloud | `orion verify-export` termina em PRONTO (contagens, importação de ensaio, backup e restauração); detalhes em [ORION_CORTE.md](ORION_CORTE.md) |
| **1 — Fundação** | Pacote `orion/` (uv, ruff, pyright, pytest), configuração, app FastAPI com `/health`, CI em Windows e macOS | CI verde nos dois SOs; `uv run orion` sobe |
| **2 — Cérebro** | OmniRoute configurado; agente com persona; `/chat` em streaming; `delegar` nas 3 CLIs | Conversa segue funcionando com um provedor derrubado de propósito; delegação testada nas 3 CLIs |
| **3 — Memória** | SQLite + FTS5 + vetores (numpy); importação do export; indexação do vault; fatos e consolidação; backup | Perguntas reais sobre o Antônio (conjunto fixo, em pytest) recuperam a memória certa |
| **4 — Ferramentas** | Cliente MCP; servidores prontos + `orion-desktop`; política, confirmação, audit | Teste prova que nenhuma ação destrutiva roda sem confirmação |
| **5 — Canais** | Telegram (texto, voz, foto, botões), login, Tailscale, autostart | Usar o Orion pelo celular fora de casa |
| **6 — Interface, voz, identidade** | Frontend escolhido, voz masculina, palavra de ativação, visual do Orion | A definir na própria fase |
| **7 — Limpeza** | Apagar `Orion_Ollama/`, `Orion_Core/`, `bin/` | Nenhum arquivo do legado no repositório |

**Status em 06/10/2026** (detalhe e o que não foi verificado: [ORION_MELHORIAS.md](ORION_MELHORIAS.md)):
a fase 0 continua pendente (é sua), mas há `orion verify-export` e o [plano de corte](ORION_CORTE.md); a fase 1 está
pronta (CI verde em Linux, Windows e macOS); a fase 3 está pronta no código (memória SQLite com esquema v3, importador
completo, embeddings pela API gratuita, consolidação, agendador, backup diário; falta a chave de embeddings e as suas
perguntas reais); a fase 2 existe com gateway e CLIs falsos (falta OmniRoute e CLIs reais); a fase 4 está pronta no
código: política com audit em banco, ferramentas de memória, operação, `delegar`, `orion-desktop` completo (comando,
arquivos, documentos, área de transferência, notificação, abrir app, Git somente-leitura, saúde, processos em segundo
plano, vigilância de pastas), web (opt-in) e cliente MCP; faltam os servidores MCP que **você** escolher (e-mail, agenda,
navegador) e o que depende de modelo multimodal (tela, imagem); a fase 5 está pronta no código: login com senha e
tela de entrada, Telegram com texto, voz e foto, `orion autostart`, guia do Tailscale ([ORION_OPERACAO.md](ORION_OPERACAO.md));
falta instalar o Tailscale, ligar o bot e ativar o autostart no notebook; a fase 6 tem o front redesenhado (só desktop);
a fase 7 não começou.

## 7. Decisões em aberto

| # | Decisão | Recomendação | Alternativa |
|---|---|---|---|
| 1 | Banco da memória | SQLite + FTS5 + vetores em tabela comum (numpy). sqlite-vec foi descartado em 02/10 | SurrealDB (servidor ou embutido) |
| 2 | Gateway de modelos | OmniRoute | LiteLLM (biblioteca, sem processo extra, cotas na mão) |
| 3 | Framework do agente | PydanticAI | Loop próprio (~200 linhas) — **adotado em 02/10 (alternativa)**, ver registro; reversível |
| 4 | Ritmo de apagar o legado | Por fase (código antigo à mão para portar) | Tudo agora, consultando o histórico do git |
| 5 | Interface | **Decidido em 07/10/2026: front só desktop**, sem versão para celular (sem PWA, gaveta, toque nem manifest). Ver [ORION_FRONT.md](ORION_FRONT.md). O celular continua só pelo Telegram, que já existia | ~~Web no celular via Tailscale~~ — descartada |
| 6 | Voz | Transcrição: Whisper no Groq (grátis); fala: edge-tts `pt-BR-AntonioNeural` (grátis); voz ao vivo: Gemini Live com voz masculina; ativação: openWakeWord treinado em "Orion" | A discutir (fase 6) |
| 7 | Busca web gratuita | `pesquisar_com_ia` (Gemini com Google Search, mesma chave do Google AI Studio) e `pesquisar_internet` (Brave Search, chave do plano gratuito), mais um servidor MCP de busca em `mcp.json` se quiser; **nenhuma validada contra a API real** (cotas mudam) | — |
| 8 | Ring 0 §4 (Hardware-Bound) | Rever — conflita com trocar de máquina | — |

## 8. Registro de decisões

| Data | Decisão | Motivo |
|---|---|---|
| 07/10/2026 | Front **só desktop**: nada de PWA nem layout para celular (decisão #5). O celular segue pelo Telegram | Pedido do usuário: "sem celular, desktop only" |
| 25/06/2026 | Cascata cloud-first (Groq → Gemini → Claude → local) | Qualidade/latência (substituída em 01/10 pelo gateway) |
| 01/07/2026 | Decay Ebbinghaus removido do ranking | Esquecia memória antiga relevante. Não reimplementar sem discussão |
| 01/07/2026 | Axônios na esfera descartados | Pedido explícito |
| 01/07/2026 | Backup automático agendado: não implementar ainda | Pedido do usuário (revisto em 01/10: backup entra na fase 3) |
| 07-08/08/2026 | Nomes das ferramentas em PT | São o contrato de function-calling com o LLM |
| 11/08/2026 | Versão 2.0 do legado: auth, routers, SvelteKit, Tauri | Histórico em [TECNICO §9](ORION_TECNICO.md#9-versão-20-do-legado-plano-de-11082026-e-execução) |
| 30/09/2026 | **Nome: Orion**, identidade masculina; visual novo a definir | Mais vozes masculinas de qualidade |
| 30/09/2026 | Reescrita em repositório próprio (`Orion`, branch `main`); o repositório antigo fica como backup do legado | Reescrita livre |
| 30/09/2026 | Arquivos e pastas renomeados para Orion; textos, persona e identificadores de dados ficam para depois | Primeira etapa da troca de nome |
| 30/09/2026 | IDE Theia e app desktop antigo excluídos | Custo de manter sozinho |
| 30/09/2026 | Front-ends v2 (React) e v3 (SvelteKit + Tauri) excluídos; fica só o v1 | *(não registrado)* |
| 30/09/2026 | Datasets genéricos saem da memória | Os LLMs já sabem; diluíam a memória pessoal |
| 30/09/2026 | Nuvem primeiro; orçamento R$0; memória pessoal pode ir para a nuvem | PC será vendido |
| 01/10/2026 | Roda no notebook (IdeaPad Slim 3, 8GB) e depois MacBook M2 16GB; acesso pelo celular | Hardware disponível |
| 01/10/2026 | Assinaturas (Claude, Gemini, Codex) só pelos programas oficiais | Token extraído dá banimento; termos da Anthropic mudaram 3 vezes em 2026 |
| 01/10/2026 | Modelo local pequeno só como último recurso | Funcionar sem internet/cota |
| 01/10/2026 | Plano refeito do zero (este documento); nada codado antes da aprovação | Pedido do usuário |
| 01/10/2026 | Código, rotas e dependências do legado sem uso apagados (sem cliente desde a saída do v2/v3) | Pedido do usuário |
| 01/10/2026 | Front-end v1 refeito com identidade Orion: constelação de Órion no centro, paleta noturna, novo ícone; mesma stack e mesmos contratos | Identidade própria (pedido do usuário); a interface definitiva continua na fase 6 |
| 01/10/2026 | Persona do backend vira Orion (masculina, direta); palavra de ativação "orion"; voz Charon (Gemini TTS e voz ao vivo) e edge-tts AntonioNeural; novas falas gravadas com ator "Orion" (nomes antigos configuráveis em `ATORES_LEGADOS`) | Identidade própria (pedido do usuário) |
| 01/10/2026 | Nome antigo removido do repositório: namespace SurrealDB e coleção Qdrant vêm do `.env` (`SURREAL_NS`, `QDRANT_COLLECTION`; defaults `orion_core`/`orion_memory`), arquivos de runtime renomeados | Identidade só Orion (pedido do usuário); dados existentes seguem acessíveis pelo `.env` |
| 01/10/2026 | Ollama fora por enquanto: cascata Groq → Gemini → Claude, sem draft nem watcher de VRAM; tradução, visão e clipboard só pelo Gemini; especialista só Claude | Pedido do usuário; substitui "modelo local só como último recurso" |
| 02/10/2026 | Auditoria do projeto e execução do plano: política de ferramentas por allowlist e aprovação fora de banda; pacote `orion/` (fundação, memória SQLite, gateway, agente, `/chat`); correções no legado (Telegram, SSRF, WebSocket/upload, imagem externa no chat) | Pedido do usuário; detalhes em ORION_MELHORIAS.md |
| 02/10/2026 | Vetores em tabela comum + numpy no lugar do sqlite-vec | O CI no macOS mostrou que o Python do `uv` vem sem `enable_load_extension`: a extensão não carregaria no MacBook. Para memória pessoal a força bruta responde em ms (teste com 30 mil trechos) |
| 02/10/2026 | Loop de agente próprio em vez de PydanticAI (decisão #3, alternativa) | Fluxo de aprovação precisa controlar quando cada ferramenta roda; testável com gateway falso; reversível |
| 02/10/2026 | `executar_comando` sempre pede confirmação, salvo leitura provada; o legado importa `orion.policy` (transitório até a fase 7) | Uma política só; fecha o bypass da blocklist da Câmara de Eco |
| 03/10/2026 | Front redesenhado ("Observatório noturno"), só desktop: sem gaveta, sem toque, sem manifest; celular = Telegram | Janela estreita quebrada, acessibilidade e zero teste no front antigo (ORION_FRONT.md). **Conflita com o diagrama §4.1 ("Celular ── web")**: ver decisão #5 |
| 03/10/2026 | Esquema da memória v2 (lembretes, agendamentos, tarefas, números, prompts, arestas, fila de avisos) com migração automática; importador traz todas as tabelas do `backup_memoria` | A fase 0 deixava 5 tabelas e o grafo sem destino; o `backup_memoria` já exporta tudo |
| 03/10/2026 | Agendador dentro do processo (`orion/jobs.py`); **agendamento só avisa, não executa a ferramenta** | Executar ação sem ninguém olhando exige política e aprovação (fase 4); regra 18 |
| 03/10/2026 | Embeddings pela API do Gemini (`gemini-embedding-001`, 768 dimensões, tarefa de documento × consulta); sem chave a busca é só por palavra-chave | Decisão #1 do plano; custo zero. **Não validado contra a API real** |
| 03/10/2026 | Consolidação da memória em fatos a partir das falas do Antônio, a cada 6 h, ignorando o canal `legado` | Substitui o ciclo de sono; só fala do usuário para não virar canal de injeção; regra 19 |
| 03/10/2026 | Host fora de `127.0.0.1` em `ORION_ALLOWED_HOSTS` exige `ORION_ADMIN_TOKEN` | "Tailscale só depois do login": a configuração recusa subir sem token; regra 17 |
| 03/10/2026 | `httpx` passa a dependência de execução (estava só no grupo `dev`) | O gateway e os embeddings o importam em tempo de execução |
| 03/10/2026 | Canal Telegram novo (`orion/channels/telegram.py`): long polling, default-deny por ID e conversa privada, aprovação só por botão, avisos da fila; sobe só com `ORION_TELEGRAM_TOKEN` + `ORION_TELEGRAM_ALLOWED_USERS` + gateway | Fase 5. O bot do legado não envia o token do `/chat` novo (401) e foi feito para o legado; regra 21. **Não validado contra o Telegram real** |
| 03/10/2026 | `orion-desktop` v0 (`executar_comando`, `ler_arquivo`, `listar_arquivos`), **desligado por padrão** (`ORION_DESKTOP_TOOLS=true`); ler segredo ou chave (`.env`, `.ssh`, `*.pem`...) passa a pedir aprovação | O Orion novo não tinha como agir no computador; tudo atrás da política; regras 6 e 22 |
| 03/10/2026 | `orion autostart` gera o arquivo de início automático (Startup no Windows, LaunchAgent no macOS, unit de usuário no Linux) e imprime o comando de ativação, sem executá-lo | Fase 5 "autostart por SO"; não mexer no sistema sem o Antônio |
| 03/10/2026 | Aprovação só do que se vê inteiro: argumento cortado ou grande demais só pode ser negado (web e Telegram) | O cartão cortava o argumento (700 caracteres na web, 2000 no servidor) e escondia o fim do comando; regra 23 |
| 03/10/2026 | Logger do `httpx` fixado em WARNING | A URL da API do Telegram carrega o token do bot; o `httpx` a registra em INFO |
| 06/10/2026 | Login com senha (PBKDF2-SHA256, 600 mil iterações) e sessão **opaca** em cookie `httpOnly`/`SameSite=Strict`, em `auth.db` fora do banco da memória; token de admin vira credencial de máquina; `/health` sem login só `status`+`version`; `/docs` e `/openapi.json` desligados | O token único no `localStorage` não tinha sessão nem saída; JWT sem estado não se revoga; o backup vai para a nuvem e não deve levar o hash. Regras 13, 17 e 25 |
| 06/10/2026 | Host de fora no `ORION_ALLOWED_HOSTS` sobe com senha **ou** token (antes: só token), checado na subida do app | A regra 17 é "só com login", e agora há login de verdade |
| 06/10/2026 | Trilha de auditoria na tabela `audit` (esquema v3, poda de 90 dias); falha de gravação nega o que não é leitura | Audit só em log não dava para consultar nem sobrevivia; regra 8 |
| 06/10/2026 | Cliente MCP com o SDK oficial; classe de risco vem do `mcp.json`, nunca do servidor; padrão `exec`; `default_risk: read` recusado; servidor sem `ORION_*` no ambiente | Decisão de 01/10 ("servidores MCP prontos + orion-desktop próprio") com a política mandando; regra 24 |
| 06/10/2026 | `orion-desktop` completo e ferramentas de web como opt-ins separados (`ORION_DESKTOP_TOOLS`, `ORION_WEB_TOOLS`); `abrir_app` passa a execução | A web tem canal de exfiltração por GET que o taint não cobre; abrir app abre qualquer programa; regra 26 |
| 06/10/2026 | `orion/netguard.py`: IP conferido = IP usado, no lugar do `url_guard` do legado | Fecha a janela de DNS rebinding que o legado documentava; regra 7 |
| 06/10/2026 | Voz no Telegram por Whisper no Groq (`ORION_TRANSCRIBE_API_KEY`), texto entendido mostrado antes da resposta; foto como imagem só no turno | Fase 5 "Telegram (texto, voz, foto, botões)"; aprovação continua só por botão; regra 27. **Não validado contra as APIs reais** |
| 06/10/2026 | `pesquisar_com_ia` (Gemini com Google Search) resolve a decisão #7 por ora; `pesquisar_internet` não foi portada | Não há API de busca gratuita estável; MCP de busca fica por sua escolha |

## 9. Visão de longo prazo (conceitual)

Emotion Engine (voz paralinguística) · Wi-Fi Sensing (ESP32) · Protocolo Darwin
(auto-evolução supervisionada) · BCI/Protocolo Narciso · Sistema de Arquivos
Líquido · Total Recall (OCR contínuo de tela) · Telepatia de Clipboard · DNA de
Projeto · OPSEC avançado · Soberania acadêmica (mentor UML/Java, resumo de PDFs de
aula) · Sensor Fusion · Biometria passiva · Ghost OS Level 2.
