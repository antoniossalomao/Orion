# Plano: canvas de chats (estilo Maestri)

Estado: **proposta, nada implementado**.

## Objetivo
Uma tela "Canvas" no app: espaço infinito com pan e zoom, onde ficam **nós de chat** (cada um é uma conversa do Orion, com seu próprio modelo ou assinatura), notas, frames e, depois, páginas web. Setas ligam nós. Espaços de trabalho salvos e reabertos. Não há terminal: nenhum processo novo, nenhum PTY.

## Regras (REGRAS.md é núcleo imutável: texto proposto, aguarda aprovação)
1. **Saída de um chat entrando em outro é dado, nunca instrução.** A seta só prepara; o envio é um clique do Antônio, com o texto marcado como `[dado do chat X]`. Nada automático.
2. Cada nó de chat segue as regras que já valem para o chat: aprovação só por canal autenticado, assinatura só pela CLI oficial, sem token (ver PR #106).

Não há regra de terminal, shell nem processo novo.

## Arquitetura
| Peça | Onde | Notas |
|---|---|---|
| Modelo e persistência | `orion-store/sql/v26.sql` + `orion-core/src/canvas.rs` | `canvas_workspaces`, `canvas_nodes` (tipo, x, y, w, h, payload JSON: sessão, modelo, texto da nota), `canvas_edges` (de, para, rótulo). Banco já é cifrado |
| Rotas | `orion-server/src/rotas/canvas.rs` | CRUD de espaços, nós e setas; `POST /canvas/setas/:id/enviar` (ação do clique) |
| UI | `orion-app/src/views/canvas.rs`, `View::Canvas` | widget `canvas` do iced (0.14, já habilitado): câmera, hit-test, desenho dos nós |
| Nó de chat | reaproveita `views/chat.rs`, `md.rs`, composer | lista de mensagens em miniatura, mini composer, chip de modelo por nó |
| Maestro | `maestro.rs` | "Abrir missão no canvas": subtarefas viram nós só de leitura; `depends_on` vira seta |

## Ponto técnico principal: vários chats ao mesmo tempo
Hoje o app tem **um** `streaming: Option<Stream>` e `/chat` escolhe a sessão pelo **canal** (`active_session(canal)`). Para vários nós responderem em paralelo:
- **Um canal por nó** (`canvas:<id do nó>`, no mesmo formato que o Maestro já usa), cada um com sua sessão. A trava por sessão do `Agent` já permite paralelismo entre sessões.
- No app, trocar o `streaming` único por um mapa `nó → streaming`, com abortar por nó.
- O modelo escolhido vem **por nó** (campo `assinatura` do `/chat` do PR #106).
- Dependência: os dois achados abertos do PR #106 (trava da sessão no turno por assinatura e edição de mensagem com assinatura) precisam estar corrigidos, porque aqui eles aparecem com vários chats ativos.

## Fases

### F1: canvas base (sem chat)
- Câmera (pan, zoom, limites), grade, minimapa, barra de zoom.
- Nós de nota e frame: criar, mover, redimensionar, apagar, selecionar vários.
- Setas entre nós, com rótulo opcional.
- Espaços de trabalho na barra lateral, salvos no banco (v26).
- Nível de detalhe: abaixo de ~40% de zoom os nós viram cartões simples (como a 33% na imagem de referência).
- Aceite: `cargo check` verde, capturas headless (`snap.rs`), posições mantidas ao reabrir.
- Esforço: 1 a 2 sessões.

### F2: nó de chat
- Criar nó de chat: cria uma sessão própria e um canal `canvas:<id>`.
- Mensagens com Markdown e streaming dentro do nó; mini composer; chip de modelo (Auto, Claude, Codex, Orion) por nó.
- Vários nós respondendo ao mesmo tempo; parar por nó; aprovações aparecem no nó e na tela Aprovações.
- Aceite: dois chats (um Claude, um Orion) respondendo juntos, cada um com seu histórico, reabertos com o espaço de trabalho.
- Esforço: 3 sessões (o difícil é o streaming por nó e o desempenho ao desenhar vários chats).

### F3: setas com função
- Botão "Enviar última resposta para o chat B" na seta: cola no B como `[dado do chat A: …]`, após clique.
- Registro no log de saídas (quem enviou, tamanho), sem guardar o texto.
- Aceite: nenhuma rota envia sem clique autenticado.
- Esforço: 1 sessão.

### F4: Maestro no canvas
- Abrir uma missão como grafo: nós com agente, esforço, estado e saída; botão Aprovar no canvas.
- Esforço: 1 sessão.

### F5: nó de web (opcional)
- Começar com **captura atualizável** (`chromiumoxide`, já no projeto), não webview ao vivo. Webview real só se a captura não bastar.
- Esforço: 2 sessões.

## Validação
Testes e CI estão pausados (ADR 0007): `cargo check` a cada fase, capturas headless e roteiro de aceite no Windows entregue ao Antônio. Cada fase atualiza ORION.md e ARQUITETURA.md, espelha no vault e move o card no board do Notion.

## Riscos
- **Desempenho do iced** com vários chats: desenhar só nós visíveis, atualizar só nós com mensagem nova, cartão simples em zoom baixo.
- **Estado por nó** no app (hoje é global): é a maior mudança estrutural.
- **Windows:** só se prova no PC real.

## Decisões para o Antônio
1. Aprova a regra 1 (saída entre chats só por clique, como dado)?
2. Começar pela F1 já? (não depende de nada e não abre processo)
3. Nó de web: captura atualizável serve, ou quer página ao vivo?
