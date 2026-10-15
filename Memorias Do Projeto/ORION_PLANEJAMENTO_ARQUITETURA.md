---
type: recurso
projeto: "[[Orion]]"
tags: [orion, front, planejamento, rust, iced, design]
date: 2026-10-15
resumo: "Planejamento do front do Orion em Rust (iced): decisões, identidade visual, telas, configurações, memória no vault, grafo 3D, efeitos e fases de implementação. Base: protótipo navegável de 15/10/2026."
---

# ORION - PLANEJAMENTO ARQUITETURA

> Projeto: [[Orion]] · Conceito: [[Rust]] · Conversa de origem: [[2026-10-15 - Orion - redesign do protótipo no estilo do app Claude]] · Protótipo anterior: [[2026-10-14 - Orion - prototipo do front desktop nativo]]
> Protótipo navegável: https://claude.ai/artifact/S4fmhxJ35UztifhKCyzqt9 (versão de 15/10/2026 já reflete as decisões desta nota: tema único Grafite, barra lateral com Aprovações e Rotinas, seletor Auto/Claude/Codex/Gemini/Orion, painel da direita em abas, nota do vault + commit, editor de notas, `/` e `@`, rotinas, nuvem, conectores, câmera, modelos locais e grafo real com linha do tempo)

## Objetivo
Um app de desktop nativo em Rust, sem WebView nem servidor local para a interface. A referência visual é o app desktop do Claude (barra lateral e configurações), mantendo a identidade do Orion: céu, cinturão e as cores de Rigel e Betelgeuse.

## Decisões

| Data | Decisão |
|---|---|
| 14/10/2026 | Front nativo em Rust. Biblioteca de interface: **iced** (arquitetura Elm, tema próprio, texto bom). |
| 15/10/2026 | Barra lateral e configurações no estilo do app Claude. |
| 15/10/2026 | Tom padrão **Grafite** (neutro). Azul só em foco, seleção, switches e conceitos no grafo. ~~Tons Noite e Contraste~~ (substituído: só Grafite escuro, sem seletor de tema). |
| 15/10/2026 | **A memória é o vault**: notas `.md` com frontmatter e wikilinks. O banco SQLite guarda só a operação e o índice. |
| 15/10/2026 | O grafo da memória é **3D**, como a visualização do Obsidian, com fundo espacial igual ao do front original. |
| 15/10/2026 | Brilho, bloom e blur são viáveis em wgpu. Detalhes em [[#Efeitos visuais]]. |
| 15/10/2026 | Ajustes da rodada de perguntas: ver [[#Respostas do Antonio (15/10/2026)]]. |

## Respostas do Antonio (15/10/2026)
| Tema | Decisão |
|---|---|
| Primeira versão | **Chat completo** primeiro: janela, barra lateral, chat com streaming e aprovação. O resto entra depois. |
| Plataformas | **PC Windows e notebook.** Celular só pelo **Telegram** (sem app nativo). macOS fica para o futuro. Linux descartado. |
| Tema | **Só Grafite escuro.** Sem tema claro, sem tom Contraste e **sem seletor de tema**. |
| Pasta das notas do Orion | **`03 Recursos/Orion/Memória/`.** |
| Gravar no vault | **Todas as conversas** viram resumo no vault, automático e sem perguntar. Ver [[#Gravação no vault]]. |
| Notebook | **Qualidade automática:** detecta a GPU e desliga bloom e blur se ficar pesado. |
| Janela | Barra de título própria, fechar vai para a bandeja, fundo Mica/Acrylic, atalho global para abrir. |
| Seletor de modelo | **5 opções:** Auto, Claude, Codex, Gemini e Orion. Claude, Codex e Gemini permitem escolher **modelo + esforço**. "Orion" = APIs gratuitas; ordem, cotas e chaves ficam nas configurações. |
| Quem respondeu | Mostrar o provedor que respondeu, **bem discreto** (ex.: na linha de hora da resposta). |
| Tela inicial | **Igual ao original por enquanto:** constelação de Órion atrás da saudação, reagindo ao estado. A decidir depois. |
| Voz na interface | Pílula de estado na barra + **constelação reage à voz** (cinturão acende com o áudio, como no `sky.js`). |
| Aprovações | Cartão dentro do chat, **fila única numa tela** e **pelo Telegram**. |
| Atividade | Fica **junto com Aprovações**: pendências no topo, histórico embaixo. |
| Rotinas | **Item curto na barra lateral**, com ícone de relógio e o nome "Rotinas", como o "Agendados" do Claude. |
| Painel de contexto | **Fechado por padrão**, abre sozinho quando há alteração em arquivos ou aprovação. |
| Idioma | **Português e inglês** desde o início (arquivos de tradução). |
| Avisos com a janela fechada | Notificação do Windows, Telegram e ponto no ícone da bandeja. |
| Atalho global | **Win + O** abre a janela focada no campo de mensagem. |
| Aprovação pelo Telegram | Só aceita do **ID do Antonio**, com botões Aprovar/Negar. |
| Modo autônomo | Interruptor nas configurações. Ligado, o Orion age sem pedir aprovação, **exceto no que pode quebrar o PC ou o projeto**. Ver [[#Permissões]]. |
| Projetos | Ver [[#Projetos (detalhe)]]. |

## Gravação no vault
- **Todas as conversas** viram uma nota de resumo em `03 Recursos/Orion/Memória/`, sem perguntar.
- **Uma nota por conversa**, atualizada a cada rodada com seções novas (como as notas de conversa do Claude).
- **Conversas triviais** ("que horas são", "abre o Spotify") entram num **resumo diário único**, gerado às **23:59** em `03 Recursos/Orion/Memória/Diário/AAAA-MM-DD.md`, linkando as conversas do dia.
- O que conta como conteúdo forte (decisão, debug resolvido, fato novo sobre o Antonio, pesquisa com fontes) também é destacado na nota e linkado ao Projeto/Área.
- **Commit e push direto na `main`** do vault, **um commit por nota gravada**. Sem PR.
- **Conflito com edição no Obsidian:** se o Antonio editou a mesma nota, o Orion **pergunta no chat**, mostrando as duas versões.
- **Onde aparece:** no **painel da direita** da conversa: a nota desta conversa (nome, última atualização, abrir no Obsidian) e o último commit feito (hash e hora).

## Permissões
| Ação | Sem aprovação | Com aprovação |
|---|---|---|
| Ler arquivos nas pastas confiáveis | ✔ | |
| Gravar notas no vault + commit na `main` | ✔ | |
| Busca na web (sem dados pessoais) | ✔ | |
| Comandos de leitura (`git status`, `cargo check`, listar processos) | ✔ | |
| Escrever arquivos de projeto, enviar para fora, rodar comandos que alteram algo | | ✔ (dispensado no modo autônomo) |
| Apagar, formatar, mexer em sistema, `force-push`, reescrever histórico, desinstalar | | ✔ **sempre**, mesmo no modo autônomo |

**Modo autônomo:** um interruptor em Configurações › Privacidade (e na pílula de estado). Ligado, a coluna do meio vale para tudo, menos a última linha: nada que possa quebrar o PC ou o projeto roda sem confirmação.

## Chat (detalhes da v1)
- **Progresso:** linha resumida ("Leu 4 arquivos, executou 1 comando") que expande para mostrar cada passo, como no Claude.
- **Anexos:** imagens e prints (colar ou arrastar), texto e código, PDF (extrai o texto) e áudio (transcreve antes). **Limite de 20 MB por arquivo.** Para arquivos maiores, colocar numa pasta confiável e passar o caminho: o Orion lê direto do disco.
- **Atalhos no campo:** `/` para comandos, `@` para citar uma nota do vault como contexto, `↑` repete a última mensagem. Enter envia, Shift+Enter quebra linha.
- **Comandos `/` da v1:**
  - `/nota` grava ou atualiza a nota desta conversa agora; `/vault <busca>` procura no vault.
  - `/rotina` cria ou lista rotinas.
  - `/modelo` troca o modelo e o esforço; `/projeto` troca o projeto da conversa.
  - `/autonomo` liga ou desliga o modo autônomo.
  - `/limpar` começa uma conversa nova com o mesmo projeto; `/resumo` resume a conversa até aqui; `/aprovar` abre a fila de aprovações; `/voz` liga ou desliga a voz ao vivo; `/ajuda` lista os comandos.
- **Suas mensagens:** editar e reenviar (cria um ramo da conversa), copiar e fixar (fica no topo como contexto permanente).
- **Markdown nas respostas:** texto, listas, tabelas, código com realce e botão copiar, **Mermaid**, **LaTeX** e **prévia de HTML** (como o `html_preview` do front original).
- **Histórico na barra lateral:** fixadas no topo, depois Hoje, Ontem, 7 dias e Mais antigos, com busca.

## Extensões: MCP, skills e plugins
Meta: suportar **tudo o que o Claude e o Codex têm e for possível no Orion**.
- **Servidores MCP:** adicionar por comando local (stdio) ou URL (HTTP/SSE), como no Claude Desktop. As ferramentas aparecem no chat com o nome do servidor.
- **Skills em Markdown:** pastas com `SKILL.md`, no mesmo formato das skills do Claude, carregadas sob demanda.
- **Plugins próprios em Rust/WASM:** código em sandbox (ex.: Wasmtime), com permissões declaradas.
- **Importar do Claude e do Codex:** ler o `mcp.json`/config e as skills que já existem no PC e oferecer para ativar.
- **Onde gerenciar:** Configurações › **Conectores**, como no Claude: lista, ligar/desligar, estado da conexão e permissão por ferramenta (sempre, perguntar, nunca).
- Comandos: `/mcp` e `/skill` para listar e ativar.

## Nuvem: Claude e Codex
- **Só quando o Antonio pedir** (`/nuvem claude <tarefa>` ou `/nuvem codex <tarefa>`).
- **Claude:** `claude --remote "<tarefa>"` cria uma sessão do Claude Code na nuvem, na assinatura; `claude --teleport` traz para o PC. Não há API HTTP documentada; o caminho é a CLI.
- **Codex:** `codex cloud exec` envia a tarefa para um ambiente em nuvem, `codex cloud list` (JSON) acompanha, `codex apply` traz o diff. Flags a confirmar na doc oficial.
- **Ao terminar:** o Orion **avisa e espera** (notificação, Telegram) com o link da sessão ou do PR. Trazer o diff para a pasta local é decisão do Antonio.
- **Tela:** seção "Nuvem" com as tarefas em andamento nos dois serviços, estado, link e botão "Trazer resultado".
- Respeita a regra do projeto: assinaturas só pelos programas oficiais, sem token extraído.

## Rotinas
- **Criar:** pelo chat em linguagem natural ("todo dia às 8h me manda o briefing"), pelo formulário na tela Rotinas ou com `/rotina`.
- **PC desligado:** rotinas marcadas como críticas viram rotinas do **Claude Code na nuvem** e rodam lá.
- **Resultado:** numa **conversa própria da rotina** (uma mensagem por execução), numa **nota no vault** e no **Telegram**.
- **Tela:** por rotina, próxima e última execução (ok, falhou, pulada), histórico de execuções e botão **Rodar agora**.

## Primeiro uso, segurança e privacidade
- **Onboarding:** criar a senha, confirmar o caminho do vault (detecta o do Obsidian), colar as chaves das APIs gratuitas (com link de onde pegar cada uma) e importar MCPs, skills e CLIs do Claude e do Codex.
- **Senha:** pedida ao abrir o app e **quando o Windows é bloqueado** (Win+L). Sem timer próprio.
- **Nunca sai do PC:** senhas e chaves (filtradas antes de enviar) e documentos pessoais (RG, CPF, dados bancários são detectados e mascarados).
- **Transparência:** cada resposta que usou uma API de fora tem um **ícone discreto**; clicando, mostra o que saiu e para qual provedor.

## Erros e modo offline
- **Erros de API** (fora do ar, cota esgotada): **faixa discreta de uma linha** ("Groq sem cota, usando Gemini") e troca automática de provedor. Só para quando nada funciona.
- **Sem internet:** cai para o **modelo local** e avisa. Vault e memória continuam funcionando.

## Painel
Em ordem de importância: **cotas das APIs gratuitas** (quanto resta hoje), **uso das assinaturas** (Claude, Codex, Gemini), **saúde do sistema** (RAM, CPU, GPU, disco, serviços) e **atividade da semana** (conversas, ações, aprovações, notas gravadas).

## Voz
- **Iniciar:** palavra **"Orion"** (escuta local) ou **duas palmas**. Os dois **trazem a janela para o foco**.
- **Enquanto fala:** uma **mini barra flutuante no topo, centralizada**, com a constelação reagindo ao áudio. Formato final a decidir com protótipo.
- **Resposta falada:** **só quando o pedido foi por voz**. Mensagem escrita recebe resposta só escrita.

## Telegram
- Conversar com o Orion; as conversas aparecem no app do PC com a marca **"via Telegram"**.
- Mandar **áudio** (ele transcreve) e receber **resposta em áudio**.
- Aprovar ou negar (só o ID do Antonio, com botões); receber rotinas e avisos.
- **Comandos do PC:** `/status` (ligado, RAM), `/rotina rodar`, `/nuvem codex <tarefa>`.
- **Ligar e desligar o PC:** desligar pede confirmação por botão. Ligar remotamente usa **Wake-on-LAN**, que precisa de outro aparelho ligado na mesma rede (notebook ou roteador) para enviar o sinal.

## Vault dentro do Orion
- **Abrir nota:** visualizar e **editar no Orion**, com edição e prévia lado a lado (Markdown à esquerda, renderizado à direita, wikilinks clicáveis). Botão para abrir no Obsidian.
- **`git pull` do vault:** ao abrir o app, a cada conversa nova, antes de gravar uma nota e ao abrir a tela Memória.
- **Busca global (Ctrl K):** conversas e notas do vault.
- **Grafo:** abre com o **vault inteiro girando devagar**. Extras: **tags como nós** (opção), **linha do tempo** (slider de data para ver o vault crescer) e **destaque das notas que o Orion leu** na última resposta.

## App no Windows
- **Instalador** `.msi`/`.exe` (atalho, iniciar com o Windows, desinstalação limpa).
- **Atualizações:** baixa do GitHub Releases em segundo plano, **avisa e instala ao fechar**.
- **Backup** do banco e das configurações: cópia **criptografada no Google Drive** e **exportar/importar manual** em Configurações (para trocar de PC).
- **Janela:** lembra tamanho e posição.
- **Desempenho:** **quanto menos RAM, melhor** (meta abaixo de 300 MB parado, índice, voz e grafo só sob demanda); **menos de 1 s** até poder digitar.
- **Testes:** testes de lógica (estado do app, comandos `/`, Markdown), **snapshots de tela** e **CI no GitHub Actions** com build Windows.

## Rotinas
- **Criar:** pelo chat em linguagem natural ("todo dia às 8h me manda o briefing"), pelo formulário na tela Rotinas ou com `/rotina`.
- **PC desligado:** rotinas marcadas como críticas viram rotinas do **Claude Code na nuvem** e rodam lá.
- **Resultado:** numa **conversa própria da rotina** (uma mensagem por execução), numa **nota no vault** e no **Telegram**.
- **Tela:** por rotina, próxima e última execução (ok, falhou, pulada), histórico de execuções e botão **Rodar agora**.

## Primeiro uso, segurança e privacidade
- **Onboarding:** criar a senha, confirmar o caminho do vault (detecta o do Obsidian), colar as chaves das APIs gratuitas (com link de onde pegar cada uma) e importar MCPs, skills e CLIs do Claude e do Codex.
- **Senha:** pedida ao abrir o app e **quando o Windows é bloqueado** (Win+L). Sem timer próprio.
- **Nunca sai do PC:** senhas e chaves (filtradas antes de enviar) e documentos pessoais (RG, CPF, dados bancários são detectados e mascarados).
- **Transparência:** cada resposta que usou uma API de fora tem um **ícone discreto**; clicando, mostra o que saiu e para qual provedor.

## Erros e modo offline
- **Erros de API** (fora do ar, cota esgotada): **faixa discreta de uma linha** ("Groq sem cota, usando Gemini") e troca automática de provedor. Só para quando nada funciona.
- **Sem internet:** cai para o **modelo local** e avisa. Vault e memória continuam funcionando.

## Painel
Em ordem de importância: **cotas das APIs gratuitas** (quanto resta hoje), **uso das assinaturas** (Claude, Codex, Gemini), **saúde do sistema** (RAM, CPU, GPU, disco, serviços) e **atividade da semana** (conversas, ações, aprovações, notas gravadas).

## Personalidade e respostas
- **Sem avatar:** respostas só em texto, como no Claude.
- **Tom:** seletor **Conciso / Normal / Detalhado** + campo livre "como o Orion deve responder", em Configurações › Geral.
- **Imagens:** geração via **Gemini/APIs gratuitas** (`/imagem`). A imagem aparece na conversa e é salva em `03 Recursos/Orion/Imagens/`, linkada na nota da conversa.

## Voz
- **Iniciar:** palavra **"Orion"** (escuta local) ou **duas palmas**. Os dois **trazem a janela para o foco**.
- **Enquanto fala:** **mini barra flutuante no topo, centralizada**, com a constelação reagindo ao áudio. Formato final a decidir com protótipo.
- **Resposta falada:** **só quando o pedido foi por voz**. Mensagem escrita recebe resposta só escrita.

## Câmera e visão
- **Dois modos, cada um com interruptor:** foto **sob pedido** ("olha isso") e **presença contínua local** (sabe se o Antonio está no PC, pausa e cumprimenta; não guarda imagem).
- **Usos:** ver e responder sobre uma foto, **"onde eu deixei?"** (lembra onde viu objetos), **reconhecer o Antonio** (rosto local) e **legenda ao vivo**.
- **Indicador:** **ponto vermelho** na barra de título e no ícone da bandeja sempre que a câmera estiver ativa.
- **"Onde eu deixei?" — retenção:** **sem prazo** (ou muito longo). Guarda só rótulos, sem imagem (`objeto, lugar, hora`). Cada objeto mantém o último lugar visto mais um histórico compactado (uma linha por mudança de lugar), então ocupa poucos KB por ano.
- **Objetos lembrados (lista inicial, editável):** chaves, carteira, celular, fone de ouvido, óculos, controle remoto, carregador, notebook, mochila, documentos (RG/CNH), cartões, remédios, garrafa/copo, mouse e teclado sem fio, pendrive, relógio, guarda-chuva, caneta.
- **Presença contínua — ao sair do PC:** pausa a voz que estiver falando e **bloqueia o Orion** (pede senha na volta). **Na volta:** cumprimenta com o que aconteceu ("Voltou. A rotina das 14h terminou.").
- **Legenda ao vivo:** faixa flutuante na base da tela, sobre qualquer janela, como legenda de vídeo.
- Ainda a definir: onde roda o reconhecimento de rosto e o modelo de detecção.

## Notificações, sons e não perturbe
- **Notificações do Windows ligadas por padrão:** aprovação pendente, rotina concluída, tarefa na nuvem terminou.
- **Sons sutis:** aprovação pendente, resposta pronta (janela fora de foco), rotina concluída e ativação da voz.
- **Não perturbe:** em Configurações; silencia **notificações e sons**.

## Conversas na barra lateral
- Conversas de **rotinas** e do **Telegram** aparecem na lista com **ícone de origem** (relógio, avião do Telegram).
- **Arquivar:** nunca sozinho, só manual.

## Telegram
- Conversar com o Orion; as conversas aparecem no app do PC com a marca **"via Telegram"**.
- Mandar **áudio** (ele transcreve) e receber **resposta em áudio**.
- Aprovar ou negar (só o ID do Antonio, com botões); receber rotinas e avisos.
- **Comandos do PC:** `/status` (ligado, RAM), `/rotina rodar`, `/nuvem codex <tarefa>`.
- **Ligar e desligar o PC:** desligar pede confirmação por botão. Ligar remotamente usa **Wake-on-LAN**, que precisa de outro aparelho ligado na mesma rede (notebook ou roteador) para enviar o sinal.

## Vault dentro do Orion
- **Abrir nota:** visualizar e **editar no Orion**, com edição e prévia lado a lado (Markdown à esquerda, renderizado à direita, wikilinks clicáveis). Botão para abrir no Obsidian.
- **`git pull` do vault:** ao abrir o app, a cada conversa nova, antes de gravar uma nota e ao abrir a tela Memória.
- **Busca global (Ctrl K):** conversas e notas do vault.
- **Grafo:** abre com o **vault inteiro girando devagar**. Extras: **tags como nós** (opção), **linha do tempo** (slider de data para ver o vault crescer) e **destaque das notas que o Orion leu** na última resposta.
- **Calendário e e-mail:** sem tela própria; só pelo chat e pelo briefing.

## Ferramentas de código
- **Painel da direita em abas:** Contexto / Arquivos (explorador do projeto) / Diff (mudanças antes de aprovar) / Terminal.
- **Terminal completo** (PowerShell de verdade dentro do app).
- **GitHub:** PRs abertos e CI do projeto, com botões **abrir PR** e **merge** (com aprovação).

## Modelos locais
- **llama.cpp embutido no Orion** (sem processo extra). Tela simples para baixar e remover modelos, escolher o fallback e ver a RAM usada.

## Vários PCs
- **PC principal manda; notebook replica pela Tailscale** (banco SQLite sincronizado quando os dois estão ligados). O vault é comum aos dois pelo git.

## App no Windows
- **Instalador** `.msi`/`.exe` (atalho, iniciar com o Windows, desinstalação limpa).
- **Atualizações:** baixa do GitHub Releases em segundo plano, **avisa e instala ao fechar**.
- **Backup** do banco e das configurações: cópia **criptografada no Google Drive** e **exportar/importar manual** em Configurações.
- **Janela:** lembra tamanho e posição.
- **Desempenho:** **quanto menos RAM, melhor** (meta abaixo de 300 MB parado; índice, voz e grafo só sob demanda) e **menos de 1 s** até poder digitar.
- **Acessibilidade:** teclado em tudo e escala da interface de 90% a 150% (Configurações › Geral e Ctrl +/-). Sem modo alto contraste.
- **Testes:** testes de lógica (estado do app, comandos `/`, Markdown), **snapshots de tela** e **CI no GitHub Actions** com build Windows.

## Projetos (detalhe)
Cada projeto tem:
- a **nota do projeto no vault** (`01 Projetos/<Nome>/<Nome>.md`), de onde o Orion lê estado e instruções;
- **instruções próprias** (prompt do projeto);
- **repositório GitHub** vinculado (PRs, CI, branch);
- **pastas do disco** vinculadas: dentro do projeto, o Orion só mexe nelas;
- **modelo padrão** (ex.: Orion usa Claude, Vault usa Gemini);
- **rotinas do projeto** (ex.: revisar PRs do Orion a cada 4 h);
- **conversas do projeto**;
- um **painel do projeto**: pendências, últimas conversas, CI e notas recentes.

**Conversa nova** abre com o **último projeto usado**. **Títulos** das conversas são gerados sozinhos e editáveis. A **busca do histórico** procura no **texto completo** das mensagens.

## Escopo da v1, captura rápida e outras decisões (15/10)
- **v1 = chat completo + aprovações e modo autônomo + nota no vault com commit + configurações completas.** O Telegram fica para depois da v1.
- **Tela Vida** (finanças locais, monitor de preços, linha do tempo, foco): entra **depois da v1**, em "Mais", numa fase própria.
- **Captura rápida (Ponte de desktop):** mini barra com **Win + Shift + O** sobre qualquer app. Uma linha vira tarefa, lembrete, gasto ou nota no `00 Inbox` do vault.
- **"Isso?":** atalho próprio (**Win + Shift + I**) que captura a tela e abre a pergunta para o modelo de visão.
- **Ao ligar o PC:** abre **minimizado na bandeja**, sem janela até ser chamado.
- **Grafo com o vault crescendo:** acima de um limite de notas, **agrupa por pasta** (clusters que abrem ao aproximar).
- **Desfazer:** botão **desfazer** em cada item do histórico de atividade, que reverte a ação quando possível (commit no vault, arquivo editado).
- **Migração do Orion antigo (Python):** a decidir depois.
- **Codex na nuvem que falha:** avisa o erro e pergunta o que fazer (tentar de novo, mandar para o Claude ou desistir). Não troca de serviço sozinho.
- **Conflito de nota com o Obsidian:** pergunta no chat, mostrando as duas versões (já decidido; reforçado aqui).
- **Captura rápida ambígua** ("reunião amanhã com o Lucas"): salva como **lembrete** e mostra o tipo, que pode ser trocado com um clique.
- **Modo autônomo:** avisa com um **resumo ao fim do dia** (no Painel e na notificação das 23:59), não a cada ação.

## Orçamento de uso (assinaturas e APIs)
Objetivo: **economizar o máximo com qualidade de entrega** e **usar a cota semanal de assinatura quase toda** antes do reset.

**Roteamento por complexidade**
- Um agente barato classifica cada pedido (simples, médio, difícil) antes de escolher o modelo.
- Pergunta simples vai para o modelo **mais barato que atenda**: APIs gratuitas, depois local.
- Se a resposta falhar em checagens (testes, formato, confiança baixa), **sobe um nível**.
- Tarefas difíceis ou de código vão para Claude ou Codex pela assinatura, quando há cota.

**Estimativa de cota** (sem API oficial confirmada de uso restante)
- **Claude:** o Orion roda `/usage` na CLI do Claude Code e lê o uso da janela de 5 horas e da semana. Não é API, então o formato da saída pode mudar: se a leitura falhar, usa a estimativa local e avisa. A leitura só funciona entre tarefas, porque o comando não roda durante o loop do agente.
- **Codex:** o `/status` da CLI mostra o limite restante das janelas de 5 horas e semanal, mas só depois que a sessão recebe uma mensagem, e pode estar desatualizado. Há relatos de divergência entre o número mostrado e o erro 429. Por isso, a estimativa local é a referência e o `/status` corrige quando disponível. A fonte oficial de histórico é o painel de uso da OpenAI.
- **Gemini:** sem comando de uso confirmado; só estimativa local.
- Registro local de cada chamada, por provedor, com tokens e janela de reset, como base de todas as estimativas.
- Calibragem manual: você informa o uso real quando a estimativa desviar.
- Configuração por provedor: dia e hora do reset semanal (ex.: sábado, 8 h).

**Gasto semanal**
- Teto de **95%** da cota, com **5% de reserva** para emergência antes do reset.
- Todo dia, o Orion projeta quanto sobra no reset, pelo ritmo de uso.
- **Automelhoria** entra na fila de sobra, e a fila é escolhida por você: ferramentas, código, lógica, performance, segurança, auditoria e manutenção do vault, testes e cobertura, refatoração e pesquisa sobre temas seus.
- **Disparo:** todo dia; se faltarem **12 horas para o reset** e o uso ainda estiver **abaixo de 90%**, dispara uma tarefa de automelhoria e repete até chegar ao teto.
- Mudanças de código saem sempre como **branch e PR para revisão**. Nunca fazem merge sozinhas.

**Limites**
- Nenhuma tarefa de automelhoria roda acima do teto de 95%.
- Automelhoria não é prioridade sobre pedido seu: se você pedir algo durante uma tarefa de fundo, a tarefa de fundo para.
- Uso automático de assinatura em segundo plano deve respeitar os termos de cada provedor; isso ainda precisa ser confirmado antes de ligar.

## Stack
- **iced** para a interface (janela, listas, formulários, tema).
- **wgpu** via widget `shader` do iced para o céu e o grafo 3D.
- **glyphon** para rótulos de texto dentro do grafo.
- **rayon** para o layout de forças em segundo plano.
- **window-vibrancy** para Mica/Acrylic no Windows.
- **Fluent** (crate `fluent`) para português e inglês.
- Notificações nativas do Windows e ícone de bandeja (`tray-icon`); atalho global (`global-hotkey`).

## Identidade visual

### Cores (tokens)
| Token | Grafite (único tema) | Uso |
|---|---|---|
| `bg` | `#151618` | fundo do palco |
| `side` | `#111214` | barra lateral |
| `raise` | `#1b1c1f` | campo de mensagem, cartões |
| `fg` / `fg-2` / `fg-3` | `#ececee` / `#a9aab0` / `#75767d` | texto, secundário, legenda |
| `accent` (Rigel) | `#a4b3de` | foco, seleção, switch |
| `signal` (Betelgeuse) | `#e9b467` | atenção, aprovação pendente |
| `primary` (Rigel pálido) | `#b9c6ea`, texto `#0f1320` | botões principais: Enviar, Aprovar |
| `ok` / `danger` | `#74cfae` / `#f07c88` | só sucesso e destrutivo |

Sem tema claro e sem seletor de tema (decidido em 15/10). Os tons Noite e Contraste do protótipo saem do plano.

### Tipografia
- **Interface:** Geist (fallback Segoe UI).
- **Respostas do chat e títulos:** Newsreader, serifada. O usuário escolhe Newsreader, Geist ou fonte do sistema.
- **Código:** Geist Mono (fallback Cascadia Code).

### Movimento e densidade
- **Animação discreta:** transições de 150 a 250 ms, sem efeitos chamativos (padrão do Claude). O céu e o grafo são as únicas partes vivas.
- **Densidade:** opção em Configurações › Geral: **Confortável** (linhas de ~32 px, padrão) ou **Compacta** (~26 px).

### Marca
- Cinturão de Órion (três pontos em diagonal) ao lado do nome, na saudação e como **ícone do app** (barra de tarefas, bandeja, atalho), como o `orion.svg` do original.
- O céu com a constelação aparece só na entrada e no fundo do grafo.

## Estrutura da janela
- **Barra de título própria:** título da conversa, pílula de estado (Pensando, Ouvindo, Aguardando você), "Buscar ou comandar" (Ctrl K), painel de contexto, minimizar, tela cheia e fechar para a bandeja.
- **Barra lateral:** recolher (Ctrl B), voltar e avançar, busca, Nova conversa, Projetos, Memória, Aprovações (fila + histórico de atividade), Rotinas (item curto com relógio) e "Mais" (Resultados, Fontes, Conhecimento, Integrações, Painel, Arquivadas). Conversas agrupadas em Hoje, Ontem e Mais antigos, com o projeto embaixo do título e indicadores (âmbar = aguardando aprovação, girando = trabalhando, azul = resposta nova). Rodapé com conta e engrenagem.
- **Palco:** a tela atual.
- **Painel de contexto** (só no chat, fechado por padrão, abre sozinho quando há alteração ou aprovação): ambiente (projeto, branch, modelo), contexto usado, fontes ligadas, memória usada, alterações em arquivos.

## Telas
| Tela | O que faz |
|---|---|
| Entrada | Senha do computador sobre o céu com a constelação. |
| Início / Nova conversa | Constelação de Órion ao fundo (como o original), saudação, campo de mensagem, projeto, seletor de modelo, linha de estado, sugestões. |
| Chat | Respostas em serifa, streaming com botão de parar, linha "Leu N arquivos" expansível, cartão âmbar de aprovação, copiar, ouvir, gerar de novo. Provedor que respondeu discreto na linha da hora. |
| Projetos | Cartões por projeto com pendências e última atualização. |
| Memória | Grafo do vault (ver [[#Grafo da memória]]). |
| Resultados | Saídas guardadas com tipo, versão e data. |
| Aprovações | Fila única de pendências (de todas as conversas e rotinas) no topo; histórico de atividade embaixo (filtro: todas, aguardando, negadas). |
| Rotinas | Lista com liga/desliga, próxima execução e resultado novo. |
| Fontes | O que o Orion pode ler, com switch por fonte. |
| Conhecimento | Fatos com fonte; corrigir abre a nota no vault, esquecer apaga com aprovação. |
| Integrações | Serviços externos com estado (conectado, token expirado). |
| Painel | Números da semana, tempo da primeira resposta, modelos e CLIs, ações por tipo, política. |
| Arquivadas | Desarquivar ou apagar. |

## Configurações
Modal no estilo do Claude, com lista à esquerda e busca. No celular ou janela estreita, vira lista → detalhe.
- **Configurações:** Geral (fonte do chat, tom das respostas, escala da interface, densidade, não perturbe, tamanho do texto, largura da conversa, movimento, notificações), Conta, Privacidade (sensores, pausar tudo com confirmação, apagar histórico), Uso, Memória, Fontes, Integrações.
- **Este computador:** Sistema (RAM por componente com gráfico, iniciar com o Windows, pastas confiáveis, navegador, versão), Voz (idioma, estilo, velocidade, palavra "Orion", duas palmas), Atalhos.

## Memória: vault + banco
| Vault (fonte da verdade, aparece no grafo) | Banco SQLite (operação) |
|---|---|
| Resumo de cada conversa (`type: conversa`) | Transcrição bruta das conversas |
| Fatos e preferências aprendidos | Fila de aprovações e log de atividade |
| Decisões, pesquisas, notas de projeto | Estado das rotinas e da sessão |
| | Índice de busca e embeddings das notas |

- A escrita no vault é **automática** quando a conversa for relevante, segue o [[claude.md]] (frontmatter, regra "não criar ilhas") e fica registrada no histórico de atividade.
- Se o banco se perder, o conhecimento continua no vault e o índice é reconstruído.
- O Orion grava em `03 Recursos/Orion/Memória/`, separado das notas do Antonio.

## Grafo da memória
3D, como a visualização do Obsidian, sobre o céu do `sky.js`. A especificação completa (dados, visual, interação, implementação) está em [[2026-10-15 - Orion - redesign do protótipo no estilo do app Claude#Especificação do grafo 3D da memória (decidido)]]. Em resumo:
- **Nós** = notas, **arestas** = wikilinks, **cor** pelo `type`, **tamanho** pelos links.
- Estrelas com brilho aditivo, links que acendem na nota em foco, rótulos só para projetos, hubs e foco, sem sobreposição.
- Girar, zoom, clicar para voar até a nota, grafo local (profundidade 1 ou 2), filtros por tipo, pasta e tag, busca navegável pelo teclado.
- Layout 3D em segundo plano com Barnes-Hut; posições guardadas no índice pelo hash do commit do vault.
- O protótipo usa um Canvas 2D com o vault real (snapshot de 15/10/2026: 895 notas, 3.804 wikilinks) só para validar dados e interação.

## Efeitos visuais
- **Brilho dos nós:** blending aditivo no pipeline wgpu, igual ao three.js do original. Fácil.
- **Bloom:** textura HDR, limiar, downsample/upsample (dual Kawase), soma. 3 a 5 passes WGSL. Médio.
- **Blur de vidro nos painéis:** dentro do grafo, borrar o retângulo atrás do painel usando a textura do frame. Fora dele, Mica/Acrylic do Windows ou fundo sólido. Médio a difícil.
- **Plano B:** grafo em Bevy (bloom pronto) ou `three-d`, ou WebGL numa webview só para o grafo.

## Acessibilidade e desempenho
- Foco visível em tudo; navegação completa pelo teclado; o grafo tem busca + lista + painel como alternativa.
- "Movimento reduzido" desliga streaming animado, rotação e brilho pulsante.
- O céu e o grafo pausam quando não estão visíveis; ≤ 20 fps parado, 60 fps só na interação.
- Qualidade automática no notebook: sem bloom e blur se a GPU não aguentar.
- Meta de RAM com a interface aberta e parada: abaixo de 300 MB, quanto menos melhor (ver [[2026-10-15 - Orion - redesign do protótipo no estilo do app Claude]]).

## Fases
- [ ] **F1 — Janela e tema:** barra de título própria, Mica/Acrylic, bandeja, atalho global, barra lateral, tokens Grafite, tipografia (Geist), escala 90–150% com Ctrl +/-, português e inglês.
- [ ] **F2 — Chat (primeira versão):** campo de mensagem com anexos (20 MB), `/` e `@`, Markdown completo (Mermaid, LaTeX, prévia de HTML), editar e reenviar, seletor de modelo (Auto, Claude, Codex, Gemini, Orion), streaming com parar, linha de ferramentas, cartão de aprovação, painel de contexto, constelação na tela inicial.
- [ ] **F2.5 — Aprovações e avisos (aprovações e modo autônomo entram na v1):** fila única com histórico, aprovação pelo Telegram (ID + botões), notificação do Windows, modo autônomo.
- [ ] **F3 — Configurações (completas na v1):** modal com lista e busca, persistência das preferências.
- [ ] **F4 — Memória no vault (próxima prioridade depois do chat):** leitor do vault, índice SQLite (texto + embeddings), gravação automática das conversas, resumo diário às 23:59, commit na `main`, aviso de conflito com o Obsidian, telas Conhecimento e Fontes.
- [ ] **F5 — Grafo 3D (junto com a F4):** widget `shader`, layout em segundo plano, céu do `sky.js`, interação completa.
- [ ] **F6 — Efeitos:** bloom e blur de vidro.
- [ ] **F7 — Voz e sensores:** palavra "Orion", voz ao vivo, pílula de estado ligada ao áudio.
- [ ] **F9 — Orçamento de uso:** roteador por complexidade, estimativa de cota, gasto semanal e automelhoria com PR.
- [ ] **F8 — Extensões e nuvem:** Conectores (MCP), skills, plugins WASM, importação do Claude/Codex, `/nuvem` com Claude e Codex.

## Respostas da rodada interativa (15/10/2026)
| Tema | Decisão |
|---|---|
| Limite do grafo | Agrupa por pasta a partir de **500 notas**. |
| Conflito com o Obsidian | Mantém as duas versões e marca o conflito para revisão (reforça a regra acima). |
| Cota estourada no meio da tarefa | **Troca de provedor** automaticamente. |
| Automelhoria | Quem escolhe a fila é **você**. Mudanças de código saem como **PR** (nunca merge sozinha). |
| Chaves de API | **Cofre do sistema** (Windows Credential Manager). Nunca vai para o vault nem para o git. |
| Conteúdo bloqueado | **Pastas marcadas** nunca saem para provedor externo. |
| Avisos | Tarefa termina, tarefa falha, cota no limite e automelhoria pronta. |
| Sincronização | Vault via git **e** SQLite sincronizados entre os PCs (como na seção Vários PCs). |
| Rotinas | **Só as críticas** vão para a nuvem; as demais rodam com o Orion aberto. |
| Mini barra de voz | **Versão 2**. |
| Clique num nó do grafo | Abre no **painel do Orion**. Obsidian fica como ação secundária. |
| Modo claro | Não. Só escuro. |
| Histórico de conversas | **Para sempre.** |
| Canal de atualização | **Estável.** |
| Relatórios de erro | **Só local.** Nada sai do PC. |
| Sons | **Ligados** por padrão. |
| APIs gratuitas | Groq, Gemini API, OpenRouter, Mistral e demais provedores com plano gratuito. Ordem **por tarefa**: código → **Groq**; conversa e resumo → **Gemini API**. |
| Origem da resposta | **Sempre**, de forma discreta. |
| Tela inicial | Constelação, saudação e campo de mensagem, mais **sugestões**, **projetos recentes** e **pendências** (como no protótipo). |
| Gasto com API paga | **Zero.** Só gratuitas; nada pago sem aprovação. |
| Modelo local de fallback | Escolhido **pela GPU** detectada. |
| Idioma das respostas | **Sempre português**. |
| Projeto | Ganha **arquivos de referência** (documentos que o Orion sempre lê). |
| Visão (rosto) | **Clássico e local** (Haar + LBPH), sem IA e sem nuvem. |
| Visão (objetos) | **Modelo pequeno local** (ONNX, CPU). |
| Ativação "Orion" | Sensibilidade **média**. |
| Clientes externos | Tela que gera **credenciais de leitura** para outros programas (ex.: Claude Code via MCP do Orion), com nome, contexto, validade e permissões por leitura. Não permite administrar o Orion. **Depois da v1**. |
| Atalho Win + O | **Manter.** O Windows só usa esse atalho para orientação em tablets. |
| Certificado de código | **Não, por enquanto.** Instalador sem assinatura. |
| Aprovação sem resposta | Cancela e registra após **24 horas**. |
| Pastas confiáveis | **Documentos, Área de trabalho e pasta dos projetos.** |
| Resumo diário com PC desligado | **Gerar na nuvem.** Ressalva: o conteúdo do dia sai do PC, o que contradiz "nunca sai do PC". A regra de privacidade precisa ser ajustada. |
| Terminal integrado | **PowerShell.** |
| Não perturbe | **Só manual.** |
| CLIs (Claude, Codex, Gemini) | **Atualiza sozinho.** |
| Logs locais | **30 dias.** |
| Backup no Drive | **Diário**, com as últimas 7 cópias. |
| Cópia local do backup | **Sim**, na pasta do Orion. |
| Monitor da janela | **Onde estava** da última vez. |
| Leitor de tela (Narrador) | **Fora da v1.** |
| Autenticação do push do vault | **Credential Manager.** |
| Streaming das respostas | **Sim.** |
| Conversa longa demais | **Resumir automaticamente** e avisar. |
| Repositório Orion | **Branch e PR**, com revisão antes do merge. |
| Formato de configuração | **TOML.** |
| Snapshots de tela | Só das **telas alteradas** no PR. |
| Histórico de atividade | **1 ano**, depois resume por mês. |
| Transcrição de voz | **Whisper no Groq** (envia o áudio para a Groq). |
| Voz de resposta | **edge-tts pt-BR** (envia o texto para a Microsoft). |
| Palavra "Orion" | **openWakeWord** (modelo treinado, local). |
| Modelo local de fallback | **llama.cpp embutido.** |
| Busca na web | **Os dois**: Gemini com Google Search primeiro; Brave quando a cota acabar. |
| Gmail e Agenda | **Só leitura.** Enviar ou criar pede aprovação. |
| Google Drive | **Drive inteiro.** Ressalva: dá acesso de leitura a todos os arquivos, não só ao backup. |
| Voz ao vivo | **Gemini Live.** |
| Marco 1 em Rust (além do Chat) | **Memória com grafo**. |
| Grafo 3D | **wgpu puro**. |
| Plano B da UI | **Slint**, se o spike do iced não atender o visual. |
| Backup no Drive | Criptografado com a **senha do Orion**. |
| Atualizações | Instala **ao fechar** o app. |
| Ponte de desktop e Planos de arquivos | **Depois da v1.** |
| Queda do PC no meio de tarefa | **Retoma do ponto salvo.** Salva o progresso a cada passo. |
| Tarefas em paralelo | **Até 3.** |
| Parada para decisão | Tarefa longa e autônoma para **só quando precisa de uma decisão sua**. |
| Legado (Qdrant + SurrealDB) | **Descartar tudo**: ~3,09 mi de vetores BGE-M3 e a memória do SurrealDB não migram. Isso contraria a paridade e a migração verificada previstas no `ORION.md` (decisão do Antonio, a atualizar no repositório). Antes do descarte, exportar as tabelas pessoais (eventos, lembretes, tarefas, contatos), conforme `ORION_LEGADO.md`. |
| Busca do vault | **BM25 + LSA direto**, código puro, sem modelo neural (decidido 15/10; o `ORION.md` ainda prevê medir BGE-M3 na fase 1, a atualizar). Os vetores ficam no SQLite (índice); o vault segue como fonte. |
| Licença-alvo | **Uso pessoal.** |
| Venda do PC | **Só no fim do ano.** A exportação das tabelas pessoais do SurrealDB precisa ser feita antes disso. |

## Perguntas em aberto
- Grafo 3D com wgpu puro ou com Bevy/`three-d` (decidir depois de um protótipo da F5).
- Tela inicial definitiva: manter a constelação do original ou mudar.
- O que mais um Projeto deve ter no Orion além de nota, repositório, conversas e instruções.
- Quais telas do front original entram: Vida (finanças, preços, linha do tempo, foco), Ponte de desktop (captura rápida e "isso?"), Planos de arquivos e Clientes externos.
- Detalhes da câmera: retenção, objetos lembrados, onde roda o reconhecimento.
- Confirmar se o `/usage` do Claude e o `/status` do Codex podem ser lidos por automação sem violar os termos, e confirmar nos termos do Claude, Codex e Gemini se tarefas automáticas em segundo plano com a assinatura são permitidas.
- Filtros do grafo: confirmar tag, pasta, projeto e tipo como os filtros da v1.
