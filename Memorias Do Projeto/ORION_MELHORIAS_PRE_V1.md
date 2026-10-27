# Orion — Melhorias e Pendências Pré-Lançamento (v1)

> **Documento Canônico de Melhorias v1.** Reúne, consolida e detalha todas as especificações, checklists e passos de implementação pré-lançamento do Orion (Marcos M0 e M1 / Fases F00 a F10, Rodada de Correção R1, Especificação de UI e Especificação do Instalador).
> 
> **Origens consolidadas:** `ORION_PLANO_IMPLEMENTACAO.md` (F00–F10), `ORION_AUDITORIA_PENDENCIAS.md`, `ORION_PLANO_CORRECAO_R1.md`, `ORION_UI_ESPECIFICACAO.md` e `ORION_INSTALADOR_ESPECIFICACAO.md`.
> **Documentos estruturais complementares:** [ORION.md](ORION.md) (estado canônico), [ARQUITETURA.md](ARQUITETURA.md) (arquitetura e crates), [REGRAS.md](REGRAS.md) (princípios e políticas imutáveis), [ORION_LEGADO.md](ORION_LEGADO.md) (histórico Python) e [ORION_MELHORIAS_POS_V1.md](ORION_MELHORIAS_POS_V1.md) (roadmap pós-v1).
>
> **Convenção de Status:**
> - `[x]` **Concluído:** implementado no código Rust e verificado por testes de integração/unitários.
> - `[~]` **Parcial / Aguardando Aceite Físico:** código implementado e testado no Linux/mock, mas pendente de homologação física no Windows (hardware, UAC, GPU Vulkan, áudio cpal, etc.).
> - `[ ]` **Pendente:** especificado, com arquitetura definida, aguardando implementação direta.
> - `[-]` **Cancelado / Substituído por Decisão:** cancelado formalmente com justificativa de arquitetura.

---

## 1. Visão Geral da v1 (Marco M0 e M1)

A versão 1.0 (v1) do Orion estabelece a fundação completa e o primeiro marco de produto usável no Windows. O produto é um executável nativo compilado em Rust (`orion.exe`) que hospeda a interface visual desenvolvida em `iced` (renderizador `wgpu`/`tiny-skia`) e o núcleo de inteligência (`orion-core`, `orion-ai`, `orion-store`, `orion-policy`) no mesmo processo em memória, sem portas TCP/HTTP abertas entre a interface e o motor.

O escopo da v1 é estritamente limitado aos seguintes componentes:
1. **Fundação e Spikes (M0 / F00):** Repositório, crates Cargo, dependências auditadas, política imutável e testes de viabilidade técnica.
2. **Casca da Interface (M1 / F01):** Janela com renderização acelerada, escala dinâmica tipográfica e espacial, paleta de comandos `Ctrl+K`, gerenciamento de atalhos configuráveis, tema Grafite escuro.
3. **Segurança e Criptografia (M1 / F02):** Hash de senhas em Argon2id com migração automática, banco de dados criptografado por SQLCipher (ChaCha20-Poly1305), mascaramento estrito de PII em logs e auditoria fail-closed.
4. **Gateway de Inteligência (M1 / F03):** Provedores gratuitos (Gemini 3.5 Flash-Lite padrão, Groq, OpenRouter, Mistral, GitHub Models), chaveamento seguro no Credential Manager do Windows, orçamento semanal persistido e worker local descartável `orion-llama.exe` (Job Object com teto estrito de 30% da RAM física).
5. **Agente e Políticas de Execução (M1 / F04):** Loop de execução com classes de risco fixas, fail-closed para argumentos truncados, proteção de dados externos como não-instrução e modo autônomo com barreira de senha.
6. **Chat e Mensagens (M1 / F05):** Streaming de texto em tempo real, renderização de Markdown, diagramas Mermaid, fórmulas matemáticas LaTeX em KaTeX, blocos de código com cópia atômica, anexos de documentos e versionamento local de artefatos.
7. **Integração com o Vault Obsidian (M1 / F06):** Escrita atômica em `.md` com frontmatter e wikilinks válidos, scanner prévio de credenciais/segredos, detecção de conflitos com propostas preservadas e Git seguro via índices temporários sem poluição de staged.
8. **Configurações Centralizadas (M1 / F07):** 15 seções em modal completo, internacionalização completa Fluent pt-BR/en, painel de métricas operacionais e font manager.
9. **Gerenciamento de Projetos (M1 / F08):** Isolamento total de memórias, instruções específicas por projeto, referências indexadas e herança de modelos.
10. **Painel de Métricas e Auditoria (M1 / F09):** Contadores diários de autorizações, acompanhamento de uso e registro de chamadas externas sem conteúdo.
11. **Instalador, Onboarding e Atualizador (M1 / F10):** `Orion Setup.exe` com constelação procedural animada (60 fps adaptativo), som instrumental cpal, elevação nativa por UAC, pacote esparso MSIX, onboarding de 7 telas/estrelas e atualizador atômico `orion update` com aplicação no fechamento.
12. **Estabilização de Hardware Real (Rodada R1):** Resolução de latência, fallback para adaptadores de CPU sem MSAA, sincronização assíncrona de anexos e diagnóstico preciso de rede.

---

## 2. Checklist Mapeado de Entregas e Pendências (F00 a F10 + R1)

### F00: Fundação, Repositório e Spikes de Viabilidade (Marco M0)
- [x] **F00.01** Criação da árvore Rust em workspace Cargo (`Orion_Rust/`) em repositório privado sob licença de uso pessoal (`LicenseRef-Proprietario`).
- [x] **F00.02** Configuração do workspace com 16 crates especializados: `orion-policy`, `orion-store`, `orion-ai`, `orion-core`, `orion-server`, `orion-app`, `orion-setup`, `orion-updater`, `orion-platform`, `orion-voice`, `orion-vault`, `orion-local`, `orion-llama`, `orion-sky`, `orion-testkit`.
- [x] **F00.03** `rust-toolchain.toml` travado em Rust 1.97.0 (MSVC x86_64), `Cargo.lock` versionado, `iced` fixado estritamente em `0.14.0`, `deny.toml` configurado.
- [x] **F00.04** Pasta `docs/` com arquitetura, ADRs (0001 a 0007), glossário e ferramentas de desenvolvimento.
- [x] **F00.05** Núcleo imutável com trava explícita: `orion-policy` recusa escrita em `REGRAS.md`, personas e no próprio crate.
- [x] **F00.06** `Orion_Rust/README.md` documentando comandos de build CPU/Vulkan e execução no Windows.
- [-] **F00.07** CI contínuo com `cargo deny` e `cargo audit` nos PRs (cancelado pela decisão de 21/10/2026; ADR 0007 — execução manual mantida).
- [-] **F00.08** Harness de testes headless e E2E automáticos do iced (cancelado temporariamente pela pausa de CI).
- [-] **F00.09** Pipeline de snapshots PNG automáticos com diff de imagem (substituído por snapshots manuais `orion-snap`).
- [x] **F00.10** Atualizações de dependências via Dependabot mensal agrupado (`.github/dependabot.yml`).
- [~] **F00.11** Revisão cruzada com templates de PR/Issue. *Pendente do Antônio: branch protection na `main` no GitHub Settings.*
- [-] **F00.12** Harness de evals automáticos de prompt injection (pausado pelo ADR 0007).
- [x] **F00.13** 21 skills de auditoria de segurança (OWASP LLM 2025, Trail of Bits) em `.claude/skills/` e `orion-auditoria`.
- [x] **F00.14** Spike iced: janela, streaming, campo de texto e medição de consumo (tempo de abertura < 1s, < 60 MB RAM).
- [x] **F00.15** Spike SQLite com FTS5 e busca híbrida (validado em `orion-store/examples/bench_busca.rs`).
- [x] **F00.16** Spike de vetor: `sqlite-vec 0.1.9` fixado para compatibilidade MSVC.
- [x] **F00.17** Spike de integração com o Git do vault: `git2` validado para pull ff-only e commit atômico.
- [x] **F00.18** Spike de bandeja e atalhos Windows: `tray-icon` e `global-hotkey` integrados.
- [x] **F00.19** Spike de áudio: captura `cpal`, mono 16 kHz e VAD por energia funcional.
- [x] **F00.20** Spike de Wake Word: engine local `oww-rs` com modelo sintético candidato.
- [x] **F00.21** Spike STT: `whisper-rs` com aceleração CPU e Vulkan.
- [x] **F00.22** Spike TTS: `edge-tts` com streaming de áudio nativo.
- [x] **F00.23** Spike de Prompt Injection: ModernBERT multilíngue selecionado sobre Shield e DeBERTa.
- [x] **F00.24** Spike de Scanner de Segredos: regex para cartões (Luhn), CPF com dígito verificador, PIX, chaves AWS/GitHub/Google.
- [x] **F00.25** Spike de Worker LLM: Job Object do Windows limitando memória do subprocesso a 30% da RAM.
- [x] **F00.26** Spike de Embeddings: BM25 + LSA mantido como base estável até testes com banco real.
- [x] **F00.27** Spike de Leitura Web: substituição do sidecar Python por Readability em Rust (`dom_smoothie` + `htmd`).
- [x] **F00.28** Navegador guiado em Rust puro via CDP (`chromiumoxide`) em instâncias efêmeras do Edge/Chrome.
- [x] **F00.29** Auditoria de termos de uso de provedores gratuitos concluída (`docs/termos-provedores.md`).
- [ ] **F00.30** Linha de base real de métricas no hardware do Antônio (latência, render, CPU).

---

### F01: Janela, Tema e Base da Interface (iced)
- [x] **F01.01** Janela sem moldura nativa com barra superior personalizada de 40 px, controles Minimizar, Maximizar e Fechar com áreas de toque acessíveis.
- [x] **F01.02** Tema único Grafite neutro escuro (`#0C0E13` background, `#14171F` superfícies, `#1E2330` bordas, `#2A3142` hover).
- [x] **F01.03** Escala tipográfica padronizada em `theme.rs` (base 14 px, título 24 px, subtítulo 18 px, legendas 11 px, código em `Geist Mono`).
- [x] **F01.04** Cantos arredondados padronizados: botões e cards com raio de 10 px a 14 px, modais com 16 px.
- [~] **F01.05** Efeito Mica no Windows 11 via `window-vibrancy`. *Pendente de teste e ativação em máquina física com Windows 11.* *Código na branch `claude/kind-bohr-j6hi1g`; compila (para o Windows só até a checagem de tipos, sem rodar). **Sem aceite na máquina real.** Mica pelo DWM (`DWMWA_SYSTEMBACKDROP_TYPE`) com a janela transparente só em GPU real; a barra lateral fica translúcida quando ativo; `ORION_MICA=0` desliga. Sem `window-vibrancy`. Falta ver no Windows 11 (22H2+).*
- [~] **F01.06** Instância única comprovada no Windows: trava via arquivo `.lock` e ativação da janela existente via named pipe / evento DWM. *Código na branch `claude/kind-bohr-j6hi1g`; compila (para o Windows só até a checagem de tipos, sem rodar). **Sem aceite na máquina real.** Instância única já usava trava de arquivo; agora a segunda cópia chama `AllowSetForegroundWindow` para a primeira conseguir trazer a janela para a frente (sem isso o Windows só pisca a barra de tarefas).*
- [x] **F01.07** Zoom e escala de interface: atalhos `Ctrl +`, `Ctrl -` e `Ctrl 0`, persistência automática em `interface.txt`.
- [x] **F01.08** Toasts discretos no canto inferior direito para notificações de sucesso/erro (cópia de código, salvamento).
- [x] **F01.09** Barra lateral retrátil (toggle `Ctrl+B`) com ícones para Início, Chat, Memória, Projetos, Rotinas e Conhecimento.
- [x] **F01.10** Rodapé da barra lateral: avatar de 32 px com iniciais do perfil, nome do usuário, computador local e botão direto de configurações.
- [x] **F01.11** Remoção definitiva da tela Resultados da navegação lateral (dados mantidos no banco e via rota `/resultados`).
- [x] **F01.12** Seção Fontes removida da barra lateral e restrita a Configurações > Fontes.
- [x] **F01.13** Painel de Métricas movido da navegação principal para dentro do modal Configurações > Painel.
- [x] **F01.21** Paleta de comandos flutuante (`Ctrl+K`): busca fuzzy de comandos de sistema, navegação direta com prefixo `>`.
- [x] **F01.22** Gerenciador de atalhos em Configurações > Atalhos: remapeamento de `Ctrl+K`, `Ctrl+Shift+O`, `Ctrl+B`, `Ctrl+,`, validação de conflitos, bloqueio de teclas reservadas (Enter, Esc, setas) e gravação atômica em `atalhos.txt`.
- [x] **F01.23** Navegação direta por comando de barra: `/config <seção>` com auto-complete e descrições internacionalizadas via Fluent.
- [x] **F01.24** Lista de Projetos Recentes exibida dinamicamente no topo da tela Início.
- [~] **F01.25** Resolução da faixa branca de renderização reportada em drivers gráficos WARP de CPU. *Código na branch `claude/kind-bohr-j6hi1g`; compila (para o Windows só até a checagem de tipos, sem rodar). **Sem aceite na máquina real.** Estilo da janela pelo DWM em `orion-platform/src/janela.rs`: pincel de fundo da classe, borda, barra de título e cantos na cor do tema, aplicado quando a janela abre. Ataca a causa provável (sobra de 1 px mostrando o fundo branco da classe); falta ver em 100/125/150% e no WARP.*

---

### F02: Dados, Segurança e Login Local
- [x] **F02.01** Autenticação local em Argon2id (19 MiB memória, 2 iterações, 1 thread). Migração transparente de hashes legados PBKDF2 no primeiro login.
- [x] **F02.02** Limpeza de credenciais na memória (`zeroize`) em todas as estruturas que manipulam senhas em claro.
- [x] **F02.03** Banco de dados SQLite criptografado via SQLCipher com derivação de chave de 256 bits (`orion.key`) duplamente embrulhada (senha Argon2id + código de recuperação de 32 caracteres).
- [x] **F02.04** Manutenção periódica de banco: rotina de `VACUUM` semanal executada estritamente durante repouso do sistema, sem expurgo de mensagens históricas, com reconstrução manual de índices FTS5.
- [x] **F02.05** Logs rotacionados diariamente em formato cifrado (`logs/orion-YYYY-MM-DD.log.enc`) com expurgo automático após retenção configurada (padrão: 30 dias).
- [x] **F02.06** Sanitização rigorosa de PII em logs de erro e tabelas de auditoria: máscaras para CPFs, e-mails, números de cartão de crédito (algoritmo de Luhn), telefones e chaves de API.
- [x] **F02.07** Desafio 2FA opcional por código efêmero enviado via Telegram no momento do login.
- [~] **F02.08** Integração nativa com Windows Hello (biometria / PIN) através da API WinRT `UserConsentVerifier`. *Código na branch `claude/kind-bohr-j6hi1g`; compila (para o Windows só até a checagem de tipos, sem rodar). **Sem aceite na máquina real.** `onboarding_hello.rs` agora usa `IUserConsentVerifierInterop` com a janela do Orion como dona (app sem pacote precisa disso para o diálogo aparecer); sem janela cai na chamada simples. Falta testar com leitor biométrico/PIN.*
- [~] **F02.09** Bloqueio automático ao travar a estação (`Win+L`): descarregamento da chave da memória RAM e encerramento seguro de sessões ativas. *Código na branch `claude/kind-bohr-j6hi1g`; compila (para o Windows só até a checagem de tipos, sem rodar). **Sem aceite na máquina real.** A leitura do bloqueio agora usa `WTSSessionInfoEx` (resposta oficial) em vez de só `OpenInputDesktop`, que também falha no prompt do UAC e em Ctrl+Alt+Del e podia apagar a chave sem bloqueio; exige duas leituras seguidas (1 s). Falta testar Win+L de verdade.*
- [ ] **F02.10** Execução assistida do comando de migração `orion db-cifrar` no banco de produção do usuário com backup de integridade prévio.

---

### F03: Gateway de IA, Roteador e Modelos Locais
- [x] **F03.01** Gateway unificado em `orion-ai` com suporte a APIs compatíveis OpenAI, Google Gemini, Groq, Cerebras, OpenRouter, Mistral e NVIDIA NIM.
- [x] **F03.02** Provedor padrão automático configurado para `gemini-3.5-flash-lite` na cota gratuita do Google AI Studio.
- [x] **F03.03** Migração automática de configurações legadas apontando para `gemini-2.5-flash` ou `gemini-3.8-flash`.
- [x] **F03.04** Armazenamento seguro de chaves de API exclusivamente no Credential Manager do Windows (Keyring nativo); o banco guarda apenas identificadores e preferências de catálogo.
- [x] **F03.05** Roteamento semântico por camadas: chat rápido utiliza modelos ultra-leves com esforço de raciocínio reduzido (`reasoning_effort: low`); tarefas pesadas delegam para modelos avançados ou CLIs oficiais.
- [x] **F03.06** Orçamento preventivo persistido no SQLite: teto de 95% da cota semanal do provedor e limite de 20% por tarefa única.
- [x] **F03.07** Avaliação local de respostas (`-1` / `+1`) armazenada no banco para histórico e calibração de respostas, sem telemetria externa para treinamento.
- [x] **F03.08** Catálogo fixo de 8 modelos locais em `orion-local`: modelos GGUF quantizados (Qwen, Phi, SmolLM) e modelos de áudio Whisper.
- [x] **F03.09** Worker local desacoplado `orion-llama.exe`: executável independente embarcando `llama-cpp-2` nativo, sem colisão de símbolos GGML com o Whisper.
- [x] **F03.10** Isolamento de recursos via Windows Job Object: o worker é restringido pelo sistema operacional a utilizar no máximo 30% da memória RAM física disponível.
- [x] **F03.11** Comunicação serializada com o worker via pipes IPC (`stdin`/`stdout`) com envelope JSON estruturado e timeout de segurança de 180 s.
- [~] **F03.12** Homologação de compilação e inferência acelerada por GPU física com backend Vulkan no hardware do usuário.

---

### F04: Agente, Políticas de Segurança e Execução
- [x] **F04.01** Loop de agente autônomo com controle estrito por turno, limites de iterações configuráveis (1 a 100 passos) e cancelamento cooperativo.
- [x] **F04.02** Classes de risco imutáveis em `orion-policy`: Leitura (execução direta), Escrita (execução com auditoria obrigatória), Execução e Destrutiva (aprovação explícita e autenticada obrigatória).
- [x] **F04.03** Cartões de aprovação com exigência de integridade: rejeição obrigatória (fail-closed) para qualquer solicitação cujos argumentos estejam truncados ou ocultos.
- [x] **F04.04** Re-verificação de contexto na consolidação: falas externas ou de terceiros são filtradas e nunca transformadas em fatos permanentes.
- [x] **F04.05** Isolamento estrito de conteúdo externo (`ExternalData`): dados lidos da web, arquivos ou e-mails recebem marcador de contaminação e não podem acionar ferramentas de escrita/execução no mesmo turno sem aval.
- [x] **F04.06** Modo autônomo com proteção por senha: a ativação do modo sem interrupções exige re-digitação da senha mestre do perfil.
- [x] **F04.07** Resumos periódicos consolidados por escopo gerados localmente e com expurgo de tentativas falhas.
- [x] **F04.08** Mensagens fixadas sobrevivem à compactação de contexto e são reapresentadas ao modelo como premissas de trabalho.
- [~] **F04.09** Classificador de injeção de prompt local (ModernBERT) integrado em tempo real antes do envio do prompt ao provedor. *Código na branch `claude/kind-bohr-j6hi1g`, compilado e com a suíte existente verde no Linux, sem testes novos (pausa de 21/10/2026). Guarda de injeção no conteúdo externo (`orion-ai/src/injecao.rs`): trait + heurística local pt/en, ligado por padrão (`ORION_INJECTION_GUARD`). Suspeito vai com aviso; bloqueado não vai ao provedor; falha de classificação bloqueia. **O ModernBERT real ainda não entrou** (só o ponto de encaixe).*

---

### F05: Chat Completo, Renderização e Artefatos
- [x] **F05.01** Streaming bidirecional de mensagens em tempo real com buffer de tokens suave e sem travamento da renderização da interface.
- [x] **F05.02** Renderizador nativo de Markdown compatível com CommonMark e GFM: suporte a títulos, ênfases, listas, tabelas alinhadas e citações em bloco.
- [x] **F05.03** Blocos de código com destaque de sintaxe, numeração de linhas e botão de cópia atômica para o clipboard do Windows.
- [x] **F05.04** Diagramas Mermaid: renderização direta de fluxogramas em canvas vetorial integrado, sem dependência de browser externo.
- [x] **F05.05** Fórmulas matemáticas em LaTeX renderizadas via KaTeX nativo.
- [x] **F05.06** Suporte a anexos de documentos locais (`.txt`, `.md`, `.pdf` até 6 MB, limite de 8 arquivos por turno) com extração de texto assíncrona.
- [x] **F05.07** Painel lateral retrátil de 4 abas: Contexto Ativo, Arquivos do Projeto, Diferenças (Diff) e Terminal.
- [x] **F05.08** Versionamento de Artefatos no banco SQLite: documentos longos gerados pelo modelo viram artefatos versionados com controle de concorrência e rollback.
- [x] **F05.09** Operações de conversa: renomear título no menu com salvamento por `Enter`, copiar mensagem inteira e arquivamento individual.
- [x] **F05.10** Gerenciador de comandos de prompt personalizados com substituição de argumentos `{{args}}` no compositor.
- [~] **F05.11** Suporte a upload e prévia visual direta de imagens (`.png`, `.jpg`, `.webp`) no chat para modelos multimodais. *Código na branch `claude/kind-bohr-j6hi1g`, compilado e com a suíte existente verde no Linux, sem testes novos (pausa de 21/10/2026). Servidor: `POST /chat/imagens` confere a assinatura (PNG/JPEG/WebP), limite de 10 MB, guarda só na memória e `/chat` aceita `imagens`. App: anexo, miniatura e envio. Não validado na janela real.*
- [~] **F05.12** Suporte a anexos de áudio no chat com envio para transcrição Whisper assíncrona. *Código na branch `claude/kind-bohr-j6hi1g`, compilado e com a suíte existente verde no Linux, sem testes novos (pausa de 21/10/2026). Servidor: `POST /chat/audios` responde na hora e transcreve em segundo plano (Whisper local, ffmpeg para converter; Groq só com chave); o texto entra como anexo. App: anexo de áudio com espera de até 10 min. Não validado com áudio real.*

---

### F06: Vault Obsidian e Memória Contínua
- [x] **F06.01** Detecção automática de vaults existentes do Obsidian no disco (`.obsidian/`) ou configuração manual de caminho.
- [x] **F06.02** Parser e validador de notas Markdown: garantia de frontmatter YAML estruturado e integridade de wikilinks (`[[Nota]]`).
- [x] **F06.03** Scanner prévio de credenciais com recusa fail-closed: notas que contenham chaves de API, senhas ou tokens privados são bloqueadas de gravação automática.
- [x] **F06.04** Gravação atômica via Git plumbing (`hash-object`, `write-tree`, `commit-tree`): commits são criados sobre a `main` sem interferir em alterações `staged` do usuário no terminal.
- [x] **F06.05** Tratamento não-destrutivo de divergências: se uma nota do vault foi editada externamente no Obsidian, o Orion gera uma proposta `- conflito Orion <hash>.md`, preservando a original.
- [x] **F06.06** Sincronização remota defensiva: o `push` só é emitido se o commit pai já estiver na `main` do GitHub; o `pull` só é feito por fast-forward com árvore de trabalho limpa.
- [~] **F06.07** Gravação automática contínua de turnos relevantes no vault como diário de sessão (atualmente requer chamada de rota). *Código na branch `claude/kind-bohr-j6hi1g`, compilado e com a suíte existente verde no Linux, sem testes novos (pausa de 21/10/2026). Diário de sessão em `03 Recursos/Orion/Memória/` depois de cada resposta, só com `ORION_VAULT_DIARIO=true`; sem commit nem push; atualiza só a nota que o Orion mesmo gravou, e se você a editou a versão nova vira proposta e o diário daquela conversa para.*
- [~] **F06.08** Interface visual integrada no chat para comparação lado a lado de versões conflitantes e mesclagem assistida. *Código na branch `claude/kind-bohr-j6hi1g`, compilado e com a suíte existente verde no Linux, sem testes novos (pausa de 21/10/2026). Rotas `/vault/conflitos*` e tela de comparação em Configurações › Memória: original ao lado da proposta, campo de texto final, descartar ou salvar. Recusa se a original mudou. Não validado na janela real.*

---

### F07: Configurações Completas e Perfil do Usuário
- [x] **F07.01** Modal de Configurações acessível globalmente via `Ctrl+,` ou `/config` com 15 seções organizadas em lista lateral.
- [x] **F07.02** Localização completa via catálogos Fluent (`pt-BR` e `en`) abrangendo todos os rótulos, botões, modais e mensagens de erro do sistema.
- [x] **F07.03** Gestão de modelos e chaves na seção Modelos: cadastro, teste de conectividade e seleção de fallback.
- [x] **F07.04** Gestão de Atalhos de teclado com detecção de colisões e redefinição aos padrões.
- [x] **F07.05** Painel de métricas operacionais embutido na seção Painel: acompanhamento de cotas, status de armazenamento e memória.
- [x] **F07.06** Seção de Fontes tipográficas permitindo personalização de tamanhos de corpo e código.
- [x] **F07.07** Persistência atômica de preferências no arquivo de perfil criptografado.
- [~] **F07.08** Recarga a quente (*hot-reload*) de todas as configurações alteradas sem necessidade de reiniciar o executável `orion.exe`. *Código na branch `claude/kind-bohr-j6hi1g`, compilado e com a suíte existente verde no Linux, sem testes novos (pausa de 21/10/2026). `POST /config/recarregar` + botão em Sistema: aplica na hora provedores/gateway, guarda de injeção e diário; o limite de resumo da conversa (`ORION_COMPACT_AT`, se já nasceu ligado); o modelo local embutido (`ORION_LOCAL_MODEL*`), os modelos por camada (rápido, pesado, visão) e o resto vêm listados em `exigem_reinicio`. Hoje as demais ~140 opções ainda exigem reiniciar.*

---

### F08: Gestão de Projetos e Isolamento
- [x] **F08.01** Criação básica de projetos via formulário nativo com nome, instruções de contexto e diretório local opcional.
- [x] **F08.02** Isolamento rígido de contexto: memórias, fatos consolidados e documentos de um projeto não são expostos em outros projetos nem na memória pessoal geral.
- [x] **F08.03** Configurações específicas por projeto: definição de endpoint de modelo preferencial e número máximo de passos por turno.
- [x] **F08.04** Associação de até 8 documentos de referência por projeto, relidos automaticamente como premissas em novos chats.
- [x] **F08.05** Arquivamento de conversas vinculado ao projeto com histórico de restauração.
- [~] **F08.06** Criação automática da nota raiz do projeto no vault (`01 Projetos/<Nome>/<Nome>.md`) no momento da criação do projeto. *Código na branch `claude/kind-bohr-j6hi1g`, compilado e com a suíte existente verde no Linux, sem testes novos (pausa de 21/10/2026). `POST /projects` cria `01 Projetos/<Nome>/<Nome>.md` (nunca sobrescreve; falha vira aviso na resposta); `GET /projects` traz `vault_note` e o cartão do projeto mostra a etiqueta "Nota no vault".*
- [~] **F08.07** Clonagem automática de repositórios remotos do GitHub ao criar projeto associado a uma URL Git. *Código na branch `claude/kind-bohr-j6hi1g`, compilado e com a suíte existente verde no Linux, sem testes novos (pausa de 21/10/2026). `POST /projects` aceita `git_url` (só `https://github.com/<dono>/<repo>` público): clone raso em `<dados>/projetos/`, sem hooks, sem credencial, bloqueado no pânico e registrado em `external_calls`. A tela de criar projeto tem o campo da URL (cena `84-projeto-novo` do `orion-snap`, só renderizada no Linux).*

---

### F09: Painel de Controle, Métricas e Auditoria
- [x] **F09.01** Agregação local diária de eventos operacionais: total de turnos, ferramentas executadas, arquivos modificados e chamadas de rede.
- [x] **F09.02** Auditoria persistida em SQLite: tabela de auditoria registrando autorizações concedidas e negadas sem vazamento de segredos.
- [x] **F09.03** Tabela `external_calls` registrando métricas de chamadas a provedores (latência, bytes enviados/recebidos, HTTP status) sem armazenar payloads sensíveis.
- [~] **F09.04** Gráficos nativos em canvas vetorial para consumo semanal e mensal de tokens e cotas. *Código na branch `claude/kind-bohr-j6hi1g`, compilado e com a suíte existente verde no Linux, sem testes novos (pausa de 21/10/2026). Painel: tokens por dia em 7 e 30 dias (canvas). **Estimado** (bytes ÷ 4): o registro de saídas não guarda tokens.*
- [~] **F09.05** Geração e exportação do relatório consolidado diário "Você Fez" no final do expediente (23:59). *Código na branch `claude/kind-bohr-j6hi1g`, compilado e com a suíte existente verde no Linux, sem testes novos (pausa de 21/10/2026). Job das 23:59 guarda o texto (`orion-core/src/relatorio.rs`); `GET /painel/voce-fez` e `POST /painel/voce-fez/exportar` (grava no vault, sem sobrescrever, um arquivo por dia e por projeto); botão no Painel.*

---

### F10: Instalador, Onboarding, Atualizador e Release
- [x] **F10.01** Instalador nativo `Orion Setup.exe` com janela de tamanho fixo 960×600, cantos arredondados e sem moldura padrão de sistema.
- [x] **F10.02** Renderizador procedural da constelação em `orion-sky`: céu com 60 fps adaptativo, bloom vetorial, linhas de constelação e suporte à opção "Reduzir movimento" do Windows (4 fps).
- [x] **F10.03** Trilha sonora ambiente instrumental gerada via osciladores em `cpal` com fade-out gradual ao fechar a janela.
- [x] **F10.04** Solicitação nativa de elevação UAC (`verb: "runas"`) antes de gravar arquivos em `C:\Program Files\Orion` ou no desinstalador.
- [x] **F10.05** Criação de pacote esparso MSIX com manifesto de identidade do aplicativo para integração moderna com o Windows 11.
- [x] **F10.06** Desinstalador nativo com opções desmarcadas por padrão para remoção de modelos locais e dados, com garantia absoluta de que o vault do Obsidian nunca é tocado.
- [x] **F10.07** Assistente de primeiro uso (Onboarding) em 7 etapas com acendimento progressivo das 7 estrelas de Orion.
- [x] **F10.08** Atualizador de sistema `orion update`: verificação de feeds assinados via Minisign + SHA-256, download em pasta `.staged/` e troca atômica de binários ao encerrar o app.
- [x] **F10.09** Tela de novidades pós-atualização `--whats-new` apresentando changelog formatado com fundo procedural.
- [ ] **F10.10** Homologação física completa no Windows: instalação limpa do pacote release em máquina de teste, primeiro uso completo, atualização e desinstalação.
- [ ] **F10.11** Publicação da primeira release oficial assinada no GitHub Releases com chaves privadas Minisign do Antônio.

---

### Rodada R1: Estabilização de Hardware Real (Windows 10/11)
- [x] **R1.01** Provedores dinâmicos em `meta.providers:configuration`: recarga em tempo real de credenciais recém-adicionadas sem necessidade de reiniciar.
- [x] **R1.02** Otimização de renderização da interface: seleção inteligente entre `wgpu` (GPU física) e `tiny-skia` (emulação por CPU sem MSAA para evitar travamentos em WARP).
- [x] **R1.03** Barra de controles de janela compartilhada uniformemente entre telas iniciais, login, onboarding e janela principal.
- [x] **R1.04** Validação segura de perfil no login inicial com suporte a perfis legados sem migração destrutiva.
- [x] **R1.05** Rotação e mascaramento aprimorado de arquivos de log com botão na UI para abrir pasta local no Explorer.
- [x] **R1.06** Descarte gracioso de recursos gráficos na saída do aplicativo, prevenindo avisos de depuração do DirectX 12 / Vulkan.
- [x] **R1.07** Tela de inicialização (Splash) exibida durante a derivação de chaves criptográficas Argon2id e abertura do banco SQLCipher.
- [x] **R1.08** Redução de carga gráfica no repouso: suspensão total de redesenho (*ticks*) quando a janela está minimizada, em segundo plano ou sem interação.
- [~] **R1.09** Diagnóstico e correção definitiva da borda branca periférica observada em telas com aceleração gráfica WARP. *Código na branch `claude/kind-bohr-j6hi1g`; compila (para o Windows só até a checagem de tipos, sem rodar). **Sem aceite na máquina real.** Mesma correção do F01.25 (`janela.rs`).*
- [x] **R1.10** Prevenção de sessões vazias no chat: a conversa só é persistida no banco após o envio da primeira mensagem.
- [x] **R1.11** Documentação atualizada do pacote de release (`LEIA-ME.txt`) distinguindo versão portátil e instalador assistido.
- [~] **R1.12** Aceite físico dos atalhos globais de teclado (`Ctrl+Alt+Espaço`, `Ctrl+Alt+P`) e ícone de bandeja no Windows.
- [x] **R1.13** Tradução integral da interface para português brasileiro e inglês utilizando Fluent.
- [x] **R1.14** Extração assíncrona de anexos no chat com timeout de 120 s e barreira de contenção contra perda de foco da conversa.
- [~] **R1.15** Medição e homologação do consumo de recursos em repouso no notebook físico (< 1% CPU, < 120 MB RAM).

---

## 3. Especificação Completa de Design e Interface da v1

### 3.1 Tokens de Cores e Estilo Visual (Tema Grafite)
A interface adota a estética minimalista e técnica, inspirada em editores de código modernos e no cliente desktop do Claude, mantendo a identidade visual astronômica do Orion.

| Token | Valor Hex | Aplicação no Sistema |
|---|---|---|
| `bg-primary` | `#0C0E13` | Fundo principal da janela, áreas externas de modais |
| `bg-surface` | `#14171F` | Superfície da barra lateral, cards, cabeçalhos de tabela |
| `bg-elevated` | `#1A1E29` | Modais flutuantes, popovers, balões de mensagem do Orion |
| `bg-user-msg` | `#222838` | Balões de mensagem enviados pelo usuário |
| `border-subtle` | `#1E2330` | Divisórias estruturais, separadores de abas, contorno de inputs |
| `border-focus` | `#3B82F6` | Anel de foco acessível em campos de texto, bordas de seleção ativa |
| `text-primary` | `#F3F4F6` | Títulos, corpo de mensagens, rótulos principais |
| `text-secondary` | `#9CA3AF` | Metadados, subtítulos, legendas, texto auxiliar |
| `text-muted` | `#6B7280` | Placeholders, timestamps, rodapés |
| `accent-rigel` | `#60A5FA` | Azul estelar para ações primárias, links, switches ligados |
| `accent-betelgeuse` | `#F97316` | Âmbar para alertas, avisos de cota atingida, aprovações pendentes |
| `accent-danger` | `#EF4444` | Ações destrutivas, erros de execução, botão de pânico |

### 3.2 Topografia e Fontes
- **Família de Texto Principal:** `Geist` (ou fallback sem serifa do sistema: `Segoe UI`, `SF Pro Display`).
- **Família de Código e Terminal:** `Geist Mono` (ou fallback monoespaçado: `Consolas`, `Cascadia Code`).
- **Família Editorial/Destaque:** `Newsreader` para citações clássicas e reflexões da persona.
- **Tamanhos e Pesos:**
  - `Hero / Título de Página:` 24 px, peso 600 (Semibold), line-height 32 px.
  - `Seção / Cabeçalho:` 18 px, peso 600, line-height 24 px.
  - `Corpo / Mensagens:` 14 px, peso 400 (Regular), line-height 20 px.
  - `Legendas / Badges:` 11 px, peso 500 (Medium), line-height 14 px.

### 3.3 Componentes Estruturais da v1
1. **Janela e Topbar:**
   - Altura fixa de 40 px, sem borda de SO. Contém o ícone estilizado do Orion à esquerda, título da conversa atual ao centro (truncado em 48 caracteres com tooltip) e controles de janela à direita (minimizar, restaurar/maximizar, fechar).
2. **Barra Lateral Retrátil (Sidebar):**
   - Largura expandida de 240 px, largura recolhida de 56 px. Botão no topo para recolher (`Ctrl+B`).
   - Lista vertical de itens com ícones SVG vetoriais e rótulos internacionalizados.
   - Badge numérico indicador de aprovações pendentes em laranja Betelgeuse.
3. **Área de Chat e Compositor:**
   - Lista virtualizada de mensagens rolando suavemente, mantendo histórico recente em memória e carregamento sob demanda ao subir a rolagem.
   - Campo de entrada com auto-crescimento vertical de 40 px até 180 px.
   - Botões de anexo, modo de raciocínio, seleção de projeto e botão de envio atômico.
4. **Painel Lateral Contextual (Direita):**
   - Largura fixa de 340 px com 4 abas: *Contexto*, *Arquivos*, *Diff* e *Terminal*. O botão de fechar oculta o painel, expandindo a área do chat.
5. **Grafo da Memória Orgânico 2D:**
   - Renderização via `canvas` vetorial do iced utilizando layout de forças multiescala. Nós representam notas do vault e fatos; arestas representam wikilinks e relações causais. Filtro rápido de vizinhança para 1 ou 2 saltos.

---

## 4. Especificação do Instalador e Atualizador (v1)

### 4.1 Janela e Comportamento
- Janela de dimensões estritas **960 × 600 px**, centralizada na tela no primeiro disparo, sem suporte a redimensionamento para garantir integridade visual quadro a quadro.
- Som ambiente executado através de 4 osciladores em síntese senoidal pura com filtro passa-baixas em 900 Hz, volume fixado em `0.028` para sensação imersiva suave, sem botão de interrupção (especificação original).
- Constelação de Orion desenhada em tempo real: estrelas centrais Betelgeuse, Bellatrix, Saiph, Rigel e o Cinturão (Alnitak, Alnilam, Mintaka) calculadas com dispersão de brilho (*bloom*).

### 4.2 Fluxo de Instalação em 5 Etapas
```
[1. Boas-vindas e Checagem] ─► [2. Seleção de Pastas] ─► [3. Seleção de Modelos] ─► [4. Opções do Sistema] ─► [5. Instalação e Conclusão]
```
1. **Etapa 1: Boas-vindas:** Detecção do sistema (Windows 10/11 x64), verificação de memória RAM instalada, placa gráfica disponível e espaço livre nos discos.
2. **Etapa 2: Pastas:** Sugestão de instalação em `C:\Program Files\Orion` (requer UAC) e seleção do diretório de armazenamento de modelos locais (sugerindo o disco com maior espaço livre).
3. **Etapa 3: Modelos Locais:** Apresentação de cartões dos modelos do catálogo (SmolLM, Qwen, Whisper) com indicação "Recomendado para o seu PC". Modelos iniciam desmarcados por padrão.
4. **Etapa 4: Opções do Sistema:** Caixas de marcação para: *Iniciar junto com o Windows*, *Criar atalho no Menu Iniciar* e *Registrar manipulador de protocolo orion://*.
5. **Etapa 5: Execução da Instalação:**
   - Elevação prévia de privilégios via UAC.
   - Cópia dos executáveis (`orion.exe`, `orion-llama.exe`).
   - Registro de Desinstalação no Painel de Controle do Windows (`HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\Orion`).
   - Registro do pacote esparso MSIX para suporte a notificações nativas.
   - Conclusão com acendimento total da constelação e botão "Abrir Orion".

### 4.3 Desinstalador
- Disparado via `Orion Setup.exe --uninstall "<pasta>"`.
- Apresenta janela dedicada com 3 opções claras:
  - `[x]` Remover executáveis e arquivos de sistema do Orion (obrigatório).
  - `[ ]` Remover modelos de IA locais baixados (opcional, desmarcado por padrão).
  - `[ ]` Remover banco de dados e logs (opcional, desmarcado por padrão).
- **Regra Imutável:** O diretório do Vault do Obsidian **NUNCA** é tocado pelo desinstalador, sob nenhuma circunstância.

---

## 5. Guia Passo a Passo de Implementação para os Itens Pendentes da v1

Para cada uma das pendências mapeadas para o lançamento da v1, este guia define a arquitetura, arquivos envolvidos, estruturas de código, lógica de execução e critérios de validação.

---

### Passo 1: Aceite Físico do Windows, UAC e Instalação Limpa (F10.10, R1.12, F01.06)
- **Objetivo:** Validar a instalação, elevação de UAC, persistência de atalhos e inicialização única em um ambiente Windows 10/11 limpo.
- **Crates e Arquivos Afetados:**
  - `crates/orion-setup/src/elevation.rs`
  - `crates/orion-setup/src/register.rs`
  - `crates/orion-platform/src/windows.rs`
  - `crates/orion-server/src/appmode.rs`
- **Passo a Passo de Implementação:**
  1. *Elevação de UAC:* Em `elevation.rs`, garantir que a chamada a `ShellExecuteExW` com verbo `runas` preserve integralmente o diretório atual de trabalho e argumentos passados com caracteres Unicode. Se o usuário recusar o prompt UAC, capturar o erro `ERROR_CANCELLED (1223)` e exibir mensagem amigável sem fechar bruscamente o instalador.
  2. *Instância Única:* Em `appmode.rs`, criar um mutex nomeado global ou arquivo de trava `%APPDATA%\Orion\orion.lock`. Na inicialização, se a trava já existir e o processo estiver vivo, emitir mensagem através de Named Pipe ou evento Win32 (`RegisterWindowMessageW("ORION_SHOW_WINDOW")`) para restaurar o foco da janela existente e encerrar o segundo processo imediatamente com código `0`.
  3. *Atalhos e Registro:* Em `register.rs`, validar se a criação do atalho `.lnk` no Menu Iniciar (`%APPDATA%\Microsoft\Windows\Start Menu\Programs\Orion.lnk`) via interface COM `IShellLinkW` define corretamente o ícone e a descrição sem dependências externas.
- **Critério de Aceite:**
  - Rodar o instalador sem privilégios de administrador em máquina real; o Windows deve exibir o diálogo de confirmação UAC.
  - Tentar abrir o `orion.exe` duas vezes seguidas; a primeira janela deve vir para o primeiro plano e nenhuma segunda instância deve permanecer na memória.

---

### Passo 2: Diagnóstico e Eliminação da Faixa Branca de Janela (F01.25, R1.09)
- **Objetivo:** Eliminar a borda branca ou linha artefatada observada no contorno da janela quando executada sob adaptadores de vídeo genéricos ou escala de DPI fracionária.
- **Crates e Arquivos Afetados:**
  - `crates/orion-app/src/window.rs`
  - `crates/orion-app/src/main.rs`
- **Passo a Passo de Implementação:**
  1. Em `window.rs`, inspecionar as configurações passadas para o builder do `iced::window::Settings`:
     ```rust
     iced::window::Settings {
         decorations: false,
         transparent: true,
         resizable: true,
         ..Default::default()
     }
     ```
  2. No Windows com DWM, janelas sem moldura (`decorations: false`) exigem que as margens do cliente DWM sejam estendidas ou que a sombra padrão seja desativada (`undecorated_shadow: false`), caso contrário o Windows renderiza uma borda de 1 px de cor branca nos cantos superiores.
  3. Adicionar em `orion-platform/src/windows.rs` uma chamada explícita para `DwmExtendFrameIntoClientArea` com margens `-1` ou definir o atributo de janela `DWMWA_BORDER_COLOR` para o valor hex `#0C0E13` (preto Grafite).
- **Critério de Aceite:**
  - Abrir a aplicação em uma máquina Windows configurada em 100%, 125% e 150% de escala DPI; nenhum pixel branco deve aparecer ao redor da moldura.

---

### Passo 3: Criação da Nota de Projeto no Vault e Vínculo Git (F08.06, F08.07)
- **Objetivo:** Ao criar um projeto na interface do Orion, criar automaticamente sua estrutura correspondente no vault do Obsidian e vincular o repositório Git local.
- **Crates e Arquivos Afetados:**
  - `crates/orion-core/src/projects.rs`
  - `crates/orion-vault/src/sync.rs`
  - `crates/orion-app/src/views/projects.rs`
- **Passo a Passo de Implementação:**
  1. *Geração da Nota:* Quando `POST /projects` for concluído com sucesso no banco, chamar `VaultManager::create_project_note(nome, instrucoes, pasta_vinculada)`.
  2. *Estrutura da Nota:* A nota deve ser gravada no caminho do vault `01 Projetos/<Nome>/<Nome>.md` com o frontmatter obrigatório:
     ```yaml
     ---
     type: projeto
     status: ativo
     tags: [orion, projeto]
     data: AAAA-MM-DD
     ---
     ```
  3. *Vínculo Git:* Se o projeto possuir uma pasta vinculada que contenha um repositório Git (`.git`), inicializar um watcher leve de branches ou ler o estado do `HEAD` via `git2::Repository::open`.
  4. *Exibição no Front:* Em `views/projects.rs`, renderizar o status da nota do vault com ícone indicando sincronização ativa.
- **Critério de Aceite:**
  - Criar um novo projeto pelo aplicativo; abrir o Obsidian e verificar que a nota `01 Projetos/<Nome>/<Nome>.md` foi criada imediatamente e sem conflitos.

---

### Passo 4: Integração de Anexos Visuais e Multimodais no Chat (F05.11, F05.12)
- **Objetivo:** Permitir o envio de capturas de tela e imagens (`.png`, `.jpg`) diretamente na conversa, com prévia em miniatura e envio aos modelos com capacidade visual.
- **Crates e Arquivos Afetados:**
  - `crates/orion-app/src/views/chat.rs`
  - `crates/orion-core/src/agent.rs`
  - `crates/orion-ai/src/gateway.rs`
- **Passo a Passo de Implementação:**
  1. *Seleção do Arquivo:* No componente de chat, estender o seletor de arquivos para aceitar filtros de imagens (`image/png`, `image/jpeg`).
  2. *Armazenamento Temporário:* Salvar a imagem selecionada em `%APPDATA%\Orion\capturas\chat_<id>.png` e gerar miniatura em memória para exibição acima do campo de entrada.
  3. *Codificação Base64 e Gateway:* No gateway `orion-ai`, formatar a requisição multimodal para o padrão do provedor (Gemini `inlineData` ou OpenAI `image_url` com codificação base64).
  4. *Segurança e Validação:* Confirmar que a imagem não ultrapassa 10 MB e que os bytes correspondem a assinaturas válidas de imagens (magic bytes PNG `89 50 4E 47`, JPEG `FF D8 FF`).
- **Critério de Aceite:**
  - Arrastar ou selecionar uma imagem PNG no chat; a miniatura deve aparecer no compositor e o modelo Gemini 3.5 Flash-Lite deve descrever o conteúdo da imagem com precisão.

---

### Passo 5: Geração de Chaves Minisign e Publicação do Release Assinado (F10.11)
- **Objetivo:** Estabelecer a infraestrutura de assinatura digital criptográfica para o atualizador automático do Orion.
- **Arquivos Afetados:**
  - `.github/workflows/release.yml`
  - `crates/orion-setup/release/minisign.pub`
  - `crates/orion-updater/src/lib.rs`
- **Passo a Passo de Implementação:**
  1. *Geração das Chaves:* O Antônio deve gerar o par de chaves Minisign localmente em seu terminal seguro:
     ```sh
     minisign -G -p minisign.pub -s minisign.key
     ```
  2. *Publicação da Chave Pública:* Gravar o conteúdo de `minisign.pub` em `crates/orion-setup/release/minisign.pub`.
  3. *Segredos no GitHub:* Cadastrar os segredos no repositório GitHub (`Settings > Secrets and variables > Actions`):
     - `MINISIGN_SECRET_KEY`: Conteúdo da chave privada `minisign.key`.
     - `MINISIGN_PASSWORD`: Senha utilizada para proteger a chave privada.
  4. *Workflow de Release:* No `.github/workflows/release.yml`, garantir que após a compilação do executável `orion.exe` e `orion-llama.exe`, o Minisign seja invocado para assinar os arquivos e gerar o manifesto assinado `latest-cpu.json.minisig`.
- **Critério de Aceite:**
  - Disparar a tag `v1.0.0`; o release deve conter os binários, as assinaturas `.minisig` e o atualizador `orion update --check` deve validar o feed com sucesso.

---

### Passo 6: Integração de Autenticação com Windows Hello (F02.08)
- **Objetivo:** Permitir o desbloqueio rápido do aplicativo usando leitor de impressão digital ou reconhecimento facial do Windows Hello em hardware compatível.
- **Crates e Arquivos Afetados:**
  - `crates/orion-platform/src/windows/hello.rs`
  - `crates/orion-app/src/views/login.rs`
  - `crates/orion-core/src/auth.rs`
- **Passo a Passo de Implementação:**
  1. *Checagem de Disponibilidade:* Em `hello.rs`, invocar assincronamente a API WinRT:
     ```rust
     windows::Security::Credentials::UI::UserConsentVerifier::CheckAvailabilityAsync()
     ```
  2. *Solicitação de Consentimento:* Se o status retornado for `UserConsentVerifierAvailability::Available`, exibir o botão "Entrar com Windows Hello" na tela de login.
  3. *Liberação da Chave no Cofre:* Ao clicar no botão, disparar `RequestVerificationAsync("Desbloquear Orion")`. Em caso de sucesso (`UserConsentVerificationResult::Verified`), recuperar a senha mestra ou chave embrulhada salva no Windows Credential Manager vinculada ao usuário verificado e inicializar o banco de dados.
- **Critério de Aceite:**
  - Em um notebook com sensor biométrico, clicar no botão de biometria; o diálogo oficial do Windows Hello deve surgir e liberar a interface após toque do dedo.

---

## 6. Critérios Finais de Aceite da Versão 1.0 (Checklist de Homologação)

Antes de declarar a v1 pronta para uso pessoal diário, os seguintes 12 testes integrados devem passar sem nenhuma ressalva no computador do Antônio:

- [ ] **T01 — Instalação Limpa com UAC:** O `Orion Setup.exe` roda em máquina Windows sem instalação prévia, solicita elevação UAC, exibe o céu animado com som ambiente, copia os arquivos para `C:\Program Files\Orion` e cria os atalhos no Menu Iniciar.
- [ ] **T02 — Onboarding em 7 Etapas:** O assistente de primeiro uso guia a criação de senha em Argon2id, detecta o vault do Obsidian, configura a chave do Google AI Studio e gera o backup inicial.
- [ ] **T03 — Primeiro Diálogo Real no Chat:** Envio de mensagem ("Olá Orion") respondido com streaming suave pelo `gemini-3.5-flash-lite`, exibindo formatação Markdown correta e latência de primeiro token inferior a 1,5 s.
- [ ] **T04 — Anexo e Leitura de Documento:** Envio de arquivo PDF de 2 MB no chat com extração de texto bem-sucedida e resposta do modelo baseada nas premissas do documento.
- [ ] **T05 — Aprovação e Segurança de Execução:** Solicitação de comando que acione ferramenta de execução ou escrita; o sistema exibe o cartão de aprovação com argumentos completos, aguardando clique explícito no botão Aprovar.
- [ ] **T06 — Gravação Defensiva no Vault:** Solicitação de gravação de nota no vault; o arquivo é criado em `01 Projetos/Orion/` com frontmatter correto, sem vazar segredos e sincronizado via Git sem alterar staged do usuário.
- [ ] **T07 — Navegação e Atalhos:** Alternância entre janelas usando `Ctrl+B` (barra lateral), `Ctrl+K` (paleta), navegação direta por `/config atalhos` e zoom via `Ctrl+` e `Ctrl-`.
- [ ] **T08 — Criptografia e Bloqueio de Sessão:** Trava do Windows via `Win+L`; o aplicativo esconde dados da memória e exige senha/Hello ao retornar.
- [ ] **T09 — Estabilidade do Worker Local:** Acionamento de tarefa com modelo local; o processo `orion-llama.exe` executa dentro do limite de 30% da memória RAM física e é finalizado sem vazamento de processos no Gerenciador de Tarefas.
- [ ] **T10 — Execução em Segundo Plano e Bandeja:** Fechamento da janela pelo "X" esconde o aplicativo na bandeja do sistema; clique duplo no ícone restaura a janela instantaneamente.
- [ ] **T11 — Atualização Automática Silenciosa:** Simulação de novo feed assinado; o app baixa o binário para `.staged/` e realiza a troca atômica no fechamento.
- [ ] **T12 — Desinstalação Sem Perda de Dados:** Execução do desinstalador remove os binários do aplicativo e registros de sistema, preservando integralmente o diretório do Obsidian.
