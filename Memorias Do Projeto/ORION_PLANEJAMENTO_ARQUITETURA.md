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
| 16/10/2026 | Segunda rodada (núcleo, provedores, segurança, voz, sync): ver [[#Respostas do Antonio (16/10/2026)]]. |
| 16/10/2026 | Terceira rodada (segurança, funcionalidades, performance, design, código, qualidade, lógica, ferramentas): ver [[#Respostas do Antonio (16/10/2026, rodada 3)]]. |
| 16/10/2026 | Quarta rodada (proatividade, rotinas, voz, plataforma, docs): ver [[#Respostas do Antonio (16/10/2026, rodada 4)]]. |
| 16/10/2026 | Quinta rodada (telas): ver [[#Telas (16/10/2026, rodada 5)]]. |
| 16/10/2026 | Estrutura completa das Configurações: ver [[#Tela de Configurações (estrutura)]]. Substitui a lista de seções da rodada 5. |
| 16/10/2026 | Sétima rodada (chat, painel, captura, editor, terminal, Telegram): ver [[#Respostas do Antonio (16/10/2026, rodada 7)]]. |
| 16/10/2026 | Pesquisa de referências + login com 2FA + limpeza das seções duplicadas: ver [[#Pesquisa e referências (16/10/2026)]] e [[#Respostas do Antonio (16/10/2026, rodada 8)]]. |
| 16/10/2026 | Funções novas (pesquisa extensa + rodada 9): ver [[#Funções novas (16/10/2026, rodada 9)]]. |
| 16/10/2026 | Pesquisa extensa (~350 links) + rodada 10: ver [[#Respostas do Antonio (16/10/2026, rodada 10)]] e [[ORION - REFERÊNCIAS DE PESQUISA]]. |
| 16/10/2026 | Instalador: ver [[#Instalador (16/10/2026, rodada 11)]]. Substitui o "MSIX puro" da rodada 10. |
| 17/10/2026 | Interface completa: especificação exata em [[ORION - UI (ESPECIFICAÇÃO)]] (protótipo `Protótipos/orion-ui-mockup.html`). Itens em que decisões posteriores vencem o protótipo: tabela U1–U15 da nota. |
| 16/10/2026 | Plano de implementação em etapas (F00 a F27) com checklist: [[ORION - PLANO DE IMPLEMENTAÇÃO]]. |

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
| Atalho global | ~~Win + O~~ → **Win + Shift + Espaço** abre a janela focada no campo de mensagem (Win+O é do Windows; 16/10). |
| Aprovação pelo Telegram | Só aceita do **ID do Antonio**, com botões Aprovar/Negar. |
| Modo autônomo | Interruptor nas configurações. Ligado, o Orion age sem pedir aprovação, **exceto no que pode quebrar o PC ou o projeto**. Ver [[#Permissões]]. |
| Projetos | Ver [[#Projetos (detalhe)]]. |

## Respostas do Antonio (16/10/2026)
| Tema | Decisão |
|---|---|
| Orion antigo (Python) | **Reescrever tudo em Rust.** Sem backend Python. |
| Processo | **Núcleo no mesmo processo do front** (um `.exe`). Ao fechar, a janela é destruída e o núcleo segue vivo na bandeja (rotinas e Telegram continuam). |
| Banco | **SQLite + FTS5** para operação, índice e busca no texto completo. |
| Embeddings | Via **API gratuita** (ex.: Gemini embeddings). |
| Assinaturas | Claude, Codex e Gemini **só pelas CLIs oficiais** como subprocesso (saída em streaming JSON). |
| APIs gratuitas | **Todas que forem encontradas**: Groq, Gemini (free tier), OpenRouter `:free`, Cerebras, Mistral e outras. Cadastro **genérico OpenAI-compatível** (base URL + chave + modelo). |
| Roteador "Auto" | **Regras + heurística** (sem LLM): palavras-chave, tamanho, anexos, projeto. |
| Senha | **Cifra o banco e as chaves** (SQLCipher + chave derivada com Argon2). Sem senha nada abre. |
| Comandos no modo autônomo | **Livres**, sem sandbox; só a linha "sempre pedir" de [[#Permissões]] segura. |
| Ramos da conversa | Setas **< 2/3 >** na mensagem editada, como no Claude. |
| Contexto longo | **Compacta automático**: resume o início, mantém fixadas e a nota do vault. |
| Fatos sobre o Antonio | **Tabela no banco + nota no vault.** O Orion usa o banco; se divergirem, **o vault vence** (reimporta no `git pull`). |
| Voz | **STT local** (Whisper) e **TTS na nuvem** (provedor a testar depois). **Sem voz offline.** |
| Wake word | **openWakeWord/rustpotter** com modelo treinado para "Orion". |
| Ordem depois da v1 | **Telegram → Voz → Grafo → Câmera.** |
| Modelos locais | Na instalação, lista de **~5 modelos** com descrição curta para escolher e baixar. Sem padrão fixo. |
| Sync PC↔notebook | **Merge por evento** (log de eventos com ID e hora, junta ao reconectar). |
| Automelhoria | **Ligada na v1**, conforme a regra 90%/12 h. Termos dos provedores ainda precisam ser checados. |
| Testes de tela | **iced_test + PNG** comparado no CI. |
| Rotinas na nuvem | Resultado via **commit no vault + Telegram**; ao ligar, o Orion lê do vault e cria a mensagem. |
| Resumo das conversas | Escrito por **API gratuita**. |
| Commits no vault | **Mantém 1 commit por nota.** |
| Plugins WASM | **Depois da v1.** v1 só MCP + skills. |

## Respostas do Antonio (16/10/2026, rodada 3)
| Área | Tema | Decisão |
|---|---|---|
| Segurança | Prompt injection | Conteúdo externo (web, nota, e-mail) é **dado, não ordem**. Ação pedida por ele sempre exige aprovação, mesmo no modo autônomo. |
| Segurança | Logs de prompts/respostas | **30 dias, cifrados**, com rotação automática. |
| Segurança | Senha errada | **Atraso crescente** (5 erros → 1, 5, 15 min) e aviso no Telegram. |
| Segurança | Esqueceu a senha | **Chave de recuperação impressa**, gerada no onboarding. |
| Segurança | Fora das pastas confiáveis | **Ler e escrever pedem aprovação.** |
| Segurança | Rede de saída | **Livre + registro** de cada host contatado. |
| Segurança | Atualizações | **Assinatura própria (minisign)**; o app recusa binário não assinado. |
| Segurança | MCPs de terceiros | **Permissão por ferramenta** (sempre/perguntar/nunca), padrão "perguntar". |
| Funcionalidade | Controle do PC (mouse/teclado/janelas) | **Livre no modo autônomo** (UI Automation). |
| Funcionalidade | Navegador | **Chrome headless via CDP**: ler páginas, preencher, prints. |
| Funcionalidade | Briefing diário | **Agenda + e-mails, PRs/CI dos projetos, tarefas do vault.** |
| Funcionalidade | Conversas em paralelo | **Sim, com limite** (ex.: até 3 agentes; o resto vai para a fila). |
| Funcionalidade | Lembretes e tarefas | **Vault (Tasks) e Google Calendar/Tasks**, os dois. |
| Funcionalidade | E-mail | **Envia com aprovação.** |
| Performance | Primeira resposta | Mostra **"pensando" em menos de 100 ms**; sem meta rígida para o modelo. |
| Performance | Conversas longas | **Virtualização + cache de layout** (desenha só o que está visível). |
| Performance | Indexação do vault | **Incremental por hash** depois de cada `git pull`. |
| Performance | Runtime assíncrono | **Tokio.** |
| Performance | Layout do grafo 3D | **Compute shader na GPU** (substitui o rayon/Barnes-Hut na CPU). |
| Performance | Bateria | **Modo economia automático**: desliga animações e efeitos; a wake word continua. |
| Design | Animações | **Sutis, 150–200 ms.** |
| Design | Fontes | **Inter + JetBrains Mono**, embutidas no binário. |
| Design | Largura do chat | **Ajustável** nas configurações. |
| Design | Ícones | **Lucide.** |
| Design | Realce de código | **Derivado dos tokens Grafite.** |
| Design | Navegação em respostas longas | Botão **"ir ao fim" + minimapa** de mensagens na barra de rolagem. |
| Design | Barra lateral | **Recolhível com Ctrl+B.** |
| Código | Repositório | **Cargo workspace**: `orion-core`, `orion-ui`, `orion-providers`, `orion-vault`, `orion-tools`… |
| Código | Erros | **thiserror** nas libs + **anyhow** no app. |
| Código | Logs | **tracing** + arquivo rotativo, com spans por conversa e ferramenta. |
| Código | SQL | **rusqlite** + migrações (compatível com SQLCipher). |
| Qualidade | CI obrigatório | **Testes + snapshots** e **cargo deny/audit**. Sem clippy/fmt nem cobertura mínima obrigatórios. |
| Qualidade | Evals do agente | **Suíte de casos fixos no CI** (roteamento, comandos, ferramentas, com respostas gravadas). |
| Qualidade | Revisão de PRs | **Claude + Codex cruzado.** |
| Qualidade | Releases | **`main` + tags semver**, canal **beta** (PC) e **stable** (notebook). |
| Lógica | Limite de passos do agente | **Configurável por projeto.** |
| Lógica | Loop de ferramentas | **Híbrido**: loop próprio com as APIs gratuitas; código pesado vai para a CLI do Claude/Codex, que usa as ferramentas dela. |
| Lógica | Troca de provedor no meio da conversa | **Resumo ao trocar.** |
| Lógica | Conflito entre pedido, vault e banco | **Perguntar sempre.** |
| Ferramentas | Nativas na v1 | **Arquivos + shell + git, web (busca + fetch + Chrome), vault, sistema** (apps, janelas, volume, área de transferência). |
| Ferramentas | Busca na web | **Várias APIs (Google, Brave etc.)**, cadastradas como as APIs gratuitas, com rodízio. |
| Ferramentas | Terminal embutido | **alacritty_terminal + ConPTY.** |
| Ferramentas | Editor | **Extensão do Orion para VS Code** (chat dentro do editor). |

## Respostas do Antonio (16/10/2026, rodada 4)
| Tema | Decisão |
|---|---|
| Onboarding | **Completo de uma vez** (todas as etapas no assistente). |
| Proatividade | Avisos importantes **e sugestões**, **até 3 por dia**, agrupadas. Observa **janela/app ativo, histórico de conversas, git/CI/projetos e agenda**. |
| Histórico de apps usados | **Só totais por dia** ("VS Code 5h"), sem títulos de janela. |
| API paga | **Nunca.** Só assinatura, gratuitas e local. |
| Reorganizar o vault | **O Orion propõe, você aprova** (lista de mudanças em lote). |
| Rotina que falha | **3 tentativas com backoff**, depois avisa. |
| Rotina perdida (PC desligado) | Roda ao ligar **se o atraso for menor que 6 h**; se não, fica marcada como "pulada". |
| Subagentes | **Sim**, dentro do limite de agentes em paralelo. |
| Imagem para provedor sem visão | **Vai para um provedor com visão** (ex.: Gemini free). |
| Exportar conversa | **Markdown.** |
| Cache | **Cache de prompt do provedor + respostas idênticas por 24 h.** |
| Mensagens fixadas | Até **~2k tokens**; avisa se passar. |
| Crash | **A automelhoria corrige sozinha** (gera PR de correção). |
| Horário de silêncio | **Nenhum automático.** Só o "não perturbe" manual. |
| Interromper a voz | **Sim, falando por cima** (precisa de cancelamento de eco). |
| Idioma do reconhecimento de voz | **Só português.** |
| Bot do Telegram | **Só o Antonio**; ignora qualquer outra pessoa ou grupo. |
| Clique no grafo | **Prévia lateral + vizinhos em destaque**; duplo clique abre o editor. |
| Windows | **Só Windows 11.** |
| GPU | **Integrada funciona**, com grafo simplificado e fallback na CPU. |
| Repositório do Orion | **Privado.** |
| Documentação do código | **Markdowns bem estruturados e organizados.** |

## Telas (16/10/2026, rodada 5)
| Tela | Decisão |
|---|---|
| Inicial | Saudação por **hora do dia + nome**, constelação, **campo de mensagem central** e **chips de sugestões rápidas** conforme o contexto do dia. |
| Barra lateral | Nova conversa, Buscar, Projetos, Aprovações, Rotinas, Memória, Painel; Configurações no rodapé, junto do perfil. Recolhe com Ctrl+B. |
| Login | Ver rodada 8: perfil local, avatar, céu animado; 2FA pelo Telegram ao entrar de novo, Windows Hello como reserva. |
| Bloqueio | **Céu + constelação com campo de senha.** Mostra só a contagem de pendências, sem conteúdo. |
| Configurações | Estilo Claude: **lista de seções à esquerda** (Geral, Modelos, Provedores, Conectores, Voz, Privacidade, Vault, Rotinas, Atalhos, Sobre) e **busca no topo**. **Salva sozinha ao mudar**, com aviso "Salvo" e desfazer. Também existe `settings.toml` editável à mão, com botão "Abrir arquivo"; os segredos ficam fora dele, no cofre. |
| Provedores | **Catálogo pronto** (URL e link da chave já preenchidos) + opção **"Personalizado"**. Por provedor: estado e latência (testar conexão), cota restante e reset, modelos disponíveis (lidos da API) e prioridade (arrastar). Chave inválida é **desativada e marcada em vermelho**, com aviso. **Uma conta por provedor**: nada de várias contas nem IP rotativo para somar cota. A cota cresce somando provedores diferentes. |
| Rede | **Exit node da Tailscale automático** em rede não confiável (sai pelo PC de casa), por segurança. |
| Conectores | Abas **Instalados / Descobrir / Skills**. Antes de instalar um MCP, mostra as ferramentas que ele expõe, o comando ou URL exato, a origem (repositório) e as permissões pedidas. |
| Projetos | **Cartões com estado.** Criar a partir de **repositório GitHub, pasta local ou do zero**. Painel do projeto: pendências e tarefas, PRs e CI, conversas e notas recentes. Arquivar **move a nota para `04 Arquivo`**, esconde as conversas e pausa as rotinas. |
| Aprovações | **Aprovar ou negar em lote** (menos o que está na lista "sempre pedir"). **"Sempre permitir isto" por projeto** direto no cartão; vira regra editável em Privacidade. |
| Rotinas | Horário em **linguagem natural com prévia** das próximas 3 execuções. |
| Memória | Grafo + **notas recentes do Orion** + **busca no vault**. |
| Painel | Gráficos com abas **Hoje / Semana / Mês**. |
| Modelos locais | Adicionar e remover modelos, **teste rápido** (prompt + tokens/s), escolher quantização (Q4/Q5/Q8), **descarregar da RAM após X min** e escolher GPU ou CPU. |
| Nuvem | Por tarefa: log ao vivo, mandar mensagem para a sessão, cancelar e abrir no navegador. |
| Privacidade | Registro do que saiu (provedor e hora), regras de permissão salvas e hosts contatados. |
| Atalhos | **Personalizáveis**, com detecção de conflito. **Ctrl+K unifica** busca e comandos (prefixo `>`). |
| Voz (config) | Sensibilidade da wake word e das palmas, microfone e alto-falante, voz e velocidade do TTS. |
| Sobre | Depois de atualizar, abre **"O que mudou"**. |

## Tela de Configurações (estrutura)
Lista à esquerda **agrupada com títulos**, busca no topo, salva sozinha ao mudar (aviso "Salvo" + desfazer), `settings.toml` espelhado (sem segredos).

**Regras gerais**
- **Descrição curta embaixo de cada opção.**
- **Restaurar padrão por opção e por seção**; opção alterada mostra um **ponto azul**.
- Opções avançadas e perigosas ficam escondidas atrás de **"Mostrar avançado"** (ex.: logs de depuração, resetar o banco).
- Mudanças sensíveis (desligar a senha, ligar o modo autônomo, apagar dados) **pedem a senha de novo**.
- **Link direto para uma seção:** `/config <seção>` e links nos avisos do chat (ex.: aviso de cota → Orçamento).
- **Configuração por projeto sobrepõe a global**, na tela do projeto (modelo, passos, autônomo, pastas, instruções), com a marca "herdado" quando não foi mudada.
- **Sem perfis** (Trabalho, Pessoal…) na v1.

**App**
- **Geral:** nome, idioma (pt/en), tom de resposta (Conciso/Normal/Detalhado + instruções livres), iniciar com o Windows e comportamento da bandeja.
- **Sobre você:** lista de fatos com editar e apagar, espelhando a nota do vault.
- **Aparência:** escala da interface (90–150%), largura do chat, animações, bloom/blur, qualidade gráfica (automática pela GPU), Mica/Acrylic.
- **Notificações:** **matriz evento × canal** (Windows, Telegram, som, bandeja) para cada evento (aprovação pendente, resposta pronta, rotina concluída, nuvem terminou, cota, sugestão proativa); "não perturbe".
- **Atalhos:** atalhos globais e do app, personalizáveis com detecção de conflito; **comandos `/` personalizados** (prompt salvo, opcionalmente ligado a um projeto).

**IA**
- **Modelos:** regras do Auto (editáveis), modelo + esforço padrão por tipo (chat, código, resumo, visão, imagem), modelo do resumo do vault e da compactação, ordem de fallback global.
- **Provedores:** catálogo + "Personalizado", estado e latência, cota e reset, modelos disponíveis, prioridade (arrastar); uma conta por provedor.
- **Orçamento:** teto de 95%, dia e hora do reset semanal por provedor, calibrar o uso real, fila da automelhoria.
- **Agente:** modo autônomo, limite de passos, agentes em paralelo e subagentes, proatividade (sugestões por dia, o que observa), pastas confiáveis.
- **Modelos locais:** adicionar e remover, teste rápido, quantização, descarregar da RAM, GPU/CPU.

**Memória**
- **Vault:** caminho, remoto git e branch; regras de gravação (o que vira nota, horário do diário); indexação (reindexar, embeddings, exclusões). As pastas do Orion (Memória, Diário, Imagens) são fixas.

**Entradas**
- **Voz:** sensibilidade da wake word e das palmas, microfone e alto-falante, voz e velocidade do TTS.
- **Câmera e visão:** os dois modos (sob pedido e presença), objetos lembrados, legenda ao vivo.

**Integrações**
- **Contas e integrações:** Telegram, Google (agenda e e-mail), GitHub.
- **Conectores:** abas Instalados / Descobrir / Skills, permissão por ferramenta.
- **Desenvolvimento:** terminal e shell, extensão do VS Code, CLIs do Claude/Codex/Gemini, nuvem, revisão cruzada.

**Sistema**
- **Privacidade e segurança:** senha, chave de recuperação, regras de permissão, registro do que saiu, hosts contatados, retenção de logs.
- **Sistema:** atualizações (canal beta/stable), backup/exportar/importar, sincronização PC↔notebook, desempenho, bateria e RAM.
- **Sobre:** versão, "O que mudou", licenças.

## Respostas do Antonio (16/10/2026, rodada 7)
| Tema | Decisão |
|---|---|
| Ações na resposta | **Copiar, regenerar e 👍/👎** (o voto alimenta os evals e o roteamento). |
| Raciocínio (thinking) | **Recolhido**, expande ao clicar. |
| Parar a resposta | Botão **Parar** e **Esc**; **mantém o texto parcial**. |
| Digitar enquanto responde | **Entra na fila** e é usado no próximo passo, como no Claude Code. |
| Artefatos (código, HTML, documento longo) | **Aba no painel da direita**, com versões, copiar e salvar em arquivo. |
| Arquivo acima de 20 MB | Oferece **referenciar o caminho**, sem copiar, se estiver numa pasta confiável. |
| Tokens e cota | **Discreto, ao passar o mouse na hora** da mensagem (tokens + % da cota usada). |
| Links e caminhos nas respostas | **Clicáveis**: arquivo abre no VS Code, URL no navegador. |
| Aba Contexto | Notas do vault usadas, arquivos lidos ou alterados, mensagens fixadas, projeto e instruções ativas. |
| Aba Diff | Aprovar **por arquivo e por bloco** (como `git add -p`). |
| Histórico lateral | **Por data, com filtro por projeto.** |
| Mover conversa de projeto | **Sim**, pelo menu da conversa; a nota do vault é religada ao novo projeto. |
| Captura rápida (Win+Shift+O) | **Barra no centro da tela, estilo Spotlight**; fecha com Esc e mostra o tipo detectado. |
| "Isso?" (Win+Shift+I) | **Seleção de região**; Enter captura a tela cheia. |
| Mini barra de voz | **Transcrição ao vivo + resposta curta**; resposta longa abre a janela. |
| Menu da bandeja | Nova conversa e captura, pausar voz e câmera, ligar e desligar o modo autônomo. |
| Telas vazias | **Ilustração do céu + 1 ação sugerida.** |
| Editor de notas | **Autocompletar `[[wikilinks]]` e `#tags`**, **pedir ao Orion para reescrever um trecho**. |
| Terminal embutido | **Você e o agente usam a mesma sessão.** |
| Respostas longas no Telegram | **Resumo + botão "ver completo"** (abre a conversa no PC ou manda o `.md`). |

## Respostas do Antonio (16/10/2026, rodada 8)
| Tema | Decisão |
|---|---|
| Login | **Perfil local** (nome + avatar + senha), **só um usuário**. Tela com **céu animado, Orion surgindo e avatar no centro**. |
| 2FA | **Código pelo Telegram**, pedido **só ao entrar de novo depois de sair da conta** (não ao desbloquear após Win+L). Reserva: **Windows Hello** quando não houver internet ou o Telegram falhar. |
| Embeddings | **API só para notas sem a tag `#privado`**; notas `#privado` ficam só no FTS (ou recebem embedding local, se houver). Reduz o conflito com "segredos nunca saem". |
| Atalho global | **Win + Shift + Espaço** (Win+O já é do Windows). |
| Camada de provedores | **Própria, OpenAI-compatível** (sem rig/genai). |
| Prompt injection | **Classificador local** (ex.: Shield Small / ProtectAI DeBERTa) em todo conteúdo externo, como **camada extra**; a aprovação continua obrigatória. |
| icebreaker | **Estudar como referência** (streaming e Markdown no iced). |
| Terminal | **Avaliar o `iced_term` primeiro.** |
| Busca no vault | **FTS5 + sqlite-vec** com rank fusion, num arquivo só. |
| Wake word | **oww-rs** (openWakeWord em Rust, modelos `.onnx`). |
| Auditorias do Orion | **OWASP LLM + Agentic Top 10**, **Trail of Bits** (Semgrep, cadeia de dependências) e **design critique + acessibilidade** nas telas. |
| Documento | Seções duplicadas **mescladas sem perder decisões** (16/10). |

## Pesquisa e referências (16/10/2026)
| Área | Referência | Uso no Orion |
|---|---|---|
| App de referência | [icebreaker](https://github.com/hecrj/icebreaker): chat local em iced + llama.cpp, do autor do iced | Padrões de streaming, Markdown e modelos locais |
| Terminal | [iced_term](https://github.com/Harzu/iced_term) | Terminal embutido sobre `alacritty_terminal` |
| MCP | [rmcp](https://github.com/modelcontextprotocol/rust-sdk): SDK Rust oficial, Tokio, stdio/HTTP/OAuth | Cliente MCP |
| Provedores | [rig](https://rig.rs) (descartado: API instável) | Só como referência de abstração |
| Busca | SQLite FTS5 + [sqlite-vec](https://github.com/asg017/sqlite-vec); [Tantivy](https://github.com/quickwit-oss/tantivy) como alternativa | Busca híbrida |
| Voz | [oww-rs](https://docs.rs/crate/oww-rs), [rustpotter](https://github.com/GiviMAD/rustpotter), [whisper-rs](https://github.com/tazz4843/whisper-rs); exemplo completo: [Recho](https://github.com/redromnon/recho) | Wake word e STT |
| Prompt injection | [ZeroLeaks Shield Small](https://huggingface.co/ZeroLeaks/shield-small), [ProtectAI deberta-v3-small v2](https://huggingface.co/protectai/deberta-v3-small-prompt-injection-v2), [ModernBERT detector](https://huggingface.co/siberiancat/modernbert-prompt-injection) | Classificador local (só inglês em alguns: **testar em pt-BR**) |
| Login e 2FA | [totp-rs](https://crates.io/crates/totp-rs), [keyring](https://crates.io/crates/keyring), [windows-dpapi](https://docs.rs/windows-dpapi); Windows Hello via crate `windows` (`UserConsentVerifier`); desenho de referência: [byteness/keyring winhello](https://pkg.go.dev/github.com/byteness/keyring/winhello) | Chave do cofre protegida pelo Hello; 2FA |
| Checklists | [OWASP LLM Top 10 2026](https://genai.owasp.org/llm-top-10/) e [OWASP Agentic Top 10](https://genai.owasp.org/) | Auditoria de segurança do agente |
| Skills de auditoria | [trailofbits/skills](https://github.com/trailofbits/skills), [secure-design-review](https://claudskills.com/skills/secure-design-review/), [anthropics/skills frontend-design](https://github.com/anthropics/skills/tree/main/skills/frontend-design) | Revisões de código, segurança e design |

**Problemas achados na releitura:** seções duplicadas (corrigido); embeddings por API contra "segredos nunca saem" (mitigado pela tag `#privado`); atalho Win+O em conflito com o Windows (trocado); exit node da Tailscale depende do PC de casa ligado.

## Funções novas (16/10/2026, rodada 9)
Todas entram **depois da v1**. A ordem das fases ainda está **a definir**.

| Função | Decisão |
|---|---|
| **Memória de tela** (estilo Recall/screenpipe) | **Só texto, sem imagem** (acessibilidade + OCR), com busca ("o que eu vi ontem sobre X"), lista de apps excluídos e ~1 GB por mês. |
| **Reuniões** (estilo Meetily/Hyprnote) | **Detecta a chamada e pergunta** se deve gravar; Whisper local → resumo + tarefas no vault. |
| **Click to Do** | **Atalho próprio + clique** em qualquer coisa na tela: resumir, traduzir, explicar, salvar no vault, criar tarefa. |
| **Skills automáticas** (estilo Hermes/Voyager) | **O Orion propõe a skill e você aprova.** A pesquisa mostra que skills geradas sem verificação pioram o agente. |
| **Alexa (Echo Dot 2)** | Orion fala no Echo (anúncios), dispara rotinas e aparelhos da Alexa, **"Alexa, pede pro Orion…"** (skill) e usa o Echo como alto-falante. Método: **oficial onde der** (skill via Tailscale Funnel, interruptores virtuais) + **API não oficial como extra** (estilo AlexaPy, pode quebrar sem aviso). Home Assistant via MCP, se houver. |
| **Finanças (Tela Vida)** | Entrada por **OFX/CSV**, **notificações/SMS de compra** e **captura rápida**. Categorias por **regras + LLM local** (dados bancários nunca saem). Mostra gastos por categoria e mês, orçamento com alerta, **assinaturas recorrentes detectadas** e previsão do saldo. |
| **Rastreio de tempo** (estilo ActivityWatch) | **Sim**, alimenta o Painel e as notas diárias (só totais). |
| **Modo foco** | **Só lembrete gentil**, sem bloquear nada. |
| **Navegador agente** | **Visível numa aba do painel** (você vê e pode assumir); pagamento sempre pede aprovação. |
| **Pesquisa profunda** (`/pesquisa`) | Relatório com fontes, **salvo como nota no vault**. |
| **Monitores** | Página ou site que mudou, **releases das libs e ferramentas que você usa** (iced, rmcp, Claude Code), notícias de temas. |
| **Briefing** | Texto no chat + Telegram, **áudio estilo podcast (2–5 min)** e nota diária no vault. |
| **Estudo** | **Flashcards com revisão espaçada (FSRS)** a partir das notas, revisão diária no chat/Telegram. |
| **Conexões no vault** (estilo Smart Connections) | Painel de **notas relacionadas no editor** + sugestão de links, aceita com um clique. |
| **Área de transferência inteligente** | Histórico de **7 dias, sem senhas** (Ctrl+Shift+V), com ações (traduzir, formatar JSON…). |
| **Organizador de arquivos** | **Automático**: move e renomeia; apagar vai **para a Lixeira**, nunca de forma definitiva; log com desfazer. |
| **Dev extras** | **`/aprendizado`** (abaixo), revisar PRs dos seus repositórios (rascunho para aprovar), explicar o erro quando um comando falha no terminal, changelog e mensagens de commit automáticos, **PR semanal de atualização de dependências**. |
| **`/aprendizado`** | O Orion **não faz por você, mostra como faz.** Dois modos, escolhidos na hora: **tutor socrático** (dicas em níveis, a solução só se pedir) ou **passo a passo com você digitando** (ele revisa). Registra o progresso em nota em `Estudo` + flashcards do que foi aprendido. |
| **Bem-estar** | Lembretes de **água** e **hora de dormir**. |
| **iPhone** | **Siri/Atalhos → Orion** (voz + automações por local e horário, ex.: "cheguei em casa" → Wake-on-LAN + briefing), **compartilhar do iPhone → Orion/Inbox**, **Phone Link** (notificações e SMS no PC, que também alimentam as finanças), **Saúde via Atalho** (sono e passos no briefing). |
| **Modo jogo** | **Automático em tela cheia**: descarrega os modelos (libera RAM e GPU) e silencia as sugestões; a voz continua. |
| **Mídia** | Controles de mídia do Windows + **API do Spotify** (tocar, pular, playlists por humor). |
| **Tradução ao vivo** | Áudio do PC em inglês → **legenda em português**, na legenda ao vivo já prevista. |
| **Retrospectiva** | **Semanal, no domingo**, em nota no vault (projetos, tempo, gastos, aprendizado). |

**Referências da pesquisa:** [screenpipe](https://screenpi.pe/about) (memória de tela, MCP), [Microsoft Recall / Click to Do](https://tomshardware.com/software/windows/microsoft-launches-recall-to-windows-11-general-availability-click-to-do-and-improved-search-also-coming), [Meetily](https://openalternative.co/alternatives/meetily) e [Hyprnote](https://char.com/docs/about/what-is-hyprnote) (reuniões), [Hermes Agent vs OpenClaw](https://charonhub.deeplearning.ai/hermes-agent-challenges-openclaw/) (skills automáticas; OpenClaw teve 341+ skills maliciosas → mais motivo para aprovar skills), [SoK: Agentic Skills](https://arxiv.org/pdf/2602.20867) (skills sem verificação pioram −1,3 pp), [Voyager](https://arxiv.org/html/2305.16291v2), [isair/jarvis](https://docsearch.algolia.com/mcp/docs/repo/isair/jarvis) (Home Assistant via MCP), [AlexaPy](https://pypi.org/project/AlexaPy/1.29.1) (API não oficial da Alexa), [actual-ai](https://github.com/sakowicz/actual-ai) e [Budgetron](https://www.mintlify.com/budgetron-org/budgetron/features/ai-categorization) (finanças com LLM), [ActivityWatch](https://activitywatch.net/) (servidor em Rust), [Smart Connections](https://community.obsidian.md/plugins/smart-connections) e [Spaced Repetition AI](https://community.obsidian.md/plugins/spaced-repetition-ai) (FSRS), [uiautomation-rs](https://gittrend.io/repo/leexgone/uiautomation-rs) (controle do PC), [panorama 2026 de assistentes com memória](https://www.vellum.ai/blog/best-personal-ai-assistants-with-memory).

## Respostas do Antonio (16/10/2026, rodada 10)
Base: [[ORION - REFERÊNCIAS DE PESQUISA]].

| Tema | Decisão |
|---|---|
| WebView | **Uma exceção ao "sem WebView":** um WebView isolado, **carregado só quando precisa**, para **MCP Apps**, **Mermaid** e **prévia de HTML**. A interface continua 100% iced. |
| Instalador | ~~MSIX puro~~ → **`Orion Setup.exe` com tela própria** + identidade de pacote registrada por trás (ver rodada 11). |
| Integração com Claude Code/Codex/Gemini | **ACP (Agent Client Protocol)** como padrão, com adaptadores prontos; **CLI direta (stream-json) como reserva**. |
| Google | Provedor **"Google" abstrato**, que usa o Gemini CLI ou o Antigravity CLI (o free tier do Gemini CLI vai ser substituído) ou a API Flash gratuita, o que existir. |
| MCP: tool poisoning e rug pull | **Hash das ferramentas** na aprovação; se a descrição ou o esquema mudar, **desativa até você reaprovar**; o classificador de injection também lê as descrições. |
| Provedores que treinam com seus dados | Campo **"usa dados para treino"** no catálogo; **código e vault privado nunca vão para esses provedores**. |
| UX das ações do agente | **Recibo de ação** (o que mudou + desfazer), **checkpoints nomeados** antes de tarefas longas, botão **"Por quê?"** nas decisões do Auto. |
| Memória de fatos | **Histórico temporal**: fato antigo marcado como substituído, não apagado ("o que era verdade em tal data"). |
| Filtro de PII | **cloakrs** (CPF/CNPJ, chaves de API, JWT) + **regras próprias** (RG, dados bancários, PIX). |
| Modelos locais do instalador | Qwen 3 8B, Gemma 4 E4B, Granite 4.1 8B, Phi-4-mini 3.8B, **Mistral Small 3.2 24B** (só PC), **MiniCPM-V 4.5** (visão/OCR), **Moondream 2** (visão leve, "Isso?" offline). Embeddings locais: **multilingual-e5-small**. |
| Reconhecimento de voz | **Whisper large-v3-turbo** (WER ~4,2% em PT) com **Vulkan**. |
| Cancelamento de eco | **aec3** (porta Rust do WebRTC) + **Silero VAD**. |
| Voz full-duplex | **Agora não**; continua o pipeline STT → LLM → TTS. |
| Wake-on-LAN remoto | **Roteador ou Raspberry como subnet router da Tailscale**, sempre ligado em casa. |
| Notificações com botões | **Aprovar/Negar no próprio aviso do Windows**, via MSIX + WinRT direto. |
| Abrir nota no Obsidian | **URI `obsidian://`.** |
| Sync PC↔notebook | **cr-sqlite** (merge de SQLite com vários escritores). **Confirmar se funciona junto com o SQLCipher.** |
| Roteador Auto | **Regras agora**; depois, **classificador treinado com seus 👍/👎** (estilo RouteLLM). |
| Evals | **Harness próprio em Rust + casos em YAML** (sem Node/Python no CI). |
| PDF | **pdf_oxide** (PDF → Markdown) + OCR quando for escaneado. |
| Modelos locais na RAM | **Todos descarregados por padrão.** Carregam só quando são usados e descarregam depois de X minutos sem uso (configurável). Exceção: só os modelos minúsculos de sempre (wake word ~5 MB, Silero VAD ~2 MB) ficam residentes, e só quando a voz estiver ligada. |
| Tamanho do instalador | **Estimativa: ~100–200 MB.** App Rust + iced/wgpu + llama.cpp + whisper.cpp (~40–80 MB), ONNX Runtime (~20–30 MB), fontes e ícones (~3 MB), wake word + VAD (~7 MB), classificador de injection quantizado (~70–150 MB), multilingual-e5-small int8 (~35–120 MB). WebView2 já vem no Windows 11 (0 MB). **LLMs, Whisper e visão não entram no instalador**: são baixados no onboarding, conforme a sua escolha. |
| Tamanho dos modelos opcionais (Q4) | Whisper turbo ~0,6–0,9 GB · Phi-4-mini ~2,4 GB · Moondream 2 ~1,8 GB · Gemma 4 E4B ~3–5 GB · Qwen 3 8B ~5 GB · Granite 4.1 8B ~5 GB · MiniCPM-V 4.5 ~5–6 GB · Mistral Small 3.2 24B ~14 GB. **Todos juntos: ~40 GB.** |
| iced 0.14 | Usar **time-travel debug, hot reload** e **testes headless/E2E** nativos no CI. |

## Instalador (16/10/2026, rodada 11)
> Especificação detalhada (fonte para implementar): [[ORION - INSTALADOR (ESPECIFICAÇÃO)]] · protótipo: `01 Projetos/Orion/Protótipos/orion-instalador.html`.

O Orion é um **app desktop nativo instalado** (Arquivos de Programas, Menu Iniciar, Configurações › Aplicativos, bandeja, busca do Windows). Todo app nativo no Windows é um `.exe`; a decisão aqui é só **como ele é entregue**.

| Tema | Decisão |
|---|---|
| Entrega | Você baixa o **`Orion Setup.exe`** (feito em iced, com a tela do Orion), que instala o app no Windows. O build gera o Setup com o app dentro, no padrão do Discord e do VS Code. |
| Identidade de pacote | O Setup **registra por trás um pacote esparso** (*packaging with external location*), invisível para você; isso mantém o OCR nativo e os avisos com botões Aprovar/Negar. |
| Assinatura | **Certificado autoassinado, instalado pelo próprio Setup.** Grátis; o SmartScreen avisa na primeira vez. |
| Instalar para | **Todos os usuários** (Arquivos de Programas). |
| Atualizações sem UAC | **Serviço de atualização com permissão** (como o Chrome e o Edge): atualiza sem pedir admin toda vez. |
| Visual | **Céu + constelação de Órion se formando** conforme o progresso; cada etapa acende uma estrela. **Som ambiente sutil, sempre ligado** (sem botão). Animação: estrela acende com clarão e raios, linhas se desenham entre as estrelas acesas, nebulosa M42, parallax lento, estrela cadente ocasional e onda de luz ao concluir. Prévia: https://claude.ai/artifact/EZNV5ftcDLDRjiZjD9uvco |
| Checagem do PC | **Windows 11, GPU, RAM e espaço em disco** antes de instalar; mostra o que roda bem. |
| Etapas | 1. **Boas-vindas + idioma** · 2. **Pasta de instalação e pasta dos modelos** (sugere sozinho o disco com mais espaço, ex.: `D:\Orion\Modelos`) · 3. **Escolha dos modelos locais** · 4. **Opções**: iniciar com o Windows, voz · 5. **Final**: constelação completa + botão **"Abrir Orion"**. |
| Escolha dos modelos | **Cartões** com tamanho, RAM, uso e o selo **"recomendado para o seu PC"** (detecta a RAM e a GPU e já deixa marcado). |
| Download dos modelos | **Em segundo plano**: o app já abre e usa as APIs enquanto baixa; pausar e retomar; verificação de hash. |
| Atalhos | **Menu Iniciar** e **fixado na barra de tarefas** (sem atalho na Área de Trabalho). |
| Onboarding | **No app, na primeira vez que abre** (perfil, senha, 2FA, vault, chaves, importar). O instalador só instala e baixa. |
| Atualização | **Mini tela com o céu + "O que mudou"**, ao fechar ou reabrir. |
| Desinstalar | **Pergunta separadamente**: app / modelos / dados (banco e configurações). **O vault nunca é tocado.** |

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
- **Segredos nunca saem:** senhas e chaves são filtradas antes de qualquer envio, e documentos pessoais (RG, CPF, dados bancários) são detectados e mascarados. Outros conteúdos podem ir para APIs externas, com transparência por resposta (ver acima).
- **Transparência:** cada resposta que usou uma API de fora tem um **ícone discreto**; clicando, mostra o que saiu e para qual provedor.

## Erros e modo offline
- **Erros de API** (fora do ar, cota esgotada): **faixa discreta de uma linha** ("Groq sem cota, usando Gemini") e troca automática de provedor. Só para quando nada funciona.
- **Sem internet:** cai para o **modelo local** e avisa. Vault e memória continuam funcionando.

## Painel
Em ordem de importância: **cotas das APIs gratuitas** (quanto resta hoje), **uso das assinaturas** (Claude, Codex, Gemini), **saúde do sistema** (RAM, CPU, GPU, disco, serviços) e **atividade da semana** (conversas, ações, aprovações, notas gravadas).

## Voz
- **Iniciar:** palavra **"Orion"** (escuta local) ou **duas palmas**. Os dois **trazem a janela para o foco**.
- **Enquanto fala:** **mini barra flutuante no topo, centralizada**, com a constelação reagindo ao áudio. Formato final a decidir com protótipo.
- **Resposta falada:** **só quando o pedido foi por voz**. Mensagem escrita recebe resposta só escrita.
- **Enquanto fala:** uma **mini barra flutuante no topo, centralizada**, com a constelação reagindo ao áudio. Formato final a decidir com protótipo.

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

## App no Windows
- **Instalador** `.msi`/`.exe` (atalho, iniciar com o Windows, desinstalação limpa).
- **Atualizações:** baixa do GitHub Releases em segundo plano, **avisa e instala ao fechar**.
- **Backup** do banco e das configurações: cópia **criptografada no Google Drive** e **exportar/importar manual** em Configurações.
- **Janela:** lembra tamanho e posição.
- **Desempenho:** **quanto menos RAM, melhor** (meta abaixo de 300 MB parado; índice, voz e grafo só sob demanda) e **menos de 1 s** até poder digitar.
- **Acessibilidade:** teclado em tudo e escala da interface de 90% a 150% (Configurações › Geral e Ctrl +/-). Sem modo alto contraste.
- **Testes:** testes de lógica (estado do app, comandos `/`, Markdown), **snapshots de tela** e **CI no GitHub Actions** com build Windows.
- **Backup** do banco e das configurações: cópia **criptografada no Google Drive** e **exportar/importar manual** em Configurações (para trocar de PC).
- **Desempenho:** **quanto menos RAM, melhor** (meta abaixo de 300 MB parado, índice, voz e grafo só sob demanda); **menos de 1 s** até poder digitar.

## Personalidade e respostas
- **Sem avatar:** respostas só em texto, como no Claude.
- **Tom:** seletor **Conciso / Normal / Detalhado** + campo livre "como o Orion deve responder", em Configurações › Geral.
- **Imagens:** geração via **Gemini/APIs gratuitas** (`/imagem`). A imagem aparece na conversa e é salva em `03 Recursos/Orion/Imagens/`, linkada na nota da conversa.

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

## Ferramentas de código
- **Painel da direita em abas:** Contexto / Arquivos (explorador do projeto) / Diff (mudanças antes de aprovar) / Terminal.
- **Terminal completo** (PowerShell de verdade dentro do app).
- **GitHub:** PRs abertos e CI do projeto, com botões **abrir PR** e **merge** (com aprovação).

## Modelos locais
- **llama.cpp embutido no Orion** (sem processo extra). Tela simples para baixar e remover modelos, escolher o fallback e ver a RAM usada.

## Vários PCs
- **PC principal manda; notebook replica pela Tailscale** (banco SQLite sincronizado quando os dois estão ligados). O vault é comum aos dois pelo git.

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
- **Migração do Orion antigo (Python):** reescrever tudo em Rust (16/10).
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
- ~~**rayon** para o layout de forças~~ → **compute shader wgpu** (16/10).
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
> **Substituídas** pelo plano detalhado em etapas: [[ORION - PLANO DE IMPLEMENTAÇÃO]] (F00 a F27, com checklist). A lista abaixo fica como histórico.

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
| Atalho Win + O | ~~Manter~~ → trocado por **Win + Shift + Espaço** (16/10). |
| Certificado de código | **Não, por enquanto.** Instalador sem assinatura. |
| Aprovação sem resposta | Cancela e registra após **24 horas**. |
| Pastas confiáveis | **Documentos, Área de trabalho e pasta dos projetos.** |
| Resumo diário com PC desligado | **Gerar na nuvem.** Ressalva: o conteúdo do dia vai para a nuvem; passa pela mesma filtragem de segredos e dados pessoais. |
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
| Modelo padrão do Claude (modo Auto) | **Sonnet.** |
| Esforço padrão | **Médio.** |
| Grafo | **Todas as notas**, incluindo Arquivo (04), Relatórios de Mudanças e notas do Orion. (Resposta com opções sobrepostas; confirmar se a intenção era mostrar tudo.) |
| Painel de cotas | Atualiza a cada **5 min**. |
| Automelhoria | **1 tarefa por dia**, no máximo. |
| Atalho da voz ao vivo | **Ctrl + Alt + V**. |
| Provedor no modo Auto sem preferência | **Roteador por complexidade**. |
| Telegram | **Só o seu ID** responde. |
| Telegram | **Resposta completa** aparece no chat. |
| Imagem (/imagem) | **Gemini por padrão**; buscar outro provedor gratuito com API. **Painel de configuração de todas as APIs na primeira abertura** do app. |
| Briefing diário | **7h.** |
| Desfazer | Disponível por **7 dias**. |
| Escala da interface | **100%.** |
| Largura da conversa | **Centralizada.** |
| Rede | **Qualquer site.** Ressalva: sem lista de domínios, o Orion pode consultar qualquer site; a filtragem de segredos continua valendo. |
| Consulta na busca web | **Pergunta com contexto** (pode incluir trechos do vault ou da conversa). Ressalva: trechos do vault ou da conversa passam pela filtragem de segredos; pastas marcadas como bloqueadas ficam de fora. |
| Pastas bloqueadas por padrão | **Nenhuma.** Você marca as suas. |
| Modo autônomo ao instalar | **Desligado.** |
| Rotinas padrão | **Briefing às 7h** e **resumo diário às 23:59**. |
| Prazo do primeiro marco | **Sem data.** |
| Senha de abertura | **Senha forte** (mínimo 12, com letras, números e símbolos). |
| Backup no Drive | **Só banco e configurações**; o vault já está no git. |
| Instrução vinda de web ou e-mail | **Tratar como dado** e pedir aprovação. Nunca executa por instrução externa. |
| Repositório do vault | **Privado.** |
| Banco SQLite | **Criptografado com SQLCipher**, chave derivada da senha do Orion. |
| Tentativas de senha | Bloqueia por **5 minutos** após **5 erros**, e registra. |
| Atualização sem certificado | **Hash SHA-256 publicado no GitHub**, conferido antes de instalar. Ressalva: o hash fica no mesmo release, então não protege contra release comprometido; uma assinatura própria seria o reforço. |
| Logs | **Mascaram** chaves, senhas e dados pessoais, sempre. |
| Plugins e MCP de terceiros | **Leitura permitida**; escrita pede aprovação. |
| Exportação de dados | **Só com senha e confirmação.** |
| Esqueci a senha | **Chave de recuperação**, mostrada uma vez na criação e guardada fora do PC. |
| Telegram (ligar, desligar, aprovar) | **PIN próprio** além do ID, para ações críticas. |
| Tailscale | **Só dispositivos da sua conta** (ACL). |
| Alerta de chaves de API | **14 dias** antes de expirar. |
| Áudio de voz | **Só a transcrição**; o áudio é descartado. |
| Bloqueio por inatividade | **Só Win + L**, sem timer próprio. |
| Comandos destrutivos | **Confirmação extra**: aprovação e digitação do texto do comando (apagar, formatar, force-push, reescrever histórico). Mesmo no modo autônomo. |
| Logs de aprovação e atividade | **Só você**, pelo app, com senha. |
| Sandbox dos comandos de shell | **Não**: rodam com o mesmo usuário. Risco aceito; a proteção fica na aprovação e na confirmação de comandos destrutivos. |
| Escaneamento de segredos antes de gravar | **Bloqueia** o commit no vault. |
| Descarte do PC | **Formatar** o computador. |
| Uso de dados para treinamento | **Verificar por provedor** e avisar se estiver ligado. |
| Pastas sensíveis do sistema (navegador, cookies, área de transferência) | **Só com aprovação caso a caso.** |
| Agentes externos no vault (Codex, Gemini) | **Leitura e escrita**, com registro no log do Orion. |
| Reconhecimento facial com falso positivo | **Não age**; pede confirmação. |
| Token do GitHub | **Acesso a todos os repos.** Ressalva: escopo amplo; reavaliar. |
| Login novo no Telegram ou IP desconhecido | **Avisa e bloqueia** a ação. |
| Timeout de API | **60 s.** |
| Cache de respostas | **24 h.** |
| Memória do modelo local | **Até 30%** da RAM. |
| Índice do vault | **Incremental** a cada commit. |
| Instalador | **Menor possível**; modelos baixados sob demanda. |
| Retentativas | **2**, depois troca de provedor. |
| Métricas de performance | **Locais**, na tela Painel. |
| Banco SQLite | **Sem limite prático.** |
| FPS do grafo | **60** na interação, 20 parado. |
| Módulos pesados (voz, grafo) | **Sob demanda.** |
| Ícone | **Cinturão de Órion.** |
| Bolhas do usuário | **Fundo sutil.** |
| Botões principais | **Rigel pálido.** |
| Cantos | **Médios** (10 a 14 px). |
| Erros de API | **Cartão no chat.** |
| Blocos de código | **Tema neutro.** |
| Gráficos do painel | **Barras e linhas simples.** |
| Céu da tela inicial | **Animado, sutil.** |
| Splash | **Com o cinturão.** |
| Digitação | **Cursor piscando.** |
| Painel de contexto | **Ao lado**, empurrando a conversa. |
| Barra lateral | **272 px.** |
| Fonte das respostas | **Newsreader.** |
| Estado vazio | **Saudação com constelação.** |
| Feedback de cópia | **Toast curto.** |
| Utilidades | **OCR de tela, histórico da área de transferência, calculadora e conversões, lembretes e timers.** |
| Conteúdo | **Tradução, resumo de PDF, leitura em voz alta.** |
| Sistema | **Abrir apps e pastas, organizar arquivos, controlar volume e mídia, controle de janelas.** Spotify pela API; YouTube e Netflix só por link. |
| Integrações | **Rascunhos no Gmail, Agenda do Google, monitor de preços.** |
| Documentos | **Word, Excel e PowerPoint na v1.** |
| Dev | **Status de PR e CI, rodar testes, logs do CI.** |
| Casa | **Wake-on-LAN e Home Assistant.** |
| Windows mínimo | **Windows 11.** |
| Ao abrir | **Conversa nova.** |
| Atalhos | **Personalizáveis** em Configurações. |
| Modo foco | **Não entra.** |
| Controle de apps no PC | **Qualquer app, com aprovação a cada ação** (clique, digitação ou tecla). Mídia pela API de transporte de mídia do Windows; janelas pela automação de interface (UI Automation). Ressalva: aprovar cada ação pode ser cansativo; avaliar aprovação por lote. |
| Apps sugeridos para controle | **O Orion sugere**; cada app precisa do seu OK. |
| Apps elevados (administrador) e jogos | **Controla se você autorizar**, com confirmação. Ressalva: é o caminho de maior risco; rodar elevado amplia o que o Orion pode fazer no sistema. |
| Ordem de controle de apps | **1.** API, MCP ou extensão do app (quando existir); **2.** controle de mídia do sistema; **3.** automação de interface; **4.** teclado e mouse simulados, só como último recurso. Integrar tudo que tiver API, MCP ou extensão, com aprovação conforme a regra de ações. |
| Fatos aprendidos sobre o usuário | **Salvar sozinho**, registrados na aba Conhecimento, com opção de apagar. Ressalva: conteúdo externo (web, e-mail) não pode gravar fatos sem passar pela regra de "conteúdo externo é dado". |
| Extensão de navegador (Chrome ou Edge) | **Na v1**, para controlar apps web. Ressalva: aumenta o escopo da v1. |
| Importar histórico antigo | **Na primeira abertura**: Claude Code, Codex e ChatGPT, gravando resumos no vault. |
| Sugestões proativas | **Poucas e relevantes**: pendências, cotas e falhas. |
| Automações por gatilho (estilo Alexa) | **Gatilhos:** frase de voz, horário, evento do PC (ligar, abrir app, conectar dispositivo), mensagem do Telegram e duas palmas. **Sequência de ações** em ordem. |
| Falha numa automação | **Para e avisa**, mostrando o passo que falhou. |
| Aprovação de automações no PC | **Sem pedir só para ações leves** (abrir app, tocar música). Escrever em arquivo e comandos que alteram algo pedem OK; apagar, formatar e force-push pedem confirmação extra. |
| Criação de automações | **Você**, pelo chat ou pela tela, revisando os passos antes de ativar. |
| Captura de tela ("isso?") | **Detecta e borra** dados sensíveis (CPF, cartão, senhas visíveis) antes do envio. |
| Modo "não escutar" | **Só pelo app**, sem atalho. |
| Avaliação de respostas (👍 e 👎) | **Registro local**, para ajustar o roteador; nada sai do PC. |
| Repositórios alteráveis em tarefas automáticas | **Todos.** Ressalva: escopo amplo para alterações automáticas; alterações saem como PR, mas reavaliar a lista. |
| Auditoria diária do vault | **Diária, com correções simples** (links e frontmatter óbvios, cada mudança registrada). Ressalva: o `AGENTS.md` do vault diz que a auditoria é mecânica e só lista; corrigir exige ajustar essa regra. |
| Teto de gasto por tarefa | **20% da cota semanal**, respeitando a janela de 5 horas. Ressalva: o Antonio observou que o limite de 5 horas impediria esse teto; confirmar o valor. |
| Cofre de chaves de API | **Só o cofre do Windows.** Sem integração com Bitwarden ou 1Password. |
| Exportação de conversa | **Markdown.** Sem PDF na v1. |
| Voz de resposta | **Masculina** (pt-BR, Antonio). |
| Silêncio para parar de ouvir | **1 segundo.** |
| Respostas faladas | **Curtas**, só o essencial; o texto completo fica na tela. |
| Microfone | **Padrão do Windows.** |
| Família do modelo local | **Qwen.** |
| Atualização de modelos locais | **Avisa antes** de baixar. |
| Quantização do modelo local | **4 bits.** |
| Sem internet | Funcionam: **memória e vault, chat com modelo local, rotinas locais e voz.** |
| Telegram com PC ocupado | **Fila**; responde quando terminar. |
| Rotina com PC suspenso | **Acorda o PC** pelo agendador do Windows. |
| Duas rotinas no mesmo horário | **Uma depois da outra.** |
| Alerta de cota | **Em 80% usado.** |
| Nome dos arquivos de nota | **Data - agente - título.** |
| Tags nas notas | **Criadas direto** pelo Orion. |
| Títulos das conversas | **Português.** |
| Projetos arquivados | **Ficam para sempre** na aba Arquivadas. |
| Tom das respostas | **Informal e direto.** |
| Incerteza | **Diz que não tem certeza**, com nível de confiança e fonte quando houver. |
| Respostas longas | **Gera direto**, sem perguntar. |
| Emojis | **Não usa.** |
| Verificação do vault antes de commit | **Checa frontmatter e links**; bloqueia nota malformada. |
| Vulnerabilidades das dependências | **Em cada PR** (bloqueia vulnerabilidade alta). |
| Metadado de imagem gerada por IA | **Não** marca. |
| Troca da chave de recuperação | **Só quando você pedir**; a antiga é invalidada. |
| Limpeza da área de transferência | **30 segundos** após copiar um segredo. |
| Troca do token do Telegram | **Só quando você pedir.** |
| Atualização de dependências | **PR automático** para revisão. |
| Sessões abertas em outros PCs ou apps | **Notifica** e permite encerrar. |
| Lista de domínios por provedor | **Não** limita. |
| Logs de erro | **Com contexto para depuração**, com dados mascarados. Ressalva: conteúdo de conversa vai para o log; depende da máscara funcionar bem. |
| Windows Hello para abrir o Orion | **Sim**, quando disponível. |
| Telemetria do sistema | **Só local.** |
| Transição entre telas | **Fade de 150 ms.** |
| Modal de configurações | **Sobreposto**, com fundo escurecido. |
| Ilustração nas telas vazias | **Cinturão de Órion, pequeno.** |
| Realce de código | **Cores de Rigel e Betelgeuse.** |
| Destaque (botões, foco, seleção) | **Azul de Rigel.** |
| Bordas dos cartões | **Finas e sutis.** |
| Texto base da interface | **14 px.** |
| Status na barra | **Ponto colorido** (âmbar, girando ou verde). |
| Espaçamento entre mensagens | **Médio.** |
| Botões de ação das respostas | **Aparecem ao passar o mouse.** |
| Animação do painel e das abas | **Suave, 200 ms.** |
| Indicador enquanto pensa | **Texto "Pensando" com ponto pulsante.** |
| Lista de conversas | **Mais recente primeiro**, agrupada por Hoje, Ontem e Mais antigos. |
| Miniatura de anexos | **Pequena**, ao lado do campo. |
| Atalho da barra lateral | **Ctrl + B.** |
| Cabeçalho de projeto nas conversas | **Nome do projeto no título.** |
| Tela de boas-vindas | **Tudo em uma tela só.** |
| Aviso de novidades | **Discreto na primeira abertura.** |
| Cores dos nós do grafo | **Paleta da identidade.** |
| Resumo do dia ("Você fez") | **Conversas e tarefas, notas gravadas e aprovações.** |
| Lembretes por voz | **Barra lateral e Telegram.** |
| Transcrição de áudio enviado | **Só o texto**; o áudio é descartado. |
| Gerenciador de tarefas | **Listas em notas do vault**, uma por projeto. |
| Tradução de tela | **Ctrl + Alt + T.** |
| Memória de longo prazo | **Busca no histórico** automaticamente. |
| Compartilhar nota ou conversa | **Não** na v1. |
| OCR | **Via API.** Ressalva: envia a imagem ao provedor; confirmar contra a regra de dados pessoais, que mascara CPF e cartão antes do envio. |
| Apresentações | **PowerPoint**, pelas skills de documento. |
| Gravação de reunião | **Não** na v1. |
| Clima na barra lateral | **Sim**, por cidade. |
| Automação de interface (apps sem API) | **Na v1**, com aprovação por ação. |
| Calendário interno | **Sim**, eventos guardados no vault. |
| Backup manual do vault | **Zip.** |
| Painel de projetos | **Na v1.** |
| Tempo para abrir a janela pronta | **Menos de 1 segundo.** |
| Bateria no notebook | **Sem mudança** de desempenho. |
| Indexação inicial do vault | **Em segundo plano**, sem travar o app. |
| Grafo: limite antes de otimizar | **500 notas.** |
| Conversas na memória de trabalho | **Dez.** |
| Conversas muito pesadas | **Descarrega imagens antigas** e recarrega ao abrir. |
| Cache de imagens | **1 GB, compactado** (WebP, 1600 px, sem duplicatas). Apaga as mais antigas ao passar do limite. |
| Animações com pouca RAM | **Reduz automaticamente.** |
| Inicialização ao ligar o PC | **Só o núcleo, na bandeja**; interface e voz carregam sob demanda. |
| Atualização do app | **Uma vez por dia**, em segundo plano. |
| Destaque de notas lidas no grafo | **Anel branco pulsante.** |
| Fontes do editor de notas | **Mono para código, serifa para texto.** |
| Mensagem de commit no vault | **Resumo da conversa.** |
| Notas do Arquivo (04) | **Aparecem esmaecidas** no grafo e na busca. |
| Erro de sintaxe em código gerado | **Corrige e mostra o que mudou.** Ressalva: alterar arquivo de projeto exige aprovação pela tabela de permissões; a correção automática precisa passar por ela. |
| Idioma do código gerado | **Inglês nos nomes, português nos comentários.** |
| Custo estimado por conversa | **Discreto no painel.** |
| Fechar com tarefa rodando | **Pergunta** e pode seguir na bandeja. |
| Pausar o Orion inteiro | **Ctrl + Alt + P**; pausa voz, rotinas e automações. |
| Relatório de sessão | **Gerado ao fim do dia** no vault. |
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
