# ORION — Plano da rodada 2 (09/10/2026)

> Continua [ORION_PLANO_PROXIMOS.md](ORION_PLANO_PROXIMOS.md) (rodada 1, 08/10, quase toda feita).
> Três fontes: as pendências de código depois do Orion#16, uma segunda varredura do legado
> (`Orion_Ollama/`, `Orion_Core/`) e dos docs da Lyra no vault (`LYRA_ESTADO_ATUAL`, `LYRA_AGENTES_E_PLANOS`,
> `LYRA_IDE_PLANO`, catálogo de 140 ideias), e ideias novas.
> Nada aqui foi testado. Item com **regra nova** precisa da linha em [ORION_REGRAS.md](ORION_REGRAS.md) antes do código.
> Respostas em §9 (duas rodadas, 09/10). Todas as perguntas têm resposta. **Nada desta rodada vira código até o Antônio liberar.**

## 0. Critério de ordem (o mesmo da rodada 1)
1. Testável sem serviço real (Linux, API falsa) vem antes.
2. Nada abre canal novo de saída de dados ou captura sem aval (Ring 0 #3, regras 18, 32, 38).
3. Só depende do Orion antes do que depende de conta ou chave.

---

## 1. Documentação (primeiro, antes de qualquer código)

`ORION_OPERACAO.md` hoje para no §7 (checklist) e não explica nenhuma novidade de 08/10. Entregas:

| ID | Seção nova em `ORION_OPERACAO.md` | Conteúdo mínimo |
|---|---|---|
| R1.1 | §8 Memória da tela | `ORION_SCREEN_MEMORY`, dependências (`tesseract` + `por`, `xdotool` no X11), lista de exclusão por título, `ORION_SCREEN_ALLOW_UNKNOWN_TITLE`, retenção, pausar/limpar (interface, `/tela` no Telegram), o que **não** é capturado, como conferir no audit |
| R1.2 | §9 Ciclo de sono | `ORION_SLEEP_AT`, o que ele faz e não faz (regra 42), onde aparecem duplicados, relações e padrões, como apagar um padrão |
| R1.3 | §10 Pesquisa noturna | `ORION_RESEARCH_AT` + `ORION_WEB_TOOLS` + `ORION_VAULT_DIR`, de onde vêm os assuntos, onde cai o relatório (`00 Inbox`), regra 39 |
| R1.4 | §11 Leitura semanal | `ORION_WEEKLY_AI`, custo (1 chamada/semana), quando chega |
| R1.5 | §12 n8n | `ORION_N8N_WEBHOOKS` (formato), exemplo de workflow, por que sempre confirma (regra 41) |
| R1.6 | §13 Plugins e skills | estrutura do pacote, `orion plugin instalar/conceder/revogar`, hash, reiniciar, `orion skills`, `carregar_skill` |
| R1.7 | §14 `orion transcrever` | arquivo e link, `ffmpeg`/`yt-dlp`, Groq, aviso antes de enviar, onde cai a nota |
| R1.8 | §15 Documentos e resultados | formatos aceitos, "Disponível em" (global vs. projeto), limites, biblioteca de resultados, versões, download |
| R1.9 | §16 Tabela-resumo de opt-ins | uma tabela: variável → o que liga → o que sai do computador → regra → como desligar. Conferida por teste contra `config.py` (como já é feito com `.env.example`) |

Pronto quando: cada opt-in de `config.py` aparece na tabela R1.9 (teste falha se faltar) e o `orion doctor` aponta a seção certa ao achar um opt-in mal configurado.

---

## 2. Conversas e projetos

| ID | Entrega | Detalhe | Regra |
|---|---|---|---|
| R2.1 | Tela de conversas arquivadas | Lista `?arquivadas=true`, busca, desarquivar, apagar de vez (com confirmação) | — |
| R2.2 | Filtro por projeto na barra lateral | Seletor no topo; selo do projeto (cor + nome curto) em cada item; "Sem projeto" como opção | — |
| R2.3 | "Mover para projeto" na paleta | Comando + `/projeto <nome>` no `slash.js` | — |
| R2.4 | "Disponível em" editável nos documentos | `PATCH /memoria/documentos/{id}` (global ↔ projeto); reindexa só o escopo | — |
| R2.5 | Editar pedido enviado como nova versão (C39) | A resposta antiga vira versão anterior (setas ‹ 1/2 ›), não é apagada; ramo novo a partir dali; aprovação pendente bloqueia | — |
| R2.6 | **Retomar de onde parou** (novo, vem do `/resumo_sessao` da Lyra) | Ao abrir uma conversa parada há >24 h, um cartão com 3 linhas: o que ficou em aberto. Gerado sob demanda, não em segundo plano | — |

---

## 3. Memória

| ID | Entrega | Detalhe | Regra |
|---|---|---|---|
| R3.1 | Isolamento por projeto para fatos e mensagens (fecha C32) | Fato ganha `projeto_id` opcional; busca no projeto vê global + do projeto; conversa sem projeto não vê fatos de projeto. Migração: todos os fatos atuais ficam globais | nova (estende 32) |
| R3.2 | OCR para PDF escaneado | Página sem texto → tesseract local (já é dependência da tela); teto de páginas | — |
| R3.3 | Progresso da indexação | Indexar vira job; `GET /memoria/documentos/{id}` com `estado` e `%`; barra no card | — |
| R3.4 | Reindexar documento | Botão + `POST .../reindexar` (útil depois de trocar o modelo de embeddings) | — |
| R3.5 | Memória da tela: busca na interface | Caixa de busca no card, resultado com hora e app; abrir o trecho | — |
| R3.6 | Memória da tela: exclusão por aplicativo | Lista por nome de processo, além do título | — |
| R3.7 | Memória da tela: não capturar com a tela bloqueada | Windows (sessão bloqueada), macOS (`CGSession`), Linux (`loginctl`/screensaver); sem detecção → não captura | estende 44 |
| R3.8 | Ciclo de sono na interface | Ver/editar/apagar relações (`kind=sono`) e padrões; "apagar duplicado" direto do aviso da caixa de atividade | — |
| R3.9 | **Auto-compact do histórico** (Lyra tinha; como nas IAs comerciais) | Ao chegar perto do limite da janela, as mensagens antigas viram um resumo feito pelo **modelo local** (L9), em segundo plano depois da resposta; o resumo fica visível e marcado como gerado e nunca substitui o registro. Sem modelo local ligado, mantém o corte de hoje | nova (decidido §9) |
| R3.10 | **Validade e substituição de fatos** (Lyra: Knowledge Freshness Tags; vault: nota de supersession) | Fato ganha `valido_ate` opcional e `substituido_por`; o sono **avisa** conflito ("mora em X" × "mora em Y"), você escolhe; busca rebaixa fato vencido/substituído | nova |
| R3.11 | **Reranker** (Lyra tinha `bge-reranker-v2-m3`) | Local (CPU, ~300 MB), etapa depois do RRF. **Fica pronto e desligado** (`ORION_RERANK=false`); ligar só se o `eval_pessoal` mostrar ganho | decidido (§9) |
| R3.12 | **Grafo de memória 3D** (D7 da rodada 1) | A tela 3D já existe (`views/memory.js`); falta `/grafo/completo` no backend novo (fatos, relações do sono, tópicos), filtro por projeto; clicar num nó abre o fato | decidido (§9) |

---

## 4. Resultados e atividade

| ID | Entrega | Detalhe | Regra |
|---|---|---|---|
| R4.1 | Painel lateral de verdade na biblioteca | Abre ao lado do chat, prévia de texto/imagem/PDF, sem sair da conversa | — |
| R4.2 | Comparar versões | Diff lado a lado para texto/Markdown/CSV; imagem lado a lado | — |
| R4.3 | Apagar versões antigas em lote | "Manter só as N últimas" por resultado e global | — |
| R4.4 | Associar resultado a projeto manualmente | (pendência que sobrou da rodada 1) | — |
| R4.5 | Fontes e atividade das extensões no chat (C27) | Cada resposta mostra quais ferramentas/servidores MCP/skills/plugins usou e as fontes, expansível | — |
| R4.6 | Pesquisa noturna pela interface | Editar assuntos (fora do `.env`, decisão de 08/10), ver relatórios, rodar agora | — |
| R4.7 | **Custo zero** | Nada usa API paga; só cota gratuita e as CLIs das assinaturas. Ao bater a cota gratuita: para os jobs opcionais (pesquisa, sono, leitura semanal) e avisa; o chat continua | nova (decidido §9) |
| R4.8 | **Telemetria por provedor** (Lyra tinha `/stats`: usos, falhas, latência, taxa de sucesso por andar) | Hoje E3 só conta respostas. Somar falhas, latência p50/p95 e qual modelo respondeu; card no painel | — |

---

## 5. MCP, plugins e skills

| ID | Entrega | Detalhe | Regra |
|---|---|---|---|
| R5.1 | Teste de SDK incompatível (C07) | Servidor que fala versão de protocolo não suportada → mensagem legível no `mcp-check` e no painel | — |
| R5.2 | Cancelamento propagado ao servidor | Cancelar o turno manda `notifications/cancelled` ao servidor MCP | — |
| R5.3 | Diagnóstico MCP na interface (C26) | Por servidor: estado, última falha, ferramentas expostas, classe de risco de cada uma, botão "testar" | — |
| R5.4 | Plugin por upload | `.zip` pela interface → mesma validação do `orion plugin instalar` (sem link, ≤200 arquivos/5 MB, nada roda) | 45 |
| R5.5 | Atualização com reversão (C22) | Versão nova fica ao lado; concessão não migra (hash muda); "voltar à anterior" em um clique | estende 45 |
| R5.6 | Importar formato Claude Code / Codex (C21) | Converte `.claude-plugin/plugin.json` (skills, MCP) e o formato do Codex; o que não tem equivalente (hooks, comandos) é listado e ignorado | estende 45 |
| R5.7 | Catálogo (C25), **local e remoto** | Local: pasta/lista sua. Remoto: índice baixado de endereços cadastrados por você, cada pacote com hash fixado no índice; baixar **não** instala nem concede: passa pela mesma validação e pela concessão por hash (regra 45); nunca atualiza sozinho | nova (decidido §9) |
| R5.8 | Pacote "Orion Pesquisa" (C28–C29) | Primeiro plugin oficial: skills de pesquisa + servidor MCP de busca, já classificado | — |
| R5.9 | Conceder sem reiniciar | Recarregar registro de ferramentas com segurança (turno em andamento termina na versão antiga) | estende 45 |
| R5.10 | Skills pelo chat e pela paleta (B3) | `/skill <nome>` e item na paleta: força `carregar_skill` no turno | 40 |
| R5.11 | **Orion como servidor MCP** (pendência nº 6 da Lyra; substitui a IDE) | Expõe só leitura: `buscar_memoria`, `listar_fatos`, `buscar_conversas`. Token próprio, revogável, stdio local. Claude Code/Codex passam a consultar a memória do Orion | nova (decidido §9) |
| R5.12 | **C13 descoberta sob demanda** | Com muitas ferramentas MCP, o modelo vê só um índice (nome + resumo) e carrega o esquema da que precisa; a classe de risco e a aprovação não mudam. Entra por limiar (ex.: >30 ferramentas) | — |
| R5.13 | **C14 resources e prompts** | *Resources*: o modelo só lê URIs que o próprio servidor listou (nunca endereço livre), leitura é conteúdo externo (contamina a sessão), com teto de tamanho e allowlist por servidor no `mcp.json`. *Prompts*: só você invoca, pela paleta/`/`; o modelo não dispara | nova |

---

## 6. O que mais a Lyra tinha e o Orion perdeu (segunda varredura)

Já tratados na rodada 1: ciclo de sono, pesquisa noturna, tela, transcrição, n8n, personas, enxame (descartado), IDE Theia (descartada), MQTT, WhatsApp, ofuscação.
Achados novos, conferidos no código:

| ID | Lyra tinha | Orion hoje | Proposta | Onde entra |
|---|---|---|---|---|
| L1 | Compressão de histórico (>14 msgs, resume as 8 antigas) | Janela fixa: o que sai da janela some do contexto | R3.9 | §3 |
| L2 | Reranker cross-encoder no RAG híbrido | FTS5 + vetores + RRF, sem rerank | R3.11 | §3 |
| L3 | Telemetria da cascata (usos, falhas, latência por andar) | Só contagem de respostas (E3) | R4.8 | §4 |
| L4 | Grafo `/grafo` + visualizador 3D | Relações existem, não há tela | R3.12 | §3 |
| L5 | Knowledge Freshness Tags | Fato tem data e fonte, sem validade | R3.10 | §3 |
| L6 | `lyra_agent`: objetivo autônomo com orçamento de iterações, persistido (`agente_run`) | Só `delegar` (CLIs) e o turno do chat | **Tarefa em segundo plano**: objetivo + orçamento (iterações, tempo, custo), em modo só leitura (E2), relatório na caixa de atividade; o que for escrita vira proposta para aprovar | nova regra (decidido §9) |
| L7 | `commands.py`: frases locais ("bom dia" → saudação + música + abrir app; "pausa", "volume") | Palavra de ativação existe, mas tudo passa pelo modelo e pede aprovação | **Rotinas**: arquivo seu com frase/atalho → passos (ferramentas existentes). Concedida por hash como plugin; passo de leitura/mídia roda direto, execução confirma. Disparo por voz, paleta, Telegram | nova regra (decidido §9) |
| L8 | Sidecar de alucinação (draft compara divergência) + avaliação factual por afirmação (vault) | Nada | **"Conferir resposta"** sob demanda: separa afirmações e marca cada uma como apoiada/sem apoio pela memória e pelas fontes do turno | — |
| L9 | Modelo local (Ollama) como último andar da cascata | Sem nuvem, sem resposta | **Modo reserva local**: `qwen3.5:4b`, só conversa e memória, sem ferramentas; por ora só o encaixe | decidido (§9) |
| L10 | Self-healing de serviços | Não há serviços para curar | Só um aviso: gateway fora do ar por >N min → caixa de atividade + Telegram | — |
| L11 | Carga cognitiva (menos contexto quando lento) | Nada | **Não recomendo**: o gateway resolve latência; fica registrado como descartado | — |
| L12 | Câmera (`camera_engine.py`, ideias "Olho de Vidro", "Grafo Social") | Nada | **Câmera**: perguntas sobre a imagem pela nuvem gratuita, detecção e rosto locais, sob demanda primeiro (§9) | nova regra (decidido §9) |

---

## 7. Ideias novas

| ID | Ideia | Por quê |
|---|---|---|
| N1 | **Busca global** (Ctrl+K): conversas, fatos, documentos, tela, resultados numa caixa só | Hoje cada um tem sua busca, em telas diferentes |
| N2 | **Painel de privacidade**: o que saiu do computador, por provedor e por dia (tipo e tamanho, nunca conteúdo) | O Ring 0 é "nada sai sem você saber"; hoje só o audit sabe |
| N3 | **Não perturbe**: um toque pausa tela, pesquisa, avisos proativos; com horário | Vários opt-ins proativos agora; desligar um por um é ruim |
| N4 | **Transcrever → tarefas**: depois do `orion transcrever`, propõe tarefas e fatos extraídos, você aprova item a item | Fecha o ciclo da reunião/aula gravada |
| N5 | **Atualização do Orion no notebook**: `orion atualizar` confere hash/assinatura da release antes de trocar | Depois da venda do PC, atualizar vai ser rotina |
| N6 | **Backup da configuração** (opt-ins, `mcp.json`, rotinas, plugins concedidos) junto com o do banco | Restaurar hoje traz dados, não o jeito que estava ligado |
| N7 | **Visão pelas assinaturas** (ideia do Antônio, 09/10) | O `delegar` já chama `claude -p`, `codex exec` e `gemini -p` sem janela. Estender para mandar **a imagem + o pedido** e devolver a resposta: Claude Code lê o arquivo da imagem, Codex aceita imagem por parâmetro, Gemini CLI por `@arquivo`. Usa a cota das assinaturas (Claude Pro, ChatGPT Plus, Google AI Pro), custo zero. Ordem proposta: Gemini API gratuita (mais rápida) → quando a cota acabar, a CLI escolhida. Mais lento (alguns segundos) e divide o limite com o seu uso pessoal. A imagem vai à Anthropic/OpenAI/Google: conta no painel de privacidade (N2) |
| N8 | **Geração de imagem pelo Google Flow** (ideia do Antônio, 09/10) | **Não recomendo automatizar o site em segundo plano**: os termos do Google proíbem acesso automatizado, e a conta em risco é a mesma do Gmail, Drive e agenda; além disso quebra a cada mudança do site e esbarra em login/2FA/captcha. Proposta segura: **semiautomático**. O Orion escreve o prompt, abre o Flow no navegador visível com o prompt copiado; você clica em gerar e baixar; a vigilância de pasta (já existe) pega a imagem nova em Downloads e a põe na biblioteca de resultados ligada ao pedido. O `gerar_imagem` atual (API, sem cota grátis para imagem) fica desligado por causa do custo zero |

---

## 8. Testes

| ID | Entrega |
|---|---|
| R8.1 | `test_orion_real.py` cobrindo as telas novas contra o backend real (SQLite temporário, gateway falso): Conhecimento, Documentos, Resultados, Atividade, Plugins, arquivadas, filtro por projeto |
| R8.2 | Para cada item novo de interface desta rodada, um cenário no backend de mentira **e** um no real |
| R8.3 | Teste de documentação (R1.9): opt-in sem linha na tabela falha |
| R8.4 | Invariantes de injeção (`test_injecao_invariantes.py`) estendidos a rotinas, tarefa em segundo plano e servidor MCP do Orion |

---

## 9. Perguntas e respostas (respondidas em 09/10/2026)

| # | Pergunta | Resposta do Antônio | Efeito no plano |
|---|---|---|---|
| 1 | Modelo local de reserva? | **Sim**, básico mas o melhor possível; por ora só deixar pronto e escolher o modelo | L9: **`qwen3.5:4b`** (Q4_K_M, ~2,5 GB de RAM, Apache 2.0); alternativa `gemma3:4b` (~3 GB). Entrega só o encaixe: `ORION_LOCAL_MODEL` + Ollama em `127.0.0.1`, último andar, sem ferramentas, desligado por padrão. Escolha conferida em guias de 2026, não testada no notebook |
| 2 | Orion como servidor MCP? | **Pode fazer** | R5.11 |
| 3 | Rotinas sem confirmar leitura e mídia? | **Sim** | L6 e L7 seguem como proposto |
| 4 | Câmera: descartar? | **Não**: identificar objetos e pessoas; a imagem pode sair do computador | L12 revisto abaixo |
| 5 | Reranker? | **Pronto e desligado** | R3.11 |
| 6 | Compressão de histórico? | **Auto-compact como nas IAs, usando o modelo local** | R3.9 |
| 7 | IDE enterrada? | **Sim, sem IDE** | — |
| 8 | Grafo 2D ou 3D? | **3D** (já existe no projeto) | R3.12: reaproveitar `views/memory.js` (3d-force-graph, vendorizado). Ela lê `/grafo/completo`, que só o legado tem: falta a rota no backend novo, com fatos, relações do sono e tópicos, filtrada por projeto |
| 9 | Ordem? | **Eu decido** | Mantida a §10 |
| 10 | Teto de custo? | **Zero**: só as assinaturas já pagas (Google AI Pro, Claude Pro, ChatGPT Plus) | R4.7 vira **"custo zero"**: nada usa API paga; só cota gratuita (Groq, Gemini grátis) e as CLIs das assinaturas (`delegar`). Assinatura **não** inclui API: o que precisar de API paga sai do plano. Ao bater a cota gratuita, o Orion para os jobs opcionais e avisa |
| 11 | Catálogo remoto ou local? | **Os dois** | R5.7 |
| 12 | C13/C14? | **Entram neste plano, sem adiar** | R5.12, R5.13 |
| 13 | Quais extras entram? | **Todos**; **ainda sem código** | L6–L12 e N1–N6 entram no plano; nada é implementado até você liberar |
| 14 | Uso da câmera? | **Tudo que der**, ex.: perguntar sobre um objeto | L12 |
| 15 | Reconhecimento facial? | **Sim, só quando você estiver no PC** | L12 |

### Explicações dadas na 1ª rodada (já respondidas)
- **2. Orion como servidor MCP.** Hoje o Orion *usa* servidores MCP (Google, navegador). A ideia é o contrário: o Orion virar um servidor MCP para o **Claude Code e o Codex** que você já usa pelas assinaturas. Exemplo: programando no Claude Code, ele pergunta ao Orion "o que o Antônio decidiu sobre o login do DrinkControl?" e recebe a resposta da sua memória. Só leitura: eles não conseguem gravar nem executar nada no Orion. Custo zero.
- **5. Reranker.** A busca na memória devolve, digamos, 20 trechos; o reranker é uma segunda passada que os relê junto da pergunta e põe os mais relevantes no topo. Melhora o acerto, custa ~0,3 s e ~300 MB de RAM. Com custo zero, só pode ser local (no notebook). Pergunta: vale ligar, se o teste com as suas perguntas (`eval_pessoal`) mostrar melhora?
- **6. Compressão de histórico.** Em conversa longa, as mensagens mais antigas saem da janela que o modelo vê, e o Orion "esquece" o começo da conversa. A correção é resumir a parte antiga em poucas linhas e mandar o resumo junto. Cada resumo é 1 chamada ao modelo (usa a cota gratuita). Pergunta: pode?
- **11. Catálogo de plugins.** É a lista de plugins disponíveis para instalar. **Local**: uma pasta/lista que você monta, sem internet. **Remoto**: o Orion baixa a lista e os pacotes de um site (tipo uma loja), o que abre um caminho de código de terceiros entrando no seu computador. Recomendo só local.
- **12. C13 e C14.** **C13 (descoberta sob demanda)**: quando houver dezenas de ferramentas MCP, em vez de mostrar todas ao modelo a cada turno (gasta contexto), o modelo busca e carrega só as que precisa. Hoje são poucas, não compensa. **C14 (resources e prompts)**: além de ferramentas, um servidor MCP pode oferecer *resources* (arquivos e dados que o modelo lê por endereço) e *prompts* (modelos de pedido prontos). Abre leitura de dados de terceiros por endereço que o modelo escolhe, então precisaria de regra própria. Recomendo manter os dois adiados.

### L12 revisto (2ª rodada): câmera
Respostas: a imagem **pode sair** do computador; uso "pra tudo que der" (perguntar sobre um objeto etc.); reconhecimento facial **sim, só quando você estiver no PC**.
- **Perguntas sobre a imagem** ("o que é isso na minha mão?"): a foto vai ao modelo de visão na nuvem pela cota gratuita (Gemini), como já faz `analisar_imagem`. Custo zero.
- **Detecção local** (YOLO nano em ONNX, CPU): rápida e gratuita, para "há uma pessoa/objeto X" sem mandar foto; serve também de filtro antes de mandar algo à nuvem.
- **Reconhecimento facial**: modelo de rosto local; o cadastro (só o seu, para começar) e os vetores de rosto **ficam no computador e nunca vão à nuvem** (dado biométrico, LGPD). Só funciona com o PC desbloqueado e você em uso (mesma detecção de tela bloqueada do R3.7).
- **Começa sob demanda** (chat, paleta, Telegram). Modo contínuo depois, com opt-in, pausa, retenção curta e regra nova (Ring 0 #3), igual à memória da tela.
- **Objetivo (3ª resposta): "ver e agir"**, estilo *The Machine*: "tá vendo esse objeto? pesquisa o preço no Mercado Livre", "tá vendo esse mouse? acha o software dele". Fluxo: foto da câmera → modelo de visão identifica (marca/modelo, texto da etiqueta) → **o agente normal** segue com as ferramentas que já existem (`pesquisar_com_ia`, `pesquisar_internet`, `buscar_url`, navegador MCP) e a política de sempre. Baixar/instalar o software continua pedindo aprovação.
- **Dois modos de "enxergar"**:
  1. **Foto por pedido** (padrão): 1 imagem por pergunta, pelo agente, com memória e ferramentas. Custo zero na cota gratuita do Gemini (entrada de imagem é grátis nos modelos Flash; o limite é de pedidos por minuto/dia).
  2. **Vídeo ao vivo** (conversar enquanto ele vê): estender a voz ao vivo (`/ws/voice`, Gemini Live) com quadros da câmera (~1 por segundo). O Live tem cota gratuita, mas é **preview com limite menor e não garantido**; e pela regra atual o Live **não tem ferramentas nem memória**: para "pesquisa o preço", o Live passa o pedido (com o quadro atual) para o agente. Assinatura Google AI Pro **não** cobre a API: se a cota gratuita acabar, para (custo zero, R4.7).
- A foto enviada à nuvem conta no painel de privacidade (N2). Imagem de terceiros que apareçam no quadro também sai: avisar na primeira vez.

## 10. Sequência sugerida

1. **R1** (documentação inteira) + R8.3.
2. **R2.1–R2.4** (rápidos, só front + 1 rota) + R8.1 para as telas que já existem.
3. **R3.1** (isolamento) → R3.2–R3.8.
4. **R4.1–R4.6**, depois R4.8.
5. **R5.1–R5.3**, R5.10, depois plugins R5.4–R5.9.
6. L9 (encaixe do modelo local) antes do R3.9, que depende dele; depois R3.10–R3.12, R5.11–R5.13, R5.7 remoto, L6–L8, L10, L12, N1–N6, R4.7.
7. R2.5 (C39) e R2.6 por último: mexem no modelo de mensagens.

## 11. Fora do código (só você)
Continua o mesmo de [ORION_CORTE.md](ORION_CORTE.md): fase 0, serviços e chaves, Telegram real, microfone real, Tailscale, senha de fábrica.

## 12. Incubação — ideias em avaliação (09/10, sem decisão)
Saídas do brainstorm a partir do perfil do Antônio no vault (estágio, faculdade, DevCore, projetos, rotina). Decisão do Antônio (09/10): **entram** N9, N10, N11, N12, N14, N16, N18, N19, N22; **continuam em incubação** N13, N15, N17, N20, N21 (não citadas).

| ID | Ideia | Por que faz sentido | Custo |
|---|---|---|---|
| N9 | **Revisão semanal guiada** (domingo): o Orion conduz os 20 min do [Sistema de Priorização] do vault — tarefas abertas, escolher **1 marco**, sugerir 2 blocos na agenda, mover ideias novas para incubação | O método já existe no vault; falta quem puxe | cota grátis |
| N10 | **Diário automático**: à noite, nota no vault com o que você fez (commits, conversas, documentos, resumo da memória da tela) | Alimenta o segundo cérebro sem esforço; base para portfólio | local/grátis |
| N11 | **Gastos pelo Telegram**: "gastei 30 no mercado" → lançamento local (`registrar_numero` já existe); resumo mensal; ler fatura em PDF localmente | Financeiro nunca sai do computador (perfil: dado financeiro não vai a terceiros) | zero |
| N12 | **Monitor de preço**: itens que você marca (inclusive pela câmera, N7) são conferidos 1x/dia; avisa queda | Junta "ver e agir" com compra do carro/peças (ideia 128 da Lyra, Auto Tracker) | cota grátis |
| N13 | **Modo estudo**: projeto por disciplina, aula gravada → `orion transcrever` → resumo + flashcards; quiz pelo Telegram com repetição espaçada | 2º termo de ADS; reaproveita transcrição e projetos | cota grátis |
| N14 | **Base de soluções do estágio**: chamado resolvido vira nota ("erro X no Firebird → `gfix ...`"); "já vi esse erro?" busca nela | Suporte repetitivo na Bredas. **Dado de cliente nunca entra** (só a solução, anonimizada por você) | zero |
| N15 | **Standup do DevCore**: resumo semanal dos commits/PRs dos repositórios do time (PreciFly) pelo MCP do GitHub | Acompanhamento semanal já é rotina do grupo | cota grátis |
| N16 | **Rascunho de case de portfólio**: a partir de decisões, commits e notas, gera o texto do case no formato do Mapa de Evidências | Objetivo "portfólio ou renda" | cota grátis |
| N17 | **Radar de vagas e freelas**: assunto fixo da pesquisa noturna (D2), filtrado por stack e Marília/remoto | Pesquisa noturna já existe | cota grátis |
| N18 | **Atalho global "o que é isso?"**: tecla que captura a janela atual e pergunta (usa `explicar_tela` + N7) | Hoje precisa abrir o chat | cota/assinatura |
| N19 | **Modo pânico**: um comando (voz, Telegram, tecla) corta tudo: captura de tela e câmera, jobs, ferramentas de rede e execução, até você liberar | Muitos opt-ins de captura agora; desligar rápido é segurança | zero |
| N20 | **Eval semanal automático**: roda o `eval_pessoal` e o teste de injeção toda semana; avisa se a busca piorou | Pega regressão de memória/embeddings cedo | local |
| N21 | **Contexto por lugar/horário**: no horário do estágio, o Orion prioriza projeto "Estágio"; à noite, faculdade/pessoal (instruções de projeto por horário) | Rotina fixa de 6 h/dia | zero |
| N22 | **Respostas por voz no carro/fone pelo Telegram**: mensagem de voz → resposta em áudio (TTS já existe) | Uso com mãos ocupadas | cota grátis |

## 13. Fora da curva (ficção científica viável) — em avaliação
Critério: dá para fazer com o que o Orion já tem, custo zero, e não quebra o Ring 0. Referência de ficção entre parênteses.
Decisão do Antônio (09/10): **entram X1, X3–X12**; **não entram X2** (conselho de IAs) **e X13** (Orion físico).

| ID | Ideia | Como seria no Orion | Risco / limite |
|---|---|---|---|
| X1 | **"Onde eu deixei?"** (*Person of Interest*) | Com a câmera ligada, a detecção local (YOLO) guarda só **rótulos + hora + posição** dos objetos (sem imagem). "Onde vi meu fone por último?" → "na mesa, 14:32". Memória do mundo físico | Captura contínua: opt-in, retenção curta, regra nova; só objetos, nunca pessoas de terceiros |
| X2 | **Conselho de IAs** (*Câmara de Eco* da Lyra, ideia 136) | Decisão importante → Claude, Codex e Gemini (assinaturas, via `delegar`) respondem separados, depois cada um critica os outros; o Orion resume consenso e divergência | Lento (minutos); gasta limite das assinaturas |
| X3 | **Gêmeo digital** ("o que eu faria?") | A partir do Registro de Decisões do vault e dos fatos, responde como você decidiria, citando as decisões passadas que embasam; útil para "isso fere meus princípios?" | É espelho, não oráculo: sempre mostra as fontes |
| X4 | **Linha do tempo da vida** (*Black Mirror*, "The Entire History of You") | "O que eu fazia dia 3/8 às 15h?" junta conversas, commits, documentos, memória da tela, diário (N10) e gastos (N11) numa linha do tempo navegável | Tudo local; respeita retenções de cada fonte |
| X5 | **Presença** (*Jarvis*; ideia 83 BLE Presence) | Sabe se você está no PC: celular por Bluetooth perto, rosto (câmera, local) ou só tela desbloqueada. Chegou → briefing; saiu → bloqueia a tela e pausa capturas. Também é o "só quando eu estiver no PC" do reconhecimento facial | Bluetooth no Windows varia; fallback = tela bloqueada |
| X6 | **Protocolo Darwin** (auto-evolução supervisionada, roadmap da Lyra) | Semanal: lê falhas, negações e erros do audit, escolhe 1 melhoria e abre um **PR no próprio repositório** via Claude Code (assinatura). Você revisa e faz merge | Nunca faz merge, nunca toca política/regras/auth (lista bloqueada); só PR |
| X7 | **Sonhos** (REM do ciclo de sono) | De madrugada, cruza notas e fatos de áreas sem ligação e entrega de manhã 1 "ideia do dia" com as duas fontes ("o que você leu sobre X serve no projeto Y") | Qualidade variável; 1 por dia, descartável |
| X8 | **Árvore de cenários** (ideia 6 da Lyra) | Para decisões com números (carro à vista × financiado), usa seus gastos reais (N11) e simula cenários (Monte Carlo local), mostrando faixas, não uma resposta única | Só tão bom quanto os dados lançados |
| X9 | **Cápsula do tempo** | "Daqui 6 meses me lembra disso": guarda intenções/previsões suas e, na data, confronta: "você disse que terminaria o Nortis até março" | Zero; só lembrete com contexto |
| X10 | **Lazarus / dead man's switch** (ideias 70 e 77) | Se você ficar N dias sem interagir, envia a uma pessoa de confiança instruções que você escreveu (onde estão backups, senhas mestras **não**) | Configuração cuidadosa; nunca manda segredo; vários avisos antes |
| X11 | **Escudo de foco** (ideias 67 e 81) | Num bloco de foco (N9 marca os blocos), se a memória da tela vir app de distração, o Orion pergunta "isso é do marco da semana?"; segura avisos não urgentes até o fim do bloco | Pode irritar: um aviso por bloco, fácil de desligar |
| X12 | **Leitura do seu estado pela voz** (Emotion Engine da Lyra) | Pelo tom (local, sem mandar áudio a mais ninguém), percebe cansaço/pressa e ajusta: respostas mais curtas, adia o que não é urgente | Inferência imprecisa; só ajusta estilo, nunca decide nada |
| X13 | **Orion físico** (ideia 75 Tamagotchi Hardware; D8 MQTT) | Um ESP32 com anel de LED na mesa (~R$ 40) mostra o estado: pensando, aviso pendente, aguardando aprovação, captura ligada (luz vermelha = alguma câmera/tela gravando) | Única com custo (hardware barato); depende de D8 |

## 14. Terceira leva de ideias (09/10)
Decisão do Antônio: **entram Y5, Y9, Y11, Y14, Y15, Y16**; as demais ficam em incubação. Y9 roda pelo Claude Code (assinatura, via `delegar`, só leitura): ele faz a varredura e o Orion entrega o relatório.

| ID | Ideia | Como seria | Risco / limite |
|---|---|---|---|
| Y1 | **Treinador de bateria** | Microfone local mede andamento enquanto você toca (desvio do BPM, aceleração em viradas); relatório do treino e evolução por semana | Só áudio local; precisa de microfone razoável perto da bateria |
| Y2 | **Ensaio de entrevista/apresentação** | Conversa por voz (Gemini Live grátis) simulando entrevista de estágio/vaga ou banca da faculdade; no fim, feedback com pontos fracos usando o que o Orion sabe de você | Cota preview limitada |
| Y3 | **Vendedor do DrinkControl** | Para cada adega/distribuidora, pesquisa pública (site, Instagram, Google Maps) e monta proposta e roteiro de demo personalizados | Só dado público; rascunho, você envia |
| Y4 | **Fábrica de conteúdo** (ideia 53 da Lyra) | Semanal: de commits, decisões e casos (N16), rascunho de post técnico (LinkedIn) no seu tom | Rascunho; nunca publica sozinho |
| Y5 | **Tradutor do mundo** (*Star Trek*) | Câmera ou tela num texto em outra língua (manual, embalagem, erro) → tradução e explicação; legenda ao vivo de vídeo/aula com transcrição local | Legenda ao vivo local pesa no notebook |
| Y6 | **Inventário vivo** (ideia 120) | Fotografa um item (peça do PC, eletrônico) → ficha com modelo, nota fiscal, data e garantia; avisa antes de a garantia vencer; liga com X1 e N12 | Nota fiscal fica local |
| Y7 | **Caçador de assinaturas** | Nas faturas lidas pelo N11, acha cobranças recorrentes; avisa renovação, aumento de preço e o que você não usa há tempo | Depende do N11 |
| Y8 | **Arquivos-isca** (ideia 48 Honeypot Files) | Arquivos falsos tentadores no notebook ("senhas.txt", "backup_banco.xlsx"); se algum processo abrir, alerta no Telegram com o nome do processo | Zero custo; só alerta, não age |
| Y9 | **Sentinela de exposição** (ideias 5 e 60) | Semanal: senhas vazadas conferidas pelo método k-anonimato (a senha nunca sai), portas abertas no notebook, atualizações pendentes, regras do Tailscale | Busca de e-mail em vazamentos costuma exigir chave paga: fica fora |
| Y10 | **Tela de visitante** (*Minority Report*) | Com a presença (X5), se a câmera vê um segundo rosto atrás de você, o Orion borra painéis sensíveis e segura avisos até a pessoa sair | Falso positivo; um toque desfaz |
| Y11 | **Tutor que percebe o travamento** (ideia 76 Dicionário Sênior) | Pela memória da tela, nota o mesmo erro de compilação 3 vezes seguidas e oferece explicar o conceito por trás (não a correção pronta) | Um aviso por erro; desliga fácil |
| Y12 | **Despertador com briefing** | No horário, fala o briefing (agenda, clima, tarefas, ideia do dia X7) pelo PC ou manda áudio no Telegram (N22) | — |
| Y13 | **Mapa de energia** (*quantified self*) | Cruza horário de uso, commits, foco (X11) e diário (N10): "você rende mais das 20h às 22h"; a revisão semanal (N9) usa isso para sugerir os blocos | Correlação, não causa |
| Y14 | **Headhunter** (ideia 58) | Compara suas habilidades do vault com vagas reais (N17) e mostra a lacuna + plano de estudo curto | Depende do radar de vagas |
| Y15 | **Documentador dos seus projetos** | Lê os repositórios (Nortis, PreciFly, DrinkControl) e mantém arquitetura e decisões no vault atualizadas, por PR no vault (como X6) | Só PR; você aprova |
| Y16 | **Andar pelo segundo cérebro** | No grafo 3D (R3.12), navegação por voz: "me leva até Firebird", "o que liga isso ao estágio?"; o vault inteiro no mesmo grafo da memória | Grafo grande pesa no front: filtrar por área |

## 15. Quarta leva de ideias (09/10)
Decisão do Antônio: **entram Z4, Z11, Z12**; as demais ficam em incubação.

| ID | Ideia | Como seria | Risco / limite |
|---|---|---|---|
| Z1 | **Orion no celular de verdade** | O front como app instalável no celular (PWA) pelo Tailscale: voz, câmera do celular ("ver e agir" fora de casa), aprovações | Só pela rede do Tailscale; login de sempre |
| Z2 | **Revisão antes do push** | Gancho local de `git pre-push` nos seus repositórios: o Claude Code (assinatura) revisa o diff com as suas regras e aponta problemas antes de subir | Atrasa o push alguns segundos; dá para pular |
| Z3 | **Curadoria de fotos** | Num lote de fotos, separa as melhores (foco, exposição, olhos fechados, duplicadas) localmente; a visão por assinatura sugere corte e edição | Fotos de terceiros: avisar antes de mandar à nuvem |
| Z4 | **Memória de pessoas** | Das notas de pessoas do vault e das conversas: antes de falar com alguém, "da última vez ele falou de X"; aniversários | Dados de terceiros: só o que você já anotou, nada coletado fora |
| Z5 | **Lembrete por lugar** | Você compartilha a localização pelo Telegram; "quando eu chegar na faculdade, me lembra de X" | Localização só quando você compartilha; nunca guardada como trilha |
| Z6 | **Planejador do dia** | De manhã propõe o plano do dia nos seus horários livres, com as tarefas e o marco da semana (N9); você aprova e ele grava na agenda do Google | Escrita na agenda pede aprovação |
| Z7 | **Teste automático dos seus apps** | Semanal: navegador automático percorre Nortis, DrinkControl e PreciFly (versões web/locais), procura erros e problemas de acessibilidade, manda relatório | Só nos seus apps, em ambiente de teste |
| Z8 | **Monitor dos sistemas em produção** | Confere se os sistemas dos seus clientes estão no ar e se houve erro novo; alerta no Telegram | Só o que você cadastrar; nada de dado de cliente no Orion |
| Z9 | **Detector de golpe** | Encaminha ao bot uma mensagem, link ou Pix suspeito; o Orion analisa sem abrir o link (barreira de rede), procura padrões de golpe e responde "é golpe?" | Serve também para ajudar a família, encaminhando por você |
| Z10 | **Leitor de contratos e termos** | Antes de assinar (estágio, freela, aluguel), destaca cláusulas de risco e o que perguntar, pela assinatura | Não substitui advogado |
| Z11 | **Do sonho ao MVP** | Descreve uma ideia de app → o Claude Code (assinatura) cria o repositório com estrutura, README e primeira tela, para você avaliar | Só cria em pasta nova; nunca publica |
| Z12 | **Seu estilo de código como skill** | Lê seus repositórios e gera uma skill com o seu jeito de nomear, comentar e organizar; o Claude Code e o Codex usam via o servidor MCP do Orion (R5.11) | Revisar a skill antes de usar |
| Z13 | **Postura e pausas** (ideia 129 Bio Clock) | Com a câmera local ligada, nota postura ruim ou muito tempo sem pausa e sugere levantar | Só local; um aviso por hora no máximo |
| Z14 | **Orçamentista de renders 3D** | Para pedidos de visualização (SketchUp/Enscape), monta orçamento, prazo e escopo a partir dos seus trabalhos anteriores | Renda extra; rascunho para você enviar |

## 16. Quinta leva: só o que se usa todo dia (09/10)
Decisão do Antônio: **entram todas (U1–U8)**.
Pedido do Antônio: menos ideia "de vitrine", mais utilidade. Filtro: resolve algo que acontece **toda semana ou todo dia**, reaproveita peça existente e cabe em um marco curto.

| ID | Ideia | Como seria | Peça que já existe |
|---|---|---|---|
| U1 | **Duas palmas → Orion abre** (pedido do Antônio) | Detector de palmas local no mesmo laço do microfone da palavra de ativação: dois picos fortes e curtos com 150–700 ms entre eles → traz a janela do Orion para frente e começa a ouvir. Configurável: só abrir, abrir e ouvir, ou rodar uma rotina (L7). Proteções: pico medido contra o ruído do ambiente, teto de ativações por hora, pausa, desligado por padrão (`ORION_CLAP_ENABLED`). Detalhe: o Orion precisa estar rodando em segundo plano (`orion autostart`); as palmas trazem a janela, não ligam o programa do zero | `orion/wake.py` já tem a interface `Detector` e o laço do microfone; é um detector a mais, sem modelo |
| U2 | **Captura rápida** | Tecla global abre uma caixinha flutuante: digita "comprar cabo HDMI amanhã" e ele decide se é tarefa, lembrete, gasto (N11) ou nota no vault. Sem abrir o app | `/capturar` do Telegram, lembretes, tarefas |
| U3 | **Lembrete insistente** | "Me cobra até eu fazer": repete no intervalo escolhido (PC e Telegram) até você marcar feito ou adiar | Lembretes e fila de avisos |
| U4 | **Histórico da área de transferência** | Guarda localmente o texto copiado nos últimos 7 dias (descartando o que parece senha/token/CPF); "o que eu copiei ontem do terminal?" | `ler_clipboard`, filtro de segredos da memória da tela |
| U5 | **Histórico de comandos pesquisável** | Indexa o histórico do terminal (PowerShell/bash) e responde "como eu fiz aquele `gbak` com restore?" com o comando exato e a data | Busca da memória; casa com N14 (estágio) |
| U6 | **Ler depois** | Manda um link (artigo, vídeo, thread) pelo Telegram ou pela captura rápida → o Orion lê/transcreve, resume em 5 linhas e guarda no vault com a fonte; lista semanal do que ficou pendente | `buscar_url`, `orion transcrever`, vault |
| U7 | **Organizador de Downloads com regras** | Regras suas ("fatura*.pdf → Finanças", ".exe → Instaladores", "foto → Fotos/AAAA-MM"); aplica sozinho com registro e **desfazer**; o que não casa fica | `organizar_pasta`, vigilância de pasta |
| U8 | **"Posso desligar?"** | Um comando que confere: repositórios com mudança não commitada ou não enviada, processos em segundo plano rodando, backup do dia feito, download em andamento | `consultar_git`, processos, backup |

