# Plano: canvas de agentes (estilo Maestri)

Estado: **proposta, nada implementado**. Depende da aprovação do Antônio para a regra nova (REGRAS.md é núcleo imutável).

## Objetivo
Uma tela "Canvas" no app: espaço infinito com pan e zoom, onde ficam nós de terminal (Claude, Codex, Gemini rodando de verdade), notas, frames e, depois, páginas web. Setas ligam nós. Espaços de trabalho salvos e reabertos.

## Regras que precisam existir antes do código
Escrever em REGRAS.md (aguarda aprovação) e registrar em ADR 0010:
1. **Terminal só de CLI oficial, por lista fixa.** O nó abre `claude`, `codex` ou `gemini` direto (sem shell, sem string de comando montada). O Orion nunca lê, copia ou guarda token; o login é feito dentro da CLI.
2. **Shell livre é outro risco** (`Exec`): fica fora da v1; se entrar, é opt-in desligado por padrão e com aprovação por canal autenticado.
3. **Saída de um nó entrando em outro é dado, nunca instrução.** A seta só prepara; o envio é um clique do Antônio, com o texto marcado como `[dado do nó X]`. Nada automático.
4. **Ambiente limpo:** a CLI não herda `ORION_*` nem segredos (igual ao `Delegator`).
5. **Opt-in:** a tela só abre terminais depois de o Antônio ligar "Terminais no canvas" em Configurações.

## Arquitetura
| Peça | Onde | Notas |
|---|---|---|
| Sessões de terminal (PTY + emulador) | crate novo `orion-term` | `portable-pty` (ConPTY no Windows) + `alacritty_terminal` para a grade. Sem UI. Expõe: abrir, escrever, redimensionar, ler grade, fechar |
| Modelo e persistência | `orion-store/sql/v26.sql` + `orion-core/src/canvas.rs` | tabelas `canvas_workspaces`, `canvas_nodes` (tipo, x, y, w, h, payload JSON), `canvas_edges` (de, para, rótulo). Banco já é cifrado |
| Rotas | `orion-server/src/rotas/canvas.rs` | CRUD de espaços, nós e setas; `POST /canvas/nos/:id/enviar` (ação do clique) |
| UI | `orion-app/src/views/canvas.rs`, `View::Canvas` | widget `canvas` do iced (já habilitado, 0.14): câmera, hit-test, desenho dos nós |
| Maestro | `maestro.rs` | "Abrir missão no canvas": cada subtarefa vira nó só de leitura; `depends_on` vira seta |

## Fases

### F1: canvas base (sem terminal)
- Câmera (pan com arrastar, zoom com roda, limites), grade, minimapa, barra de zoom.
- Nós de nota e frame: criar, mover, redimensionar, apagar, selecionar vários.
- Setas: arrastar de um nó a outro, rótulo opcional.
- Espaços de trabalho na barra lateral, salvos no banco (v26).
- Nível de detalhe: abaixo de ~40% de zoom, nós viram cartões simples (como na imagem a 33%).
- Aceite: `cargo check` verde, capturas headless (`snap.rs`) das telas, abrir e reabrir mantém posições.
- Esforço: 1 a 2 sessões.

### F2: nó de terminal
- `orion-term`: abrir `claude`/`codex`/`gemini` na pasta escolhida; leitura da grade; entrada de teclado quando o nó tem foco; redimensionar com o nó; limite de histórico.
- Desenho da grade no canvas com fonte mono, cores ANSI, cursor.
- Encerrar processos ao fechar o nó ou o app; nada de processo órfão.
- Estado do nó: rodando, parado, erro; aviso se a CLI não estiver instalada (usa `which` do Delegator).
- Aceite: abrir um Claude e um Codex lado a lado, digitar, redimensionar. **Exige teste no Windows real (ConPTY) pelo Antônio.**
- Esforço: 3 a 4 sessões (é a parte mais pesada: foco, teclado, desempenho).

### F3: setas com função
- Botão "Enviar saída para o nó B" na seta: pega as últimas N linhas de A e cola no B como `[dado do nó A: …]`, após clique.
- Registro no log de saídas (quem enviou o quê, tamanho), sem guardar o texto.
- Aceite: envio manual funciona; nenhuma rota envia sem clique autenticado.
- Esforço: 1 sessão.

### F4: Maestro no canvas
- Abrir uma missão do Maestro como grafo: nós com agente, esforço, estado e saída; botão Aprovar no próprio canvas.
- Esforço: 1 sessão.

### F5: nó de web (opcional, depois)
- Em vez de webview ao vivo (difícil no iced), começar com **captura atualizável**: `chromiumoxide` (já no projeto) gera a imagem da página e o nó mostra e atualiza. Webview real só se a captura não bastar.
- Esforço: 2 sessões.

## Validação
Testes e CI estão pausados (ADR 0007), então: `cargo check` em cada fase, capturas headless do app e um roteiro de aceite no Windows entregue ao Antônio. Cada fase atualiza ORION.md, ARQUITETURA.md (saída nova: processos de CLI), espelha no vault e move o card no board do Notion.

## Riscos
- **Desempenho do iced** com várias grades de terminal: desenhar só nós visíveis, atualizar só nós com saída nova, nível de detalhe por zoom.
- **Teclado:** mapear teclas e atalhos do terminal sem brigar com os atalhos do app (foco explícito no nó).
- **Windows:** ConPTY e fechamento de processos só se provam no PC real.
- **Superfície nova:** terminal interativo é a maior ampliação de poder do app; a regra de lista fixa e opt-in existe por isso.

## Decisões para o Antônio
1. Aprova as 5 regras acima (e que shell livre fique de fora da v1)?
2. Começar pela F1 já ou esperar a aprovação da regra? (F1 não abre processo, pode começar sem a regra.)
3. Nó de web: captura atualizável serve, ou quer página ao vivo?
