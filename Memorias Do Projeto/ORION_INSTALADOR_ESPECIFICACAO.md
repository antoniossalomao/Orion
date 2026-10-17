---
type: recurso
projeto: "[[Orion]]"
tags: [orion, instalador, design, iced, wgpu, especificacao]
date: 2026-10-16
resumo: "Especificação completa do Orion Setup.exe para reproduzir em iced/wgpu exatamente o protótipo aprovado: janela, tokens, tipografia, 5 etapas, textos, transições, céu animado (constantes, fórmulas e ordem de desenho), som ambiente, fluxo de instalação e critérios de aceite."
---

# ORION - INSTALADOR (ESPECIFICAÇÃO)

> Projeto: [[Orion]] · Planejamento geral: [[ORION - PLANEJAMENTO ARQUITETURA]] (seção "Instalador (16/10/2026, rodada 11)") · Referências: [[ORION - REFERÊNCIAS DE PESQUISA]] · Conceito: [[Rust]]
> **Fonte da verdade visual:** protótipo aprovado em 16/10/2026, versão 3 → https://claude.ai/artifact/EZNV5ftcDLDRjiZjD9uvco
> **Cópia do protótipo no vault:** `01 Projetos/Orion/Protótipos/orion-instalador.html` (abrir no navegador para comparar lado a lado).
> Regra: **o instalador em iced tem que ficar idêntico ao protótipo.** Em qualquer divergência entre esta nota e o HTML, **vale o HTML**; corrigir a nota.

---

## 1. Objetivo e escopo
- Programa: **`Orion Setup.exe`**, feito em **Rust + iced 0.14**, céu desenhado em **wgpu** (widget `shader` do iced).
- Faz: checagem do PC, escolha de pastas, escolha de modelos locais, opções, instalação (cópia, identidade de pacote, certificado, atalhos, serviço de atualização), agendamento do download dos modelos e botão **Abrir Orion**.
- **Não faz:** onboarding (perfil, senha, 2FA, vault, chaves). Isso é do app, no primeiro uso.
- Mesma arte, tokens e animação reaproveitados depois na **mini tela de atualização** ("O que mudou") e, em versão reduzida, na **tela de bloqueio/login**.

---

## 2. Janela
| Propriedade | Valor |
|---|---|
| Tamanho | **960 × 600 px** (lógicos), **fixo**, não redimensionável. Nunca muda de tamanho entre etapas. |
| Decoração | Sem moldura do Windows (`decorations: false`), cantos arredondados **12 px** (Windows 11 `DWMWCP_ROUND`), sombra do sistema. |
| Borda | 1 px `--line` + anel sutil `rgba(164,179,222,.04)`. |
| Sombra (protótipo) | `0 40px 100px rgba(0,0,0,.6)`; no app, usar a sombra nativa do DWM. |
| Posição | Centralizada no monitor principal. |
| Fundo | `--side` (#111214). Sem Mica no instalador (o céu precisa de fundo opaco). |
| DPI | Respeitar a escala do Windows; canvas do céu com no máximo **2×** de densidade. |
| Arrastar | A barra de título (36 px) arrasta a janela. |

### 2.1 Barra de título (36 px)
- Fundo `#0e0f11`, borda inferior 1 px `--line`, padding `0 6px 0 14px`.
- Esquerda: `✦ Orion Setup` em 12 px `--fg2` + ` · v0.1.0` em `--fg3`.
- Direita: dois botões 40 × 30 px, raio 6 px, texto `--fg3`: **—** (minimizar) e **✕** (fechar). Hover/foco: fundo `--raise`, texto `--fg`.
- Fechar durante a instalação: pergunta **"Cancelar a instalação?"** (Cancelar / Continuar instalando) e desfaz o que já foi feito.

### 2.2 Corpo (564 px de altura)
Grid de 2 colunas: **esquerda `1fr` (céu)** e **direita `1.05fr` (painel)** → em 960 px: céu ≈ 468 px, painel ≈ 492 px. Divisória: 1 px `--line` à direita do céu.

---

## 3. Tokens (tema único Grafite escuro)
| Token | Valor | Uso |
|---|---|---|
| `bg` | `#0a0b0e` | fundo fora da janela (só no protótipo) |
| `side` | `#111214` | fundo da janela e do painel |
| `raise` | `#1b1c1f` | campos, cartões, botão secundário |
| `line` | `#25272c` | bordas e divisórias |
| `fg` / `fg2` / `fg3` | `#ececee` / `#a9aab0` / `#75767d` | texto principal / secundário / legenda |
| `rigel` | `#a4b3de` | foco, seleção, etapas concluídas, switches |
| `betel` | `#e9b467` | etapa atual, selo "recomendado", nome da estrela, alerta |
| `primary` | `#b9c6ea` | botão principal |
| `onprimary` | `#0f1320` | texto sobre `primary` |
| `ok` | `#74cfae` | itens concluídos, checagem OK |
| céu | `#07080b` | fundo do canvas |
| curva padrão | `cubic-bezier(.22,.61,.36,1)` | todas as transições de interface |

## 4. Tipografia
- **Inter** (400, 500, 600) e **JetBrains Mono** (400, 500), **embutidas no binário**.
| Elemento | Fonte | Tamanho | Peso | Cor | Extra |
|---|---|---|---|---|---|
| Título da etapa (h1) | Inter | 24 | 600 | fg | quebra balanceada |
| Texto de apoio (lead) | Inter | 14 | 400 | fg2 | entrelinha 1,55; máx. ~52 caracteres por linha |
| Eyebrow | JetBrains Mono | 11 | 400 | fg3 | MAIÚSCULAS, espaçamento .08em |
| Rótulo de campo | Inter | 12 | 400 | fg2 | |
| Dica | Inter | 12 | 400 | fg3 | |
| Campo de texto | JetBrains Mono | 13 | 400 | fg | |
| Botões | Inter | 13 | 500 (secundário) / 600 (principal) | | |
| Rodapé (meta) | JetBrains Mono | 11 | 400 | fg3 | |
| Nome "Orion" no céu | Inter | 22 | 600 | fg | espaçamento .04em |
| Legenda da etapa no céu | JetBrains Mono | 11 | 400/500 | fg3 / betel | entrelinha 1,6, alinhada à direita |

---

## 5. Painel direito (layout fixo)
- Padding **28 px** em cima e dos lados, **20 px** embaixo. Coluna com espaço de **18 px** entre blocos.
- Ordem de cima para baixo:
  1. **Barra de etapas** (altura 3 px).
  2. **Palco** (`stage`): ocupa todo o espaço restante; o conteúdo da etapa fica posicionado dentro dele e **rola por dentro** se passar da altura (scroll vertical, padding direito 4 px para a barra de rolagem). É isso que impede a janela de mudar de tamanho.
  3. **Rodapé:** borda superior 1 px `--line`, padding superior 16 px; à esquerda o texto meta, à direita os botões **Voltar** e o botão principal, com 8 px entre eles.

### 5.1 Barra de etapas
- 5 segmentos iguais, espaço de 6 px, altura 3 px, raio 2 px, fundo `--line`.
- Preenchimento por cima, **cresce da esquerda para a direita** (`scaleX` 0 → 1) em **0,7 s** com a curva padrão.
- Concluída: `rigel`. Atual: `betel` (a troca de cor dura 0,4 s). Futura: vazia.

### 5.2 Botões
- **Principal:** fundo `primary`, texto `onprimary`, raio 8 px, padding 9 × 18 px; hover: brilho +6%; desativado: opacidade .4.
- **Secundário (Voltar, Alterar):** fundo `raise`, borda 1 px `line`, raio 8 px, padding 8 × 12 px.
- **Voltar escondido** nas etapas 1 e 5: some por **opacidade** (0,3 s), **sem sair do layout**, para o rodapé não pular.
- Foco do teclado: contorno 2 px `fg`, deslocado 2 px.

### 5.3 Componentes
- **Campo:** fundo `raise`, borda 1 px `line`, raio 8 px, padding 9 × 11 px; foco: contorno 2 px `rigel`.
- **Cartão de checagem:** fundo `raise`, borda `line`, raio 8 px, padding 10 × 12 px; rótulo 11 px `fg3`; valor mono 13 px precedido de **●** verde (`ok`) ou âmbar (`betel`).
- **Cartão de modelo:** fundo `raise`, borda `line`, raio 10 px, padding 10 × 12 px, espaço interno 6 px. Selecionado: borda `rigel`, fundo `#1c2030` (transição 0,25 s). Caixa de seleção 16 × 16 px, raio 4 px; marcada: fundo `rigel` com ✓ `onprimary`. Selo **"recomendado p/ seu PC"**: 10 px, peso 600, texto `betel`, borda `rgba(233,180,103,.4)`, formato pílula. Linha meta: mono 11 px `fg3` com "X GB" e "RAM ~Y GB".
- **Switch:** 36 × 20 px, pílula; bolinha 14 px; desligado: trilho `line`, bolinha `fg2`; ligado: trilho `rigel`, bolinha `onprimary` à direita. Animação 0,25 s.
- **Barra de progresso:** altura 6 px, trilho `raise`, preenchimento em gradiente `rigel → primary`, largura anima em **0,9 s**.

---

## 6. Etapas, textos e dados
Textos **exatamente** como abaixo (pt-BR; o inglês vem dos arquivos Fluent).

### Etapa 1 — Boas-vindas · estrela: **Betelgeuse**
- Eyebrow: `BOAS-VINDAS` · Título: **Vamos instalar o Orion**
- Lead: "Seu assistente de desktop. Leva uns 2 minutos. Os modelos locais baixam depois, em segundo plano, e o Orion já abre usando as APIs enquanto isso."
- Campo **Idioma**: seletor (Português (Brasil) / English).
- Eyebrow `SEU PC` + grade 2 × 2 de checagens: **Sistema** (versão do Windows), **Memória** (RAM total), **GPU** (modelo · VRAM · Vulkan/CUDA), **Espaço livre** (por disco). Verde se ok, âmbar se atenção (ex.: C: com menos de 60 GB).
- Dica dinâmica explicando o âmbar (ex.: "Como C: tem pouco espaço, os modelos vão para D:.").
- Rodapé: "Instalação para todos os usuários" · Botão: **Continuar**.
- Bloqueio: Windows 10 ou anterior → mensagem "O Orion precisa do Windows 11." e o botão fica desativado.

### Etapa 2 — Pastas · estrela: **Bellatrix**
- Eyebrow `ONDE INSTALAR` · Título: **Escolha as pastas**
- **App:** `C:\Program Files\Orion` + Alterar · dica "~150 MB".
- **Modelos locais:** disco com mais espaço livre + `\Orion\Modelos` + Alterar · dica "Sugerido: D: tem mais espaço livre (610 GB)." (valores reais).
- **Dados (banco cifrado e configurações):** `%ProgramData%\Orion`, só leitura · dica "O vault do Obsidian é escolhido no primeiro uso."
- Rodapé: "Pede permissão de administrador uma vez" · Botão: **Continuar**.

### Etapa 3 — Modelos locais · estrela: **Cinturão** (Alnitak → Alnilam → Mintaka)
- Eyebrow `MODELOS LOCAIS` · Título: **O que roda no seu PC**
- Lead: "Todos ficam descarregados e só ocupam RAM quando são chamados. Dá para mudar depois em Configurações › Modelos locais."
- Grade 2 colunas, espaço 8 px, 8 cartões:
| Modelo | Descrição | GB | RAM | Recomendado (PC exemplo) | Marcado por padrão |
|---|---|---|---|---|---|
| Whisper large-v3-turbo | Reconhecimento de voz em português. | 0,8 | ~2 GB | sim | sim |
| Qwen 3 8B | Geral e ferramentas. Bom em pt-BR. | 5 | ~6 GB | sim | sim |
| Gemma 4 E4B | Leve, ideal para o notebook. | 4 | ~5 GB | sim | não |
| Granite 4.1 8B | Melhor tool calling medido. | 5 | ~6 GB | não | não |
| Phi-4-mini 3.8B | Rápido, respostas curtas. | 2,4 | ~3 GB | não | não |
| Moondream 2 | Visão leve. "Isso?" sem internet. | 1,8 | ~2 GB | sim | sim |
| MiniCPM-V 4.5 | Visão e OCR de documentos. | 5,5 | ~6 GB | não | não |
| Mistral Small 3.2 24B | Mais forte. Só no PC com GPU grande. | 14 | ~16 GB | não | não |
- Regra do selo/pré-marcação: decidida pela RAM e VRAM detectadas (tabela de requisitos por modelo no código; o exemplo acima é para 32 GB RAM + 12 GB VRAM).
- Linha de total (borda superior, mono 12 px `fg2`): "N selecionados · X GB" à esquerda, "baixa depois, em segundo plano" à direita. Atualiza **sem recriar a tela** (só o cartão e o total mudam).
- Rodapé: "Detectado: 32 GB RAM, GPU 12 GB" (valores reais) · Botão: **Continuar**.

### Etapa 4 — Opções · estrela: **Saiph**
- Eyebrow `OPÇÕES` · Título: **Últimos ajustes**
- Linhas com switch (padding 11 px vertical, divisória `line`):
  - **Iniciar com o Windows** — "Abre minimizado na bandeja" — ligado
  - **Ativar voz** — "Palavra "Orion" e duas palmas" — ligado
  - **Fixar na barra de tarefas** — "O atalho no Menu Iniciar é sempre criado" — ligado
  - **Enviar relatório de falhas** — "Só para a automelhoria do Orion" — desligado
- Rodapé: "Tudo pode ser mudado depois" · Botão: **Instalar** (pede UAC aqui).

### Etapa 5 — Instalando · estrela: **Rigel**
- Eyebrow `INSTALANDO` · Título: **Formando a constelação…**
- Barra de progresso + log (mono 12 px, espaço 6 px). Cada item entra como "… texto" em `fg2` e vira "✓ texto" em `ok`:
  1. Copiando o Orion para Arquivos de Programas
  2. Registrando a identidade do app (OCR e avisos)
  3. Instalando o certificado do Orion
  4. Criando atalhos no Menu Iniciar e na barra
  5. Configurando o serviço de atualização
  6. Agendando o download dos modelos (X GB)
- No protótipo cada item leva 0,9 s; **no app o ritmo é o real**, mas cada item fica visível **no mínimo 0,6 s** para não piscar.
- Ao terminar: título vira **"Pronto. O Orion está no céu."**, rodapé "Os modelos continuam baixando no app", botão **Abrir Orion** habilitado (desativado durante a instalação). Voltar fica escondido.
- **Abrir Orion:** fecha o instalador com fade de 0,3 s e abre o app no onboarding.
- Erro em qualquer item: o item fica "✕ texto" em `danger` (#f07c88), título "Algo deu errado", botões **Tentar de novo** e **Ver detalhes** (log completo); desfaz o que foi feito.

---

## 7. Transição entre etapas (exatamente assim)
1. Clique em Continuar/Voltar → trava novos cliques (`busy`).
2. **Saída (0,34 s):** conteúdo atual vai de opacidade 1 → 0 e sobe **8 px** (curva padrão).
3. Troca o conteúdo, volta o scroll ao topo, atualiza rodapé, botão e barra de etapas.
4. **Entrada (0,34 s):** novo conteúdo começa com opacidade 0 e **10 px abaixo**, vai para opacidade 1 e posição 0.
5. Ao mesmo tempo: o segmento da barra preenche (0,7 s); o **nome da estrela** no céu some (0,25 s), troca o texto e reaparece (0,5 s); a próxima estrela começa a acender.
6. Libera cliques no fim da entrada. Ao entrar na etapa 5, a instalação começa.
- Implementação iced: estado `Transition { from, to, phase: Out|In, t }` avançado por `iced::time::every`/`window::frames`, com opacidade e deslocamento aplicados ao container do palco; usar a API de animação do iced 0.14.

---

## 8. Céu animado (canvas wgpu)
Área: toda a coluna esquerda (≈ 468 × 564 px). Coordenadas das estrelas em **frações** (0–1) da área. Tudo em `f32`, tempo `sec` em segundos, `dt` limitado a **0,05 s**.

### 8.1 Ordem de desenho por quadro
1. Fundo sólido `#07080b`.
2. **Vinheta:** gradiente radial centrado em (0,45W; 0,45H), raio 0,8·max(W,H), de `rgba(22,26,40,.9)` no centro a transparente.
3. A partir daqui, **mistura aditiva** ("lighter").
4. Poeira da Via Láctea.
5. Nebulosa de Órion (M42).
6. Estrelas de fundo.
7. Atualização do brilho das estrelas da constelação.
8. Linhas da constelação (trilha apagada em mistura normal, linha acesa em aditiva).
9. Estrelas da constelação.
10. Brilho de conclusão.
11. Estrela cadente.

### 8.2 Sprites de brilho (pré-renderizados, 128 × 128)
Gradiente radial do centro à borda com paradas: 0 → α 1 · 0,08 → α .75 · 0,25 → α .22 · 0,55 → α .05 · 1 → α 0. Cores RGB:
| Sprite | RGB |
|---|---|
| warm | 240,190,120 |
| cool | 176,192,236 |
| white | 232,236,248 |
| neb1 | 110,96,180 |
| neb2 | 214,128,140 |
Desenhar = textura centrada em (x,y) com lado `size` e opacidade `alpha` (pular se α ≤ 0,002). No wgpu: quads instanciados com essa textura e blend aditivo.

### 8.3 Câmera (respiração)
`k = 0` se "reduzir movimento" estiver ativo no Windows, senão `1`.
- Centro: cx = 0,47W, cy = 0,5H.
- `zoom = 1 + k·(0,012·sin(sec·0,11) + 0,03·done)`
- `rot = k·0,012·sin(sec·0,05)` (radianos)
- `ox = k·sin(sec·0,07)·5` · `oy = k·cos(sec·0,09)·4` (px)
- Projeção de um ponto (x,y) com profundidade z: `dx=(x·W−cx)·zoom`, `dy=(y·H−cy)·zoom`; gira por `rot·z`; soma `ox·z`, `oy·z`. Camadas mais distantes têm z menor (parallax).

### 8.4 Estrelas de fundo (260, gerador fixo)
- Gerador: **Park–Miller** `seed = seed·16807 mod 2147483647`, seed inicial **7**, valor = seed/2147483647. **Usar exatamente este gerador e esta ordem de chamadas** para o céu ficar idêntico ao protótipo.
- Por estrela, na ordem: `z=rnd()`, `x=rnd()`, `y=rnd()`, raio `0,25+z²·1,1`, alfa `0,12+z·0,55`, fase `rnd()·6,28`, velocidade `0,15+rnd()·0,35`, cor: `rnd()<0,15` → 255,220,190 (quente); senão `rnd()<0,25` → 190,205,255 (fria); senão 225,230,242.
- Profundidade na projeção: `0,3 + z·0,7`.
- Cintilação: `tw = 0,75 + 0,25·sin(sec·vel·3 + fase)` (1 sem movimento). Círculo de raio r, cor da estrela, opacidade `alfa·tw`.
- Estrelas com z > 0,85 ganham halo: sprite white, lado `r·14`, α `0,10·tw`.

### 8.5 Via Láctea (90 manchas, mesmo gerador, logo depois das estrelas)
- Por mancha: `t=rnd()`, x=t, y=`0,15 + t·0,75 + (rnd()−0,5)·0,18`, raio `20+rnd()·50`, α `0,012+rnd()·0,02`.
- Desenho: sprite cool, lado `r·4`, profundidade 0,2. Forma uma faixa diagonal tênue.

### 8.6 Nebulosa de Órion (M42)
- Âncora: (0,50; 0,64), profundidade 0,6. `m = min(W,H)`, `nb = 0,55 + 0,15·done`.
- neb1: posição (ax + 6·sin(sec·0,13·k), ay), lado 0,75m, α 0,20·nb.
- neb2: posição (ax+10, ay+8+5·cos(sec·0,1·k)), lado 0,38m, α 0,18·nb.
- núcleo: sprite white em (ax+4, ay+4), lado 0,10m, α 0,10·nb.

### 8.7 Constelação
| Estrela | x | y | raio | sprite | núcleo | etapa | atraso |
|---|---|---|---|---|---|---|---|
| Betelgeuse | 0,30 | 0,22 | 3,2 | warm | #ffd9a0 | 1 | 0 |
| Bellatrix | 0,66 | 0,26 | 2,2 | cool | #e6ecff | 2 | 0 |
| Alnitak | 0,40 | 0,52 | 2,4 | white | #f2f4ff | 3 | 0 s |
| Alnilam | 0,475 | 0,50 | 2,6 | white | #f2f4ff | 3 | 0,35 s |
| Mintaka | 0,55 | 0,48 | 2,3 | white | #f2f4ff | 3 | 0,70 s |
| Saiph | 0,64 | 0,80 | 2,2 | cool | #e6ecff | 4 | 0 |
| Rigel | 0,28 | 0,78 | 3,4 | cool | #eef2ff | 5 | 0 |

**Linhas** (índices da tabela, 0 = Betelgeuse): `0–2`, `2–3`, `3–4`, `1–4`, `0–1`, `4–5`, `2–6`.

**Quando acende:** `lit` = nº de etapas acesas. Começa em **1** (Betelgeuse acesa ao abrir). Ao entrar na etapa N, `lit = max(lit, min(N, 4))`; ao concluir a instalação, `lit = 5` (Rigel). Voltar de etapa **não apaga** estrelas.

**Brilho (`b`, 0–1)** com suavização exponencial independente da taxa de quadros:
`b ← b + (alvo − b)·(1 − e^(−taxa·dt))`, taxa **1,6** para acender e **4** para apagar. O alvo vira 1 quando `sec ≥ t0 + atraso` (t0 = instante em que a etapa acendeu). Idade = `sec − t0`.

**Desenho de cada estrela** (centro projetado (x,y), r = raio):
- `tw = 1 + 0,06·sin(sec·1,3 + x_frac·20)` (1 sem movimento).
- Apagada: ponto `#8a90a8`, raio 0,6r, α `0,25·(1−b)`. Se b < 0,01, para aqui.
- **Nascimento:** `birth = sin(min(idade/2,4; 1)·π)` nos primeiros **2,4 s** (0 depois e sem movimento): sobe e desce suave, sem flash.
- Halo colorido: sprite da estrela, lado `r·(26 + 18·birth + 6·done)·tw`, α `b·(0,55 + 0,35·birth)`.
- Halo branco: sprite white, lado `r·7·tw`, α `b·0,8`.
- **Raios de difração:** cruz com comprimento `len = r·(5 + 4·birth)·tw` (o braço vertical tem 80% do horizontal), espessura 0,8 px, gradiente transparente → cor do núcleo → transparente, α `b·(0,28 + 0,25·birth)`, girando devagar a `sec·0,03` rad.
- Núcleo: círculo raio 0,75r, cor do núcleo, α b.

### 8.8 Linhas
- Trilha apagada (sempre): mistura normal, α 0,06, cor `#8a90a8`, 1 px.
- Fica "pronta" quando as duas pontas têm b > 0,85. Progresso `p` sobe `dt/1,6` por quadro (1,6 s para atravessar) e desce `dt·2` se perder a condição.
- Posição da ponta: curva **ease-in-out cúbica** `e = p<0,5 ? 4p³ : 1−(−2p+2)³/2`.
- Linha acesa com gradiente do início (`rgba(164,179,222,.05)`) até a ponta (`rgba(190,204,240,.55)`), pontas arredondadas, em **duas passadas**: halo de 4 px com α `0,35·(0,7+0,3·done)` + fio de 1 px com α 0,9.
- Enquanto cresce (p < 1): ponto de luz na ponta, sprite white lado 28, α `0,7·(1−0,6p)`.

### 8.9 Conclusão
- `done` (0–1) suaviza para 1 com taxa **0,8** quando `lit ≥ 5` **e** Rigel tem b > 0,9.
- Efeitos: zoom +3% (já na câmera), halos maiores, nebulosa mais forte, linhas mais visíveis e um brilho geral: sprite cool centrado em (0,47; 0,5), lado `1,1·max(W,H)`, α `0,10·done·(0,85 + 0,15·sin(sec·0,8))` — respira para sempre.

### 8.10 Estrela cadente
- Primeira após **5 s**; depois a cada **9–18 s** (aleatório). Nunca com "reduzir movimento".
- Nasce em x ∈ [0,35; 0,90], y ∈ [0,04; 0,29]; dura **1,3 s**.
- Movimento: `e = 1−(1−f)²`; posição (x−0,3e)·W, (y+0,2e)·H; intensidade `a = sin(f·π)`.
- Rastro de 90 × 60 px para cima e para a direita, gradiente de `rgba(240,242,250,.8a)` a transparente, 1,2 px; cabeça: sprite white lado 14, α 0,8a.

### 8.11 Legenda do céu
- Canto inferior esquerdo: "Orion"; canto inferior direito: "Etapa N de 5" e, abaixo, o nome da estrela em `betel` (Betelgeuse, Bellatrix, Cinturão, Saiph, Rigel). Margem 22 px dos lados, 20 px de baixo. Troca do nome com fade (seção 7).

### 8.12 Desempenho
- Meta **60 fps** com menos de 5% de CPU num PC médio; tudo na GPU (quads instanciados + 1 passada de linhas).
- Janela minimizada ou sem foco por 30 s: cai para 20 fps; minimizada: pausa o céu.
- "Reduzir movimento" (Windows › Acessibilidade › Efeitos de animação desligados): sem deriva, sem cintilação, sem nascimento animado, sem estrela cadente; brilhos aparecem direto; transições de etapa instantâneas.

---

## 9. Som ambiente (sempre ligado, sem botão)
- Começa **ao abrir o instalador** (no app nativo não há a trava de primeiro clique do navegador).
- Volume sobe de 0 a **0,028** (ganho linear) em **4 s**.
- Síntese em tempo real (sem arquivo de áudio): 4 osciladores → filtro **passa-baixas 900 Hz** → saída.
| Voz | Frequência | Forma | Desafinação | Ganho base | LFO no ganho |
|---|---|---|---|---|---|
| 1 | 110 Hz | senoide | −6 cents | 0,8 | 0,04 Hz, profundidade 0,35 |
| 2 | 164,8 Hz | triangular | −2 cents | 0,8 | 0,065 Hz, 0,35 |
| 3 | 220 Hz | senoide | +2 cents | 0,8 | 0,09 Hz, 0,35 |
| 4 | 329,6 Hz | triangular | +6 cents | 0,25 | 0,115 Hz, 0,35 |
- Implementação: `cpal` (stream de saída) com síntese própria em Rust, ou `rodio` com fontes customizadas.
- Respeita o volume do sistema e o "não perturbe" não se aplica (instalador). Ao fechar: fade-out de 0,5 s.

---

## 10. Fluxo técnico da instalação
1. **Checagens** (etapa 1): versão do Windows, RAM (`GlobalMemoryStatusEx`), GPU/VRAM (DXGI) e suporte a Vulkan, espaço por disco.
2. **UAC** ao clicar em Instalar (etapa 4): o Setup se reinicia elevado e mantém as escolhas.
3. Copiar o app para a pasta escolhida.
4. Registrar o **pacote esparso** (identidade de pacote: OCR do Windows, avisos com botões).
5. Instalar o **certificado autoassinado** do Orion (repositório de máquina "Pessoas Confiáveis").
6. Criar o atalho no Menu Iniciar e fixar na barra de tarefas (se marcado); registrar "Iniciar com o Windows" (se marcado); registrar o desinstalador em Configurações › Aplicativos.
7. Instalar o **serviço de atualização** (atualiza sem UAC depois).
8. Gravar a lista de modelos escolhidos e a pasta; o **download acontece no app**, em segundo plano, com pausa, retomada e verificação de hash.
9. Cada passo tem **desfazer** registrado; erro ou cancelamento desfaz em ordem inversa.

---

## 11. Acessibilidade e teclado
- Tab percorre: campos, cartões de modelo, switches, Voltar, botão principal. **Enter** = botão principal; **Esc** = Voltar (ou cancelar na etapa 5, com confirmação).
- Cartões de modelo e switches com estado anunciado ao leitor de tela (marcado/desmarcado, ligado/desligado).
- Contraste mínimo AA em todos os textos; foco sempre visível.
- Escala do Windows de 100% a 200% sem cortar nada (o palco rola).

---

## 12. Mapeamento para o código
| Parte | Implementação |
|---|---|
| App | crate `orion-setup` no workspace, iced 0.14, `Application` com estado `Setup { step, transition, choices, pc, install }` |
| Céu | widget `shader` do iced → pipeline wgpu com: quads instanciados (sprites), pontos (estrelas), linhas com gradiente; uniforms `sec`, `zoom`, `rot`, `ox`, `oy`, `done` |
| Tempo | `window::frames()` para o céu; transições com a API de animação do iced 0.14 |
| Texto no céu | camada de widgets do iced por cima do shader (não renderizar texto no wgpu) |
| Som | `cpal` + síntese própria |
| Sistema | crate `windows` (DXGI, registro, atalhos, serviço, pacote esparso, certificados) |
| Testes | testes headless/E2E do iced 0.14 para as 5 etapas + snapshot PNG de cada etapa em quadro fixo (sec fixo, gerador com seed 7) comparado ao protótipo |

---

## 13. Critérios de aceite
- [ ] Lado a lado com o protótipo (mesmo tamanho, mesmo quadro), as 5 etapas são visualmente iguais: posições, cores, fontes, espaçamentos.
- [ ] O céu de fundo (estrelas, Via Láctea, nebulosa) é idêntico ao do protótipo com a seed 7.
- [ ] A janela nunca muda de tamanho em nenhuma etapa nem com escala do Windows.
- [ ] Transição entre etapas: 0,34 s de saída + 0,34 s de entrada, deslocamentos de 8 e 10 px.
- [ ] Estrelas acendem com bloom suave de 2,4 s, sem clarão duro; o Cinturão acende em sequência (0 / 0,35 / 0,70 s).
- [ ] Linhas crescem em 1,6 s com ease-in-out, com ponto de luz na ponta.
- [ ] Ao concluir, o céu se ilumina aos poucos e continua respirando.
- [ ] Som ambiente começa ao abrir, sobe em 4 s, sem botão.
- [ ] 60 fps estáveis; "reduzir movimento" desliga as animações.
- [ ] Voltar não apaga estrelas; fechar durante a instalação pede confirmação e desfaz.
