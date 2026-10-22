---
type: recurso
projeto: "[[Orion]]"
tags: [orion, ui, design, iced, wgpu, especificacao, prototipo]
date: 2026-10-17
resumo: "Especificação completa da interface do Orion em iced/wgpu para reproduzir exatamente o protótipo 'Orion iced Mockup': janela, tokens, tipografia, componentes, barra lateral, barra superior, início, chat, painel da direita, todas as telas, grafo da Memória, editor, Configurações (14 seções), paleta, captura rápida, 'Isso?', mini barra de voz, atalhos, estado, animações, responsivo, mapeamento para crates e fases, conflitos com decisões posteriores e critérios de aceite."
---

# ORION - UI (ESPECIFICAÇÃO)

> Projeto: [[Orion]] · Planejamento geral: [[ORION - PLANEJAMENTO ARQUITETURA]] · Plano de execução: [[ORION - PLANO DE IMPLEMENTAÇÃO]] (fases F01, F05, F06, F07, F08, F09) · Irmã desta nota: [[ORION - INSTALADOR (ESPECIFICAÇÃO)]] · Referências: [[ORION - REFERÊNCIAS DE PESQUISA]] · Conceito: [[Rust]]
> **Fonte da verdade visual:** o protótipo HTML `Orion iced Mockup` (15–16/10/2026), guardado neste vault em `01 Projetos/Orion/Protótipos/orion-ui-mockup.html` (abrir no navegador; qualquer senha entra).
> Regra: **a interface em iced tem que ficar idêntica ao protótipo.** Em divergência entre esta nota e o HTML, **vale o HTML**, exceto nos itens da §2 (decisões posteriores ao protótipo, que vencem). Corrigir a nota.
> Esta nota descreve o que o protótipo **faz e mostra**; o protótipo não chama modelo nenhum, os dados são de exemplo. O comportamento real vem das fases do plano.

---

## Estado da implementação — atualizado em 20/10/2026 (PR #30)

Estado canônico: [ORION.md](ORION.md); tarefas F01/F05/F07/F16 em [ORION_PLANO_IMPLEMENTACAO.md](ORION_PLANO_IMPLEMENTACAO.md).

- [x] Protótipo HTML e especificação das telas presentes no repositório.
- [x] Instalador iced com céu.
- [x] **App principal `orion-app` (iced) existe:** casca, barra lateral, barra superior, Início, chat com aprovação, painel da direita (4 abas), 10 telas de página, Configurações (14 seções), editor, paleta Ctrl K, captura rápida, "Isso?", mini barra, legenda, popovers e toasts; 47 cenas em `orion-snap`. Ligado ao núcleo no mesmo processo (`orion.exe`, sem porta); painéis MCP, Skills, Modelos locais e Segundo plano ligados; bandeja, mini barra e atalhos (Ctrl+Alt+Espaço, Ctrl+Alt+P).
- [~] Telas com dados de exemplo ou parciais: Painel (métricas), Resultados, Integrações, Conhecimento/Fontes e partes de Configurações.
- [ ] Editor/vault automático, painel de Arquivos/Diff por bloco/Terminal ConPTY e fluxos finais de projeto.
- [ ] Grafo nativo wgpu/compute (hoje 2D), bloom/blur, timeline 3D, acessibilidade equivalente e metas de desempenho.
- [ ] Fluent pt/en compartilhado, snapshots/E2E iced comparados ao protótipo.
- [ ] Prova num Windows real de tudo acima.

## 1. Escopo
- Tudo que existe na janela principal do Orion depois do login: barra lateral, barra superior, Início, Chat, painel da direita (Contexto / Arquivos / Diff / Terminal), Projetos, Memória (grafo), Aprovações, Rotinas, Resultados, Nuvem, Fontes, Conhecimento, Painel, Vida, Arquivadas, Configurações, paleta Ctrl K, editor de nota, captura rápida, "Isso?", mini barra de voz, legenda ao vivo, popovers e toasts.
- **Fora desta nota:** instalador (nota própria), onboarding do primeiro uso, tela de login final (ver §2, U1), mini tela de atualização.
- O motor do céu é o mesmo do instalador; constantes em [[ORION - INSTALADOR (ESPECIFICAÇÃO)]] §8. Aqui só se registra **como o mockup o usa**.

---

## 2. Conflitos entre o mockup e as decisões posteriores (as decisões vencem)
| # | O mockup faz | Decisão em vigor | Resolução na implementação |
|---|---|---|---|
| U1 | **(Atualização 20/10/2026: a entrada segue o instalador: céu à esquerda, painel à direita com Usuário e Senha, "Bem-vindo de volta"; com banco cifrado a mesma senha abre o banco. Falta avatar, Hello e o fluxo 2FA final.)** **Entrada:** cartão simples (cinturão grande, "Orion", texto, campo Senha, Entrar), qualquer senha entra | **Login com avatar, perfil local, céu animado, 2FA pelo Telegram só ao reentrar após sair da conta, Windows Hello de reserva** (rodada 8) | Implementar o login da decisão (F02.B). O cartão do mockup (§8 desta nota) fica só como **fundo/medidas de referência** do céu e do campo de senha |
| U2 | Fontes **Geist / Geist Mono / Newsreader** | O plano F01.02 diz Inter + JetBrains Mono; a nota de arquitetura (seção de design) diz Geist | **Vale o mockup: Geist (400/500/600), Geist Mono (400/500), Newsreader (400/500/600 + itálico 400).** Corrigir F01.02 no plano. Todas embutidas no binário |
| U3 | **(Decidido 20/10/2026: Ctrl+Alt+Espaço para a mini barra e Ctrl+Alt+P para o pânico; Win+combinações são reservadas pelo Windows.)** Atalho global "**Win + O**" (Configurações > Sistema, atalhos, tooltip) | **Win + Shift + Espaço** (Win+O é do Windows) | Trocar o texto e a ação. Tudo que o mockup chama de "Win + O" vira "Win + Shift + Espaço" |
| U4 | **(Feito em 20/10/2026: captura rápida Ctrl+Alt+O e "Isso?" Ctrl+Alt+I, junto de Ctrl+Alt+Espaço e Ctrl+Alt+P.)** Captura rápida e "Isso?" rotulados "Win + Shift + O/I" mas o código escuta **Alt+Shift+O/I** (limitação do navegador) | **Win + Shift + O** e **Win + Shift + I** globais | Implementar Win+Shift+O/I via `global-hotkey` |
| U5 | "Nova conversa" = **Ctrl Shift O** | Mantém | Igual |
| U6 | "Painel" dentro de **Mais** (a "Vida" foi cortada em 21/10/2026); "Buscar" não é item da barra lateral | F01.19 lista Nova conversa, Buscar, Projetos, Aprovações, Rotinas, Memória, **Painel** na lista principal | **Vale o mockup** (Painel em Mais; busca é o campo no topo + Ctrl K). Ajustar F01.19. a "Vida" não aparece mais (cortada em 21/10/2026) |
| U7 | Hex dos tokens `line` etc. em **alfa branco** (`rgb(255 255 255 / .065)`) | Plano F01.01 usa `line #25272c` | Usar os valores do mockup (§4). `#25272c` ≈ alfa .065 sobre `#151618`; no instalador continua `#25272c` |
| U8 | Seletor de provedor: Auto, Claude, Codex, Gemini, Orion | Mantém (rodadas 2–6) | Igual |
| U9 | Aprovação "Aprovar / Negar / Ver o diff" | Mantém; sem contagem regressiva (removida) | Igual |
| U10 | Cotas de exemplo (Gemini, Groq, Cerebras, OpenRouter, Mistral) | Ordem real configurável (F03.E) | Dados reais no lugar dos de exemplo; layout igual |
| U11 | "Fontes" e "Conhecimento" são telas de **Mais** e também seções de Configurações (Fontes) | Mantém | Igual |
| U12 | Configurações com **2 grupos** (Configurações / Este computador) e 14 seções | Plano F07 prevê grupos App/IA/etc. | **Vale o mockup** para a navegação; conteúdo detalhado das seções vem da F07. Ajustar F07.A |
| U13 | Zoom da interface por CSS `zoom` (90–150%) | Escala nativa do iced | Aplicar `scale_factor` do iced; mesmos 5 degraus |
| U14 | Grafo em **Canvas 2D** (protótipo) | **3D em wgpu** com layout por compute shader | O 3D substitui o 2D; **toda a interação, filtros, busca, detalhes e linha do tempo da §17 permanecem** |
| U15 | Navegador integrado em `sys.browser` (existe no estado, sem controle na tela) | WebView2 isolado sob demanda | Sem controle visível na v1 |
| U16 | Criar projeto e renomear são avisos do protótipo, sem formulário funcional | Ligação ao núcleo da v1 (F08.02/F05.18), rodada Codex de 21/10/2026 | Formulário inline em Projetos (nome, instruções e pasta existente opcional) e no rodapé do Chat (título). Foco no campo inicial, espera/erro/cancelamento. Ações ainda desconectadas ficam desabilitadas ou avisam indisponibilidade no app real; o demo mantém simulações. **Implementação parcial; comparação visual e aceite Windows pendentes**, sem afirmar aprovação de um novo desenho final |

---

## 3. Janela e grade
| Propriedade | Valor |
|---|---|
| Janela | Sem moldura do Windows; barra de título própria **integrada à barra superior** (altura `--bar-h` = **46 px**). Cantos 12 px (DWM). Mica opcional na barra lateral (Configurações > Sistema > "Fundo Mica") |
| Tamanho | Livre, **mínimo 760 × 520** (abaixo de 760 de largura entra o modo gaveta, §27). Lembrar tamanho, posição, monitor |
| Grade principal | `grid-template-columns: 272px minmax(0,1fr)` → barra lateral (`--side-w` 272) + coluna principal. Recolhida: `0` + `1fr` (a barra lateral some, `visibility:hidden`) |
| Coluna principal | Flex vertical: **barra superior 46 px** + **corpo** (`flex:1`) |
| Corpo | `grid` de 1 coluna; com painel da direita (`.with-ctx`): `minmax(0,1fr) 320px` (`--ctx-w` 320). O painel só existe no Chat e some abaixo de **1120 px** |
| Palco (`.stage`) | `position:relative; min-width:0; min-height:0`; telas rolam dentro dele (`.scroll` = `absolute inset:0; overflow-y:auto`) |
| Camadas (z-index) | gaveta 50 · scrim da gaveta 45 · modais/scrim 60 · mini barra de voz 85 · captura rápida 88 · "Isso?" 87 · legenda 84 · popover 90 · toast 95 · tela de entrada 80 · resultados de busca do grafo 6 · detalhes do grafo 5 · dica do grafo 4 · caixa de sugestões 7 |
| Rolagem | Barra fina (`scrollbar-width: thin`, cor `--line-2`) na lista de conversas |
| Overflow | `body{overflow:hidden}`: só áreas internas rolam |

### 3.1 Densidade e escala
- `--row`: **32 px** (Confortável) ou **26 px** (Compacta; classe `compact` no corpo). Na compacta, `.conv` passa de padding `6px 10px` para `3px 10px`.
- Escala da interface: **90 / 100 / 115 / 130 / 150 %** (padrão 100). Atalhos `Ctrl +` e `Ctrl −` andam um degrau e mostram toast "Escala N%". Persistir.
- Movimento "Reduzido" (ou preferência do sistema): remove **todas** as transições e animações, desliga o texto em tempo real (resposta aparece inteira) e a rotação do grafo.

---

## 4. Tokens (tema único Grafite escuro)
Sem tema claro e sem seletor de tema. `color-scheme: dark`.

### 4.1 Cores
| Token | Valor | Uso |
|---|---|---|
| `bg` | `#151618` | fundo do palco, janelas modais, campos "fundo" |
| `side` | `#111214` | barra lateral, painel da direita, blocos de código, editor (área do texto), cartão do céu |
| `raise` | `#1b1c1f` | composer, cartões, popovers, campos, paleta |
| `raise-2` | `#222327` | bolha do usuário, avatar, anexo, código inline, fundo de medidores |
| `sel` | `#26272b` | item selecionado, botão segmentado ativo, `aria-pressed` |
| `hover` | `#1e1f22` | hover geral, fundo do campo da busca lateral |
| `line` | `rgb(255 255 255 / .065)` | divisórias, bordas de cartões |
| `line-2` | `rgb(255 255 255 / .11)` | bordas de campos, botões, composer |
| `line-3` | `rgb(255 255 255 / .20)` | foco/hover de bordas, switch desligado |
| `fg` | `#ececee` | texto principal |
| `fg-2` | `#a9aab0` | texto secundário |
| `fg-3` | `#75767d` | legenda, ícones inativos, placeholders |
| `fg-4` | `#4e4f55` | desativado, "via" discreto |
| `primary` | `#b9c6ea` | botão principal, botão enviar, toast |
| `primary-ink` | `#0f1320` | texto sobre `primary` |
| `accent` (Rigel) | `#a4b3de` | foco, seleção, switch ligado, links, ponto de "resposta nova" |
| `accent-soft` | `rgb(164 179 222 / .12)` | seleção de texto, selo "info" |
| `signal` (Betelgeuse) | `#e9b467` | atenção, pendência, "aguardando", contador |
| `signal-soft` | `rgb(233 180 103 / .11)` | fundo de aprovações e contador |
| `ok` | `#74cfae` | sucesso (somente isso) |
| `ok-soft` | `rgb(116 207 174 / .11)` | fundo de selo "ok" |
| `danger` | `#f07c88` | destrutivo e erro (somente isso) |
| `danger-soft` | `rgb(240 124 136 / .11)` | fundo de erro |
| `star` | `#eef0f6` | estrelas do céu |
| `scrim` | `rgb(6 6 8 / .6)` | fundo escurecido dos modais |
| sombra | `0 24px 60px rgb(0 0 0 / .5), 0 2px 10px rgb(0 0 0 / .28)` | popovers, modais, toasts |
| ponto da câmera | `#ff4d5e` | só o ponto da pílula "Câmera" |
| terminal | fundo `#0b0c0d`, texto `#cfd2d8` | aba Terminal |
| mini barra | fundo `#0b0c10` | ilha de voz |
| legenda ao vivo | fundo `rgb(0 0 0 / .78)`, texto `#fff` | |
| grafo (palco) | `radial-gradient(ellipse at 50% 45%, #15192a 0%, #0b0c10 72%)` | |
| arestas do grafo | `#9aa3c0` | |

### 4.2 Cores dos tipos de nota (grafo, legenda, filtros, pontos)
| Tipo | Token | Valor | Rótulo plural (filtro) | Rótulo singular |
|---|---|---|---|---|
| projeto | `n-projeto` | `#e9b467` | Projetos | Projeto |
| area | `n-area` | `#d98f6a` | Áreas | Área |
| conceito | `n-conceito` | `#9fb2ea` | Conceitos | Conceito |
| pesquisa | `n-pesquisa` | `#82cfa6` | Pesquisas | Pesquisa |
| recurso | `n-recurso` | `#a7abb6` | Recursos | Recurso |
| conversa | `n-conversa` | `#e796bf` | Conversas | Conversa com IA |
| relatorio | `n-relatorio` | `#6a6e78` | Relatórios | Relatório |

### 4.3 Medidas
| Token | Valor |
|---|---|
| `--side-w` | 272 px |
| `--ctx-w` | 320 px |
| `--bar-h` | 46 px |
| `--row` | 32 px (26 compacta) |
| `--thread-w` | 46rem (Médio); 40rem (Estreito); 58rem (Largo) |
| `--chat-fs` | 16.5 px (Médio); 15 (Pequeno); 18.5 (Grande) |
| `--ease` | `cubic-bezier(.16, 1, .3, 1)` (animações de UI) |
| Raios | botão/ícone 8 · campo 8–10 · chip 7 · cartão 12–14 · composer 18 · modal 18 · paleta 16 · pílula/selo 999 · ponto 50% |

### 4.4 Estados padrão
- **Foco visível:** contorno `1.5 px --accent`, afastado 2 px, raio 8.
- **Seleção de texto:** fundo `accent-soft`.
- **Desativado:** opacidade .3 (botão de ícone) ou `fg-4` sobre `raise-2` (botão enviar).
- **Hover** de itens clicáveis: fundo `hover`, texto `fg`. Transição de fundo/cor **150 ms**.

---

## 5. Tipografia
| Papel | Família | Tamanho / peso / altura |
|---|---|---|
| Interface | **Geist** (fallback Segoe UI) | base **14 px / 400 / 1.5** |
| Respostas do chat | **Newsreader** (escolhível: Newsreader, Geist, fonte do sistema) | `--chat-fs` / 400 / **1.65**; negrito 600 |
| Títulos de página (`h1`) | Newsreader | **32 / 400 / 1.1**, espaçamento −.015em |
| Saudação do Início | Newsreader | `clamp(34px, 5vw, 46px) / 1.08`, −.02em; nome em *itálico* `fg-2` |
| Título de configuração (`.set-title`) | Newsreader | 26 / 400 / 1.15, −.01em |
| Título de cartão de projeto | Newsreader | 20 / 400 / 1.2 |
| Título do vazio | Newsreader | 28 / 400 / 1.15 |
| Número grande (RAM) | Newsreader | 36 / 400 / 1, tabulares |
| Número de estatística | Newsreader | 30 / 400 / 1.1, tabulares |
| Seção da configuração (`.set-h`) | Geist | 13 / 600 |
| Seção da lista (`.h3`) | Geist | 11.5 / 500, +.02em, `fg-3` |
| Seção da barra lateral (`.sec`) | Geist | 11.5 / `fg-3` |
| Eyebrow | Geist | 11 / 500, +.14em, MAIÚSCULAS, `fg-3` |
| Código | **Geist Mono** (fallback Cascadia Code) | 12 px (`.mono`); blocos 12.5/1.65; inline `.78em` |
| `kbd` | Geist Mono | 10.5, borda `line-2`, raio 5, padding 1×5 |
| Rótulos de linha | Geist | título 13.5; descrição 12.5 `fg-3` (máx. 54 caracteres de largura) |
| Números | `font-variant-numeric: tabular-nums` em contadores, horas, tokens, estatísticas, eixos |

Notas:
- Texto do **usuário** no chat: 15 px Geist. Texto do **campo de mensagem**: 15 px / 1.55.
- Frase de status, legendas e rótulos pequenos: 11–12.5 px.
- `text-wrap: balance` na saudação.

---

## 6. Componentes base
Todos reutilizáveis (`orion-ui`). Medidas exatas.

| Componente | Especificação |
|---|---|
| **Ícone** | SVG 24×24, traço `currentColor`, **espessura 1.55**, pontas e junções arredondadas, sem preenchimento. Tamanho padrão **16 px**; pequeno (`i-sm`) **14 px**. Os 70+ caminhos estão no objeto `P` do HTML e viram assets SVG embutidos |
| **Cinturão (marca)** | 3 pontos de 4 px, `gap 3`, cor `fg`; o primeiro sobe 3 px e o terceiro desce 3 px (diagonal). Versão grande (`lg`): pontos de 7 px, `gap 6`, deslocamento 6 px |
| **Botão de ícone `.ib`** | 30×30, raio 8, cor `fg-3`; hover fundo `hover` + `fg`; pressionado (`aria-pressed`) fundo `sel` + `fg`; desativado opacidade .3. Variantes: 26×26 (ações da bolha), 28×28 (ações da resposta), 34 de largura (controles de janela) |
| **Botão `.btn`** | altura **32**, padding 0×12, raio 8, borda 1 px `line-2`, texto 13/500; hover fundo `hover`, borda `line-3`. **Pequeno `btn-sm`:** altura 28, padding 0×10, 12.5 px. **Primário:** fundo `primary`, texto `primary-ink`, sem borda, hover `brightness(1.06)`. **Perigo:** texto `danger`, borda `danger` 40%, hover fundo `danger-soft` + borda `danger`. **Fantasma:** sem borda, texto `fg-2`, hover `fg` |
| **Botão Fechar (janela)** | hover fundo `danger`, texto branco |
| **Chip `.chip`** | altura 28, padding 0×8, raio 7, 12.5 px, `fg-2`; hover `hover`. Parte suave (`.soft`) em `fg-3` |
| **Pílula `.pill`** | altura 26, padding 0×10, raio 999, borda `line-2`, 12 px; ponto de 6 px à esquerda (`fg-3`). Variações em §10 |
| **Selo `.badge`** | padding 1×8, raio 999, borda `line-2`, 11/500, `fg-2`. `ok` (verde sobre `ok-soft`), `warn` (âmbar), `err` (vermelho), `info` (Rigel), sem borda nas coloridas |
| **Segmentado `.seg`** | borda `line-2`, raio 10, padding 2, `gap 2`; botão altura 28, padding 0×12, raio 8, 12.5 px `fg-3`; marcado: fundo `sel`, `fg`. Papel `radiogroup`/`radio` |
| **Switch `.sw`** | 34×20, raio 999; desligado `line-3`; ligado `accent`; bolinha 16 px branca com sombra `0 1px 2px rgb(0 0 0 / .3)`; desliza **14 px** em 200 ms (`--ease`); papel `switch` |
| **Seletor `.sel`** | altura 32, padding 0×8×0×12, raio 8, 13 px, seta `chevD` 14 px `fg-3`; hover `hover`. **Caixa (`box`):** borda `line-2`, largura mínima 200, rótulo à esquerda e seta à direita |
| **Campo `.input`** | altura 32, largura 220 (máx. 100%), padding 0×10, raio 8, borda `line-2`, fundo `raise`, 13 px; foco borda `line-3` |
| **Área de texto `.ta`** | largura 100%, altura mínima 70, resize vertical, padding 10×12, raio 10, borda `line-2`, fundo `raise`, 13 px/1.5 |
| **Busca `.search`** | ícone de lupa 14 px a 11 px da esquerda; campo altura 34, padding 0×12×0×34, raio 9, borda `line-2`, fundo `raise`, 13 px |
| **Cartão `.card`** | borda `line`, raio 12, fundo `raise`. Cabeçalho: padding 10×12, 12.5/500, valor à direita `fg-3` tabular. Corpo: padding 0×12×12, `gap 7`, 12.5 `fg-2` |
| **Linha de lista `.li`** | `grid 1fr auto`, `gap 16`, padding 13×4, borda inferior `line`; título 14 (flex, `gap 8`, quebra), descrição 12.5 `fg-3` (margem 1), lado direito 12 `fg-3` tabular, alinhado à direita |
| **Linha de configuração `.row`** | flex, `gap 24`, `space-between`, padding 15×0, borda inferior `line`; título 13.5; descrição 12.5 `fg-3` máx. 54ch; controles à direita (`gap 8`, não encolhem) |
| **Medidor `.meter`** | altura 3, raio 3, fundo `raise-2`; barra `fg-2` (vermelha `danger` a 100%). `hbar` (barra horizontal): altura 5 |
| **Barra horizontal `.hbar`** | grid `140px 1fr 70px`, `gap 12`, padding 7×4, 13 px `fg-2`; valor à direita `fg-3` tabular |
| **Etiqueta removível `.tagx`** | altura 26, padding 0×6×0×10, raio 999, borda `line-2`, 12.5 `fg-2`; botão × 14 px |
| **Quadradinhos de execuções `.runs`** | 7 quadrados 8×8, raio 2, `gap 3`: verde `ok`, vermelho `danger` (falha `f`), cinza `fg-4` (pulada `s`) |
| **Ponto de estado `.cdot`** | 6 px. `pending`: `signal` com halo `0 0 0 3px signal-soft`. `unread`: `accent` |
| **Girando `.spin`** | 12 px, borda 1.5 `line-3` com topo `fg`, rotação 0,9 s linear infinita |
| **`kbd`** | ver §5 |
| **Popover `.pop`** | posição fixa, largura mínima 240, máx. `100vw−32`, altura máx. `100vh−24`, padding 4, raio 12, fundo `raise`, borda `line-2`, sombra; entra com `rise` 160 ms. Itens: padding 7×10, raio 8, 13 px, ícone 14 + texto + (descrição 11.5 `fg-3`) + (✓ à direita). Grupo: 11 px `fg-3`, padding 8×10×3. Divisor 1 px `line`. Perigo em `danger`. Setas ↑↓ andam, Esc fecha |
| **Toast** | centro inferior (`bottom 20`), padding 8×14, raio 10, fundo `primary`, texto `primary-ink`, 13 px, máx. `100vw−32`; **some em 2,6 s**; um por vez (o novo substitui o anterior) |
| **Scrim** | tela inteira, `scrim`, centraliza, padding 24; entra com `fade` 180 ms |

Posicionamento do popover: abaixo do âncora (+6); se estourar embaixo, vai para cima; horizontal alinhado à esquerda (ou à direita, `align=right`) e preso a 12 px das bordas.

---

## 7. Ícones usados (nome → uso)
`menu` gaveta · `panelL` recolher/abrir barra lateral · `panelR` painel da direita · `back`/`fwd` histórico · `search` · `plus` (nova conversa, anexar, zoom) · `minus` · `folder` (Projetos, projeto) · `result` (Resultados) · `chevD`/`chevR` · `graph` (Memória) · `gear` · `mic` · `wave` (voz ao vivo) · `copy` · `speaker` · `retry` · `close` · `check` · `up` (enviar, subir) · `pin` · `user` · `shield` (Aprovações, privacidade) · `gauge` (Painel, Uso) · `plug` · `db` (Fontes, locais) · `cpu` (modelo, sistema) · `keys` (atalhos) · `logout` · `trash` · `edit` · `bell` · `branch` · `file` · `book` (Conhecimento) · `archive` · `min`/`max` · `fit` · `refresh` · `chat` · `clock` (Rotinas) · `inbox` (captura) · `cloud` (Nuvem) · `send2` (Telegram) · `camera` · `out` (saiu do PC) · `term` · `diff` · `play` · `image` · `moon` · `eye` ("Lidas agora") · `globe` · `bolt` (autônomo) · `zap` · `undo` · `scan` ("Isso?") · `heart` (Vida) · `puzzle` (Conectores). Ícones do Lucide podem substituir, desde que mantenham espessura e tamanho.

---

## 8. Tela de entrada do mockup (referência; ver U1)
- Tela cheia `fixed`, fundo `bg`, **céu em canvas** (240 estrelas, seed 7, §26) sem constelação de linhas animada (no mockup é estática: 7 pontos e 6 linhas).
- Cartão central de **340 px**, coluna com `gap 14`: cinturão grande (margem inferior 6) · título "Orion" **Newsreader 40/1** (−.02em) · texto 13.5 `fg-2` "Entre com a senha deste computador. Nada sai daqui sem a sua aprovação." · campo **42 px** (raio 10, padding 0×14, fundo `raise` a 85% com blur 6, borda `line-2`, foco `line-3`) com placeholder "Senha" · linha de erro 12 `danger` (mín. 18 px) · botão primário **42 px** "Entrar" · nota 12 `fg-3`.
- Erro sem senha: "Digite a senha para entrar." Entrar → vai ao Chat da conversa "Orion RAM otimização" e, 300 ms depois, toast "git pull do vault: 3 notas novas".
- **No app real:** avatar no centro, nome, campo de senha, 2FA do Telegram quando aplicável, Windows Hello de reserva, céu animado do instalador. O comportamento "senha volta a ser pedida quando o Windows é bloqueado (Win + L)" permanece.

---

## 9. Barra lateral (272 px)
Fundo `side`, borda direita 1 px `line`, padding `0 8px 8px`, overflow oculto, flex vertical.

### 9.1 Topo (altura 46)
`[painel][cinturão + "Orion"] ─ grow ─ [voltar][avançar]`
- Recolher (`panelL`), tooltip "Recolher (Ctrl B)".
- Marca: cinturão + "Orion" 15/500, +.01em, padding 0×8, `gap 10`.
- **Voltar/avançar:** desativados quando a pilha está vazia. A pilha guarda `{tela, conversa}`; nova navegação limpa o "avançar".
- Em modo gaveta (≤760) o primeiro botão vira **× (Fechar menu)** e voltar/avançar somem.

### 9.2 Busca
Campo "Procurar nas conversas" (altura 32, raio 8, fundo `hover`, sem borda; foco: borda `line-2` e fundo `bg`). Filtra a lista em tempo real por título, projeto e texto das mensagens. Sem resultado: "Nenhuma conversa com “…”." (13 px `fg-3`).

### 9.3 Navegação (itens de altura `--row`, `gap 1`)
| Item | Ícone | Detalhe |
|---|---|---|
| Nova conversa | `plus` | `kbd` "Ctrl ⇧ O" aparece **só no hover** |
| Projetos | `folder` | |
| Memória | `graph` | |
| Aprovações | `shield` | contador âmbar (`signal`, mín. 18×18, raio 9) com o total de pendências (conversas + proposta de rotina) |
| Rotinas | `clock` | ponto Rigel de 6 px quando há "resultado novo" |
| **Mais** (expansível) | `chevR` (gira 90° em 200 ms) | `aria-expanded`; estado persistido |
| ↳ Resultados | `result` | |
| ↳ Nuvem | `cloud` | `spin` quando há tarefa rodando |
| ↳ Fontes | `db` | |
| ↳ Conhecimento | `book` | |
| ↳ Painel | `gauge` | |
| ↳ Arquivadas | `archive` | |
- Item: padding 0×10, `gap 11`, raio 8, 13.5 px `fg-2`; hover `hover`/`fg`; atual: fundo `sel`/`fg`, `aria-current="page"`.

### 9.4 Lista de conversas (rolável, margem −8 para a barra encostar)
- Grupos, nessa ordem: **Fixadas**, **Hoje**, **Ontem**, **7 dias**, **Mais antigos**. Grupo vazio não aparece. Título do grupo: `.sec` (margem 18×10×4).
- Item `.conv`: grid `1fr auto`, `gap 8`, padding 6×10 (3×10 compacta), raio 8.
  - Linha 1: [ícone de origem 12 px: relógio = Rotina, avião = Telegram] + título (13.5, uma linha, reticências).
  - Linha 2: projeto (11.5 `fg-3`) + " · Rotina" ou " · via Telegram".
  - Direita (centralizado nas 2 linhas): indicador: ponto âmbar com halo = **aguardando aprovação**; `spin` = **trabalhando**; ponto Rigel = **resposta nova**.
  - Atual: fundo `sel`. Abrir uma conversa com estado "nova" limpa o estado.

### 9.5 Rodapé
Borda superior `line`, padding-top 8. `[avatar 26 + "Antônio · PC principal" + chevD]` (altura 40, abre menu da conta) e `[engrenagem]` (Configurações, tooltip "Configurações (Ctrl ,)").
- Avatar: círculo 26, fundo `raise-2`, borda `line-2`, iniciais **AS** 10.5/600.
- Nome: o **apelido** (Configurações > Conta) + " · PC principal" em `fg-3` 12 px.

### 9.6 Menu da conta (popover)
Cabeçalho (nome completo + "PC principal · notebook sincronizado") · divisor · Configurações · Privacidade e autonomia · Atalhos · divisor · **Sair**.

---

## 10. Barra superior (46 px)
Flex, `gap 4`, padding `0 6px 0 12px`. Da esquerda para a direita:

1. (Gaveta) botão `menu` "Abrir menu".
2. (Barra lateral recolhida) botões `panelL` "Abrir barra lateral" e `plus` "Nova conversa".
3. **Título:** no Chat, botão com o **título da conversa** + `chevD` (abre menu da conversa); nas demais telas, o nome da tela (sem seta); no Início, vazio.
4. Espaço flexível.
5. **Pílulas** (da esquerda): 
   - **Câmera** (se presença contínua ligada): borda vermelha 45%, ponto `#ff4d5e`; clica e abre Configurações > Câmera. Tooltip "Câmera ativa (presença)".
   - **Autônomo** (se ligado): borda Rigel 45%, ponto Rigel; clica e desliga. Tooltip "Modo autônomo ligado. Clique para desligar."
   - **Estado exclusivo** (só uma): "**Pensando**" (ponto âmbar **pulsando**, opacidade 100→30% em 1,2 s) enquanto responde · "**Ouvindo**" (ponto verde) com voz ao vivo · "**Aguardando você · N**" (borda âmbar 40%, texto `fg`) quando há pendência (clica e abre Aprovações).
6. **Captura rápida** (`inbox`), tooltip "Captura rápida (Win + Shift + O)".
7. **Isso?** (`scan`), tooltip "Isso? Captura a tela e pergunta (Win + Shift + I)".
8. **Buscar ⟨Ctrl K⟩** (`search` + texto + `kbd`), 12.5 px `fg-3`, altura 30.
9. **Painel da direita** (`panelR`, `aria-pressed`) — **só no Chat**.
10. Separador vertical (1×18, margem 0×6).
11. **Controles da janela** (34 de largura): minimizar ("Minimizado para a bandeja. Win + Shift + Espaço abre de novo."), tela cheia, fechar ("Fechar esconde na bandeja. As rotinas continuam.").
- Itens 6–8 e 10–11 somem em modo gaveta (≤760).
- A barra inteira é área de arrastar a janela (exceto controles).
- Título da janela do sistema: título da conversa + " · Orion" no Chat; senão "Orion".

---

## 11. Início (constelação)
- Palco com **canvas do céu** em tela cheia (alfa .55; **sem** as linhas animadas do instalador, só estrelas e a constelação estática do mockup: 7 pontos e 6 linhas, escala 92% do menor lado, centrada). Com **voz ao vivo ligada**, as três estrelas do Cinturão acendem em **verde `ok`**, raio ×1.6 e halo mais forte.
- Conteúdo centralizado verticalmente, largura **660** (`home-in`), `gap 12`, padding vertical 40/56, animação `fadein` 220 ms (opacidade + 3 px).
- Ordem:
  1. **Eyebrow:** cinturão + "SEU ESPAÇO COM ORION" (centro, `gap 10`).
  2. **Saudação:** "**{Bom dia|Boa tarde|Boa noite}**, *{apelido}*." — regra: <5 h Boa noite; <12 Bom dia; <18 Boa tarde; senão Boa noite. Margem 4×0×22.
  3. **Composer** (§12) com placeholder "Pergunte ou peça algo ao Orion. / para comandos, @ para citar uma nota".
  4. **Linha de estado** (centro, `gap 6 16`, 12 px `fg-3`): ● "Conectado" · `cpu` {modelo} · `graph` "{N} notas no vault" · `clock` "Backup do vault às 14:00".
  5. **Sugestões** (pílulas altura 34, padding 0×14, borda `line-2`, fundo `bg` 70%, 13 px; hover borda `line-3`/`fg`), margem-topo 18:
     - (se houver pendência) ponto âmbar + "**{título da conversa} espera você**" (borda âmbar 40%) → abre a conversa;
     - `bell` "Ler o briefing da manhã" → abre a conversa de briefing;
     - `book` "Resumir o vault da semana" → preenche o campo com "Resuma o que mudou no vault esta semana";
     - `graph` "Explorar a memória" → Memória.
  6. A caixa de sugestões de `/` e `@` abre **abaixo** do composer no Início (acima no Chat).
- Ao abrir o Início, o campo recebe foco (não em modo gaveta).

---

## 12. Composer (campo de mensagem)
Estrutura: `[caixa] + [linha de metadados]`.

**Caixa (`.composer`):** raio **18**, borda `line-2`, fundo `raise`, sombra `0 10px 30px rgb(0 0 0 / .18)`; `:focus-within` → borda `line-3`.
- **Textarea:** `rows=1`, altura mínima 54, **máx. 220** (cresce com o texto, depois rola), padding 16×18×4, 15 px/1.55, sem resize, placeholder `fg-3`. Placeholder no Chat: "Responder ao Orion".
- **Rodapé da caixa:** padding 6×8×8, `gap 2`, quebra de linha.
  - `plus` Anexar — tooltip "Anexar imagem, texto, PDF ou áudio (até 20 MB)". (Protótipo: cicla 4 arquivos de exemplo; ao passar de 4, toast "Até 20 MB por arquivo. Maior que isso: ponha numa pasta e passe o caminho.")
  - Anexos: pílulas altura 26, raio 7, fundo `raise-2`, ícone `file` + nome + tamanho `fg-3` + × .
  - Espaço flexível.
  - `mic` "Falar com o Orion" (toast "Gravando… clique de novo para enviar") · `wave` "Voz ao vivo" (`aria-pressed`, liga/desliga a mini barra).
  - **Enviar:** círculo 32, fundo `primary`, seta `up`, margem-esquerda 4; **desativado** sem texto (`raise-2`/`fg-4`). **Durante a resposta vira "Parar"** (quadrado arredondado 10×10 preenchido).
**Metadados (`.cmp-meta`):** `[chip do projeto — só no Início, ícone folder + nome + seta]` · espaço · `[chip do modelo: "{Provedor Modelo} {Esforço}" + seta]`. O esforço (Baixo/Médio/Alto) só aparece para Claude/Codex/Gemini.
**Teclado:** Enter envia; Shift+Enter quebra linha; **↑ com campo vazio** repete a última mensagem do usuário; Esc fecha as sugestões.
**Sugestões `/` e `@`** (`.suggest-box`): flutuante com `left/right 8`, padding 4, raio 12, fundo `raise`, borda `line-2`, sombra, máx. 260 de altura.
  - `/`: aparece quando o texto até o cursor é `/palavra`; lista os 15 comandos (§23) filtrados por prefixo; item = **comando** (Geist Mono 12.5/500) + descrição à direita (12 `fg-3`, 60% da largura). 
  - `@`: aparece com `@` no início ou após espaço, até 40 caracteres sem `@`/quebra; lista até **8 notas** do vault cujo nome contém o texto; item = ícone `file` + nome + pasta à direita. Escolher insere `@[[Nome da nota]] `.
  - ↑↓ movem, **Enter ou Tab** escolhem, Esc fecha, clique fora fecha. Item ativo: fundo `hover`.

---

## 13. Chat

### 13.1 Estrutura
Coluna: **Fio** (`thread`, rolável) + **composer** (`chat-cmp`, largura `--thread-w`, padding-bottom 14, `gap 4`). O fio rola para o fim ao abrir e a cada pedaço de resposta. Ao reconstruir a tela, **a posição de rolagem é preservada**.
- `thread-in`: largura `--thread-w`, centralizado, padding 28×20×36, **`gap 30`** entre mensagens.
- **Fixada** (se a conversa tem contexto fixado): faixa com borda **tracejada** `line-2`, raio 10, padding 8×12, 12.5 `fg-2`, ícone `pin`: "Fixado: {texto}".
- **Faixa de aviso** (`banner`, acima do composer): padding 6×12, raio 9, fundo `raise`, borda `line`, ponto âmbar + texto + × — ex.: "Groq sem cota; usando Gemini."
- **Conversa antiga vazia:** centro: cinturão grande, título (Newsreader 28), "Conversa antiga. O resumo está no vault; escreva para continuar."

### 13.2 Mensagem do usuário
- Alinhada à direita, largura máx. **80%** (90% no celular), bolha `raise-2`, raio 16, padding 10×15, 15 px, `pre-wrap`, quebra em qualquer ponto.
- Ações (aparecem no hover/foco, 150 ms): **Editar e reenviar** (`edit`; preenche o campo e avisa "Editar e reenviar cria um ramo da conversa"), **Copiar** (toast "Copiado"), **Fixar como contexto** (toast "Fixada como contexto permanente").
- Texto de anexos: `\n\nAnexos: a, b`.

### 13.3 Resposta do Orion
Coluna `gap 12`: **[ferramentas] → [corpo] → [aprovação] → [ações]**.

**Ferramentas (recolhível):** botão `tools` 13 px `fg-3` — "Leu 4 arquivos, executou 1 comando" + `chevR` (gira 90°). Aberto, mostra lista com borda esquerda `line-2` (margem −4×0×0×3, padding-left 14): linhas em grid `70px 1fr auto`, `gap 10`, 12.5 `fg-2`: **tipo** (Comando, Leu, Vault, …) em `fg-3` · valor em mono com reticências · tempo (11.5 tabular). No celular a coluna do tempo some e a primeira vira 60 px.

**Corpo (`m-body`):** fonte do chat, `--chat-fs`, 1.65. Parágrafo com margem inferior .8em; listas com recuo 1.3em, itens margem .3em, marcador `fg-3`; `b` 600; `code` mono .78em, fundo `raise-2`, raio 5, padding .1×.38em; `pre` raio 12, borda `line`, fundo `side`, padding 14×16, mono 12.5/1.65, rolagem horizontal. **Sintaxe:** palavra-chave `accent`, comentário `fg-3`, número `signal`.
- **Streaming (protótipo):** uma palavra a cada **50 ms**, cursor de bloco (7×1em, `fg-2`, pisca 1 s em passos de 2) ao final. No app real o texto vem do modelo. Movimento reduzido: aparece inteiro.

**Aprovação (`.appr`):** raio 14, padding 14×16, `gap 8`, largura máx. 40rem, borda âmbar 34% e fundo `signal-soft`.
- Cabeçalho: ponto + "**Aguardando sua aprovação**" (13/500, âmbar).
- Texto: 13.5 `fg-2` (caminhos em mono).
- Botões `btn-sm`: **Aprovar** (primário), **Negar**, e, na conversa de RAM, **Ver o diff** (fantasma → abre o painel direito na aba Diff).
- Resolvida (`done`): sem fundo, borda `line`, padding 10×14; ponto **verde** ("Aprovado às HH:MM…") ou **cinza** ("Negado às HH:MM. Nada foi alterado.").
- Ao resolver: remove o ponto âmbar da conversa, atualiza contador, pílula, Aprovações e adiciona ao histórico.

**Ações da resposta (`.m-act`, margem-esquerda −6, `fg-3`)**, escondidas durante o streaming: `copy` · `speaker` ("Ouvir") · `retry` ("Gerar de novo", só na última) · hora (11.5 tabular) · **"via"** (11.5 `fg-4`, padding 2×6, raio 6; hover `fg-2`/fundo `hover`) com ícone `out` quando algo saiu do PC.
- Clique em "via" → toast `"{via} · saiu do PC: {o que saiu}"` ou `"{via} · nada saiu do computador"`.
- Tooltip do "via": "Saiu do PC: …" ou "Respondido neste computador".
- Texto salvo: após a resposta completa, grava a nota da conversa e registra "Nota gravada no vault + commit {hash}" no histórico (automático).

### 13.4 Estados da conversa (barra lateral, pílula, histórico)
`""` normal · `running` (girando) · `pending` (ponto âmbar) · `unread` (ponto Rigel).

### 13.5 Menu da conversa (clicar no título)
Fixar/Desafixar conversa · Renomear (descrição "Título gerado sozinho, editável") · Abrir nota no vault (se ainda não existe: toast "A nota é gravada no fim da primeira rodada") · Arquivar · divisor · **Apagar conversa** (perigo).
Toasts: "Conversa fixada/desafixada", "Conversa arquivada", "Conversa apagada". Arquivar/apagar voltam ao Início.

---

## 14. Painel da direita (só no Chat)
- Coluna de **320 px**, fundo `side`, borda esquerda `line`. **Aparece sozinha** quando a conversa tem aprovação pendente; o botão `panelR` da barra superior alterna (override manual). **Some < 1120 px**. Aprovar com o painel fechado em tela estreita mostra toast com o resumo do diff.
- **Abas** (papel `tablist`): Contexto · Arquivos · Diff · Terminal. Altura 32, padding 0×10, 12.5 px; selecionada: `fg` + sublinhado 1.5 px `fg`. Padding do conjunto 8×8×0.
- Corpo: padding 10, `gap 8`, rolável.

### 14.1 Contexto (cartões)
1. **Ambiente:** `folder` projeto · `branch` branch (mono, se houver) · `cpu` modelo atual.
2. **Vault** (valor à direita: hora da nota): `file` nome do arquivo · `branch` "main · {hash}" (mono) + hora · botões **Abrir nota** (`edit`, abre o editor §19) e **Obsidian** (fantasma, toast "Abriria no Obsidian"). Sem nota: "A nota desta conversa é gravada no fim de cada rodada."
3. **Contexto** (valor: %): medidor + "{pct×2} mil de 200 mil tokens".
4. **Alterações** (só quando há mudança; valor: `+176` verde `−0` vermelho em mono): lista dos arquivos (mono) + botão "Ver o diff".

### 14.2 Arquivos
Árvore em mono 12.5/1.9: diretórios em `fg` com ▾/▸, arquivos alterados em `signal` com " M". Em seguida cartão **GitHub** (valor: repositório): selo "CI ok" + "main · {hash curto}"; selo "PR aberto" + "#N nome"; botões **Abrir PR** e **Merge** (toasts "pede aprovação").

### 14.3 Diff
Bloco mono 12/1.7, borda `line`, raio 10, fundo `bg`: cabeçalho de arquivo (`fg-3`, fundo `raise`), linhas `-` (vermelho 10%) e `+` (verde 10%), `white-space: pre`, rolagem horizontal. Abaixo, com aprovação pendente, botões **Aprovar** / **Negar**.

### 14.4 Terminal
Caixa mono 12/1.6, fundo `#0b0c0d`, borda `line`, raio 10, altura mínima 240. Saída (padding 10, `pre-wrap`) + entrada com prompt "PS>" em `accent`. Enter executa (no protótipo: ecoa). No app real: **PowerShell** com aprovação conforme a política.

---

## 15. Telas de página (largura 800)
`.page`: centralizada, padding 36×20×72, animação `fadein`. **Cabeçalho (`page-h`):** título (Newsreader 32) + descrição (13 `fg-3`) à esquerda, botão à direita, alinhado ao rodapé, `gap 12`, margem inferior 22.

### 15.1 Projetos
- Título "Projetos". Descrição: "Nota no vault, instruções, repositório, pastas, modelo padrão e rotinas de cada projeto." Botão primário "Novo projeto".
- Busca "Buscar projetos" (filtra nome + descrição). Grade **2 colunas**, `gap 12` (1 coluna ≤760).
- **Cartão `.pcard`:** altura mín. 150, padding 16×18, raio 14, borda `line-2`, `gap 6`; hover borda `line-3` + fundo `raise`. Conteúdo: nome (Newsreader 20) · descrição (13 `fg-2`) · selos (repositório com `branch` — verde se CI ok —, modelo com `cpu`, "N rotina(s)" com `clock`, pasta com `folder`) · rodapé (`margin-top:auto`): pendência à esquerda e "Atualizado {quando}" à direita (12 `fg-3`).
- Clicar abre a última conversa do projeto, ou inicia uma nova no projeto.
- Dados: Orion, Vault, Lyra, Nortis (§28).

### 15.2 Aprovações
- Título "Aprovações". Descrição: "Pendências de todas as conversas e rotinas, e o histórico de tudo o que o Orion fez fora do chat." Botão: `bolt` "Ligar/Desligar modo autônomo".
- **Aguardando você · N:** cartões `qcard` (borda âmbar 30%, fundo `signal-soft`, raio 14, padding 14×16, `gap 8`, margem 10): cabeçalho (ponto + "{conversa} · {projeto}" + hora à direita) · texto (14) · botões **Aprovar** / **Negar** / **Abrir conversa**. Mais a proposta da rotina "Organizar Downloads" (Aprovar/Negar).
- Nota: `send2` "As mesmas pendências chegam no Telegram, com botões, só para o seu ID."
- **Histórico de atividade:** filtro segmentado **Todas · Aguardando · Negadas ou com falha · Concluídas**; lista `li` (título + hora; selo de estado à direita: ok/warn/err; **Desfazer** `undo` quando a ação é desfazível e concluída). Ações desfazíveis (regex): *Nota gravada*, *Aplicar mudanças*, *Organizar Downloads*, *Fato corrigido*, *Resumo diário*, *Captura rápida*. Desfazer adiciona "Desfeito: …" (selo "revert") e marca o original "desfeito"; toast "git revert feito e push na main do vault" (se tem commit) ou "Ação revertida". Vazio: "Nada neste filtro."

### 15.3 Rotinas
- "Rotinas": "Crie pelo chat (“todo dia às 8h me manda o briefing”), por /rotina ou aqui. As críticas rodam na nuvem do Claude se o PC estiver desligado." Botão primário "Nova rotina" (toast: "Formulário: nome, horário, prompt, projeto, modelo, saída").
- Linha: título + selo verde "resultado novo" + selo Rigel `cloud` "nuvem se desligado" (rotinas críticas) · descrição "{quando} · próxima {…}|pausada · última {…} [selo ok/falhou/pulada]" · "Saída: {…}" · à direita: `runs` (7 quadrados), **Rodar agora** (`play`), switch Ativar. Abrir Rotinas apaga "resultado novo".
- Toasts: "{nome} ativada/pausada"; "{nome}: rodou agora. Saída: {saída}".

### 15.4 Resultados
"Seus resultados": "Conteúdo que vale guardar, com versões e fontes." Linhas: título + descrição · selo de tipo (Pesquisa/Revisão) · versão em mono (v1, v2, v3) · data.

### 15.5 Nuvem
"Nuvem": "Tarefas que você mandou para Claude ou Codex na nuvem, com /nuvem. Ao terminar, o Orion avisa e espera você." Botão `cloud` "Nova tarefa" (preenche `/nuvem codex ` no Início). Linha: selo do serviço + título · "{projeto} · {quando} · {link}" · à direita: selo âmbar `spin` "rodando" **ou** selo verde "pronto" + **Trazer resultado** (toast `codex apply: diff trazido para C:\Projetos\Orion` ou `claude --teleport: sessão trazida para o PC`).

### 15.6 Fontes
"Fontes": "Só o que está ligado é lido. O desligado não é consultado." Cinco linhas com switch: Vault do Obsidian · Repositórios dos projetos · Calendário · E-mail · Busca na web (descrições em §28).

### 15.7 Conhecimento
"Conhecimento": "Fatos que o Orion aprendeu, cada um com a fonte. Corrigir ou esquecer muda a nota no vault." Botão `graph` "Ver no grafo". Fatos agrupados (**Preferências · Projetos · Rotina**): linha com texto + fonte (`conversa · 12/09`); botões `edit` (Corrigir: vira campo inline, Enter ou **Salvar** grava, fonte vira "corrigido por você · hoje", toast "Fato corrigido; commit na main") e `trash` (Esquecer, toast "Fato esquecido; nota no vault atualizada"). Vazio: "Memória vazia."

### 15.8 Arquivadas
"Conversas arquivadas": "Só arquivam quando você pede. Desarquive para continuar ou apague de vez." Linhas: título + "{projeto} · {mês}" · **Desarquivar** (toast "Conversa desarquivada") + `trash`. Vazio: "Nenhuma conversa arquivada."

### 15.9 Vida (cortada em 21/10/2026)
> Esta tela saiu do app: não há item na barra lateral, na paleta nem captura. O texto abaixo fica só como referência histórica do protótipo.

"Vida": "Finanças locais, monitor de preços, linha do tempo e foco. Nada sai do computador." Caixa tracejada `soon` (padding 18, raio 14, borda tracejada `line-2`, 13.5 `fg-2`): `clock` "Entra **depois da v1**, numa fase própria. Mesmo conteúdo da tela Vida do front original: colar a fatura, revisar e lançar; preços acompanhados; linha do tempo; escudo de foco que só pergunta."

### 15.10 Painel
"Painel": "Cotas, assinaturas, saúde do sistema e a semana. Dados de exemplo." Botão `refresh` "Atualizar" (toast "Painel atualizado").
- **Cotas das APIs gratuitas · hoje:** 5 barras (`hbar`): rótulo · medidor · valor (% ou "esgotada" quando 100%, barra vermelha).
- **Assinaturas:** 3 barras com %.
- **Saúde do sistema:** 4 estatísticas (grade de 4 colunas, bordas superior/inferior `line`, divisores verticais, padding 16×4, número Newsreader 30 + legenda 12 `fg-3`): **296 MB** RAM do Orion · **0,4%** CPU parado · **12%** GPU · **41 GB** livres no disco.
- **A semana:** 38 conversas · 12 ações aprovadas · {N pendências} aguardando você · 45 notas gravadas no vault.
- **Primeira resposta · últimos 14 dias:** gráfico de linha (§26.3).
- ≤760 px: estatísticas em 2 colunas.

---

## 16. Memória (grafo do vault)
Tela cheia `absolute inset:0`, coluna: **barra** + **palco** + **linha do tempo**; animação `fadein`.

### 16.1 Barra (padding 6×16×10, `gap 8`, quebra)
- **Busca** (flex 0 1 260, mín. 180) "Buscar notas, tags ou pastas".
- **Filtros por tipo** (chips altura 30, raio 999, borda `line-2`, 12.5 px; ponto de 7 px na cor do tipo): Projetos, Áreas, Conceitos, Pesquisas, Recursos, Conversas, Relatórios + **`eye` "Lidas agora"** (tooltip "Notas que o Orion leu na última resposta"). `aria-pressed=true`: texto `fg`; `false`: opacidade .5 e borda tracejada. **Padrão ligado: todos, exceto Relatórios.**
- **Estatística** (à direita, 12 `fg-3`): "{N} notas · {N} links · git pull às HH:MM" (conta só o visível).
- **Resultados da busca** (flutuante, largura 320, máx. 280): até **8** notas visíveis que casam, ordenadas por **grau** (nº de links) decrescente; item: ponto da cor + nome + tipo à direita. Primeiro destacado; ↑↓ movem, **Enter** seleciona e centraliza, Esc fecha. Sem resultado: "Nada com “…”."

### 16.2 Palco
Raio 16, borda `line`, fundo radial (§4.1), margem lateral 12, `overflow:hidden`.
- **Camada de fundo:** céu (alfa .5, **sem** constelação).
- **Camada do grafo:** nós e arestas.
- **Legenda** (embaixo à esquerda, 11.5 `fg-3`, sem eventos): um ponto + rótulo singular por tipo. Some ≤760.
- **Zoom** (embaixo à direita, painel vertical, padding 3, raio 10, fundo `raise` 85% com blur 6): `plus` (×1.25), `minus` (×0.8), `fit` (enquadrar).
- **Dica** (hover): `memTip`, máx. 280, padding 6×10, raio 8, fundo `raise`, 12.5 px: nome + linha 11 `fg-3` "{Tipo} · dd/mm/aaaa · {n} links · lida na última resposta".

### 16.3 Desenho (regras exatas)
- **Raio base do nó:** projeto/área **4.6**; conceito **3.1**; demais **2.2**. Raio desenhado = `(base + log2(max(grau,1))·0.55) · √zoom`. Nó em foco: ×1.3.
- **Hub:** grau ≥ **45**.
- **Foco** = selecionado, senão o sob o ponteiro. Com foco, nós fora do foco e de seus vizinhos ficam a **10%** de opacidade.
- **Arestas:** largura .6, cor `#9aa3c0`, alfa **.08** (**.03** com foco, busca ou "lidas agora"); arestas do foco: largura 1, cor do tipo do foco, alfa .5.
- **Brilho** (gradiente radial, raio ×4.5, alfa .22) em hubs, projetos, foco e "lidas".
- **Selecionado/lido:** anel `fg` de 1.2 px, raio `r·1.3+4`, alfa .9.
- **Rótulos:** aparecem para foco, projetos, áreas, hubs, vizinhos do foco (se ≤40), melhores 12 resultados da busca, "lidas" e **qualquer nó quando zoom > 2.2**. Fonte Geist 11 (400) ou **12 (500)** para foco/projeto/hub; cor `fg` (grandes) ou `fg-2`; alfa .94; reticências acima de 36 caracteres; **sem sobreposição** (caixas com 3 px de folga; prioridade: foco > ponteiro > busca/lidas > projeto > grau).
- **Visibilidade:** o filtro de tipo **e** a data (`date ≤ corte`) decidem.
- **Zoom:** 0.2 a 6 (roda do mouse ×1.12 em torno do cursor); `fit` enquadra todos os nós com grau, margem 80, zoom entre .25 e 2.
- **Rotação:** abre **girando devagar** (≈ 0,00003 rad/ms) e **para ao primeiro toque**. Movimento reduzido: não gira.
- **Interação:** arrastar o fundo = pan; arrastar um nó = move o nó; clique sem mover = seleciona (ou limpa); cursor `grab`/`grabbing`/`pointer`; selecionar centraliza em **450 ms** (ease-out cúbico), zoom mínimo 1.1.
- **No 3D (wgpu):** mesma lógica de cores, tamanhos, brilho, opacidade por foco, rótulos sem colisão (glyphon) e mesma interação; a câmera orbita em vez de girar o plano 2D.

### 16.4 Painel de detalhes (nó selecionado)
Flutuante à direita do palco (top/right/bottom 12, largura 320), raio 14, fundo `raise` 92% + blur 12, borda `line-2`, sombra; entra deslizando 10 px em 220 ms.
- Cabeçalho: selo do tipo (ponto na cor) + selo do `status` (se houver) + × (Esc também fecha).
- Corpo (padding 8×16×16, `gap 10`): título (Newsreader 20) · meta (12 `fg-3`) · **caminho** (mono 11.5) · **#tags** (selos) · botões **Abrir e editar** (`edit`, abre o editor) e **Grafo local** (`graph`, seleciona, centraliza e avisa "Grafo local: só a nota e os vizinhos (profundidade 1)") · "**Ligada a · N**" (11.5 `fg-3`) com até **60** vizinhos (ponto + nome; clicar navega) e "e mais N".
- ≤760: o painel ocupa 58% da altura, ancorado embaixo.

### 16.5 Linha do tempo
Rodapé (padding 10×16×12, 12 `fg-3` tabular): data inicial · **slider** (accent Rigel, uma posição por data distinta de nota) · "até dd/mm/aaaa". Mover o slider esconde notas mais novas e atualiza estatística e seleção.

---

## 17. Editor de nota (modal)
- Aberto por: "Abrir nota" (painel direito), "Abrir e editar" (grafo), resultado de nota na paleta.
- Modal **1100 × 760 (máx.)**, raio 18, fundo `bg`, borda `line-2`, sombra; animação `rise` 240 ms. Tela cheia no celular.
- **Cabeçalho** (padding 10×12×10×18, borda inferior `line`): `file` · **nome do arquivo** (Newsreader 18) · caminho completo (mono, `fg-3`, reticências) · espaço · **Obsidian** (toast "Abriria no Obsidian") · **Salvar e commit** (primário; fecha e mostra "Nota salva; commit e push na main do vault") · × .
- **Corpo:** 2 colunas iguais. **Esquerda:** textarea mono 13/1.7, fundo `side`, borda direita `line`, padding 18, `spellcheck=false`. **Direita:** prévia em Newsreader 16/1.65, padding 18×24.
- **Prévia ao vivo:** `---…---` vira bloco de frontmatter (mono 12, `fg-3`, borda `line`, raio 10); `#` (28), `##` (21, margem), `###`; `> ` citação (borda esquerda `line-3`, `fg-2`); listas `- `; `**negrito**`; `` `código` ``; `[[wikilink]]`/`[[alvo|rótulo]]` em `accent` sublinhado (decoração a 40%).
- Esc ou clique fora fecha. Celular: coluna única, 2 linhas iguais.
- **Texto de exemplo** (nota "conversa" com frontmatter `type, date, fonte: orion, projeto, tags, resumo`) em §28.

---

## 18. Configurações (modal)
- Modal **1060 × 740 (máx.)**, grade `236px 1fr`, raio 18, fundo `bg`, borda `line-2`, sombra, `rise` 240 ms; × fixo no canto (top 10, right 12). Abre com **Ctrl ,**; Esc ou clique fora fecha. Abrir fecha a gaveta e cancela "Pausar" armado; reabrir limpa "Apagar".
- **Navegação (236):** fundo `side`, borda direita `line`, padding 12×8. Campo "Procurar" no topo (filtra seções por nome **e palavras-chave**; ao digitar, abre a primeira que casar; "Nada encontrado." se nenhuma). Dois grupos:
  - **Configurações:** Geral · Notificações e sons · Conta · Privacidade e autonomia · Uso · Modelos · Memória · Fontes · Conectores
  - **Este computador:** Sistema · Modelos locais · Voz · Câmera · Atalhos
  - Ícones: gear, bell, user, shield, gauge, cpu, graph, db, puzzle / cpu, db, wave, camera, keys.
- **Conteúdo:** padding 0×44×48. Título da seção (Newsreader 26, margem-topo 30). Seções internas `.set-h` (13/600, margem 30×0×2). Lede 12.5 `fg-3`. Rolagem preservada ao alterar uma opção; abrir outra seção volta ao topo.
- **Celular (≤760):** lista de seções em tela cheia; ao escolher, entra o detalhe com botão "← Configurações".

### 18.1 Geral
**Interface** — Idioma (Português | English; com "English" mostra toast: "English: strings come from Fluent files…") · Escala da interface (90 · 100 · 115 · 130 · 150%; "Também com Ctrl + e Ctrl −.") · Densidade (Confortável | Compacta; "Altura das linhas na barra lateral e nas listas.") · Movimento (Sistema | Reduzido; "Animações discretas. Reduzido desliga o texto em tempo real e a rotação do grafo.").
**Conversa** — Fonte do chat (seletor: **Newsreader** "Serifada, como uma página" · **Geist** "Sem serifa, igual à interface" · **Fonte do sistema** "Segoe UI no Windows") · Tamanho do texto (Pequeno | Médio | Grande) · Largura da transcrição (Estreito | Médio | Largo) · Tom das respostas (Conciso | Normal | Detalhado; "Vale para todos os projetos; cada projeto pode ter instruções próprias.") · Área de texto "Como o Orion deve responder (opcional)" com placeholder "Ex.: sem preâmbulo, código completo, português técnico".

### 18.2 Notificações e sons
**Notificações do Windows** (switches): Aprovação pendente · Rotina concluída · Tarefa na nuvem terminou · Resposta pronta com a janela em segundo plano (padrão desligado).
**Sons sutis** (switches, padrão ligados): Aprovação pendente · Resposta pronta (janela fora de foco) · Rotina concluída · Ativação da voz ("Ao ouvir “Orion” ou duas palmas.").
**Não perturbe:** switch "Silencia notificações e sons. Rotinas e Telegram continuam." + lede: "Com a janela fechada, avisos também chegam pelo Telegram e por um ponto no ícone da bandeja."

### 18.3 Conta
**Perfil** — Nome completo (campo) · "Como o Orion deve te chamar" (campo; "Aparece na saudação e na barra lateral."; vazio volta ao padrão) · Senha deste computador (botão **Trocar**; "Pedida ao abrir o Orion e quando o Windows é bloqueado (Win + L).").
**Sessão** — Sair deste computador (botão **Sair**; "Pede a senha de novo para entrar.").

### 18.4 Privacidade e autonomia
- **Modo autônomo** (switch): "Ligado, o Orion age sem pedir aprovação. Continua pedindo para tudo que pode quebrar o PC ou o projeto: apagar, mexer no sistema, force-push, reescrever histórico, desinstalar." Ao alternar: toast "Modo autônomo ligado. Ações que podem quebrar o PC ou o projeto continuam pedindo confirmação." / "Modo autônomo desligado".
- **Sempre sem aprovação** (grade de 2 colunas, ✓ verde): Ler arquivos nas pastas confiáveis · Gravar notas no vault + commit na main · Busca na web sem dados pessoais · Comandos de leitura (git status, cargo check).
- **Nunca sai do computador** (escudo): Senhas e chaves (filtradas antes de enviar) · Documentos pessoais (RG, CPF, banco: mascarados). Lede: "Cada resposta que usou uma API de fora mostra um ícone discreto; clicando, aparece o que saiu e para qual provedor."
- **Parada — Pausar tudo:** "Corta ferramentas de escrita, rede e execução até você retomar." Fluxo de 3 estados: **Pausar** → (**Cancelar** | **Confirmar pausa** vermelho) → selo vermelho "Pausado" + **Retomar**. Ao confirmar: desliga modo autônomo, presença da câmera e voz; toast "Tudo pausado. Nenhuma ferramenta de escrita, rede ou execução roda."
- **Dados — Apagar histórico de conversas:** "Remove as conversas deste computador. As notas no vault continuam." **Apagar…** → (**Cancelar** | **Apagar N conversas** vermelho). Toast "Histórico apagado"; zera pilhas e volta ao Início.

### 18.5 Uso
**Cotas das APIs gratuitas · hoje:** barras `hbar` (rótulo · medidor · "usado de limite"): Gemini (API) 62% "930 de 1.500" · Groq 100% "esgotada até 00:00" (vermelha) · Cerebras 18% "180 de 1.000" · OpenRouter (free) 40% "20 de 50" · Mistral 9% "90 de 1.000".
**Assinaturas:** Claude 41% ("41% do limite semanal") · Codex 23% · Gemini CLI 12% ("12% do limite diário").

### 18.6 Modelos
**Seletor do chat** — lede: "Auto, Claude, Codex, Gemini e Orion. Claude, Codex e Gemini usam as assinaturas pelas CLIs oficiais, com modelo e esforço." · "Padrão para conversas novas" (seletor em caixa) · "Mostrar quem respondeu: Discreto, na linha da hora de cada resposta." (selo verde **sempre**).
**Orion · ordem das APIs gratuitas** — lede: "Quando uma cota acaba, passa para a próxima. Se todas acabarem ou não houver internet, usa o modelo local." Linhas numeradas (1 Gemini, 2 Groq, 3 Cerebras, 4 OpenRouter, 5 Mistral): "{% disponível}% da cota disponível" ou "Sem cota até 00:00"; botões `up` (subir) e **Chave** (colar a chave). Última linha: **∞ Local · Qwen 2.5 7B** — "Fallback sem internet. Gerenciado em Modelos locais." (selo **fallback**).

### 18.7 Memória
Lede: "A memória é o vault: {N} notas ligadas por {N} wikilinks." Linhas: Usar memória nas respostas (switch; "Desligado, cada conversa começa do zero.") · Pasta das notas do Orion (`03 Recursos/Orion/Memória/`) · Resumo diário das conversas triviais (selo verde **23:59**; "Uma nota por dia em Memória/Diário, gerada às 23:59.") · Commit e push (selo verde **automático**; "Direto na main do vault, um commit por nota gravada. Sem PR.") · git pull do vault (botão **Agora**, toast "git pull: já atualizado"; "Ao abrir o app, a cada conversa nova, antes de gravar uma nota e ao abrir a Memória.") · Se você editou a mesma nota no Obsidian (selo **perguntar**; "O Orion pergunta no chat e mostra as duas versões.") · Grafo (botão **Abrir**; "3D no app final, como o Obsidian, sobre o céu do front original.").

### 18.8 Fontes
Lede "Só o que está ligado é lido." + as 5 linhas com switch da §15.6.

### 18.9 Conectores
**Servidores MCP** (lede "Por comando local (stdio) ou URL (HTTP). As ferramentas aparecem no chat com o nome do servidor."): linha = nome + selo de estado (verde Conectado / vermelho Token expirado) · "{tipo} · {N} ferramentas · permissão por ferramenta: perguntar" · botão **Gerenciar** (ou **Reconectar** se erro). Linha final **Adicionar servidor** (botão `plus`).
**Skills** (lede "Pastas com SKILL.md, no formato das skills do Claude."): nome em mono + origem ("Importada do Claude" / "Própria") + switch.
**Plugins** — "Plugins em Rust/WASM: Código isolado em sandbox, com permissões declaradas." (**Instalar**).
**Importar** — "Do Claude e do Codex: Lê mcp.json, configs e skills já instaladas neste PC." (**Procurar**; toast "Encontrados 3 servidores e 5 skills").

### 18.10 Sistema
- **Memória RAM:** número grande "**296 MB**" + "em uso agora · quanto menos, melhor" · legenda "Uso nas últimas 18 h" (cinza) e "Otimização aplicada" (âmbar) · **gráfico de linha** (§26.3, h 150, eixo 0–1400 MB, marca no 10º ponto "mmap e voz sob demanda", início "ontem, 16:00", fim "agora") · **barra empilhada** 8 px (28% `fg-2`, 63% `fg-2` a 45%, 8% `signal`, `gap 2`) · legenda 2 colunas: Núcleo e interface **84 MB** · Índice vetorial (mmap) **188 MB** · Cache de embeddings **24 MB** · Voz e grafo (descarregados) **0 MB**.
- **Janela:** Atalho global (**Win + Shift + Espaço**, ver U3) "Abre a janela focada no campo de mensagem." · Fundo Mica "Transparência do Windows 11 na barra lateral." · Iniciar com o Windows "Ao ligar o PC, abre minimizado na bandeja, sem janela até ser chamado. Fechar a janela também vai para a bandeja." · Pastas confiáveis (**Gerenciar**; "O Orion pode ler e escrever nessas pastas e nas de dentro.").
- **Este PC e o notebook:** Sincronia do banco (selo verde "notebook em dia · 09:10"; "PC principal manda; o notebook replica pela Tailscale quando os dois estão ligados.").
- **Atualizações e backup:** Atualizações (selo "0.6.0 · em dia"; "Baixa do GitHub Releases em segundo plano, avisa e instala ao fechar.") · Backup no Google Drive (selo verde "ontem, 03:00"; "Banco e configurações, criptografados.") · Exportar ou importar (**Exportar** | **Importar**; "Para trocar de PC.").

### 18.11 Modelos locais
Lede: "llama.cpp embutido no Orion, sem processo extra. Carregam só quando usados." Linhas: nome + selo (fallback/embeddings) · "{tamanho} no disco · baixado|não baixado" · **Remover**. Final: "Baixar modelo: Do Hugging Face, formato GGUF." (**Baixar**).

### 18.12 Voz
**Ativação:** Palavra “Orion” (switch; "Escuta localmente. Traz a janela para o foco.") · Duas palmas (switch; "Também traz a janela para o foco. Em calibração no notebook.") · Testar a mini barra (botão **Mostrar/Fechar**; "Barra flutuante no topo da tela enquanto você fala.").
**Fala:** Resposta falada (selo "só por voz"; "Só quando o pedido foi por voz.") · Idioma (Português (Brasil) | English (US)) · Estilo (Masculina, grave | Masculina, clara) · Velocidade (Normal | Rápido).

### 18.13 Câmera
**Modos:** Foto sob pedido ("“Olha isso”: tira uma foto e responde.") · Presença contínua ("Local, sem guardar imagem. Ao sair: pausa a voz e bloqueia o Orion. Na volta: cumprimenta com o que aconteceu.") — lede: "Com a câmera ativa, aparece um ponto vermelho na barra de título e no ícone da bandeja."
**O que o Orion faz com a câmera:** “Onde eu deixei?” ("Guarda só rótulos (objeto, lugar, hora), sem prazo. Poucos KB por ano.") · Reconhecer você ("Rosto local do dono.") · Legenda ao vivo ("Faixa flutuante na base da tela.").
**Objetos lembrados:** etiquetas removíveis (padrão: chaves, carteira, celular, fone de ouvido, óculos, controle remoto, carregador, notebook, mochila, documentos (RG/CNH), cartões, remédios, garrafa/copo, mouse sem fio, teclado sem fio, pendrive, relógio, guarda-chuva, caneta) + campo "Adicionar objeto" (Enter ou **Adicionar**).

### 18.14 Atalhos
Linhas (rótulo | `kbd`): Abrir o Orion de qualquer lugar | **Win + Shift + Espaço** (U3) · Captura rápida para o Inbox | Win + Shift + O · “Isso?”: capturar a tela e perguntar | Win + Shift + I · Buscar conversas e notas | Ctrl K · Nova conversa | Ctrl Shift O · Recolher barra lateral | Ctrl B · Configurações | Ctrl , · Escala da interface | Ctrl + / Ctrl − · Enviar | Enter · Nova linha | Shift Enter · Repetir a última mensagem | ↑ · Comandos | / · Citar nota do vault | @ · Fechar painel | Esc. **No app real:** editáveis, com detecção de conflito (F01.22).

---

## 19. Paleta Ctrl K
- Fundo escurecido, caixa **600** de largura, topo a **14vh**, raio 16, fundo `raise`, borda `line-2`, sombra, `rise` 200 ms.
- Campo: ícone de lupa + entrada 15 px de **52 px** de altura + `kbd` "Esc"; placeholder "Buscar conversas e notas do vault".
- Lista (padding 6, máx. 360): ícone 14 + título + grupo à direita (11.5, `fg-3`); selecionado fundo `sel`/`fg`.
- **Resultados, nesta ordem:** até **6 conversas** (título; grupo "Conversa · {projeto}") · até **6 notas** do vault (só com texto digitado; grupo "Nota · {pasta raiz}"; abre o editor) · **ações**: Nova conversa · Configurações · Captura rápida (grupo "Ação · Win + Shift + O") · Isso? (capturar a tela) ("Ação · Win + Shift + I") · Ligar/Desligar modo autônomo · "Ir para {tela}" para todas as telas. Filtra por título + grupo. Vazio: "Nada encontrado".
- ↑↓ movem (circular), Enter executa, Esc fecha, Ctrl K de novo fecha. Nada abre sobre Configurações.
- **No app real:** prefixo `>` para comandos (decisão F01.21).

---

## 20. Captura rápida (Win + Shift + O) e "Isso?" (Win + Shift + I)

### 20.1 Captura rápida
- Fundo `rgb(0 0 0 / .25)`. Caixa: topo **22vh**, largura **620** (máx. `100vw−32`), raio 16, `raise`, borda `line-2`, sombra, `rise` 180 ms.
- Linha: `inbox` + campo de **54 px** (16 px) com placeholder "Uma linha: tarefa, lembrete, gasto ou nota" + `kbd` "Enter".
- Rodapé (padding 8×12, borda superior `line`, 12 `fg-3`): segmentado **Tarefa · Lembrete · Gasto · Nota** + espaço + "Vai para `00 Inbox` · Win + Shift + O".
- **Detecção automática do tipo** enquanto digita (até o usuário escolher manualmente): `r$`, `dd,dd`, "gastei", "paguei" → **Gasto**; "amanh", "às N", "hoje", "lembra" → **Lembrete**; começa com comprar/fazer/ligar/mandar/revisar → **Tarefa**; senão **Nota**.
- **Enter** salva: fecha, registra no histórico ("Captura rápida (tipo): texto", origem "Inbox") e mostra toast "{Tipo} salva em 00 Inbox · commit na main". Esc ou clique fora fecha.

### 20.2 "Isso?"
- Tela inteira escurecida (`rgb(0 0 0 / .45)`), cursor em cruz, `fade` 150 ms. Dica no topo: "Arraste para escolher a área · Esc cancela · Win + Shift + I".
- Área de seleção (no protótipo, fixa em 30% × 28%, 34% × 26% da tela): borda 1.5 `primary`, raio 4, fundo `primary` 8%, fora dela escurece (`box-shadow 0 0 0 9999px rgb(0 0 0 / .25)`). **No app real:** arrastar escolhe a região; **Enter captura a tela cheia** (decisão).
- Cartão de pergunta abaixo da seleção (largura 420, raio 14, `raise`, borda `line-2`, padding 10, `gap 8`): campo 36 px "O que é isso? (Enter pergunta)"; resposta em Newsreader 15/1.55 (aparece ao dar Enter); botões **Fechar** (fantasma) e **Levar para o chat** (toast "Captura anexada a uma conversa nova").

---

## 21. Mini barra de voz e legenda ao vivo
- **Mini barra (ilha):** topo centralizado (top 10), altura **44**, padding 0×8×0×10, raio 999, fundo `#0b0c10`, borda `line-2`, sombra, `rise` 200 ms, `gap 12`, máx. `100vw−32`. Canvas 72×32 com **7 pontos** de céu: (10,8) `signal` r2.4 · (62,9) `star` r1.4 · (28,17), (36,16), (44,15) `ok` r2 (**cinturão**) · (14,27) `star` r1.4 · (60,26) `n-conceito` r2.4. Texto "**Ouvindo… diga “Orion” ou bata duas palmas**" (13 `fg-2`, reticências). Botão × "Parar de ouvir".
- Estado: liga/desliga por `wave` do composer, `/voz` e Configurações > Voz. Com ela ligada: pílula "Ouvindo" e Cinturão do Início em verde.
- **Legenda ao vivo:** faixa na base (bottom 28), padding 8×16, raio 10, fundo `rgb(0 0 0 / .78)`, texto branco 16 px, centralizado, máx. 760: "— Orion, quanto de RAM o app está usando agora?" (exemplo). Liga em Configurações > Câmera > Legenda ao vivo.

---

## 22. Gaveta (≤ 760 px)
A barra lateral vira gaveta fixa: largura `min(300px, 86vw)`, desliza de `-100%` em **250 ms** (`--ease`), sombra, scrim `scrim` atrás (clique ou Esc fecha). Abrir pelo botão `menu` da barra superior. Escolher qualquer item/conversa fecha a gaveta.

---

## 23. Comandos `/` (15)
`/nota` Gravar ou atualizar a nota desta conversa · `/vault` Procurar no vault · `/rotina` Criar ou listar rotinas · `/modelo` Trocar modelo e esforço · `/projeto` Trocar o projeto da conversa · `/autonomo` Ligar ou desligar o modo autônomo · `/nuvem` Mandar tarefa para Claude ou Codex na nuvem · `/imagem` Gerar imagem (Gemini) · `/limpar` Conversa nova no mesmo projeto · `/resumo` Resumir a conversa até aqui · `/aprovar` Abrir a fila de aprovações · `/voz` Ligar ou desligar a voz ao vivo · `/mcp` Listar e ativar servidores MCP · `/skill` Listar e ativar skills · `/ajuda` Todos os comandos.
Efeitos: `/aprovar` → Aprovações; `/rotina` → Rotinas; `/autonomo` → alterna; `/voz` → alterna; `/limpar` → Início; `/modelo` → abre o seletor; `/nota` → grava ("Nota gravada e commit na main") ou "Abra uma conversa primeiro"; `/vault texto` → Memória já buscando; `/nuvem [claude|codex] texto` → cria tarefa "rodando" e abre Nuvem ("Tarefa enviada para a nuvem"); `/imagem` → "Gerando imagem com Gemini (simulado). Fica na conversa e em 03 Recursos/Orion/Imagens."; `/mcp` e `/skill` → Configurações > Conectores; `/resumo` → "Resumo gerado na nota desta conversa"; `/ajuda` → lista; desconhecido → "Comando {x} não existe. /ajuda lista todos."

---

## 24. Seletores
### 24.1 Seletor de modelo (popover alinhado à direita)
Itens: **Auto** ("Escolhe pelo tipo de tarefa") · grupo "**Claude · Assinatura, pela CLI oficial**": Claude Opus / Sonnet / Haiku · grupo "**Codex · …**": Codex Padrão / Mini · grupo "**Gemini · …**": Gemini Pro / Flash · grupo "**Orion**": Orion ("APIs gratuitas; ordem e cotas nas configurações") · divisor · grupo "**Esforço (Claude, Codex, Gemini)**": Baixo / Médio / Alto. ✓ no item atual. Padrão: Auto, Sonnet, Médio.
**Rótulo de quem respondeu (`via`):** Auto → "Orion · Cerebras"; Orion → "Orion · Gemini"; senão o nome do modelo; "Local · Qwen 7B" quando local. `out` descreve o que saiu do PC (vazio quando local).
### 24.2 Seletor de projeto (Início)
Popover com: Orion, Vault, Lyra, Nortis, Sem projeto (ícone `folder`, ✓ no atual).
### 24.3 Fonte do chat, idioma e estilo da voz
Popovers à direita com as opções da §18.

---

## 25. Teclado (resolução em ordem)
1. Popover aberto: Esc fecha; ↑↓ percorrem os itens.
2. Fora do login, nada responde.
3. Captura rápida / "Isso?" abertos: Esc fecha; Enter em cada campo age.
4. Win+Shift+O / Win+Shift+I (U4) abrem as ferramentas.
5. Editor aberto: Esc fecha.
6. Paleta aberta: Esc, ↑↓, Enter.
7. Composer com sugestões: ↑↓, Enter/Tab, Esc.
8. Composer vazio + ↑: repete a última mensagem.
9. Terminal: Enter executa. Campo da busca do grafo: Enter/↑↓/Esc. Campo "novo objeto": Enter.
10. `Ctrl +`, `Ctrl =`, `Ctrl −`: escala.
11. `Ctrl K`: abre/fecha a paleta (não abre sobre Configurações).
12. Esc: fecha Configurações (celular: detalhe volta à lista); fecha a gaveta; limpa o nó selecionado no grafo.
13. `Ctrl ,`: abre/fecha Configurações. Com Configurações aberta, as demais teclas estão suspensas.
14. `Ctrl Shift O`: Início. `Ctrl B`: recolhe a barra lateral.
15. Enter no composer envia (sem Shift).
Navegar de tela: limpa rascunho, anexos, sugestões, tab do painel (volta a "Contexto") e a edição de fato.

---

## 26. Animações e motor do céu
### 26.1 Tabela
| Animação | Duração / curva |
|---|---|
| Entrada de página (`fadein`) | 220 ms `--ease`: opacidade 0→1 e deslocamento 3 px de baixo |
| Modais (`rise`) | 240 ms `--ease`: opacidade + 6 px de baixo (popover 160, paleta 200, captura 180, toast 200, mini barra 200) |
| Scrim (`fade`) | 180 ms |
| Detalhes do grafo (`slidein`) | 220 ms `--ease`: 10 px da direita |
| Gaveta | 250 ms `--ease` |
| Seta de "Mais" e de ferramentas | 200 ms `--ease`, 90° |
| Switch | 180 ms fundo, 200 ms bolinha |
| Hover | 150 ms |
| Pílula "Pensando" | 1,2 s `--ease`, infinita (opacidade 1→.3→1) |
| `spin` | 0,9 s linear infinita |
| Cursor de digitação | 1 s, 2 passos |
| Streaming (protótipo) | 1 palavra / 50 ms |
| Centralizar no grafo | 450 ms ease-out cúbico |
| Rotação do grafo | ≈ 0,03 rad/s (0,00003 por ms), para ao interagir |
Movimento reduzido: tudo `none`.

### 26.2 Céu usado na UI (`drawSky`)
- Gerador **Park–Miller**: `seed = 7; rnd = (seed = seed·16807 mod 2147483647) / 2147483647`.
- **240 estrelas:** alfa `(0.06 + rnd·0.38) · alfaGlobal`, raio `0.3 + rnd·0.9`, posição `(rnd·w, rnd·h)`, cor `star`. Ordem de sorteio: alfa, x, y, raio (consumir `rnd` exatamente nessa ordem para o céu ser idêntico).
- **Constelação** (só entrada e Início): escala `s = min(w,h)·0.92`, origem centralizada; pontos (fração de `s`): 0 (.28,.12) Betelgeuse · 1 (.72,.18) · 2 (.42,.50) · 3 (.50,.53) · 4 (.58,.56) (Cinturão: 2, 3, 4) · 5 (.32,.88) · 6 (.71,.86) Rigel. Linhas: 0–2, 1–4, 2–3, 3–4, 2–5, 4–6; traço .8 px, alfa .12.
- **Brilhos:** cor 0 = `signal`; cor 6 = `n-conceito`; Cinturão = `star` (ou `ok` com voz); raio 3.6 (estrelas grandes) / 2 (demais), cinturão × (1 + .6·belt); halo radial de raio ×6 com alfa .25 (cinturão ativo `.25 + .4·belt`); núcleo com alfa .95. Alfa global: 1 (entrada), **.55 (Início)**, **.5 (grafo, sem constelação)**.
- Este desenho é **estático** no mockup; no app usa o motor animado do instalador (respiração da câmera, bloom, linhas crescendo), mantendo as mesmas posições, seed e cores.

### 26.3 Gráfico de linha
SVG `viewBox 0 0 W H`, W = 680 (360 no celular), H = 150. Margens: esquerda 44, direita 10, topo 12, base 22. Linhas de grade horizontais em `line-2` (1 px) nos `ticks`, rótulos 10.5 px `fg-3` à esquerda; traço da série 1.6 px `fg-2` (pontas e junções arredondadas), **área** preenchida `fg-2` a 8%; **ponto final** círculo r3.5 fundo `bg` com traço da série; **marca** opcional (linha tracejada `3 3` em `signal` no índice dado, rótulo em `signal` ao lado); rótulos inferiores esquerdo e direito (início e fim). Série de RAM: `[1180,1210,1195,1230,1225,1240,1215,1236,1228,1205,860,420,318,302,298,296,301,296]` (marca no índice 10); série de latência: `[2.1,1.9,2.3,1.8,1.7,1.9,1.6,1.5,1.6,1.4,1.5,1.3,1.4,1.4]` (eixo 1–2,5 s, formato "1,5 s").
Texto alternativo: "RAM caiu de cerca de 1,2 GB para 296 MB depois da otimização" / "Tempo da primeira resposta caiu de 2,1 para 1,4 segundo em 14 dias".

---

## 27. Responsivo
| Largura | Mudanças |
|---|---|
| **> 1120** | Tudo visível; painel da direita disponível no Chat |
| **≤ 1120** | Painel da direita e botão `panelR` somem |
| **≤ 760** | Gaveta no lugar da barra lateral; barra superior sem captura, "Isso?", busca, separador e controles da janela; grades de 2 colunas viram 1; estatísticas em 2 colunas; bolha do usuário 90%; linha de ferramentas sem tempo; grafo sem legenda nem estatística; painel de detalhes embaixo (58%); Configurações e editor em tela cheia, sem raio nem borda, Configurações em duas etapas (lista → detalhe); `.row` quebra linha; `hbar` 96/1fr/56; `ram-legend` e `never` em 1 coluna |
Respeitar áreas seguras (`safe-area-inset`) onde existirem. O app real é desktop; o modo ≤760 existe para janelas estreitas e para um eventual cliente móvel.

---

## 28. Estado e persistência
**Preferências persistidas** (`orion4.prefs`, `collapsed`, `more`): idioma `pt`, escala 100, densidade `conf`, movimento `system`, fonte `serif`, tamanho `m`, largura `m`, tom `conciso` + texto livre, não perturbe `false`, **autônomo `false`**, notificações `{appr:true, rot:true, nuvem:true, resp:false}`, sons `{appr, resp, rot, voz: true}`, fontes `{vault, repo, cal, email, web: true}`, memória `{mem:true}`, câmera `{pedido:true, presenca:false, onde:true, rosto:true, legenda:false}`, voz `{lang: "Português (Brasil)", style: "Masculina, grave", speed: "normal", wake:true, clap:true}`, sistema `{boot:true, winO→atalho global:true, mica:true}`, nome "Antônio Salomão", apelido "Antônio", provedor `auto`, modelo `Sonnet`, esforço `medio`, projeto `Orion`, objetos (lista da §18.13).
**Estado de sessão (não persiste):** tela, conversa, rascunho, anexos, resposta em curso, filtro do histórico, sugestões, busca lateral, edição de fato, pilha voltar/avançar, abas do painel, expansão das ferramentas, voz/legenda, estado de "Pausar"/"Apagar".
**Regras:** ligar/desligar qualquer opção persiste na hora; "Sair" volta ao login limpando popovers, modais, voz e legenda.

### 28.1 Dados de exemplo (para os testes de snapshot)
- **Conversas:** *Orion RAM otimização* (Orion, Hoje, fixada, branch `claude/funny-hamilton-jb1svh`, ctx 30%, nota `03 Recursos/Orion/Memória/2026-10-15 - Orion RAM otimização.md` 09:13 commit `a3f9c12`, fixado "Meta do projeto: RAM parada abaixo de 300 MB", via "Claude Sonnet", 5 ferramentas, aprovação pendente) · *Orion para Rust* (rodando, ctx 54%, via "Orion · Groq", banner "Groq sem cota; usando Gemini.") · *Plano rodada 2* (pendente, ctx 41%, via "Gemini Flash") · *Briefing da manhã* (Rotina, nova) · *Lista do mercado* (Telegram, Ontem, via "Local · Qwen 7B") · *Revisão da PR do vault* (Vault, Ontem) · *Análise do frontend Orion* e *Organização do vault* (7 dias, vazias) · *Backup e restauração* e *Avaliação de RAG* (Mais antigos, vazias).
- **Resposta de RAM:** "Medi com o Orion parado por 10 minutos. O pico vem de três lugares, e nenhum deles precisa ficar residente o tempo todo:" + lista numerada (Índice vetorial 610 MB com `mmap` · Modelo de voz residente 280 MB · Cache de embeddings sem limite → LRU 64 MB) + bloco de código Rust (`// src/memory/index.rs`, `File::open`, `Mmap::map`, `VectorIndex::from_bytes(&map, 1024)`) + "Estimativa: de **1,2 GB** para cerca de **300 MB** parado, sem perder velocidade na busca."
- **Projetos:** Orion (Assistente local em Rust. Fase 6: front e voz. · 3 pendências · há 2 h · repo `antoniossalomao/Orion` CI ok · Claude Sonnet · `C:\Projetos\Orion` · 1 rotina) · Vault (Segundo cérebro em Obsidian, estrutura PARA. · 1 PR aberto · ontem · Gemini Flash · `C:\Users\Antonio\Vault` · 2 rotinas) · Lyra (Avaliação de RAG com o vault como base. · pausado · set. · Auto) · Nortis (Backup e restauração antes de trocar de PC. · concluído · ago. · Auto · `D:\Backup`).
- **Rotinas:** Briefing da manhã (dias úteis 08:45, crítica) · Backup do vault (todo dia 14:00, crítica) · Revisar PRs do Orion (a cada 4 h, **falhou**) · Resumo diário das conversas (23:59) · Organizar Downloads (sábado 10:00, pede aprovação, **pausada**).
- **Histórico de atividade:** 09:13 Nota gravada + commit (ok, automático) · 09:12 Leitura de `src/memory/index.rs` (ok) · 09:12 Aplicar mudanças em 3 arquivos (warn, aguardando) · 08:00 Rotina “Revisar PRs do Orion”: GitHub fora do ar (err, falhou) · 07:58 Envio da branch para o origin (warn) · ontem 23:59 Resumo diário (ok) · ontem 11:47 Apagar rascunhos em Downloads/tmp (err, negada).
- **Nuvem:** Claude “Portar o gateway de provedores para Rust” (rodando, 12 min) · Codex “Testes do crate orion-ai” (pronto, PR #31) · Claude “Briefing (PC desligado no sábado)” (pronto).
- **Fatos:** Preferências (“Respostas diretas, sem preâmbulo”, “Português, tom técnico”) · Projetos (“Orion: reescrita em Rust, fase 6”, “Vault: segundo cérebro em Obsidian”) · Rotina (“Briefing da manhã às 08:45 nos dias úteis”).
- **Fontes (descrições):** Vault “Lê as notas; grava em 03 Recursos/Orion/Memória.” · Repositórios “Código do Orion e do vault no GitHub.” · Calendário e E-mail “Só pelo chat e pelo briefing.” · Busca na web “Sem dados pessoais; não pede aprovação.”
- **MCP:** GitHub (HTTP, 41, conectado) · Google Workspace (HTTP, 18, conectado) · Arquivos locais (stdio, 9, **token expirado**) · Telegram (canal, 6, conectado · ID permitido). **Skills:** segundo-cerebro-obsidian e code-review (importadas do Claude), release-windows (própria). **Locais:** Qwen 2.5 7B Instruct Q4_K_M 4,7 GB (fallback) · Llama 3.2 3B Q4_K_M 2,0 GB · bge-m3 1,1 GB (embeddings).
- **Nota de exemplo no editor:** frontmatter `type: conversa; date: 2026-10-15; fonte: orion; projeto: "[[Orion]]"; tags: [conversa, orion, rust, memoria]; resumo: "RAM parada de 1,2 GB; três causas e plano para ~300 MB."` + `# {nome}` + `> Projeto: [[Orion]] · Conceito: [[Rust]]` + `## Pergunta` / `## Achados` / `## Decisão`.
- **Grafo:** dados reais do vault (snapshot de 15/10/2026): títulos, tipos, pastas, datas, status, tags, wikilinks; sem conteúdo; pessoas e pastas pessoais fora. No app, vem do índice do vault (F06).

---

## 29. Textos de toast (todos)
"Copiado" · "Selecione o texto para copiar" · "Fixada como contexto permanente" · "Editar e reenviar cria um ramo da conversa" · "Lendo em voz alta (simulado)" · "Gerar de novo…" · "Conversa fixada/desafixada/arquivada/apagada/desarquivada" · "A nota é gravada no fim da primeira rodada" · "Nota gravada e commit na main" · "Nota salva; commit e push na main do vault" · "Abriria no Obsidian" · "git pull do vault: 3 notas novas" · "git pull: já atualizado" · "Fato corrigido; commit na main" · "Fato esquecido; nota no vault atualizada" · "Escala N%" · "Painel atualizado" · "Tarefa enviada para a nuvem" · "Minimizado para a bandeja…" · "Fechar esconde na bandeja. As rotinas continuam." · "Gravando… clique de novo para enviar" · "Até 20 MB por arquivo…" · "Tudo pausado. Nenhuma ferramenta de escrita, rede ou execução roda." · "Histórico apagado" · "Grafo local: só a nota e os vizinhos (profundidade 1)" · "{Tipo} salva em 00 Inbox · commit na main" · "Captura anexada a uma conversa nova" · "git revert feito e push na main do vault" · "Ação revertida". Todas em português, via `orion-i18n` (Fluent), com versão em inglês.

---

## 30. Acessibilidade
- Foco visível em tudo (contorno Rigel); ordem de tab lógica; **papéis ARIA** do mockup: `dialog` + `aria-modal` (Configurações, editor, paleta, captura), `tablist/tab`, `radiogroup/radio`, `switch`, `menu/menuitem`, `listbox/option`, `status` (pílulas, mini barra, legenda), `alert` (erro de entrada), `group`, `img` com descrição no grafo e nos gráficos, `aria-pressed` nos botões de alternância, `aria-expanded` em "Mais" e ferramentas, `aria-current` na navegação.
- Rótulos para leitor de tela em todos os botões de ícone; campos com rótulo oculto (`sr`).
- Contraste AA nos textos; informação de estado nunca só por cor (ponto + texto/ícone).
- **Grafo navegável sem mouse:** busca + painel de detalhes + lista de vizinhos (mesmo requisito do mockup).
- Movimento reduzido respeitado (sistema ou preferência).

---

## 31. Mapeamento para o código
| Parte | Implementação |
|---|---|
| Tema e componentes | crate `orion-theme` (tokens §4, fontes §5) + `orion-ui` (componentes §6); estilos por classe de widget do iced; ícones SVG embutidos |
| Casca (janela, barra lateral, barra superior, gaveta) | `orion-app` com `Application`; `window-vibrancy` (Mica); `tray-icon`; `global-hotkey` (Win+Shift+Espaço/O/I) |
| Chat, composer, sugestões | `orion-ui::chat` + `orion-core` (fluxo do agente, aprovações) |
| Painel da direita | `orion-ui::ctx` (abas), terminal via PTY/PowerShell sob a política de aprovação |
| Telas de página | uma `View` por tela; listas virtualizadas |
| Grafo | widget `shader` (wgpu) com compute shader (layout) e glyphon (rótulos); fallback CPU (Barnes-Hut); a mesma interação da §16 |
| Editor de nota | `text_editor` do iced + prévia Markdown (pulldown-cmark → widgets) |
| Configurações | `orion-ui::settings` com registro declarativo das seções (§18), busca por palavras-chave |
| Paleta, captura rápida, "Isso?" | janelas/overlays do iced; captura de tela via `windows-capture`/GDI; região por seleção |
| Voz e legenda | `orion-voice` (oww-rs, whisper-rs, aec3, Silero VAD, edge-tts) + overlay sem foco |
| Céu | motor compartilhado do instalador (F01.29) |
| Testes | headless/E2E do iced + **snapshot PNG por tela e estado**, quadro fixo e dados de exemplo da §28, comparado ao protótipo |

### 31.1 Fases do plano que cobrem esta nota
F01 (casca, tema, barra lateral/superior, Início, paleta, motor do céu) · F02.B (login) · F04 (aprovações, modo autônomo, pausa) · F05 (composer, mensagens, sugestões, histórico) · F06 (Memória 2D de dados, editor, Conhecimento, nota no vault) · F07 (Configurações) · F08 (Projetos) · F09 (Painel e telas auxiliares: Resultados, Fontes, Arquivadas, Vida) · F12 (Rotinas, Nuvem) · F13 (Uso e cotas) · F15 (voz, mini barra, legenda) · F16 (grafo 3D) · F17 (efeitos de vidro) · F18 (câmera) · F19 (Conectores) · F20 (captura rápida e "Isso?").
**Ações no plano:** corrigir F01.02 (fontes), F01.19 (lista da barra lateral), F01.14 (atalho já correto) e F07.A (grupos); referenciar esta nota em F01, F05, F06, F07, F08 e F09.

---

## 32. Critérios de aceite
- [~] Lado a lado com o protótipo (mesma janela, mesmos dados da §28), **cada tela e estado** é visualmente igual: posições, cores, fontes, espaçamentos, raios, sombras.
- [~] Tokens e fontes exatamente como §4 e §5 (Geist, Geist Mono, Newsreader embutidas).
- [~] Barra lateral: 272 px, recolhe com Ctrl+B, grupos de conversas, indicadores e "Mais" persistido.
- [~] Barra superior: pílulas (Câmera, Autônomo, Pensando/Ouvindo/Aguardando), captura rápida, "Isso?", Ctrl K, painel, controles de janela.
- [~] Composer: cresce até 220 px, `/` e `@` com navegação por teclado, anexos, parar resposta, repetir última com ↑.
- [~] Resposta: ferramentas recolhíveis, aprovação (3 estados), ações e "via" corretos; streaming com cursor.
- [~] Painel da direita: 4 abas, abre sozinho com aprovação pendente, some < 1120 px.
- [~] Todas as 13 telas de página/grafo conforme §15–16; filtro e desfazer do histórico; rotinas com `runs`.
- [~] Grafo: cores, raios, brilho, opacidade por foco, rótulos sem colisão, busca, filtros, "Lidas agora", linha do tempo, detalhes e grafo local — idêntico em comportamento ao protótipo, em 3D.
- [~] Editor com prévia ao vivo e frontmatter; "Salvar e commit".
- [~] Configurações: 14 seções, busca por palavra-chave, **todas** as linhas da §18 com seus textos e comportamentos (Pausar tudo e Apagar com confirmação em 2 passos).
- [~] Paleta Ctrl K com a ordem de resultados da §19; captura rápida com detecção do tipo; "Isso?" com seleção e resposta.
- [~] Mini barra de voz, Cinturão verde com voz ao vivo, legenda ao vivo.
- [~] Escala 90–150%, densidade, movimento reduzido, largura/tamanho/fonte do chat funcionam como no protótipo.
- [~] Responsivo: gaveta ≤ 760, painel da direita some ≤ 1120.
- [~] Atalhos corrigidos (U3, U4) e login conforme decisão (U1).
- [ ] 60 fps na interação, 20 fps parado no Início; RAM parada < 300 MB; janela pronta < 1 s.
- [ ] Nenhum item da §2 permanece divergente sem registro.
