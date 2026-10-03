# ORION — Front-end: brief de design e engenharia

> Direção, sistema de design, arquitetura e orçamentos do front. Criado em 03/10/2026.
> O que cada tela faz e o que foi verificado: seção "Entrega" no fim. Regras de segurança do
> front (markdown, imagem externa, links): [ORION_REGRAS.md](ORION_REGRAS.md) regras 10–12.

## 1. Diagnóstico do front anterior (02/10/2026, com capturas)

| # | Problema | Evidência |
|---|---|---|
| 1 | **Mobile quebrado**: sidebar fixa de 252 px cobre a tela; pílula de status, controles de janela e composer sobrepostos | captura 390×844 |
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
index.html · manifest.webmanifest · orion.svg · orion_app.py (launcher) · ponte.py (regras puras da ponte)
css/    tokens · base · layout · components · chat · views
js/     util · md · sse · store · charts · fuzzy             (puros, UMD, testados em Node)
        core · api · transport · ui                          (infra: DOM, bus, prefs, HTTP, chat SSE/hub, toast/diálogo/menu)
        sky · sound · voice                                  (mídia)
        chat · composer · sidebar · palette · app            (interface, rotas, atalhos, boot)
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
| Mobile | 390 px sem rolagem horizontal; alvos de toque ≥ 44 px |
| Streaming | 1 render por quadro (rAF), não por pedaço |
| Céu fora da home | ≤ 20 fps; pausado com a aba oculta |

## 6. Entrega (03/10/2026)

### O que mudou, tela por tela

| Tela | Antes | Agora |
|---|---|---|
| **Casca** | Sidebar fixa de 252 px cobrindo o celular; toast de "hub desconectado" permanente na web | Sidebar recolhível (`Ctrl+B`) e, no celular, gaveta com foco preso; rotas por hash (`#/chat`…), botão voltar funciona; conexão em linguagem humana, sem alarme |
| **Início** | Campo de texto + constelação | Saudação, chips de estado reais (cérebro, modelo, nº de memórias), sugestões, constelação deslocada para cima, paralaxe e rótulo ao passar o mouse nas estrelas |
| **Chat** | Reparseia o texto todo a cada pedaço; sem tabela/link/realce | Streaming com ≤ 1 render por quadro; tabelas, listas aninhadas, citação, tarefas, código com linguagem, realce e "Copiar"; ferramentas usadas viram chips; **cartão de aprovação** de ação; erro com "Tentar de novo"; "Mais recentes"; parar com `Esc`; anexos (clipe, colar, arrastar) |
| **Memória** | Overlay de grafo sem teclado | Grafo pausado fora da tela; busca por trecho + lista de resultados + painel de detalhes com vizinhos (caminho por teclado); filtros por tipo; aviso explícito quando os dados são de demonstração |
| **Integrações** | Cartões estáticos | Estado real de `/integracoes`, ação por cartão (voz ao vivo, resposta por voz), aviso quando o cérebro não responde |
| **Configurações** | Cartões de altura igual, gráfico quebrado com 1 ponto | Aparência (3 temas, densidade, escala, movimento), voz e sons, modelo, conexão (endereço, token, testar), atividade (CPU/RAM/GPU, latência, cascata, serviços), atalhos, sobre |
| **Paleta (`Ctrl+K`)** | Não existia | Ir para, ações, tema/densidade/escala, conversas (por trecho), "Perguntar ao Orion" |

### O que foi verificado

| Orçamento | Resultado | Onde |
|---|---|---|
| Axe (serious/critical **e** moderate/best-practice) | 0 violações em 5 telas × 3 temas, no chat com resposta + aprovação + erro, com paleta e menu abertos, e na gaveta mobile | `test_axe_*` |
| Console | 0 erros/avisos em todos os 60 testes de navegador (a fixture derruba o teste) | `conftest.py` |
| Teclado | `Ctrl+K`, `Alt+1..5`, `/`, `?`, `Ctrl+B`, `Esc`; telas ocultas `inert`; gaveta com foco preso e foco devolvido | `test_atalhos_*`, `test_paleta_*`, `test_mobile_gaveta_*` |
| Mobile | Sem rolagem horizontal em 320 e 390 px nas 5 telas; alvos de toque ≥ 44 px (`pointer: coarse`) | `test_mobile_*` |
| Streaming | Renderizações ≤ quadros + 2 (a resposta chega em ~100 pedaços) | `test_streaming_rende_*` |
| Céu fora da home | ≤ 22 quadros/s | `test_ceu_fora_*` |
| Segurança | `<script>`, `onerror`, `javascript:` e imagem externa em texto do modelo: nada executa, nada sai do app; CSP com o hash do único script inline (teste falha se o script mudar sem atualizar o hash) | `test_markdown_malicioso_*`, `test_csp_*` |
| Desktop | Caminho `process_command` + hub com shim (eco sem duplicar, ferramenta, modelo, fala vinda do microfone, link por `open_external`, controles da janela) | `test_desktop_*` |
| Lógica pura | 79 testes em Node (md, sse, store, util, fuzzy, charts) | `tests/front/` |
| Ponte e servidor | `ponte.py` (relay de eventos, URL externa, cabeçalho) e `/ui/` do `orion.app` (sem `.py`, sem `__pycache__`) | `tests/legacy/test_ponte.py`, `tests/test_app_ui.py` |

Peso: ~1,1 MB no primeiro carregamento (three.js 600 KB, Inter 48 KB, CSS 80 KB, JS ~200 KB);
o 3d-force-graph (700 KB) só baixa na primeira visita à Memória.

### Achados dos próprios testes (já corrigidos)

- Toda mensagem saía com `\n\n` no fim (montagem do prompt com anexos).
- `Esc` digitado no mesmo quadro em que a paleta abre não fechava.
- Alvos de toque de 30–42 px (botões de ícone, enviar, chip de modelo).
- Níveis de título do markdown pulavam de `h1` para `h4` (axe); `<dl>` com título solto.
- Menu de modelo cobria o clipe e o texto digitado.
- `Enter` com a resposta em andamento a cancelava; agora só avisa (parar é `Esc` ou o botão).
- Aprovar o cartão enquanto o texto final ainda chegava misturava a retomada na mensagem anterior (a
  resposta "Feito. Removi…" ficava partida em duas); agora a retomada espera a resposta acabar e abre
  mensagem nova. Teste vermelho sem a correção, verde com ela.

### O que **não** foi verificado

- O app desktop real (pywebview no Windows): só o shim dos testes. `get_config`/`open_external`
  e o relay de `tool`/`approval`/`error` em `orion_app.py` têm teste da lógica pura, não do launcher.
- Safari e Firefox; leitor de tela de verdade (NVDA/VoiceOver) — só axe e a árvore de acessibilidade.
- Desempenho da constelação em GPU real: o CI usa WebGL por software; o orçamento medido é de
  quadros desenhados, não de milissegundos.
- Voz ao vivo ponta a ponta (precisa de microfone e `/ws/voice`).

### Limites conhecidos

- No caminho do hub (desktop), "parar" só descarta o resto da resposta: `process_command` não
  cancela o stream no cérebro. No caminho SSE o `fetch` é abortado de verdade.
- O legado (`cerebro_maestro.py`) não serve a interface; o celular usa o `orion.app` em `/ui/`
  (e `ORION_ALLOWED_HOSTS` com o nome do Tailscale).
- A saudação usa o nome fixo "Antônio" (sai quando houver perfil).

### Como ver

```
uv run python -m tests.front_e2e.mock_backend      # http://127.0.0.1:8000/ui/
```
Mensagens que acionam fluxos: "apague…" (cartão de aprovação), "me lembra…" (ferramenta),
"falha" (erro), "lento" (demora), "xss" (markdown malicioso), qualquer pergunta longa (tabela + código).
