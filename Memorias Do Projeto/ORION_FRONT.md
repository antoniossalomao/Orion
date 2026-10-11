# ORION — Front-end: brief de design e engenharia

> Direção, sistema de design, arquitetura e orçamentos do front. Criado em 03/10/2026.
> O que cada tela faz e o que foi verificado: seção "Entrega" no fim. Regras de segurança do
> front (markdown, imagem externa, links): [ORION_REGRAS.md](ORION_REGRAS.md) regras 10–12.

## 1. Diagnóstico do front anterior (02/10/2026, com capturas)

| # | Problema | Evidência |
|---|---|---|
| 1 | **Janela estreita quebrada**: sidebar fixa de 252 px cobre a tela; pílula de status, controles de janela e composer sobrepostos (visto a 390 px) | captura 390×844 |
| 2 | Toast "Hub desconectado — reconectando…" **permanente** fora do pywebview | toda captura em modo web |
| 3 | Composer é `<input type="text">`: sem múltiplas linhas, sem Shift+Enter | `index.html` |
| 4 | Markdown sem tabela, link, citação, lista aninhada; código sem rótulo de linguagem, sem copiar, sem realce | `md.js` |
| 5 | Cada pedaço do streaming **reparseia o texto inteiro** (`innerHTML = markdown(texto)`): custo O(n²) em resposta longa | `Chat.trecho` |
| 6 | Contraste do texto terciário (`#6b7489`) é **4,06–4,33:1**: reprova AA em corpo pequeno | cálculo WCAG |
| 7 | Views escondidas só por `opacity: 0` continuam focáveis por Tab | `.view-overlay` |
| 8 | `role="log" aria-live="polite"` no container inteiro: leitor de tela narra cada pedaço do streaming | `index.html` |
| 9 | Tooltips só no hover (`[data-tip]::after`): inacessíveis ao teclado | `style.css` |
| 10 | WebGL renderiza a 60 fps por baixo do chat/configurações opacos | `Ceu._quadro` |
| 11 | `script.js` com 1,5 mil linhas (céu + chat + hub + voz + grafo + som + boot), handlers `onclick=` inline, zero teste de lógica | repositório |
| 12 | Sem paleta de comandos, sem atalhos além de `/` e `Esc`, sem deep link (botão voltar não funciona) | `ui.js` |
| 13 | Configurações: cartões de altura igual com vazio, gráfico quebrado com 1 ponto, "Sobre" sem informação | captura |
| 14 | Sem aprovação de ação no front (o backend novo pede confirmação fora de banda) | `ORION_REGRAS.md` regra 2 |

## 2. Direção

**"Observatório noturno."** O Orion é uma extensão do dia a dia, não um chatbot: a interface é calma,
densa em informação útil, sem enfeite que não carregue significado. O céu (constelação de Órion) é o
único elemento decorativo e reage ao estado real do sistema.

- **Rigel (azul frio)** é o acento: foco, seleção, ação primária.
- **Betelgeuse (âmbar)** só significa *atividade/atenção*: processando, aguardando aprovação.
- Verde só para *ok*; vermelho só para *erro/destrutivo*. Cor nunca é o único sinal (sempre rótulo/ícone).
- Tipografia: Inter variável (local, offline) + mono do sistema para dados. Escala modular, corpo do
  chat 15,5 px / 1,7, linha de leitura limitada a ~72 caracteres.
- Movimento com propósito (entrada de mensagem, troca de tela, estado do céu); `prefers-reduced-motion`
  e o ajuste do app desligam tudo que não é essencial.

## 3. Sistema de design

Tokens em `css/tokens.css` (cores, espaçamento 4 px, raios, sombras, durações, z-index). Três temas por
`[data-theme]`: **Noite** (padrão), **Grafite** (neutro, sem tinta azul), **Alto contraste**. Densidade
`[data-density]` e escala de texto (`--ui-scale`, tudo em `rem`).

Contraste calculado (WCAG): texto primário ≥ 14:1, secundário ≥ 8,8:1, terciário ≥ 5,18:1 em todas as
superfícies; `text-4` (3,0–3,7:1) só para decoração, nunca para texto.

## 4. Arquitetura

Scripts simples com namespace `Orion.*` (sem bundler; módulos ES não carregam em `file://` no pywebview).
Cada arquivo tem uma responsabilidade; os puros têm UMD e teste em Node.

```
index.html · orion.svg · orion_app.py (launcher) · ponte.py (regras puras da ponte)
css/    tokens · base · layout · components · chat · views
js/     util · md · sse · store · charts · fuzzy · slash     (puros, UMD, testados em Node)
        core · api · transport · ui                          (infra: DOM, bus, prefs, HTTP, chat SSE/hub, toast/diálogo/menu)
        sky · sound · voice                                  (mídia)
        chat · composer · sidebar · palette · busca · app    (interface, rotas, atalhos, boot)
        views/home · memory · integrations · settings
vendor/ three r128 · 3d-force-graph (carregado só na 1ª visita à Memória) · Inter
```

Backend: o front detecta o que existe (`/` do legado ou `/health` do `orion.app`) e degrada sem erro
quando um endpoint não existe. Transporte do chat: no pywebview, API + hub WS (como antes); na web,
`fetch` em streaming no `/chat` (SSE). Mesmos eventos internos nos dois.

## 5. Orçamentos (verificados nos testes)

| Item | Orçamento |
|---|---|
| Violações axe (serious/critical) | 0 em todas as telas e temas |
| Contraste de texto | ≥ 4,5:1 (AA) |
| Console | 0 erros/avisos de página ao navegar por todas as telas |
| Teclado | toda ação alcançável sem mouse; foco sempre visível; views ocultas `inert` |
| Janela estreita | ≥ 700 px sem rolagem horizontal; a barra lateral vira trilho de ícones. **Só desktop (decidido em 06/10/2026):** abaixo de 700 px não é suportado, celular = Telegram |
| Streaming | 1 render por quadro (rAF), não por pedaço |
| Céu fora da home | ≤ 20 fps; pausado com a aba oculta |

## 6. Entrega (03/10/2026)

### O que mudou, tela por tela

| Tela | Antes | Agora |
|---|---|---|
| **Casca** | Sidebar fixa de 252 px; toast de "hub desconectado" permanente na web | Sidebar recolhível (`Ctrl+B`; vira trilho de ícones em janela estreita), rotas por hash (botão voltar funciona), modo foco (`Ctrl+.`), conexão em linguagem humana, sem alarme |
| **Início** | Campo de texto + constelação | Saudação, chips de estado reais (cérebro, modelo, nº de memórias), sugestões, constelação deslocada para cima, paralaxe e rótulo ao passar o mouse nas estrelas |
| **Chat** | Reparseia o texto todo a cada pedaço; sem tabela/link/realce | Streaming com ≤ 1 render por quadro; tabelas, listas aninhadas, citação, tarefas, código com linguagem, realce e "Copiar"; ferramentas usadas viram chips; **cartão de aprovação** de ação; erro com "Tentar de novo"; "Mais recentes"; parar com `Esc`; anexos (clipe, colar, arrastar) |
| **Memória** | Overlay de grafo sem teclado | Grafo pausado fora da tela; busca por trecho + lista de resultados + painel de detalhes com vizinhos (caminho por teclado); filtros por tipo; aviso explícito quando os dados são de demonstração |
| **Integrações** | Cartões estáticos | Estado real de `/integracoes`, ação por cartão (voz ao vivo, resposta por voz), aviso quando o cérebro não responde |
| **Painel** (06/10) | Não existia | Modelos (estado por endpoint, quarentena por cota, último erro), CLIs oficiais (uso do dia), aprovações pendentes, política das últimas 24 h e sistema (jobs, memória, Telegram, MCP), com alertas em texto (`GET /painel`; atualiza a cada 10 s). Só `textContent`; lógica pura em `js/painel.js` |
| **Configurações** | Cartões de altura igual, gráfico quebrado com 1 ponto | Aparência (3 temas, densidade, escala, movimento), voz e sons, modelo, conexão (endereço, token, testar), atividade (CPU/RAM/GPU, latência, cascata, serviços), atalhos, sobre |
| **Entrada (login)** | Só o token colado em Configurações | Tela cheia e **opaca** "Entrar no Orion" (usuário e senha, `autocomplete` certo, foco preso; o app fica `inert` por trás e nada dele aparece; não há "agora não" e `Esc` não fecha): abre sozinha no boot, quando um 401 chega do `/chat` e depois de sair; erro de usuário/senha e bloqueio (429) na própria tela; consulta de fundo (aprovações pendentes) **não** a reabre. Configurações › Conexão ganha a linha **Sessão** (Entrar/Sair). Sem `/auth/status` (legado) ou com token configurado, nada disso aparece |
| **Paleta (`Ctrl+K`)** | Não existia | Ir para, ações, modelo, tema/densidade/escala, conversas (por trecho), "Perguntar ao Orion", "Revisar ação pendente" |
| **Comandos `/`** | Não existia | `/` no campo abre a lista: `/nova`, `/buscar`, `/copiar`, `/exportar`, `/limpar`, `/modelo`, `/tema`, `/foco`, `/mudo`, `/voz`, `/ajuda` e as telas. Completa com `Tab`/`Enter`; `//` envia uma barra literal; caminho de arquivo e texto de várias linhas não são comando |
| **Na conversa** | — | `Ctrl+F` busca (sem acento, destaca todas as ocorrências, `Enter` navega); selecionar um trecho oferece **Citar**; tempo de cada resposta; `Ctrl+Shift+C` copia a última resposta; rascunho guardado por conversa; aprovação pendente acende o ponto do Chat e entra na paleta |

### O que foi verificado

| Orçamento | Resultado | Onde |
|---|---|---|
| Axe (serious/critical **e** moderate/best-practice) | 0 violações em 6 telas × 3 temas, no chat com resposta + aprovação + erro, e com paleta e menu abertos | `test_axe_*` |
| Entrada | Cobre a janela inteira com cor sólida (pontos dos cantos e do centro caem na tela, app inerte, só 3 controles focáveis), pede usuário e senha, recusa usuário ou senha errados sem deixá-los no campo, mostra o aviso de espera, volta quando a sessão some, sair leva de volta a ela, axe sem violações nos 3 temas (tela aberta e com erro) | `test_tela_de_entrada_*`, `test_login_*`, `test_sessao_vencida_*`, `test_sair_*`, `test_axe_tela_de_entrada_aberta` |
| Orion de verdade | App, política e SQLite reais com o front real (só o modelo é de mentira): login por cookie `httpOnly`/`Strict`, conversa em streaming, aprovação que executa o comando depois do clique, audit no banco, servidor MCP real. Achou o 422 do `resume` | `tests/front_e2e/test_orion_real.py` |
| Console | 0 erros/avisos em todos os 97 testes de navegador com backend de mentira (a fixture derruba o teste) | `conftest.py` |
| Teclado | `Ctrl+K`, `Alt+1..6`, `/`, `?`, `Ctrl+B`, `Ctrl+.`, `Ctrl+F`, `Esc`; telas ocultas `inert`; menu `/` e paleta no padrão combobox | `test_atalhos_*`, `test_paleta_*`, `test_comandos_*` |
| Janela estreita | Sem rolagem horizontal a 700 e 860 px nas 6 telas; barra vira trilho e volta ao alargar | `test_janela_*` |
| Streaming | Renderizações ≤ quadros + 2 (a resposta chega em ~100 pedaços) | `test_streaming_rende_*` |
| Céu fora da home | ≤ 22 quadros/s | `test_ceu_fora_*` |
| Segurança | `<script>`, `onerror`, `javascript:` e imagem externa em texto do modelo: nada executa, nada sai do app; CSP com o hash do único script inline (teste falha se o script mudar sem atualizar o hash) | `test_markdown_malicioso_*`, `test_csp_*` |
| Desktop | Caminho `process_command` + hub com shim (eco sem duplicar, ferramenta, modelo, fala vinda do microfone, link por `open_external`, controles da janela) | `test_desktop_*` |
| Lógica pura | 99 testes em Node (md, sse, store, util, fuzzy, charts, slash, painel) | `tests/front/` |
| Ponte e servidor | `ponte.py` (relay de eventos, URL externa, cabeçalho, geração de comandos) e `/ui/` do `orion.app` (sem `.py`, sem `__pycache__`) | `tests/legacy/test_ponte.py`, `tests/test_app_ui.py` |

Peso: ~1,1 MB no primeiro carregamento (three.js 600 KB, Inter 48 KB, CSS 80 KB, JS ~200 KB);
o 3d-force-graph (700 KB) só baixa na primeira visita à Memória.

### Achados dos próprios testes (já corrigidos)

- Toda mensagem saía com `\n\n` no fim (montagem do prompt com anexos).
- `Esc` digitado no mesmo quadro em que a paleta abre não fechava.
- **A página inteira travava** quando já havia 3 toasts e um deles estava saindo (laço que limita a pilha nunca terminava). Visto só porque o teste do `/` empilhou avisos; agora há teste dedicado.
- Níveis de título do markdown pulavam de `h1` para `h4` (axe); `<dl>` com título solto.
- Menu de modelo cobria o clipe e o texto digitado.
- `Enter` com a resposta em andamento a cancelava; agora só avisa (parar é `Esc` ou o botão).
- Aprovar o cartão enquanto o texto final ainda chegava misturava a retomada na mensagem anterior (a
  resposta "Feito. Removi…" ficava partida em duas); agora a retomada espera a resposta acabar e abre
  mensagem nova. Teste vermelho sem a correção, verde com ela.
- Revisão de código (10 achados, todos com teste vermelho sem a correção e verde com ela):
  duas chamadas da mesma ferramenta sobrescreviam o chip; `/limpar` e trocar de conversa com resposta em
  andamento deixavam um stream órfão escrevendo na conversa vazia; "Tentar de novo" reenviava o pedido de
  outra conversa; arrastar texto sobre a janela abria o seletor de anexos; a miniatura do anexo ficava na
  memória depois do envio; o modo foco escondia os controles da janela; a busca (`Ctrl+F`) marcava texto
  oculto e perdia o foco; no desktop, parar uma resposta não interrompia o launcher (agora há
  `cancel_command` e geração: um comando velho não escreve no hub do novo).

### O que **não** foi verificado

- O app desktop real (pywebview no Windows): só o shim dos testes; ele continua entrando por token (`get_config`), não pela tela de senha. `get_config`/`open_external`
  e o relay de `tool`/`approval`/`error` em `orion_app.py` têm teste da lógica pura, não do launcher.
- Safari e Firefox; leitor de tela de verdade (NVDA/VoiceOver) — só axe e a árvore de acessibilidade.
- Desempenho da constelação em GPU real: o CI usa WebGL por software; o orçamento medido é de
  quadros desenhados, não de milissegundos.
- Voz ao vivo ponta a ponta (precisa de microfone e `/ws/voice`).

### Limites conhecidos

- No caminho do hub (desktop), "parar" só descarta o resto da resposta: `process_command` não
  cancela o stream no cérebro. No caminho SSE o `fetch` é abortado de verdade.
- O legado (`cerebro_maestro.py`) não serve a interface; o `orion.app` a serve em `/ui/` (útil para abrir no navegador do PC)
  (e `ORION_ALLOWED_HOSTS` com o nome do Tailscale).
- A saudação usa o nome fixo "Antônio" (sai quando houver perfil).

### Como ver

```
uv run python -m tests.front_e2e.mock_backend      # http://127.0.0.1:8000/ui/
```
Mensagens que acionam fluxos: "apague…" (cartão de aprovação), "me lembra…" (ferramenta),
"falha" (erro), "lento" (demora), "xss" (markdown malicioso), qualquer pergunta longa (tabela + código).

## 7. Acabamento (06/10/2026, depois da análise visual com capturas)

Decisão: **só desktop** (fecha a decisão #5 do NUCLEO). Corrigido, cada item com teste em `test_front.py`:

| Achado | Correção |
|---|---|
| Composer mais largo que a coluna de mensagens (832 × 750 px) | `.composer-inner` com a mesma largura útil da coluna: borda esquerda no avatar, direita na bolha do usuário |
| Parágrafo depois de tabela/código/citação colado no bloco | `.prose p { margin: 0 }` (0,1,1) vencia `.prose > * + *` (0,1,0); agora `:where(p)` |
| Pílula "Em espera" permanente, em mono | Some quando o Orion está parado; aparece em `Processando`/`Ouvindo`/`Respondendo` e **"Aguardando aprovação"** (âmbar) enquanto houver cartão pendente |
| Memória: resultados e vizinhos só com título truncado; grafo pequeno e sem texto | Linha extra com tipo · data · nº de ligações; tópicos com rótulo no 3D; câmera mais perto |
| Integrações: nome de arquivo (`mic_engine.py`), status em mono, faixa de altura irregular, Microfone sem explicação | Texto humanizado, faixa com altura mínima igual, dica "Ligar é feito no computador onde o cérebro roda" (não há endpoint para ligar o microfone daqui) |
| Atividade: gráfico esticado com 1–2 pontos | Só desenha a partir de 3 medições; legenda com nº de medições, mín e máx |
| Alto contraste: constelação sumia também na home | Véu total só fora da home |
| `style=""` no `index.html` | Classes utilitárias em `components.css`; teste impede a volta |

**CSP não foi apertada (verificado):** `style-src 'unsafe-inline'` continua porque o `3d-force-graph` injeta um `<style>` em
tempo de execução e o `md.js` emite `style="text-align:…"` nas células de tabela; `connect-src *` e `img-src` seguem porque o
endereço do cérebro é configurável (Tailscale). Tirar isso exige trocar a lib do grafo ou usar hash/nonce por estilo.

**Capturas:** `uv run python -m tests.front_e2e.capturas capturas/` gera 5 telas × 3 temas × 2 tamanhos para revisão visual.
Não há teste de pixel (céu em WebGL e fonte mudam por plataforma); o que é medível está nos testes.

## 8. Conversas: renomear, fixar e apagar (07/10/2026)

A barra lateral do `orion.app` ficava vazia: o front chamava `/sessoes` e `/historico` (contrato do legado) e o app novo não os tinha.
Agora tem, com a mesma forma do legado + os campos novos, todas atrás do login/token (e do bloqueio de outra origem para cookie):

| Rota | O que faz |
|---|---|
| `GET /sessoes?canal=web` | Conversas do canal + as importadas do legado (somente leitura), sem as apagadas; título = o dado, ou a 1ª fala, ou "Nova conversa"; `favorita`, `ultima_atividade`, `ativa` |
| `POST /sessoes` | Nova conversa (se a ativa ainda está vazia, reaproveita: não empilha conversas em branco) |
| `POST /sessoes/ativar` | Torna a conversa a ativa do canal e devolve as mensagens (`user`/`assistant`); importada → 409 |
| `GET /historico?sessao=` | Mensagens de uma conversa (a importada abre só por aqui) |
| `PATCH /sessoes/{id}` | `titulo` e/ou `favorita` |
| `DELETE /sessoes/{id}` | Apaga = **esconde** (`deleted=1`): as mensagens ficam no banco e o que o Orion já consolidou delas continua na memória; vai para o log de auditoria |

- **Esquema v4** (migração automática de banco v3): `sessions.pinned` e `sessions.deleted`.
- **Escopo:** só conversas do canal `web` e importadas. Telegram e outros canais não são alcançáveis por essas rotas (404).
- **Front:** menu ⋯ em cada conversa (aparece com mouse, foco ou menu aberto; teclado completo), grupo "Fixadas", confirmação para apagar
  (foco em "Cancelar"), e na paleta `Ctrl+K`: renomear, fixar e apagar a conversa atual. Conversa importada não tem menu.
- **Legado:** se o cérebro não aceitar `PATCH`/`DELETE`, o erro aparece num aviso legível e a lista não muda.
- **Provado:** `tests/test_app_sessoes.py`, `tests/memory/test_conversas.py`, 10 testes de navegador em `test_front.py` (inclui axe com menu e
  diálogos abertos) e um em `test_orion_real.py` (login real, SQLite real, recarregar a página, mensagens ainda no banco após apagar).

## 9. Conversas e projetos (E3, 10/10/2026)

| Item | Onde | Como funciona |
|---|---|---|
| Arquivadas | `#/arquivadas` (`views/arquivadas.js`), grupo "Arquivadas" da barra lateral → "Gerenciar arquivadas" | `GET /sessoes?arquivadas=true`; busca reaproveita `/sessoes/busca?arquivadas=true` (sem busca no backend, filtra pelo título); Desarquivar = `PATCH {arquivada:false}`; Apagar = `DELETE /sessoes/{id}` depois de confirmar com o título |
| Filtro por projeto | `<select id="sb-projeto">` na barra lateral; `js/conversas.js` (puro, testado em Node) | `todos`, `nenhum` ou o id do projeto, em `prefs.filtro_projeto`; projeto apagado/arquivado vira `todos`; cada conversa de projeto leva um selo (`.conv-projeto`) com nome curto e cor `hsl(matiz)` derivada do nome |
| Mover para projeto | `/projeto <nome>` (`slash.js`), paleta "Mover conversa para projeto…" | `O.projects.moverPorNome` / `moverAtual`; `PUT /projects/sessions/{id}` |
| Disponível em | cartão de cada documento em Fontes | `PATCH /documents/{id}` com `{project_id}`; o texto é reindexado no escopo novo |
| Editar pedido | lápis na bolha do usuário (`aria-label` "Editar pedido (nova versão)") e ‹ n/m › | `POST /historico/{id}/editar` (SSE, igual ao `/chat`); a tela remove a bolha e o que veio depois, mostra o texto novo, e ao fim do turno recarrega o histórico (ids e versões). As setas só trocam o texto exibido (versão antiga em itálico, "só leitura") |

- **Capacidade nova:** `message_edit` (precisa de gateway e login). No backend legado o lápis nem aparece.
- **Bug corrigido na E3.6:** `api.autenticado()` (token **ou** sessão de login). Antes, quem entrava por senha via Projetos e skills vazios.
- **Provado:** `tests/front_e2e/test_etapa3.py` (6 cenários contra o `orion.app` real em processo), `test_orion_real.py::test_prova_e3_...` (login por senha, processo à parte) e `tests/front/{conversas,slash}.test.js`.

