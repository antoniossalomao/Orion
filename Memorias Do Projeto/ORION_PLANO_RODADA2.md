# ORION — Plano da rodada 2 (versão de execução, 09/10/2026)

> Só o que o Antônio escolheu, em etapas, na ordem de execução. Cada etapa é **um marco** no formato do
> [Sistema de Priorização Pessoal] do vault: resultado observável, prova de conclusão, limite de escopo
> e próximo passo separado. Uma etapa = um PR. Não se começa a seguinte com a anterior vermelha.
> O histórico do brainstorm (ideias descartadas e em incubação) está no git deste arquivo e na nota
> de conversa do vault de 09/10/2026.
> Rodada anterior: [ORION_PLANO_PROXIMOS.md](ORION_PLANO_PROXIMOS.md). Regras: [ORION_REGRAS.md](ORION_REGRAS.md).

---

## 0. Como usar este plano

### 0.1 Decisões que valem para tudo (tomadas em 09/10)
| Decisão | Efeito em cada etapa |
|---|---|
| **Custo zero** | Nada usa API paga. Só cota gratuita (Groq, Gemini grátis), modelo local e as CLIs das assinaturas (Claude Pro, ChatGPT Plus, Google AI Pro) pelo `delegar`. Assinatura **não** inclui API. Ao bater cota gratuita: jobs opcionais param e avisam; o chat continua |
| **Modelo local de reserva** | `qwen3.5:4b` no Ollama (alternativa `gemma3:4b`), só conversa e memória, sem ferramentas, desligado por padrão. Também é o modelo do auto-compact |
| **Imagem pode sair do computador** | Perguntas sobre imagem/câmera vão à nuvem gratuita (Gemini) ou à CLI de assinatura. **Rosto e vetores de rosto nunca saem** (biometria) |
| **Sem IDE** | A memória chega ao editor pelo Orion como servidor MCP (E7) |
| **Grafo em 3D** | Reaproveita `views/memory.js` (3d-force-graph já vendorizado) |
| **Catálogo de plugins local e remoto**; **C13 e C14 entram** | Ver E8 e E7 |
| **Rotinas e tarefas em segundo plano** | Passos de leitura e mídia rodam sem confirmar; escrita e execução sempre confirmam |

### 0.2 Definição de pronto (vale para todo item)
1. Regra nova escrita em `ORION_REGRAS.md` **antes** do código, quando o item marcar "regra nova".
2. Testes: unitário com falsos (`tests/fakes.py`), teste Node para lógica pura do front (`tests/front/*.test.js`), cenário de navegador no backend de mentira (`tests/front_e2e/test_front.py`) **e** contra o backend real (`tests/front_e2e/test_orion_real.py`) para toda tela nova.
3. Ferramenta nova do modelo: classe de risco em `orion/policy/classes.py` (o `tests/test_ferramentas_doc.py` cobra), entrada no `tests/policy/test_injecao_invariantes.py` se for leitura externa ou egress.
4. Opt-in novo: entra em `config.py`, no `.env.example` (o `test_env_example.py` cobra) e na tabela de opt-ins do `ORION_OPERACAO.md` (E0 cria o teste que cobra).
5. `uv run ruff check`, `uv run ruff format --check`, `uv run pyright`, `uv run pytest -q`, `node --test "tests/front/*.test.js"` e `uv run pytest tests/front_e2e -q` verdes. CI verde em Linux, Windows e macOS.
6. `ORION_MELHORIAS.md` ganha uma linha por item com o que foi e o que **não** foi validado com serviço real.

### 0.3 Numeração usada aqui
- **Esquema do banco**: hoje é v10. As versões novas estão reservadas por etapa (v11…v17), na ordem deste plano.
- **Regras**: a última é a 45. As novas vão de 46 em diante, na ordem em que aparecem.
- IDs antigos (R, L, N, X, Y, Z, U, V) ficam entre parênteses para rastrear de onde veio cada item.

---

## 1. Sequência de execução

| # | Etapa | Entrega em uma frase | Depende de | Precisa do Antônio |
|---|---|---|---|---|
| E0 | Documentação | Todo opt-in explicado e conferido por teste | — | — |
| E1 | Base de custo, privacidade e controle | O Orion sabe quanto gastou, o que saiu, e para tudo com um comando | E0 | — |
| E2 | Ponte de desktop | Teclas globais, bandeja, palmas, captura rápida, OCR da tela | E1 | Testar teclas/microfone no notebook |
| E3 | Conversas e projetos | Arquivadas, filtro por projeto, editar pedido | E0 | — |
| E4 | Memória: núcleo | Isolamento por projeto, OCR de PDF, progresso, auto-compact, validade de fatos, reranker | E1 (modelo local) | Instalar Ollama + `qwen3.5:4b` |
| E5 | Memória: tela, sono e grafo 3D | Busca da tela, exclusões, sono editável, grafo 3D por voz, busca global | E4 | — |
| E6 | Resultados e atividade | Painel lateral, comparar versões, fontes no chat, pesquisa noturna pela interface | E1 | — |
| E7 | MCP e skills | Diagnóstico, cancelamento, C13, C14, skills no chat, Orion como servidor MCP | E1 | Registrar o MCP do Orion no Claude Code |
| E8 | Plugins | Upload, atualizar/voltar, importar Claude Code/Codex, catálogo, conceder sem reiniciar, "Orion Pesquisa" | E7 | — |
| E9 | Automação do dia a dia | Rotinas, tarefa em segundo plano, lembrete insistente, Downloads, "posso desligar?", históricos, ler depois, conversor | E2, E4 | — |
| E10 | Vida pessoal | Revisão semanal, diário, gastos, contas, monitor de preço, cápsula, cenários, linha do tempo, sonhos, foco, estado pela voz | E9 | — |
| E11 | Visão e câmera | Ver e agir, visão pelas assinaturas, detecção local, rosto local, "onde eu deixei", presença, tradutor, Flow | E2, E10 | Webcam; cadastrar o próprio rosto |
| E12 | Trabalho, estudo e carreira | Base do estágio, portfólio, tutor, headhunter, documentador, pessoas, MVP, gêmeo, conferir resposta | E7, E10 | Modelos de documentos da faculdade |
| E13 | Segurança e continuidade | Sentinela, plano de emergência, Protocolo Darwin, `orion atualizar`, backup da configuração | E8, E12 | Pessoa de confiança (E13.2) |

Ordem fixa: E0 → E1 → E2 → E3 → E4 → E5 → E6 → E7 → E8 → E9 → E10 → E11 → E12 → E13.
E3 e E6 não dependem de E2: se E2 travar em teste de hardware, adiantam-se E3 e E6 sem mudar o resto.

---

## 2. Checklist de execução

Marque ao concluir. Uma etapa só fecha com todos os itens, a prova e o PR mergeado.

### E0 — Documentação dos opt-ins
- [x] E0.1 Seções novas em `ORION_OPERACAO.md` (R1.1–R1.8)
- [x] E0.2 Tabela-resumo de opt-ins (R1.9)
- [x] E0.3 `orion doctor` aponta a seção
- [x] Testes verdes (ruff, pyright, pytest, Node, navegador nos dois backends) e CI verde nos 3 sistemas
- [x] `ORION_MELHORIAS.md` atualizado (o que foi e o que não foi validado com serviço real)
- [x] **Prova:** `uv run pytest tests/test_operacao_doc.py` verde; ler a tabela e conseguir ligar a memória da tela só com ela.
- [x] PR da E0 mergeado (junto com a E1: antoniossalomao/Orion#18)

### E1 — Base de custo, privacidade e controle
- [x] Regras escritas em `ORION_REGRAS.md` antes do código (46 (custo zero), 47 (registro de saída), 48 (pânico e não perturbe), 49 (modelo local))
- [x] E1.1 Registro de chamadas externas (base de R4.7, R4.8 e N2)
- [x] E1.2 Custo zero e cota gratuita (R4.7)
- [x] E1.3 Telemetria por provedor (R4.8 / L3)
- [x] E1.4 Aviso de gateway fora do ar (L10)
- [x] E1.5 Painel de privacidade (N2)
- [x] E1.6 Modo pânico e não perturbe (N19, N3)
- [x] E1.7 Encaixe do modelo local (L9)
- [x] Testes verdes (ruff, pyright, pytest, Node, navegador nos dois backends) e CI verde nos 3 sistemas
- [x] `ORION_MELHORIAS.md` atualizado (o que foi e o que não foi validado com serviço real)
- [x] **Prova:** com o gateway falso, 10 chamadas (2 falhas) aparecem no card "Provedores"; `orion panico` corta captura e ferramentas de rede e o painel mostra "Modo pânico"; com `ORION_LOCAL_MODEL` apontando para um servidor falso, o chat responde quando o gateway falha.
- [x] PR da E1 mergeado (antoniossalomao/Orion#18)

### E2 — Ponte de desktop
- [ ] Antônio: testar teclas globais e microfone no notebook; calibrar as palmas
- [ ] Regras escritas em `ORION_REGRAS.md` antes do código (50 (ponte), 51 (palmas))
- [ ] E2.1 Processo da ponte
- [ ] E2.2 Duas palmas abrem o Orion (U1)
- [ ] E2.3 Captura rápida (U2)
- [ ] E2.4 Copiar texto de uma área da tela (V3)
- [ ] E2.5 "O que é isso?" (N18)
- [ ] Testes verdes (ruff, pyright, pytest, Node, navegador nos dois backends) e CI verde nos 3 sistemas
- [ ] `ORION_MELHORIAS.md` atualizado (o que foi e o que não foi validado com serviço real)
- [ ] **Prova:** no notebook, `Ctrl+Alt+Espaço` abre a captura rápida e grava uma tarefa; duas palmas abrem `/ui/`; `Ctrl+Alt+T` copia texto de uma área da tela.
- [ ] PR da E2 mergeado

### E3 — Conversas e projetos
- [x] E3.1 Tela de conversas arquivadas (R2.1)
- [x] E3.2 Filtro por projeto na barra lateral (R2.2)
- [x] E3.3 "Mover para projeto" na paleta (R2.3)
- [x] E3.4 "Disponível em" editável nos documentos (R2.4)
- [x] E3.5 Editar o pedido como nova versão (R2.5 / C39)
- [x] E3.6 Telas novas contra o backend real (R8.1)
- [ ] Testes verdes (ruff, pyright, pytest, Node, navegador nos dois backends) e CI verde nos 3 sistemas
- [ ] `ORION_MELHORIAS.md` atualizado (o que foi e o que não foi validado com serviço real)
- [ ] **Prova:** cenários de navegador nos dois backends (mentira e real) para cada item.
- [ ] PR da E3 mergeado

### E4 — Memória: núcleo
- [ ] Antônio: instalar o Ollama e rodar `ollama pull qwen3.5:4b`
- [ ] Regras escritas em `ORION_REGRAS.md` antes do código (52 (isolamento por projeto), 53 (auto-compact), 54 (validade de fatos))
- [ ] E4.1 Isolamento por projeto para fatos e mensagens (R3.1)
- [ ] E4.2 OCR para PDF escaneado (R3.2)
- [ ] E4.3 Progresso e reindexar (R3.3, R3.4)
- [ ] E4.4 Auto-compact do histórico (R3.9 / L1)
- [ ] E4.5 Validade e substituição de fatos (R3.10 / L5)
- [ ] E4.6 Reranker pronto e desligado (R3.11 / L2)
- [ ] Testes verdes (ruff, pyright, pytest, Node, navegador nos dois backends) e CI verde nos 3 sistemas
- [ ] `ORION_MELHORIAS.md` atualizado (o que foi e o que não foi validado com serviço real)
- [ ] **Prova:** `eval_pessoal` roda antes/depois e não piora; conversa de 200 mensagens mantém resumo; PDF só de imagem vira texto pesquisável.
- [ ] PR da E4 mergeado

### E5 — Memória: tela, sono, grafo 3D e busca global
- [ ] Regras escritas em `ORION_REGRAS.md` antes do código (estende 44)
- [ ] E5.1 Busca da memória da tela na interface (R3.5)
- [ ] E5.2 Exclusão por aplicativo (R3.6)
- [ ] E5.3 Não capturar com a tela bloqueada (R3.7)
- [ ] E5.4 Ciclo de sono na interface (R3.8)
- [ ] E5.5 Grafo de memória 3D (R3.12 / L4)
- [ ] E5.6 Andar pelo segundo cérebro por voz (Y16)
- [ ] E5.7 Busca global (N1)
- [ ] Testes verdes (ruff, pyright, pytest, Node, navegador nos dois backends) e CI verde nos 3 sistemas
- [ ] `ORION_MELHORIAS.md` atualizado (o que foi e o que não foi validado com serviço real)
- [ ] **Prova:** cenários de navegador; tela bloqueada não gera linha em `screen_log`.
- [ ] PR da E5 mergeado

### E6 — Resultados e atividade
- [ ] E6.1 Painel lateral da biblioteca (R4.1)
- [ ] E6.2 Comparar versões (R4.2)
- [ ] E6.3 Apagar versões antigas em lote (R4.3)
- [ ] E6.4 Fontes e atividade das extensões no chat (R4.5 / C27)
- [ ] E6.5 Pesquisa noturna pela interface (R4.6)
- [ ] Testes verdes (ruff, pyright, pytest, Node, navegador nos dois backends) e CI verde nos 3 sistemas
- [ ] `ORION_MELHORIAS.md` atualizado (o que foi e o que não foi validado com serviço real)
- [ ] **Prova:** cenários de navegador nos dois backends.
- [ ] PR da E6 mergeado

### E7 — MCP e skills
- [ ] Antônio: registrar o MCP do Orion no Claude Code (`claude mcp add orion -- orion mcp-servidor`)
- [ ] Regras escritas em `ORION_REGRAS.md` antes do código (55 (C14), 56 (Orion como servidor MCP))
- [ ] E7.1 Versão de SDK incompatível (R5.1 / C07)
- [ ] E7.2 Cancelamento propagado (R5.2)
- [ ] E7.3 Diagnóstico MCP na interface (R5.3 / C26)
- [ ] E7.4 Descoberta sob demanda (R5.12 / C13)
- [ ] E7.5 Resources e prompts (R5.13 / C14)
- [ ] E7.6 Skills pelo chat e pela paleta (R5.10 / B3)
- [ ] E7.7 Orion como servidor MCP (R5.11)
- [ ] E7.8 Seu estilo de código como skill (Z12)
- [ ] Testes verdes (ruff, pyright, pytest, Node, navegador nos dois backends) e CI verde nos 3 sistemas
- [ ] `ORION_MELHORIAS.md` atualizado (o que foi e o que não foi validado com serviço real)
- [ ] **Prova:** testes com o servidor MCP de teste real (`tests/mcp_cliente`); `claude mcp add orion -- orion mcp-servidor` lista as 3 ferramentas.
- [ ] PR da E7 mergeado

### E8 — Plugins
- [ ] Regras escritas em `ORION_REGRAS.md` antes do código (estende 45; 57 (catálogo remoto))
- [ ] E8.1 Instalar por upload (R5.4)
- [ ] E8.2 Atualização com reversão (R5.5 / C22)
- [ ] E8.3 Importar formato Claude Code e Codex (R5.6 / C21)
- [ ] E8.4 Catálogo local e remoto (R5.7 / C25)
- [ ] E8.5 Conceder sem reiniciar (R5.9)
- [ ] E8.6 Plugin "Orion Pesquisa" (R5.8 / C28–C29)
- [ ] Testes verdes (ruff, pyright, pytest, Node, navegador nos dois backends) e CI verde nos 3 sistemas
- [ ] `ORION_MELHORIAS.md` atualizado (o que foi e o que não foi validado com serviço real)
- [ ] **Prova:** instalar o "Orion Pesquisa" pelo catálogo local, conceder, usar sem reiniciar, atualizar, voltar.
- [ ] PR da E8 mergeado

### E9 — Automação do dia a dia
- [ ] Regras escritas em `ORION_REGRAS.md` antes do código (58 (rotinas), 59 (tarefa em segundo plano), 60 (históricos locais))
- [ ] E9.1 Rotinas (L7)
- [ ] E9.2 Tarefa em segundo plano (L6)
- [ ] E9.3 Lembrete insistente (U3)
- [ ] E9.4 Organizador de Downloads com regras (U7)
- [ ] E9.5 "Posso desligar?" (U8)
- [ ] E9.6 Histórico da área de transferência (U4)
- [ ] E9.7 Histórico de comandos pesquisável (U5)
- [ ] E9.8 Ler depois (U6)
- [ ] E9.9 Conversor de arquivos (V5)
- [ ] E9.10 Voz no Telegram (N22)
- [ ] E9.11 Transcrever → tarefas (N4)
- [ ] Testes verdes (ruff, pyright, pytest, Node, navegador nos dois backends) e CI verde nos 3 sistemas
- [ ] `ORION_MELHORIAS.md` atualizado (o que foi e o que não foi validado com serviço real)
- [ ] **Prova:** rotina "bom dia" por palmas abre o briefing, toca música e abre o app; histórico de comandos acha um `gbak` de ontem.
- [ ] PR da E9 mergeado

### E10 — Vida pessoal
- [ ] Regras escritas em `ORION_REGRAS.md` antes do código (61 (finanças locais), 62 (escudo de foco), 63 (estado pela voz))
- [ ] E10.1 Revisão semanal guiada (N9)
- [ ] E10.2 Diário automático (N10)
- [ ] E10.3 Gastos pelo Telegram (N11)
- [ ] E10.4 Contas e vencimentos (V7)
- [ ] E10.5 Monitor de preço (N12)
- [ ] E10.6 Cápsula do tempo (X9)
- [ ] E10.7 Árvore de cenários (X8)
- [ ] E10.8 Linha do tempo da vida (X4)
- [ ] E10.9 Sonhos: ideia do dia (X7)
- [ ] E10.10 Escudo de foco (X11)
- [ ] E10.11 Estado pela voz (X12)
- [ ] Testes verdes (ruff, pyright, pytest, Node, navegador nos dois backends) e CI verde nos 3 sistemas
- [ ] `ORION_MELHORIAS.md` atualizado (o que foi e o que não foi validado com serviço real)
- [ ] **Prova:** uma semana real: revisão de domingo feita, 7 diários no vault, gastos lançados pelo Telegram, um aviso de conta a vencer.
- [ ] PR da E10 mergeado

### E11 — Visão e câmera
- [ ] Antônio: webcam ligada no notebook; cadastrar o próprio rosto (`orion rosto cadastrar`)
- [ ] Regras escritas em `ORION_REGRAS.md` antes do código (64 (câmera), 65 (rosto), 66 (presença))
- [ ] E11.1 Ver e agir por foto (L12)
- [ ] E11.2 Visão pelas assinaturas (N7)
- [ ] E11.3 Detecção local de objetos (base de X1 e E11.6)
- [ ] E11.4 "Onde eu deixei?" (X1)
- [ ] E11.5 Reconhecimento facial local (pedido "só quando eu estiver no PC")
- [ ] E11.6 Presença (X5)
- [ ] E11.7 Tradutor do mundo (Y5)
- [ ] E11.8 Geração de imagem pelo Flow, semiautomático (N8)
- [ ] Testes verdes (ruff, pyright, pytest, Node, navegador nos dois backends) e CI verde nos 3 sistemas
- [ ] `ORION_MELHORIAS.md` atualizado (o que foi e o que não foi validado com serviço real)
- [ ] **Prova:** com a webcam, perguntar sobre um mouse e receber o modelo e o link do software; "onde vi meu fone?" responde com hora.
- [ ] PR da E11 mergeado

### E12 — Trabalho, estudo e carreira
- [ ] Antônio: enviar os modelos de documentos da faculdade e preencher `ORION_CLIENT_NAMES`
- [ ] Regras escritas em `ORION_REGRAS.md` antes do código (67 (base do estágio sem dado de cliente))
- [ ] E12.1 Base de soluções do estágio (N14)
- [ ] E12.2 Rascunho de case de portfólio (N16)
- [ ] E12.3 Tutor que percebe o travamento (Y11)
- [ ] E12.4 Headhunter (Y14)
- [ ] E12.5 Documentador dos seus projetos (Y15)
- [ ] E12.6 Memória de pessoas (Z4)
- [ ] E12.7 Do sonho ao MVP (Z11)
- [ ] E12.8 Gêmeo digital (X3)
- [ ] E12.9 Conferir resposta (L8)
- [ ] Testes verdes (ruff, pyright, pytest, Node, navegador nos dois backends) e CI verde nos 3 sistemas
- [ ] `ORION_MELHORIAS.md` atualizado (o que foi e o que não foi validado com serviço real)
- [ ] **Prova:** um chamado real resolvido entra na base e é achado depois; um case de portfólio gerado; um MVP criado em pasta nova.
- [ ] PR da E12 mergeado

### E13 — Segurança e continuidade
- [ ] Antônio: escolher a pessoa de confiança e escrever o texto do plano de emergência
- [ ] Regras escritas em `ORION_REGRAS.md` antes do código (68 (sentinela), 69 (plano de emergência), 70 (Protocolo Darwin), 71 (atualização))
- [ ] E13.1 Sentinela de exposição pelo Claude (Y9)
- [ ] E13.2 Plano de emergência (X10)
- [ ] E13.3 Protocolo Darwin (X6)
- [ ] E13.4 `orion atualizar` (N5)
- [ ] E13.5 Backup da configuração (N6)
- [ ] Testes verdes (ruff, pyright, pytest, Node, navegador nos dois backends) e CI verde nos 3 sistemas
- [ ] `ORION_MELHORIAS.md` atualizado (o que foi e o que não foi validado com serviço real)
- [ ] **Prova:** relatório da sentinela no vault; PR do Protocolo Darwin aberto e revisado; `orion atualizar` recusa pacote com hash errado.
- [ ] PR da E13 mergeado

---

## E0 — Documentação dos opt-ins

**Marco:** qualquer opt-in existente tem seção em `ORION_OPERACAO.md` dizendo como ligar, o que sai do computador e como desligar; um teste falha se aparecer opt-in sem documentação.
**Prova:** `uv run pytest tests/test_operacao_doc.py` verde; ler a tabela e conseguir ligar a memória da tela só com ela.
**Fora do escopo:** mudar comportamento de qualquer recurso.

### E0.1 Seções novas em `ORION_OPERACAO.md` (R1.1–R1.8)
Formato fixo de cada seção: **O que faz · Como ligar (variáveis com exemplo) · Dependências a instalar · O que sai do computador · Onde ver o resultado · Como pausar/desligar · Regra**.
| § | Recurso | Variáveis e comandos | Fontes no código |
|---|---|---|---|
| 8 | Memória da tela | `ORION_SCREEN_MEMORY`, `ORION_SCREEN_INTERVAL_S`, `ORION_SCREEN_RETENTION_DAYS`, `ORION_SCREEN_EXCLUDE`, `ORION_SCREEN_ALLOW_UNKNOWN_TITLE`, `ORION_SCREEN_OCR_LANGS`; instalar `tesseract` + pacote `por`, `xdotool` (X11); `POST /tela/pausa`, `DELETE /tela`, `/tela` no Telegram, `orion tela` | `orion/screen_memory.py`, regra 44 |
| 9 | Ciclo de sono | `ORION_SLEEP_AT`; onde ver duplicados, relações (`kind=sono`) e padrões (`sono:destilado:<data>`); como apagar um padrão (`orion esquecer`) | `orion/memory/sleep.py`, regra 42 |
| 10 | Pesquisa noturna | `ORION_RESEARCH_AT` + `ORION_WEB_TOOLS` + `ORION_VAULT_DIR`, `ORION_RESEARCH_TOPICS`; relatório no `00 Inbox` | `orion/research.py`, regra 39 |
| 11 | Leitura semanal | `ORION_WEEKLY_AI`; 1 chamada por semana, segunda com o briefing | `orion/briefing.py`, `jobs.py` |
| 12 | n8n | `ORION_N8N_WEBHOOKS` (formato `nome=url;nome=url`), exemplo de workflow, por que sempre confirma | `orion/tools/n8n.py`, regra 41 |
| 13 | Plugins e skills | estrutura de `plugin.json` e de `SKILL.md`, `orion plugin instalar/conceder/revogar`, hash, reiniciar, `orion skills` | `orion/plugins.py`, `orion/skills.py`, regras 40 e 45 |
| 14 | `orion transcrever` | arquivo ou link, `ffmpeg`, `yt-dlp`, Groq, aviso antes de enviar, nota no `00 Inbox` | `orion/media_transcribe.py`, regra 43 |
| 15 | Documentos e resultados | formatos aceitos, "Disponível em" (global × projeto), limites de tamanho, biblioteca, versões, download | `app.py` (`/memoria/documentos`, `/resultados`), `orion/resultados.py` |

### E0.2 Tabela-resumo de opt-ins (R1.9)
- Nova §16 com colunas: **Variável · Liga · Sai do computador? (o quê, para quem) · Regra · Desligar**.
- Uma linha por campo de `Settings` que liga recurso: `desktop_tools`, `web_tools`, `vision_tools`, `voice_enabled`, `voice_live_enabled`, `wake_enabled`, `screen_memory`, `sleep_at`, `research_at`, `weekly_ai`, `consolidate`, `briefing_at`, `n8n_webhooks`, `telegram_token`, `plugins_enabled`, `skills_enabled`, `mcp_enabled`, `jobs_enabled`.
- **Teste novo** `tests/test_operacao_doc.py`: lê `Settings.model_fields`, filtra por uma lista explícita `OPT_INS` mantida no próprio teste, e falha se algum nome (`ORION_<CAMPO>`) não aparece na §16. Toda etapa seguinte que criar opt-in acrescenta a linha **e** o nome em `OPT_INS`.

### E0.3 `orion doctor` aponta a seção
- Em `orion/doctor.py`, cada aviso de opt-in mal configurado ganha o sufixo `(ver ORION_OPERACAO §N)`.
- Teste em `tests/test_doctor.py`: memória da tela ligada sem `tesseract` → mensagem contém `§8`.

---

## E1 — Base de custo, privacidade e controle

**Marco:** o painel mostra, por provedor, chamadas, falhas, latência e quanto da cota gratuita foi usado; mostra o que saiu do computador; um comando pausa tudo; o modelo local pode ser ligado.
**Prova:** com o gateway falso, 10 chamadas (2 falhas) aparecem no card "Provedores"; `orion panico` corta captura e ferramentas de rede e o painel mostra "Modo pânico"; com `ORION_LOCAL_MODEL` apontando para um servidor falso, o chat responde quando o gateway falha.
**Esquema:** v11. **Regras:** 46 (custo zero), 47 (registro de saída), 48 (pânico e não perturbe), 49 (modelo local).
**Fora do escopo:** medir a cota real do provedor (só o que o Orion contou).

### E1.1 Registro de chamadas externas (base de R4.7, R4.8 e N2)
- **Esquema v11** em `orion/memory/schema.py`:
  ```sql
  CREATE TABLE external_calls (
      id INTEGER PRIMARY KEY,
      ts REAL NOT NULL,
      provider TEXT NOT NULL,      -- gateway:<camada>, groq, gemini, brave, ollama, cli:<nome>
      kind TEXT NOT NULL,          -- chat, embed, transcribe, vision, search, tts, image, cli
      model TEXT NOT NULL DEFAULT '',
      ok INTEGER NOT NULL,
      latency_ms INTEGER NOT NULL,
      bytes_out INTEGER NOT NULL DEFAULT 0,
      bytes_in INTEGER NOT NULL DEFAULT 0,
      content_kind TEXT NOT NULL DEFAULT ''   -- texto, imagem, audio (nunca o conteúdo)
  );
  CREATE INDEX idx_external_calls_ts ON external_calls(ts);
  CREATE INDEX idx_external_calls_provider ON external_calls(provider, ts);
  ```
  `MIGRATIONS[10] = DDL_V11`, `SCHEMA_VERSION = 11`, teste de migração v10→v11 em `tests/memory/`.
- `MemoryStore.add_external_call(...)`, `external_calls_summary(desde: float)` (agrupado por provider/kind: total, falhas, p50/p95 de latência, bytes) e poda de 90 dias junto com `audit_prune`.
- **Onde registrar** (um ponto por cliente, nunca o texto):
  - `orion/gateway.py`: em `_run`, ao fim de cada tentativa por endpoint (já existe `EndpointStats`; gravar também no banco por um callback `on_call` passado no construtor).
  - `orion/transcribe.py`, `orion/vision.py`, `orion/tools/web.py` (busca, Gemini Search, imagem), `orion/memory/embedders.py`, `orion/voice.py` (TTS), `orion/delegate.py` (`cli:<nome>`, bytes do prompt).
- **Regra 47**: toda saída de dado do computador passa por esse registro; registro só com tamanho e tipo, nunca conteúdo. Teste: varrer chamadas `httpx` em `orion/` (teste estático) e exigir que o módulo esteja na lista de clientes registrados.

### E1.2 Custo zero e cota gratuita (R4.7)
- Novo `orion/costs.py`:
  - `QUOTAS`: dicionário por provedor com limites **diários** configuráveis por variável (`ORION_QUOTA_GROQ_DIA`, `ORION_QUOTA_GEMINI_DIA`, `ORION_QUOTA_BRAVE_MES`…), padrão = limites gratuitos conhecidos, documentados como estimativa.
  - `uso_hoje(provider)` a partir de `external_calls`; `pode_usar(provider, opcional: bool) -> bool`: se `opcional` e uso ≥ 90%, nega.
  - Jobs opcionais (pesquisa noturna, sono, leitura semanal, consolidação, auto-compact pela nuvem) chamam `pode_usar(..., opcional=True)` antes de rodar; se negado, `ops.notify("cota", ...)` uma vez por dia.
  - O chat não é bloqueado; ao passar de 100% o painel mostra aviso.
- **Regra 46**: nenhum código chama API paga; uma lista `PROVEDORES_GRATUITOS` em `costs.py`, e `doctor` avisa se `gateway_url` apontar para provedor fora dela sem confirmação (`ORION_ALLOW_PAID=false` por padrão).
- Card "Cota de hoje" no painel (`views/painel.js`): barra por provedor.

### E1.3 Telemetria por provedor (R4.8 / L3)
- `GET /painel` ganha `provedores`: por provider/kind, chamadas, falhas, p50/p95 e modelo mais usado, na semana.
- Card "Provedores" em `views/painel.js` com tabela; teste Node para a formatação (`tests/front/painel.test.js`).

### E1.4 Aviso de gateway fora do ar (L10)
- No `JobRunner.tick`, passo `saude_gateway` a cada 60 s: se as últimas N chamadas do gateway falharam há mais de `ORION_GATEWAY_DOWN_MIN` (padrão 10) minutos, `notify("gateway", "Modelos fora do ar desde HH:MM")` uma vez; ao voltar, outro aviso. Sem chamada extra ao provedor: usa só `external_calls`.

### E1.5 Painel de privacidade (N2)
- Rota `GET /privacidade?dias=7` → por dia e provedor: nº de envios, bytes, tipo (texto/imagem/áudio).
- Tela nova `#/privacidade` (arquivo `views/privacidade.js`, item na paleta e no menu): gráfico de barras empilhadas por provedor (usar `charts.js` existente) e lista "o que saiu hoje".

### E1.6 Modo pânico e não perturbe (N19, N3)
- Novo `orion/modos.py` com estado persistido em `meta` (`modo:panico`, `modo:nao_perturbe_ate`):
  - **Pânico**: para memória da tela, palavra de ativação, palmas (E2), câmera (E11), jobs que usam rede, e tira do registro de ferramentas tudo que é `egress`, `exec` ou `external`. Só sai por comando explícito (`orion panico --sair`, botão no painel com senha de novo, `/panico sair` no Telegram).
  - **Não perturbe**: segura avisos não urgentes (`notify` ganha `urgente: bool`; não urgentes ficam na fila sem entregar) e pausa capturas proativas; com horário (`ORION_DND_AT=22:30-07:00`) ou por toque até um horário.
- Gatilhos: `orion panico`, `POST /modo/panico`, `/panico` no Telegram, botão no painel, tecla global (E2).
- **Regra 48**: pânico é corte, não pausa: nada volta sozinho; o audit registra entrada e saída.
- Testes: com pânico ligado, `ToolRegistry.schemas()` não contém nenhuma ferramenta egress/exec/external; job de tela não roda.

### E1.7 Encaixe do modelo local (L9)
- Variáveis: `ORION_LOCAL_MODEL` (padrão vazio; recomendado `qwen3.5:4b`), `ORION_LOCAL_URL` (padrão `http://127.0.0.1:11434/v1`).
- Em `app.py` (onde os `Endpoint` são montados, perto da linha 229): se `local_model` definido, acrescenta `Endpoint("local", local_url, local_model, "", tier="local")` como **último** da lista, com flag `tools=False`.
- Em `gateway.py`: endpoint com `tools=False` recebe a requisição sem `tools` e com uma linha a mais no sistema: "Modo reserva: sem ferramentas".
- `orion doctor`: confere se o Ollama responde em `/api/tags` e se o modelo está baixado.
- Testes: gateway falso que falha + servidor falso compatível com OpenAI no lugar do Ollama → resposta vem do `local`, sem `tools` no corpo.
- **Regra 49**: modelo local nunca recebe ferramentas; endereço só local (`127.0.0.1`/`localhost`), validado na config.
- **Antônio**: `ollama pull qwen3.5:4b` no notebook.

---

## E2 — Ponte de desktop

O Orion hoje é servidor + navegador: não há processo com tecla global, bandeja ou acesso para colar texto no programa em foco. Esta etapa cria esse processo, base de U1, U2, N18, V3 e dos atalhos de E9–E11.

**Marco:** um processo leve (`orion ponte`) fica na bandeja, registra teclas globais, recebe comandos do servidor (abrir janela, colar texto) e duas palmas abrem o Orion.
**Prova:** no notebook, `Ctrl+Alt+Espaço` abre a captura rápida e grava uma tarefa; duas palmas abrem `/ui/`; `Ctrl+Alt+T` copia texto de uma área da tela.
**Regras:** 50 (ponte), 51 (palmas). **Esquema:** nenhum.
**Fora do escopo:** atalhos de outras etapas (só a infraestrutura e os 4 itens abaixo).

### E2.1 Processo da ponte
- Novo pacote `orion/ponte/`:
  - `__main__.py`: `orion ponte` (subcomando em `orion/__main__.py`).
  - Bandeja: `pystray` + ícone `assets/` existente; menu: Abrir Orion, Captura rápida, Não perturbe, Pânico, Sair.
  - Teclas globais: `pynput.keyboard.GlobalHotKeys`; mapa em `ORION_HOTKEYS` (JSON) com padrões: captura `ctrl+alt+space`, "o que é isso?" `ctrl+alt+o`, OCR `ctrl+alt+t`, pânico `ctrl+alt+shift+p`.
  - Janela pequena: `pywebview` (já usado pelo front antigo) abrindo uma página nova do front `ui/#/rapido` sem barras.
  - Colar no programa em foco: grava na área de transferência e envia `Ctrl+V` (`pynput`), devolvendo o conteúdo anterior da área de transferência depois de 1 s.
- Comunicação: a ponte fala com o servidor por HTTP local (`127.0.0.1`) com um **token próprio** (`orion ponte --parear` grava em `auth.db` um token com escopo `ponte`); o servidor manda comandos à ponte por um WebSocket que a ponte abre (`/ws/ponte`).
- `orion autostart` passa a instalar também a ponte.
- **Regra 50**: a ponte não executa nada por conta própria: só abre janelas, lê a tela/área de transferência quando você aperta a tecla, e cola o que você aprovou; token de escopo `ponte` não acessa `/chat` com ferramentas de escrita.
- Testes: `tests/test_ponte.py` com `pynput`/`pystray` falsos (injeção pelo construtor); WebSocket com o `TestClient`.

### E2.2 Duas palmas abrem o Orion (U1)
- `orion/wake.py`: novo `ClapDetector(Detector)`:
  - Por quadro de 80 ms: RMS e pico; mantém o ruído médio (já existe `_acompanhar_ruido`).
  - Palma = pico ≥ `ORION_CLAP_RATIO` (padrão 6×) acima do ruído e com queda rápida (energia do quadro seguinte < 40% do pico).
  - Duas palmas com 150–700 ms entre elas → `True`. Terceira palma em menos de 700 ms cancela (aplauso, batida).
- O laço do microfone passa a aceitar vários detectores; palmas e palavra de ativação dividem o mesmo `AudioSource`.
- Ação (`ORION_CLAP_ACTION`): `abrir` (manda à ponte "abrir/focar janela"; sem ponte, `webbrowser.open('/ui/')`), `abrir_e_ouvir` (abre e inicia o turno de voz como a palavra de ativação), `rotina:<nome>` (E9.1).
- Proteções: `ORION_CLAP_ENABLED=false` por padrão, `ORION_CLAP_MAX_PER_HOUR=20`, respeita pausa, pânico e não perturbe; audit sem áudio.
- **Regra 51**: palmas só abrem; nunca aprovam nem executam ferramenta.
- Testes: sinais sintéticos (duas palmas, três palmas, porta batendo = um pico longo, digitação = picos fracos) em `tests/test_wake.py`.
- **Antônio**: calibrar `ORION_CLAP_RATIO` no quarto (o painel mostra os picos detectados na última hora).

### E2.3 Captura rápida (U2)
- Página `#/rapido`: um campo e Enter. Envia a `POST /captura` (nova rota).
- `POST /captura {texto}`: o modelo **rápido** classifica em `tarefa | lembrete | gasto | nota` com JSON validado (pydantic) e extrai campos (data, valor). Sem modelo disponível: regra simples por palavra-chave ("amanhã", "R$", "lembra") e, se nada casar, vira nota no `00 Inbox`.
- Resultado mostrado na própria janela com "desfazer" (rota `DELETE /captura/{id}` desfaz o que foi criado).
- `gasto` só funciona depois de E10.3; até lá cai em nota.

### E2.4 Copiar texto de uma área da tela (V3)
- Tecla → a ponte abre seleção de área (sobreposição de tela cheia com `tkinter`, arrastar para escolher), recorta a captura (`mss`) e roda `tesseract` local (`pytesseract`, mesmos idiomas da memória da tela).
- Texto vai para a área de transferência e aparece uma notificação "Copiado (N caracteres)". Imagem descartada na hora. Nada vai ao servidor nem à nuvem.

### E2.5 "O que é isso?" (N18)
- Tecla → captura a janela em foco (comando nativo de `capturar_tela` já existente), abre a janela pequena com a imagem e um campo de pergunta; envia ao `explicar_tela` (vai à nuvem: aviso na primeira vez). Depois de E11.2, usa a ordem de visão definida lá.

---

## E3 — Conversas e projetos

**Marco:** dá para ver/desarquivar conversas, filtrar a barra por projeto, mover por paleta, editar "Disponível em" e editar um pedido criando versão.
**Prova:** cenários de navegador nos dois backends (mentira e real) para cada item.
**Esquema:** v12 (versões de mensagem). **Regras:** nenhuma.

### E3.1 Tela de conversas arquivadas (R2.1)
- API já existe (`GET /sessoes?arquivadas=true`, `PATCH /sessoes/{id}` com `arquivada`).
- Front: seção "Arquivadas" em `views/conhecimento.js` (ou rota `#/arquivadas`): lista com busca (reusa `/sessoes/busca`), botões Desarquivar e Apagar (confirmação com o título).

### E3.2 Filtro por projeto na barra lateral (R2.2)
- API: `GET /sessoes?projeto=<id|nenhum>`; se faltar, acrescentar o filtro em `store.list_sessions_ui`.
- Front (`sidebar.js`): seletor no topo (Todos · Sem projeto · cada projeto), lembrado em `localStorage`; selo com cor (hash do nome) + nome curto em cada item.

### E3.3 "Mover para projeto" na paleta (R2.3)
- `palette.js`: comando "Mover conversa para projeto…" que abre a lista de projetos; `slash.js`: `/projeto <nome>` (teste em `tests/front/slash.test.js`).
- Usa `PATCH /sessoes/{id}` com `projeto_id` (já existe).

### E3.4 "Disponível em" editável nos documentos (R2.4)
- `PATCH /memoria/documentos/{id} {projeto_id: int|null}` → `UPDATE documents SET project_id=?`; não precisa reindexar (o filtro é na consulta).
- Front: seletor no card Documentos.

### E3.5 Editar o pedido como nova versão (R2.5 / C39)
- **Esquema v12**: `ALTER TABLE messages ADD COLUMN version_of INTEGER REFERENCES messages(id)`; `ALTER TABLE messages ADD COLUMN superseded INTEGER NOT NULL DEFAULT 0`.
- `POST /historico/{msg_id}/editar {texto}`: marca `superseded=1` na mensagem original e em todas as posteriores da sessão; cria a nova mensagem com `version_of=<original>` e roda o turno normal.
- `context_history` ignora `superseded=1`; `history_page` devolve as versões agrupadas.
- Front: lápis na bolha do usuário; setas ‹ 1/2 › trocam a versão exibida (só exibição).
- Bloqueios: aprovação pendente ou resposta em andamento → 409 (mesma regra da limpeza).

### E3.6 Telas novas contra o backend real (R8.1)
- Acrescentar a `tests/front_e2e/test_orion_real.py` os cenários que hoje só rodam no backend de mentira: Conhecimento (projetos e fatos), Documentos (upload), Resultados, Atividade, Plugins, arquivadas, filtro por projeto.

---

## E4 — Memória: núcleo

**Marco:** fatos e mensagens respeitam o projeto, PDF escaneado entra, a indexação mostra progresso, conversa longa não "esquece o começo", fato vencido perde peso e o reranker fica pronto para ligar.
**Prova:** `eval_pessoal` roda antes/depois e não piora; conversa de 200 mensagens mantém resumo; PDF só de imagem vira texto pesquisável.
**Esquema:** v13. **Regras:** 52 (isolamento por projeto), 53 (auto-compact), 54 (validade de fatos).

### E4.1 Isolamento por projeto para fatos e mensagens (R3.1)
- v13: `ALTER TABLE facts ADD COLUMN project_id INTEGER REFERENCES projects(id) ON DELETE SET NULL`. Fatos atuais ficam globais (NULL).
- `add_fact(..., project_id)`: a consolidação grava o projeto da sessão de origem.
- `search_facts`/`search(...)`: novo argumento `project_id`; dentro de projeto → globais + do projeto; fora → só globais. Mensagens: a busca de conversas já filtra por sessão; acrescentar o mesmo filtro por projeto em `search_sessions` e no `hits` do agente.
- Tela Conhecimento: coluna "Projeto" editável por fato.
- **Regra 52**: memória de um projeto não aparece em conversa de outro projeto nem sem projeto.

### E4.2 OCR para PDF escaneado (R3.2)
- Em `orion/tools/documents.py` (leitura de PDF): página com menos de 20 caracteres extraídos → renderiza (`pypdfium2`) e roda `tesseract` local. Teto `ORION_OCR_MAX_PAGES=50`.

### E4.3 Progresso e reindexar (R3.3, R3.4)
- v13: `ALTER TABLE documents ADD COLUMN status TEXT NOT NULL DEFAULT 'pronto'`, `ADD COLUMN progress INTEGER NOT NULL DEFAULT 100`.
- Upload vira job em segundo plano (`asyncio.create_task`): `status='indexando'`, `progress` atualizado por página/trecho. `GET /memoria/documentos/{id}` devolve os dois; barra no card com polling de 1 s.
- `POST /memoria/documentos/{id}/reindexar`: apaga trechos e vetores e reindexa.

### E4.4 Auto-compact do histórico (R3.9 / L1)
- v13:
  ```sql
  CREATE TABLE session_summaries (
      session_id TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
      upto_message_id INTEGER NOT NULL,
      text TEXT NOT NULL,
      model TEXT NOT NULL,
      created_at REAL NOT NULL
  );
  ```
- Novo `orion/memory/compact.py`: quando a sessão passa de `ORION_COMPACT_AT` mensagens (padrão 40), resume as que estão fora da janela de contexto em até 12 linhas, **só com o modelo local**. Roda depois da resposta (não atrasa o turno). Sem modelo local: não faz nada (mantém o corte atual).
- `Agent._mensagens`: se há resumo, entra como bloco `[RESUMO DA CONVERSA ATÉ AQUI: gerado, pode ter erro]` antes do histórico.
- Front: aviso discreto "conversa resumida até a mensagem X" com "ver resumo".
- **Regra 53**: o resumo nunca substitui o registro; é marcado como gerado; só roda com modelo local.

### E4.5 Validade e substituição de fatos (R3.10 / L5)
- v13: `ALTER TABLE facts ADD COLUMN valid_until REAL`, `ADD COLUMN superseded_by INTEGER REFERENCES facts(id)`.
- Busca: fato vencido ou substituído recebe peso 0,3 no ranking e aparece com a marca "(antigo)".
- Ciclo de sono (`sleep.py`): nova verificação de conflito: pares de fatos parecidos com informação diferente ("mora em X" × "mora em Y") viram **aviso** na caixa de atividade com dois botões: "o novo vale" (marca `superseded_by`) e "os dois valem".
- **Regra 54**: o Orion nunca decide sozinho qual fato vale.

### E4.6 Reranker pronto e desligado (R3.11 / L2)
- `orion/memory/rerank.py`: interface `Reranker.rerank(pergunta, trechos) -> trechos`; implementação local com cross-encoder pequeno via `onnxruntime` (modelo baixado sob demanda para `<dados>/modelos/`).
- `ORION_RERANK=false` por padrão. `python -m orion.memory.eval ... --rerank` compara com e sem; ligar só se o acerto subir.

---

## E5 — Memória: tela, sono, grafo 3D e busca global

**Marco:** a memória da tela é pesquisável e mais segura; o resultado do sono é editável; o grafo 3D volta a funcionar no backend novo e navega por voz; uma busca procura em tudo.
**Prova:** cenários de navegador; tela bloqueada não gera linha em `screen_log`.
**Esquema:** v14. **Regras:** estende 44.

### E5.1 Busca da memória da tela na interface (R3.5)
- `GET /tela/busca?q=&dias=` (usa `search_screen` existente); caixa de busca no card da tela com hora e janela.

### E5.2 Exclusão por aplicativo (R3.6)
- v14: `ALTER TABLE screen_log ADD COLUMN app TEXT NOT NULL DEFAULT ''`.
- Nome do processo da janela em foco: Windows (`win32gui` + `psutil`), macOS (`NSWorkspace` via `osascript`), Linux X11 (`xdotool getactivewindow getwindowpid`).
- `ORION_SCREEN_EXCLUDE_APPS` (ex.: `KeePassXC;1Password;bitwarden`) → nem captura.

### E5.3 Não capturar com a tela bloqueada (R3.7)
- Função `tela_bloqueada()` em `screen_memory.py`: Windows (`OpenInputDesktop` falha), macOS (`CGSessionCopyCurrentDictionary` → `CGSSessionScreenIsLocked`), Linux (`loginctl show-session -p LockedHint`). Sem como detectar → não captura. Reaproveitada pela presença (E11.6).

### E5.4 Ciclo de sono na interface (R3.8)
- `GET/DELETE /memoria/relacoes` (arestas `kind=sono`), padrões são fatos `sono:destilado:*` (já editáveis em `/memoria/fatos`).
- Aviso de duplicado na caixa de atividade ganha botão "apagar o repetido" → `DELETE /memoria/fatos/{id}`.

### E5.5 Grafo de memória 3D (R3.12 / L4)
- Rota `GET /grafo/completo?projeto=&limite=500` no backend novo, no formato que `views/memory.js` já espera (nós e links): fatos, tópicos (`topic:*`), relações do sono e documentos.
- Remover o "modo demo" quando o backend responde; clicar no nó abre o fato na tela Conhecimento.

### E5.6 Andar pelo segundo cérebro por voz (Y16)
- Notas do vault entram no grafo como nós (`doc:<caminho>`) ligadas por wikilinks (extraídos na indexação do vault, `index_vault`).
- Comandos no campo/voz: "me leva até X" (foca o nó cujo título casa), "o que liga X a Y" (menor caminho no grafo, BFS no servidor: `GET /grafo/caminho?de=&para=`).
- Filtro por área/pasta para não carregar o vault inteiro.

### E5.7 Busca global (N1)
- `GET /busca?q=`: junta `search_sessions`, `search_facts`, chunks de documentos, `search_screen`, resultados (nome) e, depois de E9, área de transferência e comandos. Resposta agrupada por fonte com 5 itens cada.
- Front: a paleta (Ctrl+K) ganha seção "Em tudo" que chama essa rota com 300 ms de espera após digitar.

---

## E6 — Resultados e atividade

**Marco:** a biblioteca abre ao lado do chat, compara versões e limpa em lote; cada resposta mostra o que usou; a pesquisa noturna é editável pela interface.
**Prova:** cenários de navegador nos dois backends.
**Esquema:** nenhum (usa `artifacts` v10). **Regras:** nenhuma.

### E6.1 Painel lateral da biblioteca (R4.1)
- Componente de painel à direita do chat (`views/resultados.js` novo), aberto pelo card ou por link na resposta; prévia de texto, imagem e PDF (`<iframe>` com `GET /resultados/{id}/arquivo` em `Content-Disposition: inline` só para PDF, mantendo CSP).

### E6.2 Comparar versões (R4.2)
- `GET /resultados/{id}/diff?com=<id>`: diff de linhas (`difflib.unified_diff`) para texto/Markdown/CSV; imagens lado a lado no front.

### E6.3 Apagar versões antigas em lote (R4.3)
- `POST /resultados/limpar {manter: N, nome?: str}` apaga as versões além das N mais novas (arquivo + linha), com confirmação mostrando o total.

### E6.4 Fontes e atividade das extensões no chat (R4.5 / C27)
- O evento final do `/chat` (SSE) já tem `provenance`; acrescentar lista de ferramentas usadas com origem (`nativa`, `mcp:<servidor>`, `skill:<nome>`, `plugin:<nome>`) e links das fontes.
- Front: rodapé expansível na bolha "Usou: …".

### E6.5 Pesquisa noturna pela interface (R4.6)
- Assuntos saem do código para `meta` (`research:topics`), editáveis por `GET/PUT /pesquisa/assuntos` (até 5, 200 caracteres cada, mesma validação de `research.py`).
- `GET /pesquisa/relatorios` lista os relatórios do `00 Inbox` (nome com prefixo fixo); "rodar agora" (`POST /pesquisa/rodar`) respeita a cota (E1.2).

---

## E7 — MCP e skills

**Marco:** problemas de MCP aparecem na interface; cancelar chega ao servidor; muitas ferramentas não lotam o contexto; resources e prompts entram com regra; skills por `/`; Claude Code e Codex consultam a memória do Orion.
**Prova:** testes com o servidor MCP de teste real (`tests/mcp_cliente`); `claude mcp add orion -- orion mcp-servidor` lista as 3 ferramentas.
**Regras:** 55 (C14), 56 (Orion como servidor MCP).

### E7.1 Versão de SDK incompatível (R5.1 / C07)
- Servidor de teste que anuncia versão de protocolo não suportada → `mcp-check` e painel mostram "o servidor X fala o protocolo Y, o Orion fala Z".

### E7.2 Cancelamento propagado (R5.2)
- Ao cancelar o turno, cancelar a tarefa que roda `session.call_tool`; confirmar no teste com servidor real que chega `notifications/cancelled` (o servidor de teste registra).

### E7.3 Diagnóstico MCP na interface (R5.3 / C26)
- `GET /mcp/servidores`: estado, última falha, ferramentas expostas e classe de risco de cada; `POST /mcp/servidores/{nome}/testar`. Card em Integrações.

### E7.4 Descoberta sob demanda (R5.12 / C13)
- Acima de `ORION_MCP_INDEX_AT` (padrão 30) ferramentas MCP, o modelo vê só uma ferramenta `procurar_ferramenta(consulta)` + índice (nome + uma linha); escolhida a ferramenta, o esquema completo entra no turno seguinte. Classe de risco e aprovação não mudam.

### E7.5 Resources e prompts (R5.13 / C14)
- Resources: ferramenta `ler_recurso_mcp(servidor, uri)` só aceita URI que o próprio servidor listou em `resources/list` e que bata com a allowlist do servidor no `mcp.json` (`"resources": ["file:///docs/*"]`); leitura é `external` (contamina a sessão), teto 200 KB.
- Prompts: só você invoca, pela paleta e por `/prompt <servidor>:<nome>`; o modelo não vê prompts como ferramenta.
- **Regra 55**.

### E7.6 Skills pelo chat e pela paleta (R5.10 / B3)
- `/skill <nome>` e item "Usar skill…" na paleta: o turno começa com o corpo da skill já carregado (equivale a `carregar_skill` forçado).

### E7.7 Orion como servidor MCP (R5.11)
- `orion mcp-servidor` (stdio) com o SDK `mcp` já fixado: ferramentas **só leitura** `buscar_memoria`, `listar_fatos`, `buscar_conversas`, respeitando projeto (E4.1) se o cliente passar `projeto`.
- Autenticação: o processo exige `ORION_MCP_SERVER_TOKEN` (gerado por `orion mcp-servidor --token`, guardado no cofre do sistema); revogável.
- Doc em `ORION_OPERACAO.md`: `claude mcp add orion -- orion mcp-servidor` e o equivalente do Codex.
- **Regra 56**: nada de escrita, execução ou egress por esse servidor; cada chamada vai ao audit com o cliente.

### E7.8 Seu estilo de código como skill (Z12)
- `orion estilo <pasta-repo> [...]`: delega ao Claude Code (assinatura) a leitura dos repositórios e gera `SKILL.md` "estilo-antonio" (nomes, comentários, testes, commits) em `<dados>/skills/`; você revisa antes de valer (skill nova nasce desativada).
- Exposta também pelo servidor MCP (E7.7) como prompt `estilo-antonio`.

---

## E8 — Plugins

**Marco:** plugin entra pela interface, atualiza com volta, vem do formato do Claude Code/Codex, de catálogo local ou remoto, sem reiniciar; existe o primeiro plugin oficial.
**Prova:** instalar o "Orion Pesquisa" pelo catálogo local, conceder, usar sem reiniciar, atualizar, voltar.
**Regras:** estende 45; 57 (catálogo remoto).

### E8.1 Instalar por upload (R5.4)
- `POST /plugins/upload` (multipart `.zip`): extrai em pasta temporária sem seguir links, aplica `_validar_pasta` (≤200 arquivos, ≤5 MB) e `PluginStore.instalar`.

### E8.2 Atualização com reversão (R5.5 / C22)
- `PluginStore` guarda versões em `<plugins>/<nome>/versoes/<hash>/`; `instalar(atualizar=True)` cria versão nova (concessão não migra); `POST /plugins/{nome}/voltar` restaura a anterior e a concessão dela.

### E8.3 Importar formato Claude Code e Codex (R5.6 / C21)
- `orion/plugins_import.py`: lê `.claude-plugin/plugin.json` (skills em `skills/`, MCP em `.mcp.json`) e o formato do Codex; converte para `plugin.json` do Orion. O que não tem equivalente (hooks, comandos, agentes) vai para uma lista "ignorado" mostrada antes de instalar.

### E8.4 Catálogo local e remoto (R5.7 / C25)
- Local: pasta `<dados>/catalogo/` com `catalogo.json` (nome, versão, descrição, caminho, hash).
- Remoto: `ORION_PLUGIN_CATALOGS` com URLs cadastradas por você; baixa só o `catalogo.json` pela barreira de rede (`netguard`); cada pacote tem hash fixado no índice e é conferido após baixar; **baixar não instala nem concede**; nunca atualiza sozinho.
- **Regra 57**.
- Tela: aba "Catálogo" no card Plugins.

### E8.5 Conceder sem reiniciar (R5.9)
- `ToolRegistry` ganha `replace_all(tools)` atômico; skills (`SkillCatalog.scan()`) e servidores MCP do plugin sobem/caem na concessão/revogação. Turno em andamento termina com a versão antiga (o agente copia a lista no início do turno).

### E8.6 Plugin "Orion Pesquisa" (R5.8 / C28–C29)
- Pasta `plugins_oficiais/orion-pesquisa/`: skills "pesquisa-com-fontes" e "comparar-precos", servidor MCP de busca já classificado (`read`, `external`); publicado no catálogo local.

---

## E9 — Automação do dia a dia

**Marco:** rotinas e tarefas em segundo plano funcionam com a política de sempre; os utilitários diários (lembrete insistente, Downloads, "posso desligar?", históricos, ler depois, conversor, voz no Telegram, transcrever→tarefas) estão no ar.
**Prova:** rotina "bom dia" por palmas abre o briefing, toca música e abre o app; histórico de comandos acha um `gbak` de ontem.
**Esquema:** v15. **Regras:** 58 (rotinas), 59 (tarefa em segundo plano), 60 (históricos locais).

### E9.1 Rotinas (L7)
- Arquivo `<dados>/rotinas/<nome>.yaml`: `gatilhos` (frase, palmas, tecla, Telegram), `passos` (ferramenta + argumentos fixos, ou "falar texto").
- `orion rotina conceder <nome>` guarda o hash (como plugin); mudou o arquivo → concessão cai.
- Executor: passos `read` e mídia (`controlar_midia`, `abrir_app` **se listado na concessão**) rodam direto; `write`/`exec` pedem aprovação pelo botão como sempre.
- Rotina pronta de exemplo: `bom-dia.yaml` (briefing falado + música + abrir o Orion).
- **Regra 58**.

### E9.2 Tarefa em segundo plano (L6)
- v15: `CREATE TABLE background_tasks (id INTEGER PRIMARY KEY, objective TEXT NOT NULL, budget TEXT NOT NULL, status TEXT NOT NULL, report TEXT NOT NULL DEFAULT '', proposals TEXT NOT NULL DEFAULT '[]', created_at REAL NOT NULL, finished_at REAL)`.
- `POST /tarefas-bg {objetivo, max_passos, max_min}`: roda `Agent.run(read_only=True)` em sessão própria; o que seria escrita vira **proposta** (lista) que você aprova depois; relatório na caixa de atividade.
- **Regra 59**: tarefa em segundo plano nunca escreve nem executa; teto de passos, tempo e cota.

### E9.3 Lembrete insistente (U3)
- v15: `ALTER TABLE reminders ADD COLUMN insist_min INTEGER NOT NULL DEFAULT 0`.
- `gerenciar_lembretes` aceita `insistir_min`; o job reenvia a cada N minutos até `done=1`; botões "Feito" e "Adiar 1 h" no Telegram e na notificação.

### E9.4 Organizador de Downloads com regras (U7)
- `<dados>/regras_downloads.yaml`: padrão de nome/extensão → pasta destino.
- Usa a vigilância de pasta existente (`ops.watch_*`); cada movimento registra origem/destino em `meta`; `orion downloads desfazer [--ultimo|--hoje]`. O que não casa fica.

### E9.5 "Posso desligar?" (U8)
- `orion posso-desligar`, `/desligar` no Telegram e item na bandeja: repositórios em `ORION_REPOS` com mudança não commitada ou não enviada (`consultar_git`), processos em segundo plano vivos, backup do dia feito, downloads em andamento (arquivos `.crdownload/.part` em Downloads).

### E9.6 Histórico da área de transferência (U4)
- A ponte observa a área de transferência (texto apenas); filtra o que parece segredo (mesmo filtro da memória da tela); v15: `clipboard_log(id, ts, text)` + FTS; retenção `ORION_CLIPBOARD_DAYS=7`.
- Tecla `ctrl+alt+v` abre lista pesquisável; ferramenta `buscar_area_transferencia` (read, external).
- **Regra 60**: históricos ficam só no computador, com retenção e opt-in.

### E9.7 Histórico de comandos pesquisável (U5)
- Lê o histórico do PowerShell (`ConsoleHost_history.txt`) e do bash/zsh a cada hora; v15: `command_log(id, ts, shell, command)` + FTS; descarta linhas com cara de segredo.
- Ferramenta `buscar_comandos` (read) e busca global (E5.7). Opt-in `ORION_COMMAND_HISTORY`.

### E9.8 Ler depois (U6)
- `/ler <link>` no Telegram, captura rápida com link, ou `POST /ler-depois`: `buscar_url` (ou `orion transcrever` para vídeo) → resumo de 5 linhas → nota no vault `Ler depois/` com fonte. v15: `read_later(id, url, title, status, note_path, created_at)`; lista semanal no briefing de segunda.

### E9.9 Conversor de arquivos (V5)
- `orion converter <arquivo> <formato>` e envio pelo Telegram ("converte para PDF"): `ffmpeg` (áudio/vídeo), `img2pdf`/`Pillow` (imagens), LibreOffice headless (`soffice --convert-to`) para Word↔PDF quando instalado, `pikepdf` para comprimir PDF. Tudo local; resultado na biblioteca de resultados.

### E9.10 Voz no Telegram (N22)
- Mensagem de voz recebida já é transcrita; se veio por voz, responder também em áudio (edge-tts já usado em `voice.py`), `ORION_TELEGRAM_VOICE_REPLY=true`.

### E9.11 Transcrever → tarefas (N4)
- Depois de `orion transcrever`, o modelo propõe tarefas, lembretes e fatos extraídos; aparecem na caixa de atividade com aprovar/descartar item a item.

---

## E10 — Vida pessoal

**Marco:** semana planejada com o Orion, diário automático, finanças locais com contas e preços, e as ferramentas de reflexão (cápsula, cenários, linha do tempo, sonhos, foco, estado pela voz).
**Prova:** uma semana real: revisão de domingo feita, 7 diários no vault, gastos lançados pelo Telegram, um aviso de conta a vencer.
**Esquema:** v16. **Regras:** 61 (finanças locais), 62 (escudo de foco), 63 (estado pela voz).

### E10.1 Revisão semanal guiada (N9)
- Domingo no horário `ORION_WEEKLY_REVIEW_AT`: conversa guiada (projeto "Revisão") com os 5 passos do Sistema de Priorização: tarefas abertas → escolher **1 marco** → 2 blocos na agenda (proposta; escrita na agenda pede aprovação) → ideias novas para incubação → bloqueio e próxima ação de cada projeto. Resultado salvo como nota no vault.

### E10.2 Diário automático (N10)
- Job às `ORION_DIARY_AT` (ex.: 23:00): junta commits do dia (`ORION_REPOS`), conversas (títulos), documentos novos, resultados, resumo da memória da tela (se ligada) e gastos; escreve `Diário/AAAA-MM-DD.md` no vault com frontmatter `type: diario`. Resumo pelo modelo local se ligado, senão cota gratuita.

### E10.3 Gastos pelo Telegram (N11)
- v16: `transactions(id, ts, cents INTEGER, category TEXT, description TEXT, source TEXT)`.
- "gastei 30 no mercado" (Telegram, captura rápida) → parser local (valor e categoria por palavra; modelo local se ambíguo). Resumo mensal por categoria no briefing do dia 1. Fatura em PDF: leitura local e lançamento das linhas após confirmação.
- **Regra 61**: dado financeiro nunca vai à nuvem (só modelo local ou parser).

### E10.4 Contas e vencimentos (V7)
- v16: `bills(id, description, cents, due_at, barcode TEXT, paid INTEGER DEFAULT 0, file TEXT)`.
- Boleto/fatura (PDF ou foto) → OCR local + regex de linha digitável e vencimento; avisos 3 dias e 1 dia antes; "paguei" marca e lança em `transactions`.

### E10.5 Monitor de preço (N12)
- v16: `price_watch(id, item, query, target_cents, last_cents, history TEXT, active)`.
- 1×/dia por item: `pesquisar_internet`/`pesquisar_com_ia` (cota) com a consulta; extrai preço; avisa queda ou alvo atingido. Item pode vir da câmera (E11).

### E10.6 Cápsula do tempo (X9)
- v16: `ALTER TABLE reminders ADD COLUMN kind TEXT NOT NULL DEFAULT 'lembrete'`.
- "daqui 6 meses me cobra isso": lembrete `kind='capsula'` com o contexto da conversa (link para a sessão); no dia, aviso com o texto original.

### E10.7 Árvore de cenários (X8)
- Ferramenta `simular_cenarios(descricao)`: o modelo monta as variáveis; um simulador local (Monte Carlo, `random`, 10 000 rodadas) usa os gastos reais (E10.3) como base; resposta com faixas (p10/p50/p90), nunca um número só.

### E10.8 Linha do tempo da vida (X4)
- `GET /linha-do-tempo?de=&ate=`: junta mensagens, commits, documentos, tela, diário, gastos e área de transferência por hora; tela `#/linha-do-tempo` com zoom de dia/semana. Respeita a retenção de cada fonte.

### E10.9 Sonhos: ideia do dia (X7)
- No ciclo de sono, um passo a mais: escolhe 2 fatos/notas de áreas sem ligação no grafo e pede ao modelo 1 ideia que conecte os dois, com as duas fontes; entra no briefing da manhã; botão "boa" grava como fato, "descartar" apaga.

### E10.10 Escudo de foco (X11)
- Blocos de foco vêm da revisão semanal (E10.1) ou de `/foco 50` no chat.
- Durante o bloco: avisos não urgentes ficam presos (usa o não perturbe de E1.6); se a memória da tela vir app/site da lista `ORION_FOCUS_DISTRACTIONS`, uma pergunta discreta "isso é do marco da semana?" (no máximo 1 por bloco).
- **Regra 62**: o escudo pergunta, nunca bloqueia nem fecha nada.

### E10.11 Estado pela voz (X12)
- Nas falas por voz (já transcritas), extrair localmente ritmo (palavras/min), pausas e energia média do áudio; comparação com a sua média → estilo "curto" quando há pressa/cansaço. Nada é guardado além das médias.
- **Regra 63**: só muda o estilo da resposta; nunca decide nada nem entra na memória.

---

## E11 — Visão e câmera

**Marco:** "tá vendo esse objeto? pesquisa o preço" funciona por foto; o Orion reconhece você localmente; sabe onde viu um objeto; percebe presença; traduz o que a câmera vê; gera imagem pelo Flow de forma segura.
**Prova:** com a webcam, perguntar sobre um mouse e receber o modelo e o link do software; "onde vi meu fone?" responde com hora.
**Esquema:** v17. **Regras:** 64 (câmera), 65 (rosto), 66 (presença).

### E11.1 Ver e agir por foto (L12)
- Ferramenta `ver_camera(pergunta)` (opt-in `ORION_CAMERA=true`): uma foto (`opencv-python`, dispositivo `ORION_CAMERA_DEVICE`), enviada ao modelo de visão; o agente continua com as ferramentas normais (pesquisar preço, achar software). Classe `exec` + `external` na primeira vez da sessão (confirma), depois `read`.
- Atalho: "o que a câmera vê?" na paleta, voz e Telegram (`/camera`).
- Vídeo ao vivo: estender `/ws/voice` (Gemini Live) com 1 quadro/s da câmera quando você ligar no botão; o Live continua sem ferramentas: pedidos de ação passam o quadro atual ao agente (regra 36 atualizada). Cota preview: some se acabar.
- **Regra 64**: câmera só liga por pedido seu; luz/indicador na interface enquanto ligada; foto descartada após a resposta (salvo se você pedir para guardar).

### E11.2 Visão pelas assinaturas (N7)
- `Vision` ganha ordem configurável `ORION_VISION_ORDER=gemini,claude,codex,gemini_cli`: Gemini (cota grátis) primeiro; ao falhar por cota, `delegar` com a imagem: `claude -p` lendo o arquivo da imagem em pasta temporária, `codex exec -i <img>`, `gemini -p "@<img> pergunta"`. Resposta volta ao chat. Registro em `external_calls` como `cli:<nome>`.

### E11.3 Detecção local de objetos (base de X1 e E11.6)
- `orion/visao_local.py`: YOLO nano em ONNX (`onnxruntime`, CPU), modelo baixado sob demanda; devolve rótulo, confiança e caixa. Usado como filtro antes de mandar foto à nuvem e pelos itens abaixo.

### E11.4 "Onde eu deixei?" (X1)
- Modo contínuo opcional (`ORION_CAMERA_WATCH=true`, intervalo padrão 60 s, só com presença): guarda **só rótulos**; v17: `object_sightings(id, ts, label, confidence, x, y)`; retenção 7 dias; **nunca a classe "pessoa"**.
- Ferramenta `onde_vi(objeto)` (read).

### E11.5 Reconhecimento facial local (pedido "só quando eu estiver no PC")
- `orion rosto cadastrar` tira 5 fotos e guarda **vetores** (modelo de rosto ONNX local) em v17: `faces(id, name, embedding BLOB, created_at)`, cifrados com chave no cofre do sistema.
- Só roda com tela desbloqueada (E5.3) e câmera ligada; uso: confirmar que é você (presença, E11.6).
- **Regra 65**: rosto nunca sai do computador; cadastro só por você, e só de você por enquanto; apagar com `orion rosto apagar`.

### E11.6 Presença (X5)
- Sinais, do mais barato ao mais caro: tela desbloqueada e entrada de teclado/mouse recente → celular por Bluetooth perto (`bleak`, endereço em `ORION_PRESENCE_BT`) → rosto (E11.5) se a câmera estiver ligada.
- Chegou → briefing curto; saiu (sem sinal por `ORION_AWAY_MIN`) → bloquear a tela (pede confirmação na primeira configuração) e pausar capturas.
- **Regra 66**: presença só pausa/retoma e cumprimenta; nunca envia nada para fora.

### E11.7 Tradutor do mundo (Y5)
- "traduz o que a câmera/tela mostra": OCR local (tesseract com idiomas `eng+spa+...`) → tradução pelo modelo; explicação curta. Legenda ao vivo de áudio/vídeo: Vosk local (já usado na palavra de ativação) em janela flutuante da ponte; opt-in, pesado.

### E11.8 Geração de imagem pelo Flow, semiautomático (N8)
- Ferramenta `preparar_imagem_flow(descricao)`: o modelo escreve o prompt; a ponte copia para a área de transferência e abre o Flow no navegador visível. Você gera e baixa.
- A vigilância de Downloads (E9.4) reconhece a imagem nova e a põe na biblioteca de resultados ligada ao pedido. **Nada é automatizado no site** (termos do Google e risco à conta principal).

---

## E12 — Trabalho, estudo e carreira

**Marco:** o estágio tem base de soluções e respostas prontas; portfólio, vagas e documentação dos projetos andam sozinhos; o Orion lembra das pessoas, cria MVP, responde como você e confere a própria resposta.
**Prova:** um chamado real resolvido entra na base e é achado depois; um case de portfólio gerado; um MVP criado em pasta nova.
**Regras:** 67 (base do estágio sem dado de cliente).

### E12.1 Base de soluções do estágio (N14)
- Pasta do vault `02 Areas/Estágio - Bredas Sistemas/Soluções/` com template (sintoma, causa, comando, cuidado); `/solucao` no Telegram e captura rápida criam nota.
- Antes de gravar: filtro de CPF/CNPJ/nome de cliente/IP (regex + lista de clientes em `ORION_CLIENT_NAMES` para mascarar).
- "já vi esse erro?": busca restrita a essa pasta.
- **Regra 67**.

### E12.2 Rascunho de case de portfólio (N16)
- `orion portfolio <projeto>`: lê decisões (nota do projeto no vault), commits e testes; gera rascunho no formato do "Mapa de Evidências de Portfólio" do vault; nota em `00 Inbox`.

### E12.3 Tutor que percebe o travamento (Y11)
- Pela memória da tela: o mesmo texto de erro (hash da linha de erro) 3 vezes em 15 min → aviso "quer que eu explique o conceito por trás?"; explicação conceitual, sem colar a correção pronta. 1 aviso por erro.

### E12.4 Headhunter (Y14)
- `orion vagas` (sob demanda; o radar diário ficou em incubação): busca vagas pela stack do vault (perfil), compara com as habilidades e devolve lacunas + plano de estudo de 2 semanas.

### E12.5 Documentador dos seus projetos (Y15)
- Semanal por repositório em `ORION_REPOS`: `delegar` ao Claude Code em modo leitura para resumir arquitetura e decisões novas; abre PR **no vault** atualizando a nota do projeto. Nunca faz merge.

### E12.6 Memória de pessoas (Z4)
- Notas de `03 Recursos/People/` + menções nas conversas → "antes de falar com X": última conversa, assuntos, aniversário. Só o que já está anotado; nada coletado de fora.

### E12.7 Do sonho ao MVP (Z11)
- `orion mvp "<descrição>"`: o Claude Code (assinatura) cria pasta nova em `ORION_MVP_DIR` com estrutura, README e primeira tela; nunca publica, nunca toca em repositório existente.

### E12.8 Gêmeo digital (X3)
- Ferramenta `como_eu_decidiria(pergunta)`: busca no Registro de Decisões do vault e nos fatos; responde citando as decisões usadas; sem fonte suficiente, diz que não sabe.

### E12.9 Conferir resposta (L8)
- Botão "conferir" na bolha: separa afirmações, busca cada uma na memória e nas fontes do turno, marca apoiada/sem apoio; usa o modelo rápido.

---

## E13 — Segurança e continuidade

**Marco:** exposição verificada toda semana, plano de emergência configurado, o Orion propõe melhorias para si mesmo por PR, atualiza com segurança e restaura a configuração junto com os dados.
**Prova:** relatório da sentinela no vault; PR do Protocolo Darwin aberto e revisado; `orion atualizar` recusa pacote com hash errado.
**Regras:** 68 (sentinela), 69 (plano de emergência), 70 (Protocolo Darwin), 71 (atualização).

### E13.1 Sentinela de exposição pelo Claude (Y9)
- Semanal: `delegar` ao Claude Code (modo leitura) com roteiro fixo: portas abertas (`netstat`), atualizações pendentes, regras do Tailscale, programas na inicialização; senhas vazadas por k-anonimato (só os 5 primeiros caracteres do SHA-1 saem) a partir de uma lista que você mantém localmente.
- Relatório no vault e resumo na caixa de atividade. **Regra 68**: só leitura; nenhuma correção automática.

### E13.2 Plano de emergência (X10)
- Configuração: dias sem interação (`ORION_DEADMAN_DAYS`), pessoa de confiança (contato Telegram), texto que você escreveu (sem senhas; o Orion recusa texto com cara de segredo).
- Avisos a você em 3 etapas antes de disparar (D-3, D-1, D-0 com "estou bem"). **Regra 69**.

### E13.3 Protocolo Darwin (X6)
- Semanal: lê falhas, negações e erros de `audit` e logs; escolhe 1 melhoria pequena; `delegar` ao Claude Code em clone temporário do repositório Orion; abre PR com testes.
- Lista bloqueada de caminhos que o Darwin não pode tocar: `orion/policy/`, `orion/auth.py`, `ORION_REGRAS.md`, `orion/netguard.py`, `orion/ponte/` (o PR é recusado se tocar).
- **Regra 70**: nunca faz merge; nunca toca segurança.

### E13.4 `orion atualizar` (N5)
- Baixa a release do GitHub do repositório Orion, confere o hash publicado nas notas da release (e assinatura quando houver), troca a instalação mantendo a anterior para `orion atualizar --voltar`. **Regra 71**.

### E13.5 Backup da configuração (N6)
- `backup_to` passa a incluir `config/` com `.env` **sem segredos** (só nomes), `mcp.json`, rotinas, regras de Downloads, concessões de plugins e skills; `orion restore` restaura junto, pedindo os segredos que faltam.

---

## 3. Fora deste plano (não escolhidos)
Em incubação: modo estudo (N13), standup do DevCore (N15), radar diário de vagas (N17), eval semanal (N20), contexto por horário (N21), Y1–Y4, Y6–Y8, Y10, Y12, Y13, Z1–Z3, Z5–Z10, Z13, Z14, V1, V2, V4, V6, V8, V9, "retomar de onde parou", associar resultado a projeto manualmente.
Descartados: conselho de IAs (X2), Orion físico (X13), carga cognitiva (L11), IDE, WhatsApp (D9), ofuscação de tráfego (D10), automação do site do Flow.
