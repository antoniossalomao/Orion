# Orion — Melhorias e Roadmap Pós-v1 (v1.1 a v2.0+)

> **Documento Canônico de Melhorias Pós-v1.** Reúne, consolida e detalha o planejamento arquitetural, as especificações técnicas, os checklists e os passos detalhados de implementação para todas as funcionalidades pós-lançamento da v1 do Orion (Marcos M2 a M9 / Fases F11 a F27).
>
> **Origens consolidadas:** `ORION_PLANO_IMPLEMENTACAO.md` (F11–F27), `ORION_PLANEJAMENTO_ARQUITETURA.md` (seções de efeitos visuais, grafo 3D e integrações avançadas) e `ORION_REFERENCIAS_PESQUISA.md` (catálogo de referências técnicas, crates e modelos do Hugging Face).
> **Documentos estruturais complementares:** [ORION.md](ORION.md) (estado canônico), [ARQUITETURA.md](ARQUITETURA.md) (arquitetura e contratos), [REGRAS.md](REGRAS.md) (políticas imutáveis de segurança), [ORION_LEGADO.md](ORION_LEGADO.md) (histórico de referência) e [ORION_MELHORIAS_PRE_V1.md](ORION_MELHORIAS_PRE_V1.md) (fundações e lançamento v1).
>
> **Convenção de Status:**
> - `[x]` **Concluído / Prototipado com Sucesso:** funcionalidade ou backend adiantado implementado no código.
> - `[~]` **Parcial / Backend Entregue:** componentes estruturais ou de banco presentes, mas dependentes da interface nativa, de hardware especializado ou de serviços externos reais.
> - `[ ]` **Pendente no Roadmap:** especificado, com arquitetura mapeada, aguardando início após a conclusão e estabilização da v1.

---

## 1. Visão Geral dos Marcos Pós-v1 (M2 a M9)

A evolução pós-v1 transforma o Orion de um assistente de desktop focado em produtividade local para um centro de operações pessoal e proativo, unindo computador, ambiente físico, canais remotos e automelhoria contínua.

```
M1 (v1 Base) ──► M2 (Trabalho & Código: F11-F13)
             ──► M3 (Canais Remotos: F14 Telegram & WoL)
             ──► M4 (Voz Nativa & Áudio: F15)
             ──► M5 (Visualização Espacial: F16 Grafo 3D & F17 Efeitos wgpu)
             ──► M6 (Percepção Física: F18 Câmera & Visão Contínua)
             ──► M7 (Ecossistema: F19 MCP Avançado, WASM & Extensões)
             ──► M8 (Integrações de Vida: F20-F25 Memória de Tela, Web, Casa, Finanças, Estudo)
             ──► M9 (Fechamento: F26 Automelhoria & F27 Release 2.0 / Portabilidade macOS)
```

| Marco | Fases | Foco Principal | Saída Esperada |
|---|---|---|---|
| **M2** | F11 a F13 | Trabalho, Ferramentas de Código e Controle | Terminal ConPTY nativo, visualizador de diff em bloco, agendador em linguagem natural e orçamento integrado de CLIs oficiais. |
| **M3** | F14 | Canal Remoto Telegram e Acesso Externo | Controle remoto seguro do PC pelo Telegram, aprovações fora de banda no celular e acionamento por Wake-on-LAN. |
| **M4** | F15 | Voz Completa e Sentidos Acústicos | Detecção contínua da palavra de ativação "Orion", Silero VAD, cancelamento de eco acústico (AEC3), barge-in suave e podcast matinal. |
| **M5** | F16 e F17 | Grafo 3D da Memória e Shaders | Visualização tridimensional da memória via `wgpu` e compute shaders (Barnes-Hut), efeitos de bloom e blur de vidro em WGSL. |
| **M6** | F18 | Câmera, Presença e Visão Espacial | Detecção em tempo real via YOLO ONNX local, biometria facial clássica Haar/LBPH, rastreamento "Onde deixei meus objetos?". |
| **M7** | F19 | Extensões, WASM e Plataforma MCP | Sandboxing de plugins via Wasmtime Component Model, suporte completo ao protocolo MCP, interface gráfica para MCP Apps e conector Claude Code. |
| **M8** | F20 a F25 | Sentidos de Sistema, Web, Casa e Vida | Memória visual de tela com OCR Windows Media, pesquisa web autônoma profunda, integração residencial Alexa/Home Assistant, finanças OFX/CSV e motor de estudo FSRS. |
| **M9** | F26 e F27 | Automelhoria, Auditoria e Multiplataforma | Fila de automelhoria autônoma do Orion, auditoria final de segurança estrita e portabilidade para macOS. |

---

## 2. Checklist Detalhado das Fases Pós-v1 (F11 a F27)

### F11: Ferramentas Avançadas de Código e Terminal (Marco M2)
- [~] **F11.01** Ferramentas de shell, Git e processos já portadas em `orion-core/src/tools/`.
- [ ] **F11.02** Painel de Terminal emulado nativo no app iced via pseudo-console do Windows (ConPTY / `iced_term`).
- [ ] **F11.03** Visualizador de diferenças de código (*diff viewer*) com navegação por blocos modificados e botões de aceite parcial.
- [ ] **F11.04** Painel de navegação na árvore de arquivos do projeto com filtro dinâmico de `.gitignore`.
- [ ] **F11.05** Integração bidirecional com GitHub: listagem de pull requests, visualização de status de CI e comentários em linhas de código.
- [ ] **F11.06** Revisão cruzada assistida: o Orion gera relatórios de segurança e boas práticas antes de cada commit.
- [ ] **F11.07** Extensão leve para VS Code conectada ao núcleo via stdio/loopback seguro.

---

### F12: Rotinas em Segundo Plano, Automações e Nuvem (Marco M2)
- [~] **F12.01** Motores de agendamento e automação já portados em `orion-core/src/jobs.rs` e `rotinas.rs`.
- [~] **F12.02** Integração com CLIs em nuvem (Claude Code, OpenAI Codex) via subprocessos em `orion-core/src/nuvem.rs`.
- [ ] **F12.03** Parser de agendamento em linguagem natural ("toda terça às 14h", "em 45 minutos").
- [ ] **F12.04** Fila resiliente de tarefas em segundo plano (`/tarefas-bg`) com retentativas exponenciais e persistência contra quedas de energia.
- [ ] **F12.05** Rotina de organização inteligente da pasta Downloads: categorização por tipo e regras declarativas em `regras_downloads.yaml`.
- [ ] **F12.06** Monitor de eventos do sistema: observador de pastas locais com gatilhos automáticos para rotinas concedidas.
- [ ] **F12.07** Briefing matinal proativo às 07:00 com resumo de compromissos da agenda, pendências e previsão climática local.

---

### F13: Orçamento e Telemetria de Uso das Assinaturas Oficiais (Marco M2)
- [x] **F13.01** Tabela SQLite e lógica de cálculo de orçamentos em `orion-ai/src/budget.rs`.
- [ ] **F13.02** Leitura e reconciliação oficial do consumo de tokens através das CLIs instaladas (`claude /usage`, `codex /status`).
- [ ] **F13.03** Notificação preventiva ao atingir 80% e 90% da cota semanal estipulada para qualquer provedor.
- [ ] **F13.04** Preempção inteligente de tarefas pesadas: tarefas de segundo plano cedem recursos automaticamente caso o usuário inicie uma conversa interativa.
- [ ] **F13.05** Projeção preditiva de esgotamento de cotas com recomendação de troca dinâmica de modelo.

---

### F14: Telegram e Controle Remoto Seguro (Marco M3)
- [~] **F14.01** Cliente Telegram baseado em HTTP assíncrono em `orion-core/src/channels/telegram.rs` com lista de IDs restrita (default-deny).
- [~] **F14.02** Envio e recebimento de mensagens de texto, comandos, fotos da câmera e áudio de resposta via voz edge-tts.
- [~] **F14.03** Cartões de aprovação fora de banda com botões *Aprovar* e *Negar* no Telegram.
- [ ] **F14.04** PIN numérico de segurança obrigatório para comandos críticos emitidos pelo Telegram.
- [ ] **F14.05** Protocolo de Wake-on-LAN (WoL) para ligar o computador remotamente através de sinal de rede enviado via rede local / Tailscale.
- [ ] **F14.06** Fila de mensagens bidirecional com tratamento para momentos em que o PC anfitrião estiver em modo de suspensão.

---

### F15: Voz Nativa, Wake Word e Processamento de Áudio (Marco M4)
- [~] **F15.01** Captura e reprodução de áudio de baixa latência em `orion-voice` utilizando `cpal` e reamostragem em tempo real para mono 16 kHz.
- [~] **F15.02** Detector heurístico de duas palmas (`ClapDetector`) com janela configurada entre 150 ms e 700 ms.
- [~] **F15.03** Cancelamento de eco acústico (AEC3) e barge-in ativado por voz: interrupção suave do TTS ao detectar fala do usuário.
- [~] **F15.04** Transcrição local offline com `whisper-rs` (whisper.cpp) com suporte a CPU e GPU Vulkan.
- [ ] **F15.05** Integração de VAD neural via `silero-vad` (ONNX) para segmentação perfeita de voz sem cortes em ambientes ruidosos.
- [ ] **F15.06** Treinamento e homologação do modelo acústico definitivo para a palavra de ativação "Orion", alcançando recall > 95% e falsos positivos < 0,5/hora.
- [ ] **F15.07** Mini barra de voz flutuante (*HUD*) com animação de onda sonora espectral reativa ao microfone.
- [ ] **F15.08** Overlay transparente de legendas em tempo real sobre a tela para traduções ao vivo ou reuniões.
- [ ] **F15.09** Geração do briefing diário em formato de podcast em áudio (síntese TTS encadeada com vinheta instrumental).

---

### F16: Grafo 3D da Memória em wgpu (Marco M5)
- [~] **F16.01** Grafo 2D orgânico multiescala em canvas implementado e estabilizado em `orion-app/src/graph.rs`.
- [ ] **F16.02** Pipeline de renderização 3D nativa sobre `wgpu` com suporte a câmera orbital, rotação, zoom suave e iluminação espacial.
- [ ] **F16.03** Simulação de física de nós em GPU via Compute Shader implementando o algoritmo de árvore Barnes-Hut.
- [ ] **F16.04** Capacidade de renderização com desempenho estável a 60 fps em grafos massivos contendo de 1.000 a 10.000 nós e arestas.
- [ ] **F16.05** Visualização em linha do tempo tridimensional (*Timeline 3D*) mostrando a evolução temporal das memórias e conexões do vault.
- [ ] **F16.06** Filtros dinâmicos por tipos de nós (Projetos, Fatos, Pessoas, Conceitos, Decisões) com isolamento instantâneo de agrupamentos (*clusters*).

---

### F17: Efeitos Visuais Avançados e Shaders (Marco M5)
- [~] **F17.01** Céu estrelado procedural com dispersão luminosa calculado em CPU em `orion-sky`.
- [ ] **F17.02** Shaders em WGSL dedicados para pós-processamento gráfico: efeito bloom de alta qualidade nos nós e estrelas.
- [ ] **F17.03** Efeito de desfoque de vidro (*acrylic / frosted glass*) em tempo real sob painéis flutuantes e menus.
- [ ] **F17.04** Escalonador dinâmico de qualidade visual: redução automática de efeitos para economizar bateria ao desconectar da tomada.

---

### F18: Percepção Visual, Câmera e Visão Espacial (Marco M6)
- [~] **F18.01** Captura de vídeo nativa via `nokhwa` através da Media Foundation do Windows em `orion-core/src/tools/camera.rs`.
- [~] **F18.02** Reconhecimento facial clássico local utilizando cascatas de Haar para detecção e histogramas LBPH para identificação do dono.
- [ ] **F18.03** Runtime de inferência YOLO ONNX conectado de ponta a ponta para identificação contínua de objetos e pessoas em quadros de vídeo.
- [ ] **F18.04** Recurso "Onde eu deixei?": registro histórico de avistamentos de chaves, celular, fones e carteira com timestamp e último local visto.
- [ ] **F18.05** Detecção de presença física: o Orion cumprimenta ao sentar à mesa e pausa notificações/esconde a tela ao se ausentar.
- [ ] **F18.06** Painel de controle de privacidade com lâmpada virtual indicadora do status da webcam e chave física lógica de corte.

---

### F19: Plataforma de Extensões, WASM e MCP Avançado (Marco M7)
- [x] **F19.01** Servidor e cliente MCP proprietários via JSON-RPC em `orion-mcp`.
- [x] **F19.02** Banco de dados com credenciais de leitura com hash, validade, permissões e limitação de taxa (F19.13, F19.14).
- [ ] **F19.03** Sandboxing de plugins utilizando Wasmtime e WebAssembly Component Model (WIT) para execução de código com isolamento rígido de memória.
- [ ] **F19.04** Interface gráfica para *MCP Apps*: renderização segura de interfaces web interativas enviadas por servidores MCP.
- [ ] **F19.05** Tela "Conectores e Extensões" com catálogo curado de servidores MCP (GitHub, Slack, PostgreSQL, Notion, Linear).
- [ ] **F19.06** Registro oficial do Orion como servidor de contexto para ferramentas externas (Claude Code, Zed, Cursor).
- [ ] **F19.07** Extensão de navegador para captura com um clique de páginas e seleção de texto diretamente para o vault.

---

### F20: Memória de Tela, OCR e Inteligência de Trabalho (Marco M8)
- [~] **F20.01** Captura pontual de tela via `xcap` ou APIs nativas do Windows em `orion-core/src/tools/vision.rs`.
- [ ] **F20.02** Pipeline de captura contínua e silenciosa da tela (1 quadro a cada 5 segundos) com descarte automático de telas bloqueadas ou janelas privadas.
- [ ] **F20.03** Motor de OCR nativo do Windows (`Windows.Media.Ocr`) processando localmente sem enviar imagens para fora.
- [ ] **F20.04** Banco de dados criptografado de memória de tela permitindo busca por texto visto no monitor ("qual era aquele gráfico que vi na segunda-feira?").
- [ ] **F20.05** Recurso "Click to Do": clicar sobre qualquer texto ou imagem na tela para acionar ações inteligentes contextuais.
- [ ] **F20.06** Detecção automática de reuniões ativas (Google Meet, Teams, Zoom) com sugestão de gravação e geração de ata.

---

### F21: Web Avançada e Pesquisa Autônoma Profunda (Marco M8)
- [x] **F21.01** Leitura de páginas em Rust via Readability (`dom_smoothie` + `htmd`) e navegador headless CDP (`chromiumoxide`).
- [ ] **F21.02** Modo de Pesquisa Profunda (*Deep Research*): o Orion navega por múltiplos links de forma autônoma, sintetiza fontes e elabora dossiês temáticos em Markdown.
- [ ] **F21.03** Visualização em tempo real do navegador guiado com painel de progresso da navegação e tomada de notas.
- [ ] **F21.04** Monitores periódicos de páginas web para detecção de alterações (preços, editais, atualizações de documentação).

---

### F22: Ecossistema Multi-Dispositivo, Rede e Automação Residencial (Marco M8)
- [ ] **F22.01** Sincronização segura entre múltiplos computadores do usuário através de rede de malha privada (Tailscale) com criptografia ponta a ponta.
- [ ] **F22.02** Integração com Home Assistant para controle de iluminação, tomadas e sensores do escritório.
- [ ] **F22.03** Habilidade de interação por voz com Amazon Alexa (via skill privada vinculada ao gateway do Orion).
- [ ] **F22.04** Integração com Apple Shortcuts e atalhos rápidos de celular.
- [ ] **F22.05** Integração com Microsoft Phone Link para notificações e respostas a SMS pelo desktop.

---

### F23: Gestão de Vida Pessoal, Google e Finanças Locais (Marco M8)
- [~] **F23.01** Modelos de dados e rotas backend de finanças em `orion-core/src/vida.rs` (tabelas `transactions`, `bills`, `price_watch`).
- [ ] **F23.02** Importação e conciliação automática de extratos bancários em formato OFX e CSV com categorização local via modelo embarcado.
- [ ] **F23.03** Sincronização oficial com Google Calendar via OAuth2 para gestão da agenda diária.
- [ ] **F23.04** Monitor de preços inteligente: rastreamento de produtos com avisos em caso de queda de preço histórica.
- [ ] **F23.05** Diário reflexivo noturno e cápsula do tempo guardados no vault com análise semântica de humor.

---

### F24: Estudo, Conhecimento e Motor de Repetição Espaçada (Marco M8)
- [ ] **F24.01** Implementação do algoritmo FSRS (Free Spaced Repetition Scheduler) para memorização otimizada.
- [ ] **F24.02** Modo Tutor Socrático no chat: o Orion realiza perguntas para fixação de conteúdos a partir das notas do usuário.
- [ ] **F24.03** Geração automática de cartões de estudo (*flashcards*) a partir de arquivos PDF ou notas de pesquisa.
- [ ] **F24.04** Mapeamento automático de lacunas de conhecimento e sugestão de novos tópicos de estudo.

---

### F25: Utilidades do Sistema, Mídia e Manipulação de Documentos (Marco M8)
- [~] **F25.01** Extração de texto de documentos PDF já integrada na v1.
- [ ] **F25.02** Ferramenta nativa de compressão e otimização de arquivos PDF sem uso de utilitários externos.
- [ ] **F25.03** Conversor de formatos de imagem e documentos (`.docx` para `.pdf`, `.png` para `.webp`).
- [ ] **F25.04** Integração oficial com o cliente desktop do Spotify para controle de reprodução e trilha de foco.
- [ ] **F25.05** Modo de Alto Desempenho / Jogos: suspensão automática de todas as rotinas em segundo plano e liberação de memória RAM ao iniciar jogos.

---

### F26: Motor de Automelhoria Contínua (Marco M9)
- [ ] **F26.01** Fila autônoma de sugestões de melhoria arquitetural e correção de pequenos bugs identificados pelo próprio uso.
- [ ] **F26.02** Criação automatizada de branches e Pull Requests pelo Orion com execução de suíte de testes locais.
- [ ] **F26.03** Interrupção obrigatória de qualquer processo de automelhoria caso o usuário necessite dos recursos da máquina.
- [ ] **F26.04** Auditoria contínua de dependências e sugestão de refatorações de segurança.

---

### F27: Auditoria Final, Documentação e Release 2.0 (Marco M9)
- [ ] **F27.01** Auditoria completa e independente de segurança cobrindo todas as classes de risco do `orion-policy`.
- [ ] **F27.02** Validação de operação 100% offline do sistema sem envio de nenhum byte para fora do computador.
- [ ] **F27.03** Medição rigorosa de latência e consumo de hardware frente aos limites ideais de produto.
- [ ] **F27.04** Empacotamento oficial da versão 2.0 e elaboração das instruções de migração definitiva.
- [ ] **F27.05** Portabilidade e compilação para macOS com geração de instalador `.dmg` assinado e notarizado.

---

## 3. Especificação Passo a Passo para as Principais Implementações Pós-v1

---

### Implementação A: Terminal ConPTY Integrado no Painel (F11.02)
- **Crates e Dependências:** `windows::Win32::System::Console`, `iced_term` ou `portable-pty`.
- **Arquitetura Técnica:**
  1. No Windows, criar um pseudo-console chamando `CreatePseudoConsole` com dimensões calculadas a partir da área do painel lateral da interface.
  2. Inicializar um processo filho (`powershell.exe` ou `cmd.exe`) associando seus handles de entrada e saída aos pipes do pseudo-console.
  3. No crate `orion-app`, instanciar o widget de terminal que consome o fluxo de bytes ANSI emitido pelo processo filho e o converte em glifos renderizados com a fonte `Geist Mono`.
  4. Redimensionamento: ao redimensionar a barra lateral, chamar `ResizePseudoConsole` para sincronizar as colunas e linhas do terminal sem quebra de layout.
- **Critério de Aceite:**
  - Clicar na aba "Terminal" do chat; um prompt do PowerShell deve surgir, responder a comandos (`dir`, `cargo --version`) e exibir cores ANSI sem travar o app.

---

### Implementação B: Grafo 3D em wgpu com Compute Shader Barnes-Hut (F16.02, F16.03)
- **Crates e Dependências:** `wgpu 24.x`, `bytemuck`, `glam` (álgebra linear de vetores).
- **Arquitetura Técnica:**
  1. *Estruturas em Memória GPU:* Criar buffers de armazenamento (*Storage Buffers*) contendo as posições dos nós (`vec3<f32>`), velocidades (`vec3<f32>`) e massa de grau.
  2. *Compute Shader Barnes-Hut:* Em `shaders/barnes_hut.wgsl`, implementar dois passos:
     - Passo 1: Construção paralela do Octree tridimensional a partir das posições dos nós.
     - Passo 2: Cálculo das forças de repulsão eletrostática e atração de mola por grau, aplicando o avanço temporal com amortecimento Euler-Verlet.
  3. *Pipeline de Renderização:* Desenhar nós como esferas instanciadas e arestas como linhas conectadas por coordenadas de nós atualizadas pelo compute shader.
  4. *Interação:* Implementar raycasting do cursor do mouse em coordenadas 3D para destacar o nó sob o cursor e abrir seu painel de detalhes no vault.
- **Critério de Aceite:**
  - Abrir a visualização 3D da Memória com 3.000 nós e 10.000 arestas; a taxa de quadros deve se manter estável a 60 fps em hardware gráfico dedicado.

---

### Implementação C: VAD Neural Silero e Palavra de Ativação "Orion" Final (F15.05, F15.06)
- **Crates e Dependências:** `ort 2.x` (ONNX Runtime nativo com aceleração DirectML/CPU), `cpal`, `rubato`.
- **Arquitetura Técnica:**
  1. *Segmentação Acústica Silero VAD:* Carregar o modelo pré-quantizado `silero_vad.onnx`. Para cada bloco de 512 amostras de áudio (32 ms a 16 kHz), realizar inferência rápida (< 2 ms) para obter a probabilidade de fala humana. Se a probabilidade for > 0,65 por 3 blocos consecutivos, acionar o estado de fala ativa.
  2. *Wake Word "Orion":* Encaminhar o áudio de fala ativa para o buffer circular do `openWakeWord` com o modelo treinado definitivo. Se a pontuação passar do limiar de 0,75, disparar o sinal `Msg::WakeWordDetected`.
  3. *Transição para o Chat:* O app abre imediatamente a interface ou o HUD flutuante, emite um som suave de confirmação e inicia a gravação do comando até que o Silero detecte 800 ms de silêncio contínuo, enviando para o Whisper.
- **Critério de Aceite:**
  - Com música tocando em volume ambiente na sala, pronunciar pausadamente "Orion"; o assistente deve acender a interface sem disparar em palavras foneticamente próximas como "avião" ou "união".

---

### Implementação D: Visão Contínua com YOLO ONNX e Localização de Objetos (F18.03, F18.04)
- **Crates e Dependências:** `nokhwa`, `image`, `ort` (ONNX Runtime), `rusqlite`.
- **Arquitetura Técnica:**
  1. *Captura Otimizada:* O serviço de vigia acorda a cada 10 segundos, captura 1 quadro da webcam em resolução 640×480 e desliga o sensor imediatamente para não manter a luz da câmera acesa continuamente.
  2. *Inferência YOLO:* Passar o tensor normalizado para o modelo `yolo11n.onnx` local. Aplicar Non-Maximum Suppression (NMS) e extrair caixas delimitadoras e classes de interesse: `cell phone`, `keys`, `wallet`, `cup`, `backpack`.
  3. *Persistência Espacial:* Gravar na tabela `object_sightings`:
     ```sql
     INSERT INTO object_sightings (objeto, confianca, x, y, largura, altura, timestamp)
     VALUES (?, ?, ?, ?, ?, ?, ?);
     ```
  4. *Consulta em Linguagem Natural:* Ao perguntar no chat "Onde deixei meu celular?", o agente consulta a tabela de avistamentos e responde: *"Seu celular foi visto pela última vez sobre a mesa, à direita, há 22 minutos (às 15:40)."*
- **Critério de Aceite:**
  - Colocar a carteira em frente à câmera e tirá-la 5 minutos depois; ao perguntar "onde está minha carteira?", o Orion deve indicar com precisão o último horário registrado.

---

### Implementação E: Sandboxing de Plugins com Wasmtime Component Model (F19.03)
- **Crates e Dependências:** `wasmtime 28.x`, `wit-bindgen`.
- **Arquitetura Técnica:**
  1. *Contrato de Interface (WIT):* Definir o arquivo `orion-plugin.wit` especificando as funções exportadas pelo plugin (`executar`, `informar_metadados`) e as importadas do host (`ler_configuracao_especifica`, `log_mascarado`).
  2. *Instanciação Segura:* Carregar o binário `.wasm` do plugin dentro de uma instância isolada do Wasmtime com limites de consumo de combustível (*fuel metering*) e teto de memória estrito (máximo 64 MB por plugin).
  3. *Barreira de Rede e Sistema:* Nenhum plugin WASM tem acesso a arquivos do computador, portas de rede ou variáveis de ambiente a menos que uma concessão explícita tenha sido concedida pelo Antônio via diálogo de permissões.
- **Critério de Aceite:**
  - Instalar um plugin de exemplo em WASM que tente abrir um socket de rede ou ler o disco; a execução é rejeitada com erro de sandbox sem afetar o processo principal do Orion.

---

### Implementação F: Memória de Tela Contínua com OCR Nativo (F20.02, F20.03, F20.04)
- **Crates e Dependências:** `windows::Graphics::Imaging`, `windows::Media::Ocr`, `orion-store`.
- **Arquitetura Técnica:**
  1. *Captura Periódica:* Em segundo plano, acionar `xcap` para capturar a área do monitor a cada 5 segundos se houver movimento do mouse ou teclado.
  2. *OCR Local do Windows:* Converter o bitmap em um `SoftwareBitmap` e chamar `OcrEngine::TryCreateFromUserProfileLanguages()` seguido de `RecognizeAsync()`.
  3. *Deduplicação:* Se o texto extraído tiver mais de 95% de similaridade de Levenshtein com o quadro anterior, descartar para não inchar o banco.
  4. *Indexação no SQLite:* Salvar na tabela virtual FTS5 `screen_memory_fts`:
     ```sql
     INSERT INTO screen_memory_fts (texto, titulo_janela, processo, timestamp)
     VALUES (?, ?, ?, ?);
     ```
- **Critério de Aceite:**
  - Abrir um documento técnico no navegador e fechar em seguida. Perguntar no chat: "Qual era o valor da constante que aparecia no PDF?"; o Orion localiza o texto indexado da tela e informa o contexto exato.

---

### Implementação G: Motor de Repetição Espaçada FSRS e Estudo (F24.01, F24.03)
- **Crates e Dependências:** `fsrs` (implementação pura em Rust do algoritmo FSRS v5).
- **Arquitetura Técnica:**
  1. *Modelagem no SQLite:* Criar tabelas `flashcards` e `card_reviews` armazenando estabilidade (*stability*), dificuldade (*difficulty*), repetições e data de agendamento calculado pelo FSRS.
  2. *Extração Automática de Cartões:* O agente lê uma nota de estudo do vault (`03 Recursos/Conceitos/`) e gera pares de pergunta e resposta contextualizados com wikilinks.
  3. *Interface de Estudo:* Na aba Conhecimento > Estudo, apresentar o card, permitir virar e registrar a nota de avaliação do usuário (1: Esqueci, 2: Difícil, 3: Bom, 4: Fácil).
- **Critério de Aceite:**
  - Responder a uma rodada de revisão de 10 cartões; os próximos prazos de revisão devem ser calculados matematicamente conforme as curvas de esquecimento do FSRS.

---

## 4. Matriz de Pesquisa Técnica, Modelos e Crates Candidatos

Consolidada a partir de `ORION_REFERENCIAS_PESQUISA.md` e dos dossiês de arquitetura:

| Domínio Técnico | Crate / Tecnologia Recomendada | Alternativa Avaliada | Motivo da Escolha Técnica |
|---|---|---|---|
| **Renderizador 3D** | `wgpu 24.x` | `three-d`, `kiss3d` | Rust puro, semântica moderna WebGPU, suporte a compute shaders em Direct3D 12 e Vulkan. |
| **Terminal Emulado** | `iced_term` + ConPTY Win32 | `portable-pty` | Integração direta com os widgets do iced sem pontes de threads complexas. |
| **VAD Acústico** | `silero-vad` (ONNX via `ort`) | VAD por energia | Robustez absoluta contra ruídos de teclado, respiração e trânsito; baixíssimo consumo de CPU. |
| **Extensões Sandboxed** | `Wasmtime` (Component Model) | `wasmer`, Lua | Padrão industrial, suporte completo a WebAssembly System Interface (WASI) e segurança verificada. |
| **Motor de Busca Local** | `sqlite-vec` + FTS5 SQLite | Qdrant, LanceDB | Mantém a filosofia de processo único (`orion.exe`) sem servidores auxiliares consumindo portas e RAM. |
| **Navegação Web Guiada** | `chromiumoxide` (CDP direto) | `thirtyfour` (WebDriver) | Dispensa o download de drivers separados (`chromedriver`), comunica-se diretamente com o Edge instalado. |
| **Repetição Espaçada** | `fsrs-rs` | SM-2 clássico (Anki) | Algoritmo moderno baseado em aprendizado de máquina com retenção de longo prazo superior em até 30%. |

---

## 5. Diretrizes para Início das Fases Pós-v1

1. **Estabilidade Primeiro:** Nenhuma funcionalidade descrita neste documento (M2 a M9) deve ter seu desenvolvimento acelerado antes que todos os 12 testes de homologação da v1 em [ORION_MELHORIAS_PRE_V1.md](ORION_MELHORIAS_PRE_V1.md) estejam 100% aprovados e validados no computador do Antônio.
2. **Prioridade Imediata Pós-v1:** Ao concluir a v1, a sequência inicial obrigatória será:
   - **F25.06 (Documentos e Utilitários)** seguido imediatamente por **F20 (Memória de Tela)** e **F11 (Ferramentas de Código e Terminal)**.
3. **Respeito aos Princípios Imutáveis:** Qualquer ferramenta nova adicionada nestas fases deve ter sua classe de risco registrada em [REGRAS.md](REGRAS.md) antes de qualquer linha de código de execução ser escrita.
