//! Dados de exemplo do protótipo (ORION - UI (ESPECIFICAÇÃO), §28.1). No app final vêm do núcleo.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ConvState {
    Normal,
    Running,
    Pending,
    Unread,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Origin {
    Rotina,
    Telegram,
}

#[derive(Clone, Debug)]
pub struct Note {
    pub path: &'static str,
    pub at: &'static str,
    pub commit: &'static str,
}

#[derive(Clone, Debug)]
pub struct Tool {
    pub kind: &'static str,
    pub value: &'static str,
    pub time: &'static str,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ApprState {
    Open,
    Approved(String),
    Denied(String),
}

#[derive(Clone, Debug)]
pub struct Approval {
    /// Marcação mínima: `código` entre crases vira mono.
    pub text: &'static str,
    pub state: ApprState,
    pub diff_button: bool,
}

#[derive(Clone, Debug)]
pub struct OrionMsg {
    pub time: &'static str,
    pub via: &'static str,
    pub out: &'static str,
    pub tools_label: &'static str,
    pub tools: Vec<Tool>,
    /// Corpo em marcação mínima (ver `md.rs`).
    pub body: &'static str,
    pub saved: bool,
    pub approval: Option<Approval>,
}

#[derive(Clone, Debug)]
pub enum ChatMsg {
    User(&'static str),
    Orion(OrionMsg),
}

#[derive(Clone, Debug)]
pub struct Conv {
    pub id: &'static str,
    pub title: &'static str,
    pub proj: &'static str,
    pub origin: Option<Origin>,
    pub day: &'static str,
    pub state: ConvState,
    pub ctx: u32,
    pub branch: Option<&'static str>,
    pub pinned: bool,
    pub note: Option<Note>,
    pub pin: Option<&'static str>,
    pub banner: Option<&'static str>,
    pub msgs: Vec<ChatMsg>,
}

pub const RAM_BODY: &str = "Medi com o Orion parado por 10 minutos. O pico vem de três lugares, e nenhum deles precisa ficar residente o tempo todo:\n\n1. **Índice vetorial carregado inteiro** na inicialização: 610 MB. Dá para abrir com `mmap` e deixar o sistema paginar.\n2. **Modelo de voz residente**: 280 MB, mesmo com a voz desligada. Carregar só quando a palavra “Orion” estiver ativa.\n3. **Cache de embeddings sem limite**: cresce sem parar. Limitar a 64 MB com LRU.\n\n```rust\n// src/memory/index.rs\nlet file = File::open(&path)?;\nlet map = unsafe { Mmap::map(&file)? };\nlet index = VectorIndex::from_bytes(&map, 1024)?;\n```\n\nEstimativa: de **1,2 GB** para cerca de **300 MB** parado, sem perder velocidade na busca.";

pub fn convs() -> Vec<Conv> {
    use ChatMsg::*;
    let tool = |kind, value, time| Tool { kind, value, time };
    vec![
        Conv {
            id: "ram",
            title: "Orion RAM otimização",
            proj: "Orion",
            origin: None,
            day: "Hoje",
            state: ConvState::Normal,
            ctx: 30,
            branch: Some("claude/funny-hamilton-jb1svh"),
            pinned: true,
            note: Some(Note { path: "03 Recursos/Orion/Memória/2026-10-15 - Orion RAM otimização.md", at: "09:13", commit: "a3f9c12" }),
            pin: Some("Meta do projeto: RAM parada abaixo de 300 MB"),
            banner: None,
            msgs: vec![
                User("O Orion está usando 1,2 GB parado. Dá pra baixar sem perder velocidade?"),
                Orion(OrionMsg {
                    time: "09:12",
                    via: "Claude Sonnet",
                    out: "Anthropic (Claude Code CLI): pergunta + trechos de 3 arquivos .rs",
                    tools_label: "Leu 4 arquivos, executou 1 comando",
                    tools: vec![
                        tool("Comando", "cargo bloat --release --crates", "2,1 s"),
                        tool("Leu", "src/memory/index.rs", "0,1 s"),
                        tool("Leu", "src/voice/wake.rs", "0,1 s"),
                        tool("Leu", "src/embed/cache.rs", "0,1 s"),
                        tool("Vault", "Orion · Orion - Arquitetura · Rust", "0,2 s"),
                    ],
                    body: RAM_BODY,
                    saved: true,
                    approval: Some(Approval {
                        text: "Aplicar as três mudanças na branch `claude/funny-hamilton-jb1svh` e rodar `cargo test`. Escreve 3 arquivos no projeto Orion.",
                        state: ApprState::Open,
                        diff_button: true,
                    }),
                }),
            ],
        },
        Conv {
            id: "rust",
            title: "Orion para Rust",
            proj: "Orion",
            origin: None,
            day: "Hoje",
            state: ConvState::Running,
            ctx: 54,
            branch: Some("claude/rust-port"),
            pinned: false,
            note: Some(Note { path: "03 Recursos/Orion/Memória/2026-10-15 - Orion para Rust.md", at: "08:41", commit: "77be0d4" }),
            pin: None,
            banner: Some("Groq sem cota; usando Gemini."),
            msgs: vec![
                User("Qual biblioteca de interface faz mais sentido para o front em Rust?"),
                Orion(OrionMsg {
                    time: "08:40",
                    via: "Orion · Groq",
                    out: "Groq: pergunta, sem dados pessoais",
                    tools_label: "",
                    tools: vec![],
                    body: "Para um app de desktop com barra de título própria, atalhos e lista longa de conversas, o **iced** é o mais próximo do que o front atual faz: arquitetura Elm, estilo por tema e boa renderização de texto.\n\nO grafo da memória vira um widget `shader` (wgpu) em 3D, com o céu do `sky.js` ao fundo.",
                    saved: true,
                    approval: None,
                }),
            ],
        },
        Conv {
            id: "plano",
            title: "Plano rodada 2",
            proj: "Orion",
            origin: None,
            day: "Hoje",
            state: ConvState::Pending,
            ctx: 41,
            branch: Some("claude/quirky-dijkstra-65d6gq"),
            pinned: false,
            note: Some(Note { path: "03 Recursos/Orion/Memória/2026-10-15 - Plano rodada 2.md", at: "07:59", commit: "c41d2aa" }),
            pin: None,
            banner: None,
            msgs: vec![
                User("Qual o status da reescrita e o que falta para a fase 6?"),
                Orion(OrionMsg {
                    time: "07:58",
                    via: "Gemini Flash",
                    out: "Google (Gemini CLI): pergunta + Orion - Estado e plano.md",
                    tools_label: "",
                    tools: vec![],
                    body: "A fase 6 tem o front redesenhado e a voz por clique. Falta validar a palavra de ativação com microfone real.\n\n- Voz por clique: pronta no código.\n- Voz ao vivo: pronta, sem teste de áudio.\n- Palavra “Orion”: falta testar com o seu microfone.\n\nQuer que eu envie a branch para revisão?",
                    saved: true,
                    approval: Some(Approval {
                        text: "Enviar `claude/quirky-dijkstra-65d6gq` para o origin e abrir o PR. É uma escrita fora deste computador.",
                        state: ApprState::Open,
                        diff_button: false,
                    }),
                }),
            ],
        },
        Conv {
            id: "brief",
            title: "Briefing da manhã",
            proj: "Rotina",
            origin: Some(Origin::Rotina),
            day: "Hoje",
            state: ConvState::Unread,
            ctx: 0,
            branch: None,
            pinned: false,
            note: None,
            pin: None,
            banner: None,
            msgs: vec![Orion(OrionMsg {
                time: "08:45",
                via: "Orion · Gemini",
                out: "",
                tools_label: "",
                tools: vec![],
                body: "Bom dia. Três eventos hoje, o primeiro às 10:00. Um PR esperando sua revisão no vault. Backup de ontem concluído sem erros.",
                saved: false,
                approval: None,
            })],
        },
        Conv {
            id: "merc",
            title: "Lista do mercado",
            proj: "Sem projeto",
            origin: Some(Origin::Telegram),
            day: "Ontem",
            state: ConvState::Normal,
            ctx: 0,
            branch: None,
            pinned: false,
            note: None,
            pin: None,
            banner: None,
            msgs: vec![
                User("(áudio, 0:12) põe café, leite e pão na lista"),
                Orion(OrionMsg {
                    time: "ontem, 19:02",
                    via: "Local · Qwen 7B",
                    out: "",
                    tools_label: "",
                    tools: vec![],
                    body: "Anotei na lista do mercado: café, leite e pão. Mandei a lista atualizada no Telegram.",
                    saved: false,
                    approval: None,
                }),
            ],
        },
        Conv {
            id: "vaultpr",
            title: "Revisão da PR do vault",
            proj: "Vault",
            origin: None,
            day: "Ontem",
            state: ConvState::Normal,
            ctx: 0,
            branch: None,
            pinned: false,
            note: None,
            pin: None,
            banner: None,
            msgs: vec![
                User("Revisa a PR do vault e me diz se pode fazer merge."),
                Orion(OrionMsg {
                    time: "ontem, 18:20",
                    via: "Claude Sonnet",
                    out: "Anthropic (Claude Code CLI): pergunta + diff da PR",
                    tools_label: "",
                    tools: vec![],
                    body: "Duas checagens verdes e nenhum conflito. Um ponto: a nota nova não tem link para nenhum Projeto, o que quebra a regra de “não criar ilhas” do `claude.md`.",
                    saved: false,
                    approval: None,
                }),
            ],
        },
        empty("front", "Análise do frontend Orion", "Orion", "7 dias"),
        empty("org", "Organização do vault", "Vault", "7 dias"),
        empty("nortis", "Backup e restauração", "Nortis", "Mais antigos"),
        empty("lyra", "Avaliação de RAG", "Lyra", "Mais antigos"),
    ]
}

fn empty(id: &'static str, title: &'static str, proj: &'static str, day: &'static str) -> Conv {
    Conv { id, title, proj, origin: None, day, state: ConvState::Normal, ctx: 0, branch: None, pinned: false, note: None, pin: None, banner: None, msgs: vec![] }
}

pub struct Project {
    pub n: &'static str,
    pub d: &'static str,
    pub pend: &'static str,
    pub up: &'static str,
    pub repo: &'static str,
    pub ci: bool,
    pub model: &'static str,
    pub pastas: &'static str,
    pub rot: u32,
}

pub const PROJ: &[Project] = &[
    Project { n: "Orion", d: "Assistente local em Rust. Fase 6: front e voz.", pend: "3 pendências", up: "há 2 h", repo: "antoniossalomao/Orion", ci: true, model: "Claude Sonnet", pastas: "C:\\Projetos\\Orion", rot: 1 },
    Project { n: "Vault", d: "Segundo cérebro em Obsidian, estrutura PARA.", pend: "1 PR aberto", up: "ontem", repo: "antoniossalomao/vault", ci: true, model: "Gemini Flash", pastas: "C:\\Users\\Antonio\\Vault", rot: 2 },
    Project { n: "Lyra", d: "Avaliação de RAG com o vault como base.", pend: "pausado", up: "set.", repo: "", ci: false, model: "Auto", pastas: "", rot: 0 },
    Project { n: "Nortis", d: "Backup e restauração antes de trocar de PC.", pend: "concluído", up: "ago.", repo: "", ci: false, model: "Auto", pastas: "D:\\Backup", rot: 0 },
];

pub const RES: &[(&str, &str, &str, &str, &str)] = &[
    ("Pesquisa: memória RAM desnecessária", "Índice vetorial, voz residente e cache sem limite.", "15/10", "Pesquisa", "v2"),
    ("Diagnóstico do PR do vault", "Duas checagens verdes, sem conflito.", "14/10", "Revisão", "v1"),
    ("Revisão da fase 5 de voz", "Pendente: calibrar palmas no notebook.", "12/10", "Revisão", "v3"),
];

#[derive(Clone)]
pub struct Rot {
    pub id: &'static str,
    pub t: &'static str,
    pub d: &'static str,
    pub next: &'static str,
    pub last: &'static str,
    pub st: &'static str,
    pub on: bool,
    pub news: bool,
    pub crit: bool,
    pub out: &'static str,
    pub runs: &'static str,
}

pub fn rots() -> Vec<Rot> {
    let r = |id, t, d, next, last, st, on, news, crit, out, runs| Rot { id, t, d, next, last, st, on, news, crit, out, runs };
    vec![
        r("r1", "Briefing da manhã", "Dias úteis, 08:45", "amanhã, 08:45", "hoje, 08:45", "ok", true, true, true, "Conversa · Telegram", "oooooso"),
        r("r2", "Backup do vault", "Todo dia, 14:00", "hoje, 14:00", "ontem, 14:00", "ok", true, false, true, "Conversa · nota no vault", "ooooooo"),
        r("r3", "Revisar PRs do Orion", "A cada 4 horas · projeto Orion", "hoje, 12:00", "hoje, 08:00", "falhou", true, false, false, "Conversa · Telegram", "oofoooo"),
        r("r4", "Resumo diário das conversas", "Todo dia, 23:59", "hoje, 23:59", "ontem, 23:59", "ok", true, false, false, "Nota em Memória/Diário", "ooooooo"),
        r("r5", "Organizar Downloads", "Sábado, 10:00 · pede aprovação", "sábado, 10:00", "sábado passado", "pulada", false, false, false, "Conversa", "sssssss"),
    ]
}

#[derive(Clone)]
pub struct Act {
    pub when: &'static str,
    pub text: String,
    pub kind: &'static str, // ok | warn | err
    pub label: String,
    pub undone: bool,
    pub undoable: bool,
}

pub fn acts() -> Vec<Act> {
    let a = |when, text: &str, kind, label: &str| Act { when, text: text.to_string(), kind, label: label.to_string(), undone: false, undoable: false };
    let mut v = vec![
        a("09:13", "Nota gravada no vault + commit a3f9c12", "ok", "automático"),
        a("09:12", "Leitura de src/memory/index.rs", "ok", "concluído"),
        a("09:12", "Aplicar mudanças em 3 arquivos", "warn", "aguardando"),
        a("08:00", "Rotina “Revisar PRs do Orion”: GitHub fora do ar", "err", "falhou"),
        a("07:58", "Envio da branch para o origin", "warn", "aguardando"),
        a("ontem, 23:59", "Resumo diário em Memória/Diário/2026-10-14.md", "ok", "automático"),
        a("ontem, 11:47", "Apagar rascunhos em Downloads/tmp", "err", "negada"),
    ];
    for x in v.iter_mut() {
        let t = x.text.as_str();
        x.undoable = ["Nota gravada", "Aplicar mudanças", "Organizar Downloads", "Fato corrigido", "Resumo diário", "Captura rápida"].iter().any(|k| t.contains(k)) && x.kind == "ok";
    }
    v
}

pub struct CloudTask {
    pub svc: &'static str,
    pub t: &'static str,
    pub proj: &'static str,
    pub running: bool,
    pub at: &'static str,
    pub link: &'static str,
}
pub const CLOUD: &[CloudTask] = &[
    CloudTask { svc: "Claude", t: "Portar o gateway de provedores para Rust", proj: "Orion", running: true, at: "há 12 min", link: "claude.ai/code · sessão" },
    CloudTask { svc: "Codex", t: "Testes do crate orion-ai", proj: "Orion", running: false, at: "há 1 h", link: "PR #31" },
    CloudTask { svc: "Claude", t: "Briefing (PC desligado no sábado)", proj: "Rotina", running: false, at: "sábado, 08:45", link: "rotina na nuvem" },
];

pub struct Fact {
    pub g: &'static str,
    pub t: &'static str,
    pub src: &'static str,
}
pub const FACTS: &[Fact] = &[
    Fact { g: "Preferências", t: "Respostas diretas, sem preâmbulo", src: "conversa · 12/09" },
    Fact { g: "Preferências", t: "Português, tom técnico", src: "conversa · 12/09" },
    Fact { g: "Projetos", t: "Orion: reescrita em Rust, fase 6", src: "conversa · 14/10" },
    Fact { g: "Projetos", t: "Vault: segundo cérebro em Obsidian", src: "fato · 03/10" },
    Fact { g: "Rotina", t: "Briefing da manhã às 08:45 nos dias úteis", src: "rotina · 01/10" },
];

pub const SRC: &[(&str, &str, &str)] = &[
    ("vault", "Vault do Obsidian", "Lê as notas; grava em 03 Recursos/Orion/Memória."),
    ("repo", "Repositórios dos projetos", "Código do Orion e do vault no GitHub."),
    ("cal", "Calendário", "Só pelo chat e pelo briefing."),
    ("email", "E-mail", "Só pelo chat e pelo briefing."),
    ("web", "Busca na web", "Sem dados pessoais; não pede aprovação."),
];

pub struct Mcp {
    pub t: &'static str,
    pub k: &'static str,
    pub st: &'static str,
    pub ok: bool,
    pub tools: u32,
}
pub const MCP: &[Mcp] = &[
    Mcp { t: "GitHub", k: "MCP · HTTP", st: "Conectado", ok: true, tools: 41 },
    Mcp { t: "Google Workspace", k: "MCP · HTTP", st: "Conectado", ok: true, tools: 18 },
    Mcp { t: "Arquivos locais", k: "MCP · stdio", st: "Token expirado", ok: false, tools: 9 },
    Mcp { t: "Telegram", k: "Canal · depois da v1", st: "Conectado · ID permitido", ok: true, tools: 6 },
];
pub const SKILLS: &[(&str, &str)] = &[("segundo-cerebro-obsidian", "Importada do Claude"), ("code-review", "Importada do Claude"), ("release-windows", "Própria")];
pub const LOCAL: &[(&str, &str, &str)] = &[
    ("Qwen 2.5 7B Instruct (Q4_K_M)", "4,7 GB", "fallback"),
    ("Llama 3.2 3B (Q4_K_M)", "2,0 GB", ""),
    ("bge-m3 (embeddings)", "1,1 GB", "embeddings"),
];
pub const OBJ: &[&str] = &[
    "chaves", "carteira", "celular", "fone de ouvido", "óculos", "controle remoto", "carregador", "notebook", "mochila", "documentos (RG/CNH)", "cartões", "remédios",
    "garrafa/copo", "mouse sem fio", "teclado sem fio", "pendrive", "relógio", "guarda-chuva", "caneta",
];
pub const READ_NOTES: &[&str] = &["Orion", "Orion - Arquitetura", "Rust", "Orion - Estado e plano", "Ollama e LLM Local"];
pub const LAT: &[f32] = &[2.1, 1.9, 2.3, 1.8, 1.7, 1.9, 1.6, 1.5, 1.6, 1.4, 1.5, 1.3, 1.4, 1.4];
pub const RAMS: &[f32] = &[1180.0, 1210.0, 1195.0, 1230.0, 1225.0, 1240.0, 1215.0, 1236.0, 1228.0, 1205.0, 860.0, 420.0, 318.0, 302.0, 298.0, 296.0, 301.0, 296.0];
pub const CMDS: &[(&str, &str)] = &[
    ("/nota", "Gravar ou atualizar a nota desta conversa"),
    ("/vault", "Procurar no vault"),
    ("/rotina", "Criar ou listar rotinas"),
    ("/modelo", "Trocar modelo e esforço"),
    ("/projeto", "Trocar o projeto da conversa"),
    ("/autonomo", "Ligar ou desligar o modo autônomo"),
    ("/nuvem", "Mandar tarefa para Claude ou Codex na nuvem"),
    ("/imagem", "Gerar imagem (Gemini)"),
    ("/limpar", "Conversa nova no mesmo projeto"),
    ("/resumo", "Resumir a conversa até aqui"),
    ("/aprovar", "Abrir a fila de aprovações"),
    ("/voz", "Ligar ou desligar a voz ao vivo"),
    ("/mcp", "Listar e ativar servidores MCP"),
    ("/skill", "Listar e ativar skills"),
    ("/ajuda", "Todos os comandos"),
];

pub const TERM: &[&str] = &[
    "PS C:\\Projetos\\Orion> cargo test -p orion-memory",
    "   Compiling orion-memory v0.6.0",
    "    Finished test [unoptimized] in 8.41s",
    "     Running unittests (target\\debug\\deps\\orion_memory)",
    "test index::mmap_abre_sem_copiar ... ok",
    "test cache::lru_respeita_64mb ... ok",
    "test result: ok. 2 passed; 0 failed",
    "",
];

pub const ARCH: &[(&str, &str, &str)] = &[("Configurar Tailscale no notebook", "Orion", "agosto"), ("Exportar tabelas antes de vender o PC", "Nortis", "julho")];

pub const NOTE_SAMPLE: &str = "---\ntype: conversa\ndate: 2026-10-15\nfonte: orion\nprojeto: \"[[Orion]]\"\ntags: [conversa, orion, rust, memoria]\nresumo: \"RAM parada de 1,2 GB; três causas e plano para ~300 MB.\"\n---\n\n# Orion RAM otimização\n\n> Projeto: [[Orion]] · Conceito: [[Rust]]\n\n## Pergunta\nO Orion está usando 1,2 GB parado. Dá pra baixar sem perder velocidade?\n\n## Achados\n- Índice vetorial carregado inteiro: 610 MB\n- Modelo de voz residente: 280 MB\n- Cache de embeddings sem limite\n\n## Decisão\nAbrir o índice com `mmap`, carregar a voz sob demanda e limitar o cache a **64 MB** com LRU.\n";
