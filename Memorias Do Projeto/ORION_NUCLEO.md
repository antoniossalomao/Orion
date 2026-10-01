# ORION — Plano do Projeto

> Antiga Lyra. Visão, princípios, arquitetura-alvo, inventário do legado, fases e
> decisões. Atualizado em 01/10/2026. Nada da reescrita foi codado ainda: este
> documento é o planejamento. Como o código **atual** funciona:
> [ORION_TECNICO.md](ORION_TECNICO.md) · Operação do legado: [README.md](../README.md) ·
> Histórico completo do legado: repositório [Lyra](https://github.com/antoniossalomao/Lyra).

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
                     │               └─► último recurso: modelo pequeno local (Ollama)
                     ├─ Tarefa pesada ──► CLIs oficiais: claude -p · codex exec · gemini -p
                     ├─ Ferramentas ──► servidores MCP (prontos + orion-desktop próprio)
                     │                   └─ política: leitura livre · escrita com log ·
                     │                      destrutiva só com confirmação
                     ├─ Memória ──► SQLite (FTS5 + sqlite-vec), um arquivo
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
| Último recurso | Ollama com modelo de 1–4B (8GB) / até 8B (M2 16GB), desligado por padrão | Funciona sem internet ou sem cota |
| Agente | PydanticAI | Ferramentas tipadas, MCP nativo, streaming, testável com modelo falso |
| Memória | SQLite + FTS5 (palavra-chave) + sqlite-vec (vetor) + fusão RRF | Arquivo único, zero servidor, multiplataforma, backup = copiar o arquivo |
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
- Login obrigatório em toda rota (herdado do legado: PBKDF2 + JWT em cookie
  httpOnly); bot do Telegram só responde ao ID do Antônio.
- OmniRoute com a senha padrão trocada e só provedores por chave de API (os
  marcados como risco ficam desligados).
- Política de ferramentas com rate limit, Câmara de Eco e audit log (portados do legado).

## 5. Inventário do legado

O que existe hoje e o destino de cada peça. "Fase" = quando é substituído e apagado.

| Legado | Destino | Fase |
|---|---|---|
| `cerebro_maestro.py`, `routers/`, `models/` | Reescrever como app FastAPI com lifespan e injeção de dependências; portar chat, sessões, prompts, logs, sistema | 1–2 |
| `config.py`, `logger.py`, `utils/secrets.py`, `test_smoke.py`, `requirements.txt` | pydantic-settings + keyring, logging padrão, pytest, `pyproject` | 1 |
| `routers/gateway.py` (WS de leitura) | Apagar | 1 |
| `llm_cascade.py` (4 formatos de API) | Apagar — OmniRoute | 2 |
| Andar Claude CLI / `consultar_especialista` | Generalizar para Claude Code, Codex e Gemini CLI | 2 |
| Draft do spec-decoding, roteamento MoE por regex | Apagar | 2 |
| `routers/models_hub.py` | Apagar | 2 |
| `orion_agent.py` (ReAct), `orion_agentes.py` (enxame) | Apagar — o agente vira o núcleo; tarefa grande vai para as CLIs | 2 |
| `rag_engine.py`, `bm25_index.py`, `surreal_client.py`, `reconciliar_episodios.py`, `embed_service.py`, Qdrant, SurrealDB | Apagar — memória nova em SQLite | 3 |
| `session_manager.py` | Portar a lógica | 3 |
| `orion_shadow_thoughts.py` | Reescrever como consolidação da memória (job) | 3 |
| `proactive_loop.py` | Reescrever como agendador (lembretes, consolidação, backup); self-healing de bancos deixa de existir | 3–4 |
| `tools/` (58 ferramentas) | Triagem: o que um servidor MCP pronto já faz sai; o resto vai para `orion-desktop` multiplataforma | 4 |
| `orion_seguranca.py` (rate limit, Câmara de Eco, audit, keyring) | Portar como política de ferramentas | 4 |
| `orion_browser.py`, `orion_google_workspace.py` | Servidores MCP (navegador, Google) | 4 |
| `orion_telegram.py` | Reescrever como canal principal do celular | 5 |
| `routers/auth.py`, `utils/auth.py` | Portar | 5 |
| `bin/startup/*.bat`, `orion_boot.vbs` | Apagar — um comando só e autostart por SO | 5 |
| `orion_voice_live.py`, `audio_manager.py`, `mic_engine.py`, `commands.py` | Voz nova (masculina, palavra de ativação "Orion") | 6 |
| Frontend v1 (pywebview + Three.js), `dashboard.html` | Decidir na fase de interface | 6 |

**Já apagado (30/09–01/10):** IDE Theia, Lyra Desktop, front-ends v2 (React) e
v3 (SvelteKit + Tauri), scripts de ingestão e
vetorização dos datasets, `ingest_webdocs.py`, `build_bm25_index.py`,
`validador_cortical.py`, `calibrar_pesos_rag.py`, `Modelfile`, `Modelfile_mini`,
`webcam.py`, `lyra_launcher.py`, PDF e gerador do Gênesis, screenshots, mockups.
Tudo continua no repositório Lyra.

## 6. Fases

| Fase | Entrega | Pronto quando |
|---|---|---|
| **0 — Antes de vender o PC** *(urgente)* | Exportar do SurrealDB as tabelas pessoais (`evento`, `sessao`, `prompt`, `lembrete`, `agendamento`, `tarefa`, `numero`) para JSON; copiar `.env` e `Orion_Core/google_auth/`; conferir o vault do Obsidian no iCloud | Arquivos de export abrem e têm as contagens esperadas |
| **1 — Fundação** | Pacote `orion/` (uv, ruff, pyright, pytest), configuração, app FastAPI com `/health`, CI em Windows e macOS | CI verde nos dois SOs; `uv run orion` sobe |
| **2 — Cérebro** | OmniRoute configurado; agente com persona; `/chat` em streaming; `delegar` nas 3 CLIs; modelo local opcional | Conversa segue funcionando com um provedor derrubado de propósito; delegação testada nas 3 CLIs |
| **3 — Memória** | SQLite + FTS5 + sqlite-vec; importação do export; indexação do vault; fatos e consolidação; backup | Perguntas reais sobre o Antônio (conjunto fixo, em pytest) recuperam a memória certa |
| **4 — Ferramentas** | Cliente MCP; servidores prontos + `orion-desktop`; política, confirmação, audit | Teste prova que nenhuma ação destrutiva roda sem confirmação |
| **5 — Canais** | Telegram (texto, voz, foto, botões), login, Tailscale, autostart | Usar o Orion pelo celular fora de casa |
| **6 — Interface, voz, identidade** | Frontend escolhido, voz masculina, palavra de ativação, visual do Orion | A definir na própria fase |
| **7 — Limpeza** | Apagar `Orion_Ollama/`, `Orion_Core/`, `bin/` | Nenhum arquivo do legado no repositório |

## 7. Decisões em aberto

| # | Decisão | Recomendação | Alternativa |
|---|---|---|---|
| 1 | Banco da memória | SQLite + FTS5 + sqlite-vec | SurrealDB (servidor ou embutido) |
| 2 | Gateway de modelos | OmniRoute | LiteLLM (biblioteca, sem processo extra, cotas na mão) |
| 3 | Framework do agente | PydanticAI | Loop próprio (~200 linhas) |
| 4 | Ritmo de apagar o legado | Por fase (código antigo à mão para portar) | Tudo agora, consultando o repositório Lyra |
| 5 | Interface | A discutir (fase 6) | — |
| 6 | Voz | Transcrição: Whisper no Groq (grátis); fala: edge-tts `pt-BR-AntonioNeural` (grátis); voz ao vivo: Gemini Live com voz masculina; ativação: openWakeWord treinado em "Orion" | A discutir (fase 6) |
| 7 | Busca web gratuita | Pesquisar na fase 4 (cotas mudam) | — |
| 8 | Ring 0 §4 (Hardware-Bound) | Rever — conflita com trocar de máquina | — |

## 8. Registro de decisões

| Data | Decisão | Motivo |
|---|---|---|
| 25/06/2026 | Cascata cloud-first (Groq → Gemini → Claude → local) | Qualidade/latência (substituída em 01/10 pelo gateway) |
| 01/07/2026 | Decay Ebbinghaus removido do ranking | Esquecia memória antiga relevante. Não reimplementar sem discussão |
| 01/07/2026 | Axônios na esfera descartados | Pedido explícito |
| 01/07/2026 | Backup automático agendado: não implementar ainda | Pedido do usuário (revisto em 01/10: backup entra na fase 3) |
| 07-08/08/2026 | Nomes das ferramentas em PT | São o contrato de function-calling com o LLM |
| 11/08/2026 | Lyra 2.0: auth, routers, SvelteKit, Tauri | Histórico em [TECNICO §9](ORION_TECNICO.md#9-lyra-20--legado-plano-de-11082026-e-execução) |
| 30/09/2026 | **Nome: Orion**, identidade masculina; visual novo a definir | Mais vozes masculinas de qualidade |
| 30/09/2026 | `lyra-v2` é a branch principal; `main` congelada, sem PR | Reescrita livre; `main` é o backup do legado (substituída no mesmo dia pelo repositório próprio) |
| 30/09/2026 | Reescrita em repositório próprio (`Orion`, branch `main`); repositório `Lyra` fica como backup do legado | Substitui a branch `lyra-v2` |
| 30/09/2026 | Arquivos e pastas renomeados de Lyra para Orion; textos, persona e identificadores de dados (namespace SurrealDB, coleção Qdrant, cookie, `localStorage`) continuam Lyra no legado | Primeira etapa da troca de nome |
| 30/09/2026 | IDE Theia e Lyra Desktop excluídos | Custo de manter sozinho |
| 30/09/2026 | Front-ends v2 (React) e v3 (SvelteKit + Tauri) excluídos; fica só o v1 | *(não registrado)* |
| 30/09/2026 | Datasets genéricos saem da memória | Os LLMs já sabem; diluíam a memória pessoal |
| 30/09/2026 | Nuvem primeiro; orçamento R$0; memória pessoal pode ir para a nuvem | PC será vendido |
| 01/10/2026 | Roda no notebook (IdeaPad Slim 3, 8GB) e depois MacBook M2 16GB; acesso pelo celular | Hardware disponível |
| 01/10/2026 | Assinaturas (Claude, Gemini, Codex) só pelos programas oficiais | Token extraído dá banimento; termos da Anthropic mudaram 3 vezes em 2026 |
| 01/10/2026 | Modelo local pequeno só como último recurso | Funcionar sem internet/cota |
| 01/10/2026 | Plano refeito do zero (este documento); nada codado antes da aprovação | Pedido do usuário |

## 9. Visão de longo prazo (conceitual)

Emotion Engine (voz paralinguística) · Wi-Fi Sensing (ESP32) · Protocolo Darwin
(auto-evolução supervisionada) · BCI/Protocolo Narciso · Sistema de Arquivos
Líquido · Total Recall (OCR contínuo de tela) · Telepatia de Clipboard · DNA de
Projeto · OPSEC avançado · Soberania acadêmica (mentor UML/Java, resumo de PDFs de
aula) · Sensor Fusion · Biometria passiva · Ghost OS Level 2.
