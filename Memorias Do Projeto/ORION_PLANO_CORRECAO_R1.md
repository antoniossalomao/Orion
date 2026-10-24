# ORION - PLANO DE CORREÇÃO R1

> **Origem:** primeiro teste do Orion num Windows real, em 23/10/2026. **Estado:** correções em implementação e validação em 23/10/2026, após o pedido do Antônio nesta sessão para aplicar o planejamento. A espera anterior pelos prints não bloqueia este trabalho; aceite de hardware continua separado.
> **Onde acompanhar:** os 15 itens (R1.01 a R1.15) são cards no board "Orion — Board" do Notion, com prioridades ajustadas no board conforme o diagnóstico, e estão listados como caixas de marcar na seção "Rodada R1" de [ORION_PLANO_IMPLEMENTACAO.md](ORION_PLANO_IMPLEMENTACAO.md). Este arquivo guarda o detalhe: evidência, causa, o que fazer e como aceitar.

## Execução de 23/10/2026

O estado vigente fica em [ORION.md](ORION.md). Nenhum aceite Windows é inferido de compilação Linux ou do código do workflow. Testes novos e CI automático continuam pausados.

| Item | Alteração nesta entrega | Limite / aceite restante |
|---|---|---|
| R1.01 | Provedores sem segredo em `meta.providers:configuration`, descoberta das chaves salvas em ordem do catálogo e defaults gratuitos; Gemini automático agora usa 3.8 Flash, pois a série 2.5 ficou restrita a usuários anteriores; configuração antiga exata migra sem alterar ambiente explícito; `ORION_PROVEDORES` explícito, inclusive no `.env` e `[]`, tem precedência. Gateway recarrega entre turnos e mantém orçamento, auditoria, política e agente. Erro do chat oferece Adicionar chave. | Credential Manager e primeira chamada Gemini reais no Windows |
| R1.02 / R1.15 | GPU real usa wgpu; adaptador de CPU usa tiny-skia sem MSAA. Tick só durante atividade, até 24 fps, pausado sem foco/oculto. Bandeja consulta em segundo plano e só acorda a UI quando há evento. Grafo estático, cache de geometria, layout fora da UI. Markdown, Mermaid, LaTeX e widgets de resposta reutilizados. Tempos de `view`/`update` em `orion.performance`, nível debug. | Medir CPU/GPU e latência no notebook; sem prometer os limites antes da medição |
| R1.03 / R1.04 | Barra de controles compartilhada nas telas iniciais. Login usa o perfil, continua editável, confere o usuário digitado e preserva contas antigas sem perfil. | Controles e login no pacote Windows |
| R1.05 | Padrão warn, arquivo mascarado com rotação em `logs/` quando o banco é em claro; logs cifrados continuam no destino existente. Botão abre a pasta real da instalação. | Abrir pasta e conferir Credential Manager no Windows |
| R1.06 | Saída fecha a janela antes de encerrar o runtime, permitindo descarte dos recursos. | Ausência de avisos DX12/Vulkan só pode ser aceita com renderizador real |
| R1.07 | Splash com logo, derivação/verificação da senha, abertura do banco, preparação dos modelos e ferramentas. Núcleo sem bloqueio passa a abrir depois da janela. | Capturas e fluxo visual; perfil e dados continuam protegidos antes da senha |
| R1.08 | Céu estático/cacheado, bitmap na resolução do painel com GPU e reduzido em software; entrada vetorial finita de estrelas/linhas no login com GPU, transições limitadas e sem animação contínua no repouso. | Acabamento da animação e fonte continuam dependendo do aceite visual |
| R1.09 | Conferido `decorations: false` e `undecorated_shadow: false` (padrão do iced). | Não reproduzido em Windows; não há correção de causa comprovada da faixa branca |
| R1.10 | Nova conversa já era local até o envio; adicionado bloqueio de envios duplicados enquanto a criação está pendente. Listar/abrir a tela não cria sessão. | Não apaga conversas vazias antigas nem altera histórico |
| R1.11 | LEIA-ME do pacote esclarece portátil × primeiro uso e mantém Setup/Vulkan/SQLCipher para o teste seguinte. | Nenhum release publicado; workflow continua somente manual |
| R1.12 | Sem mudança de estado por inferência. | Antônio: Mica, bandeja, atalhos, Hello, autostart, GPU |
| R1.13 | Antônio decidiu incluir localização completa nesta sessão. Catálogos Fluent pt-BR/en, rótulos da interface/bandeja, estados e idioma persistido no perfil autenticado; conteúdo do usuário preservado. | Revisão visual nos dois idiomas; 855 traduções e 69 interpolações Unicode validadas |
| R1.14 | Anexos txt/md/pdf (6 MB, até 8), com espera da extração antes da confirmação e extraídos pelo fluxo existente e resolvidos por ID no escopo da conversa; texto externo limitado a 32 mil caracteres por turno. Alturas do chat vêm do layout. Documentos longos aparecem como artefatos; guardar/atualizar e navegar versões usa o SQLite, com controle de concorrência. | Imagem/áudio no chat não fazem parte do upload de documentos atual; interface diz os formatos suportados |

**Reteste Gemini:** logs enviados pelo Antônio às ~20h BRT mostram HTTP 404 no pacote 30359bd. Modelo automático atualizado de 2.5 Flash para 3.8 Flash, com migração estreita e metadados opacos de ferramentas preservados. Diagnóstico agora exibe HTTP/modelo/último erro; resposta real ainda depende de reteste. Fontes: [modelos Gemini](https://ai.google.dev/gemini-api/docs/models) e [cota gratuita](https://ai.google.dev/gemini-api/docs/pricing).

**Validação Linux (23/10/2026, Rust 1.97.0):** suíte existente exigida pelo AGENTS.md: **550 testes, 0 falhas**. Os crates alterados de UI/servidor passaram novamente (47 testes); após o ajuste da espera pela extração de anexos, os 33 testes existentes da UI passaram novamente. Os 4 testes nativos `appmode::tests` também passaram com `scripts/mock_llm.py` em loopback, exercitando chat, aprovação de escrita e proteção do vault. Nenhum teste novo foi criado. `orion-platform` passou em `cargo check --locked -p orion-platform --target x86_64-pc-windows-msvc`; isso não é execução Windows. `git diff --check` passou.

**Medição Linux/Xvfb (1440×900, tiny-skia, app de demonstração):** amostras de 10 s por tela, CPU como percentual de **um núcleo**: login 0,1%; chat 0,0%; Memória/grafo 0,1%; minimizado 0,3%. O grafo mostrou 802 de 895 notas e 3.394 links. Capturas pt-BR/en foram inspecionadas; conteúdo das conversas e nomes do grafo permaneceram originais. São medidas de repouso nesta máquina, não prova de latência de clique, uso de GPU ou aceite do notebook Windows.

**Ambiente:** a primeira suíte encontrou falta de ALSA/udev de desenvolvimento e restrição de sockets. Pacotes de validação foram extraídos fora do repositório; a suíte foi executada com acesso aos sockets locais e passou. Sem mudanças no sistema, em segredos ou nos gatilhos de CI. A compilação e medição locais não usam API paga nem chave real.

---

## 1. O teste

| | |
|---|---|
| **Pacote** | `Orion-Windows-CPU` do workflow `windows-app-cpu.yml` (release, sem banco cifrado, sem `orion-llama`, sem modelo), execução 39984324040 |
| **Máquina** | Windows 10 (driver 10.0.19041), **sem placa de vídeo**, escala 100% |
| **Único adaptador gráfico** | "Microsoft Basic Render Driver" (WARP), `device_type: Cpu`, DX12. OpenGL falhou ("GL context is 3.1, when 3.3+ is needed") |
| **Pasta de dados** | `C:\Users\<usuário>\AppData\Roaming\orion` (`orion.db`, `auth.db`, `logs\`); chaves de API no Gerenciador de Credenciais, serviço "orion" |
| **Evidências** | relatório do Antônio (2 revisões), 5 prints, log do console (com `ICED_BACKEND=wgpu`), `orion.db` enviado |

**Relato do Antônio, resumido:** o `orion.exe` abre direto, sem instalar; janela sem botões nas telas iniciais; constelação pixelada; transições ausentes; login "não aparece" (depois funcionou); app borrado e lento ("inutilizável"), com animações engasgando; o chat nunca responde mesmo com chave do Gemini válida ("Nenhum modelo configurado"); o tiny-skia (desenho por software) também trava; fonte desalinhada ou estranha; faltou a tela de carregamento que ele pediu.

---

## 2. Sintoma, evidência e causa

| Sintoma | Evidência | Causa | Item |
|---|---|---|---|
| Chat não responde, "Nenhum modelo configurado", toast "falta o gateway" | `orion.db`: 0 mensagens, 0 chamadas externas, 0 auditoria; 1 conversa vazia | `settings.provedores` só vem da variável `ORION_PROVEDORES` (JSON); nada no onboarding nem em Configurações a preenche. Sem provedor listado não há endpoint, o gateway não existe e o agente nunca é montado (`estado.rs` monta o agente uma vez, ao abrir). A chave fica guardada, mas ninguém a usa | R1.01 |
| App "inutilizável de tanto travar", animações engasgando | Log: `Selected: Microsoft Basic Render Driver`, `antialiasing: MSAAx4`. No Linux o login parado gasta **100% de um núcleo** (thread principal), sem limite de quadros | O desenho cai num emulador de GPU por CPU com MSAA 4x; a constelação força redesenho contínuo; `views/chat.rs` refaz `md::parse` de cada mensagem a cada quadro; a constelação é um bitmap gerado por CPU | R1.02 |
| Janela sem minimizar, fechar nem arrastar no onboarding e no login | Prints do Linux (onboarding e login sem controles) e log (`decorations: false`) | Janela sem moldura; só o app principal desenha os próprios botões | R1.03 |
| Usuário do login aparece como `antoniosalomao` | Print do login | Padrão fixo em `app.rs`, `views/onboarding.rs` e `orion-core/src/config.rs` | R1.04 |
| Logs enormes no console | Log com dezenas de shaders | Nível padrão `INFO` (`orion-core/src/config.rs`, `orion-server/src/log.rs`) | R1.05 |
| Dezenas de avisos "leak detected" ao fechar | Final do log | Buffers de vídeo não liberados na saída (`gpu_allocator`) | R1.06 |
| Faixa branca no topo da janela | Print da Memória | Não investigado (janela sem moldura, desenho em modo CPU ou Windows 10) | R1.09 |
| "3 conversas sem título" na lista, 1 no banco | Print da lista, `sessions` com 1 linha | A conversa é criada antes da primeira mensagem | R1.10 |
| "Não é instalador nem onboarding" | Relato | O `orion.exe` é portátil e o instalador (`Orion Setup.exe`) é outro executável, fora do pacote CPU | R1.11 |

**O que o teste não prova:** Mica só existe no Windows 11 (esta máquina é Windows 10), então bandeja, atalhos globais, Windows Hello, autostart e Mica seguem sem prova (R1.12).

## 2.1 Segundo teste: notebook de casa (23/10/2026)

| | |
|---|---|
| **Máquina** | Notebook com **GPU integrada**, Windows 11 |
| **Resultado** | Antes (Windows 10 sem GPU): 100% de CPU. Agora: pico de ~30% de CPU, RAM normal, **mas tudo lento**. O resto do app roda de boa |
| **Chat** | Trava **só de entrar na tela**, até o cursor engasga; também ao digitar, ao rolar e durante resposta em stream |
| **Memória** | Mesmo comportamento do chat ao entrar na tela |
| **Grafo** | Trava ao abrir, ao arrastar, ao dar zoom e **parado**, sem mexer |
| **Não medido ainda** | Adaptador/backend usado (wgpu ou tiny-skia), tempo de quadro, volume de nós e de mensagens |

**O que isso muda no plano:**

- A lentidão **não é só falta de GPU**: com GPU integrada e RAM sobrando, o app continua lento. Isso aponta para trabalho por quadro no próprio app (desenho em canvas e redesenho contínuo), não para o driver.
- Trava **parado** no grafo e **ao entrar** em Chat e Memória sugere redesenho contínuo ou trabalho caro repetido a cada quadro (parse de Markdown, layout, constelação de fundo). CPU em 30% com cursor travando é típico de **uma thread só saturada**, enquanto o total do processador parece folgado.
- A R1.02 deixa de ser "só sem GPU": passa a valer para **qualquer máquina**. A constelação parada e o cache de parse deixam de depender de detectar o adaptador.
- **Novo item R1.15** (abaixo) para chat, Memória e grafo.

**Diagnóstico pedido ao Antônio (antes de mexer em código):**

1. (feito, ver 2.2) Rodar `$env:ICED_BACKEND="tiny-skia"; .\orion.exe` e depois `$env:ICED_BACKEND="wgpu"; .\orion.exe`, e dizer qual fica menos lento.
2. Com `$env:RUST_LOG="info"`, mandar a linha `Selected:` (adaptador) e `antialiasing`.
3. Dizer se parar de mexer o mouse no Chat reduz a lentidão, e se com a janela minimizada a CPU cai.
4. Gerenciador de Tarefas, aba Detalhes: CPU do `orion.exe` e, na aba Desempenho, uso da GPU.

## 2.2 Diagnóstico no notebook (23/10/2026, tela Memória com o grafo)

| | tiny-skia (software) | wgpu (GPU) |
|---|---|---|
| CPU | 12% | 22% |
| GPU (Intel UHD) | 0% | 33% |
| Sensação do Antônio | pior | **muito melhor** |

- **wgpu é o caminho certo com GPU real.** O tiny-skia só deve entrar quando o adaptador for de CPU (WARP). Confirma o plano da R1.02, item 1.
- **Mesmo no wgpu, 22% de CPU e 33% de GPU numa tela parada** (grafo de 475 notas e 3.632 links): há redesenho contínuo. Reforça a R1.15 (cache da geometria do grafo, parar de redesenhar sem movimento).
- **RAM do notebook: 7,4 de 7,7 GB em uso (96%)**, 324 MB livres, com Brave, Notion, VS Code, Spotify e outros abertos. Em memória cheia o Windows usa o disco como RAM, e isso trava tudo. Falta medir quanto é do `orion.exe` (Gerenciador de Tarefas, aba Detalhes) e repetir com os outros apps fechados.
- **Minimizado ainda gasta ~18% de GPU** (relato do Antônio). Causa no código: `app.rs` mantém um `time::every` de 60 ms (16 ms quando ocupado) que dispara `Msg::Tick` sempre, e nada pausa com a janela minimizada ou sem foco. Corrigir na R1.15: ouvir eventos de janela (minimizar, foco) e parar o tick e o redesenho enquanto a janela não está visível; o tick de 250 ms da bandeja (`Msg::Bandeja`) pode continuar.
- **Barra de título duplicada e faixa branca no topo:** o Windows 11 desenha a barra nativa branca ("Orion" com minimizar, maximizar e fechar) por cima, e o app desenha os seus botões logo abaixo. Entra na R1.03 e na R1.09.

---

## 3. Decisões do Antônio (23/10/2026)

| Tema | Decisão |
|---|---|
| Provedores (R1.01) | **Automática:** cada chave salva liga o provedor dela, numa ordem fixa do catálogo; reordenar depois em Configurações; vale na hora |
| Erro sem modelo (R1.01) | Texto + botão **Adicionar chave**, que abre Configurações › Modelos |
| Animação sem GPU (R1.02) | **Constelação parada** nesse caso (além do desenho por software e sem MSAA) |
| Janela (R1.03) | **Os mesmos botões do app** (minimizar, maximizar, fechar e faixa de arrastar) no onboarding e no login |
| Login (R1.04) | Campo **preenchido com o usuário do onboarding, editável** |
| Carregamento (R1.07) | **Splash ao abrir**: logo e mensagem de progresso, antes do login e depois de entrar |
| Próximo pacote para testar em casa | **Só o `orion.exe` portátil**, build release manual solicitado ao terminar; GPU automática na interface. Instalador, Vulkan para IA e banco cifrado ficam para depois |
| Localização (R1.13) | Incluir tradução completa pt-BR/en agora (resposta explícita nesta sessão) |
| Início | Espera histórica, superada pelo pedido explícito de implementar em 23/10/2026 |

---

## 4. Itens

Tamanho: **P** = até meio dia, **M** = 1 dia, **G** = mais de 1 dia. Executor: Claude Code, salvo onde indicado. **Arquivos prováveis** são onde o código mora hoje, para quem for implementar; confira antes de editar.

### Onda 1: destravar o próximo teste

#### R1.01 · P0 · M · Chat sem modelo mesmo com chave
- **Problema:** a chave do Gemini colada no onboarding nunca é usada; o chat responde "Nenhum modelo configurado".
- **O que fazer:**
  1. Ao salvar uma chave (onboarding e Configurações › Modelos), registrar o provedor dela na configuração persistente, com os modelos padrão do catálogo, em ordem fixa do catálogo.
  2. Montar ou remontar o agente (gateway) ao salvar, sem reiniciar o app.
  3. Quando não houver modelo, o erro do chat traz o botão **Adicionar chave** (Configurações › Modelos).
  4. Manter `ORION_PROVEDORES` funcionando: quando existir, ele vale e a ordem é a dele.
- **Arquivos prováveis:** `orion-server/src/onboarding.rs` (`SECRET_NAMES`), `orion-server/src/rotas/geral.rs` (`chave_guardar`, `chaves_estado`), `orion-server/src/estado.rs` (`montar`, `endpoints_dos_provedores`), `orion-ai/src/provedores.rs` (catálogo, `chave_env`), `orion-core/src/config.rs` (`provedores`), `orion-app/src/views/chat.rs` e `net.rs` (mensagem 503).
- **Aceite:** colar a chave do Gemini no onboarding e receber resposta à primeira mensagem, sem reabrir o app. Em Configurações, salvar uma chave nova e usar o provedor na hora.
- **Cuidado:** a chave nunca aparece em log nem em tela; a lista de provedores não pode virar um segundo lugar com segredo.

#### R1.02 · P0 · G · Desempenho sem GPU
- **Problema:** o login parado gasta um núcleo inteiro; o app trava em PC sem GPU.
- **O que fazer:**
  1. **Detectar adaptador de CPU** (`device_type: Cpu`, "Microsoft Basic Render Driver") e escolher o desenho por software puro (tiny-skia) nesse caso, sem MSAA. Com GPU real o comportamento atual segue.
  2. **Constelação parada** quando não há GPU (decisão do Antônio). Com GPU, anima no máximo a ~24 quadros por segundo e **para** quando nada se move, com a janela sem foco ou minimizada.
  3. **Não refazer `md::parse` a cada quadro:** guardar os blocos por mensagem e só refazer quando o texto muda (inclui o diagrama Mermaid, a fórmula LaTeX e o aviso de imagem, hoje recalculados por quadro).
  4. Gerar o bitmap da constelação em resolução menor e só quando o tamanho da janela muda.
  5. Limite de quadros (sem redesenho sem motivo): redesenhar só quando há evento ou animação ativa.
- **Arquivos prováveis:** `orion-app/src/run.rs` (`.antialiasing(true)`, escolha do renderizador), `orion-app/src/app.rs` (assinatura de tempo), `orion-app/src/onboarding.rs` e `orion-sky` (`render_image_handle`), `orion-app/src/views/chat.rs` (`md::parse`), `orion-app/src/md.rs`.
- **Aceite:** CPU em repouso abaixo de 5% no login e no chat (medir com `top` no Linux e no Gerenciador de Tarefas no Windows); clique responde em menos de 100 ms em máquina sem GPU; em máquina com GPU, nada piora.
- **Como medir antes e depois:** o harness do Linux (Xvfb, LLM falso) já mostra o uso de CPU em repouso (hoje ~100% de um núcleo no login). Registrar os números no card.

#### R1.03 · P0 · P · Janela sem botões no onboarding e no login
- **O que fazer:** desenhar minimizar, maximizar, fechar e a faixa de arrastar nessas telas, iguais às do app principal.
- **Arquivos prováveis:** `orion-app/src/views/onboarding.rs`, `views/login.rs`, o componente de barra de título do app principal (`views/` e `ui.rs`).
- **Aceite:** dá para mover, minimizar e fechar a janela em todas as telas.

#### R1.04 · P1 · P · Usuário padrão do login
- **O que fazer:** o login vem preenchido com o usuário do perfil criado no onboarding, editável; remover o padrão pessoal fixo (`antoniosalomao`) do código e dos exemplos de tela.
- **Arquivos prováveis:** `orion-app/src/app.rs` (`user`), `orion-app/src/views/onboarding.rs` (exemplo do campo), `orion-core/src/config.rs` (`auth_user`).
- **Aceite:** depois do onboarding, o login mostra o usuário criado.

#### R1.05 · P1 · P · Logs úteis
- **O que fazer:** nível padrão `warn` (hoje `INFO` despeja o código dos shaders); gravar em `%APPDATA%\orion\logs`; botão **Abrir pasta de logs** em Configurações.
- **Arquivos prováveis:** `orion-core/src/config.rs` (`log_level`), `orion-server/src/log.rs`, `orion-app/src/views/settings.rs`.
- **Aceite:** abrir o app não imprime shader; o botão abre a pasta de logs, que tem arquivo.

#### R1.06 · P1 · P · Encerramento limpo do renderizador
- **O que fazer:** liberar os recursos de vídeo antes de sair, para eliminar os avisos "leak detected".
- **Aceite:** fechar o app sem aviso de vazamento no log.

#### R1.15 · P0 · G · Chat, Memória e grafo lentos em qualquer máquina
- **Problema:** em notebook com GPU integrada, entrar em Chat ou Memória já trava o cursor; o grafo trava até parado.
- **O que fazer:**
  1. **Medir primeiro:** tempo por quadro (`view`/`update`/desenho) por tela, com log em `debug`; achar o que domina antes de otimizar.
  2. **Grafo:** parar de redesenhar quando nada se move; cachear a geometria (`canvas::Cache`) e invalidar só ao mudar nós, zoom ou posição; simular o layout em passos limitados, não por quadro; limitar rótulos ao zoom suficiente.
  3. **Chat e Memória:** cache de `md::parse`, Mermaid, LaTeX e imagens por mensagem (junto da R1.02, item 3); não refazer a lista inteira ao digitar; stream atualizando só a última mensagem.
  4. **Constelação de fundo:** só anima quando a tela precisa; parada por padrão fora do login.
  5. **Limite de quadros:** redesenhar só com evento ou animação ativa.
  6. **Janela minimizada ou sem foco:** pausar `Msg::Tick` e o redesenho (hoje o tick de 60 ms roda sempre, e minimizado ainda gasta ~18% de GPU).
- **Arquivos prováveis:** `orion-app/src/views/chat.rs`, `views/memory*.rs`, `views/graph*.rs`, `md.rs`, `app.rs` (subscrições de tempo), `run.rs`.
- **Aceite:** no notebook de casa, entrar em Chat, Memória e grafo sem o cursor travar; CPU em repouso abaixo de ~3%; GPU perto de 0% com a janela minimizada; stream de resposta fluido; sem regressão visual.
- **Depende de:** diagnóstico do Antônio (seção 2.1); no Linux dá para medir CPU em repouso e tempo de quadro com o harness.

### Onda 2: acabamento e fluxo

#### R1.07 · P1 · M · Splash ao abrir
- **O que fazer:** tela com logo e mensagem de progresso ("Abrindo o banco...", "Carregando o vault...") entre o clique no `.exe` e o login, e também depois de entrar, enquanto o app prepara tudo.
- **Aceite:** nunca fica janela em branco ou parada; o progresso muda conforme as etapas.

#### R1.08 · P1 · M · Qualidade visual
- **Atualização (teste em casa, wgpu):** a qualidade no notebook com GPU foi **muito superior**; o borrado e a fonte estranha vieram do desenho por CPU no PC sem GPU. Fonte e nitidez deixam de ser urgentes e só voltam se o problema aparecer com GPU. Fica o pedido do Antônio: **animação da constelação ainda melhor** (com GPU, mais fluida e bonita, sem custo em repouso, ver R1.15).
- **O que fazer:** constelação nítida na resolução da janela; revisar fonte e alinhamento. **Depende dos prints** do Antônio.

#### R1.09 · P2 · P · Faixa branca no topo
- **O que fazer:** investigar a origem (desenho em modo CPU, janela sem moldura ou Windows 10) e corrigir.

#### R1.10 · P2 · P · Conversas vazias
- **O que fazer:** só criar a conversa na primeira mensagem enviada.
- **Aceite:** abrir "Nova conversa" sem escrever não deixa item na lista nem no banco.

#### R1.11 · P1 · M · Instalador × portátil
- **O que fazer:** deixar claro o que é onboarding (primeiro uso) e o que é instalação; incluir o `Orion Setup.exe` num teste seguinte (decisão: não entra no próximo pacote).

### Onda 3: sem prova e pendências herdadas

#### R1.12 · P1 · Antônio · Provar em Windows real com GPU
- Mica (só Windows 11), bandeja, atalhos globais, Windows Hello e autostart. Teste em casa, com o pacote novo.

#### R1.13 · Codex · Localização pt-BR/en
- **Decisão nesta sessão:** incluir localização completa agora. O app passou a usar catálogos Fluent pt-BR/en para os textos do produto; Configurações muda o idioma e salva a preferência no perfil. A revisão visual nos dois idiomas continua no aceite. O estado anterior sem Fluent fica registrado no diagnóstico histórico.

#### R1.14 · P2 · G · Herdados do chat
- Anexos do usuário chegarem ao chat; medição real da altura das mensagens (hoje estimada pela média); versões de artefato e documento longo como artefato.

---

## 5. Ordem

1. **R1.01, R1.03 e R1.04** primeiro: destravam o uso.
2. **R1.02 e R1.15** em seguida: são o que mais pesa (o teste em casa mostrou lentidão também com GPU).
3. **R1.05 e R1.06** junto da R1.02.
4. **Onda 2** depois dos prints e das respostas do Antônio.
5. **Onda 3:** R1.12 acontece no teste em casa; R1.13 foi autorizado e R1.14 entrou nesta entrega.

Cada entrega vai num commit, com o card do Notion e este plano atualizados, como nas rodadas anteriores.

## 6. Como validar

- **Linux (harness):** Xvfb, openbox, LLM falso (`scripts/mock_llm.py`), capturas de tela e medição de CPU em repouso. Cobre R1.01 (com um provedor falso), R1.03, R1.04, R1.05, R1.07, R1.10 e a parte de CPU da R1.02.
- **Testes existentes:** rodar `cargo test --locked -p orion-app` e os crates tocados (`orion-vault`, `orion-server`, `orion-core`). Testes novos seguem pausados (ADR 0007).
- **Windows real (Antônio):** pacote `Orion-Windows-CPU` do workflow manual `windows-app-cpu.yml`; em casa, com GPU, confirmar R1.12 e que nada piorou.
- **Não dá para provar no Linux:** WARP/DX12, Mica, Credential Manager, bandeja, atalhos globais, Windows Hello e autostart.

## 7. Riscos e perguntas em aberto

| Risco ou pergunta | Estado |
|---|---|
| Remontar o agente ao salvar a chave pode afetar conversas em andamento e tarefas de fundo | Tratar na R1.01: trocar o agente só entre mensagens |
| Lista de provedores persistente vira mais um lugar de configuração | Definir onde mora (perfil ou configuração) antes de implementar |
| Desligar a animação sem GPU muda a identidade visual nesse caso | Decisão do Antônio já registrada; revisar depois do teste com GPU |
| Splash precisa de eventos reais de progresso do núcleo | Verificar o que o núcleo já informa ao abrir |
| Fonte "estranha" sem print | Aguarda os prints do Antônio |
| O que ele pediu de "tela de carregamento" | Resolvido: splash ao abrir (R1.07) |
| Localização pt-BR/en | Decisão do Antônio (R1.13) |

Reteste R1.01 em 23/10, 21h36 BRT: `6628248` falhou com `ReadTimeout` em “Oi”. Ajustados prazos por fase/inatividade e esforço baixo no Gemini rápido. A primeira resposta real no Windows segue pendente; não marcar R1.01 como concluído somente por compilar.

Atualização R1.01 (24/10/2026): comparação direta do Antônio com `gemini-3.5-flash-lite` retornou HTTP 200 nas três rotas: nativa 1.661 ms, compatível sem streaming 779 ms e com streaming 822 ms (primeiro texto 781 ms). Padrão automático ajustado para esse modelo da cota gratuita; migração só dos antigos defaults exatos 2.5/3.8. Ambiente, campos personalizados e chaves preservados. Primeiro turno real pelo novo aplicativo continua pendente.

Polimento R1.08 (24/10/2026): corrigido o alinhamento do rodapé do perfil enviado pelo Antônio; nome/computador em duas linhas, avatar com iniciais reais, nome longo contido e dica completa, seta do menu e botão de configurações maior. Suíte existente: 550 testes aprovados. Prévia Linux/Xvfb inspecionada em pt-BR 100%, en 130% e gaveta estreita. A pausa de builds deste ajuste foi substituída pelo pedido posterior de app + instalador sem modelos; o aceite no Windows continua pendente.


Conciliação FE-VIS (24/10/2026): visual enviado diretamente à main em [7ae8efd](https://github.com/antoniossalomao/Orion/commit/7ae8efd43bafa691711447e504ed80e79a9105de), integrado ao [PR #55](https://github.com/antoniossalomao/Orion/pull/55), ainda aberto. R1.03 ganha controles/busca na topbar estreita; R1.08 recebe polimento do grafo 2D, rótulos e casca; R1.02/R1.15 ganham pausa de ticks em segundo plano e menos invalidações de hover vazio. Controles iniciais, animação do login e cache de geometria são reaproveitados. Suíte conjunta em `f0082f1`: 550 testes existentes aprovados no Linux, 0 falhas. R1.09 continua sem reprodução/correção de causa comprovada; R1.08/R1.15/R1.12 continuam sem aceite/medição Windows. O pacote combinado manual usa release, SQLCipher, app + instalador e nenhum modelo/worker local; não publica release/tag.
