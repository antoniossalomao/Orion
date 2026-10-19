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
    pub path: String,
    pub at: String,
    pub commit: String,
}

#[derive(Clone, Debug)]
pub struct Tool {
    pub kind: String,
    pub value: String,
    pub time: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ApprState {
    Open,
    Approved(String),
    Denied(String),
}

#[derive(Clone, Debug)]
pub struct Approval {
    /// Id da aprovação no servidor (vazio nos dados de exemplo).
    pub id: String,
    /// Marcação mínima: `código` entre crases vira mono.
    pub text: String,
    pub state: ApprState,
    pub diff_button: bool,
}

#[derive(Clone, Debug)]
pub struct OrionMsg {
    pub time: String,
    pub via: String,
    pub out: String,
    pub tools_label: String,
    pub tools: Vec<Tool>,
    /// Corpo em marcação mínima (ver `md.rs`).
    pub body: String,
    pub saved: bool,
    pub approval: Option<Approval>,
}

#[derive(Clone, Debug)]
pub enum ChatMsg {
    User(String),
    Orion(OrionMsg),
}

#[derive(Clone, Debug)]
pub struct Conv {
    pub id: String,
    pub title: String,
    pub proj: String,
    pub origin: Option<Origin>,
    pub day: String,
    pub state: ConvState,
    pub ctx: u32,
    pub branch: Option<String>,
    pub pinned: bool,
    pub note: Option<Note>,
    pub pin: Option<String>,
    pub banner: Option<String>,
    pub msgs: Vec<ChatMsg>,
}

pub const RAM_BODY: &str = "Medi com o Orion parado por 10 minutos. O pico vem de três lugares, e nenhum deles precisa ficar residente o tempo todo:\n\n1. **Índice vetorial carregado inteiro** na inicialização: 610 MB. Dá para abrir com `mmap` e deixar o sistema paginar.\n2. **Modelo de voz residente**: 280 MB, mesmo com a voz desligada. Carregar só quando a palavra “Orion” estiver ativa.\n3. **Cache de embeddings sem limite**: cresce sem parar. Limitar a 64 MB com LRU.\n\n```rust\n// src/memory/index.rs\nlet file = File::open(&path)?;\nlet map = unsafe { Mmap::map(&file)? };\nlet index = VectorIndex::from_bytes(&map, 1024)?;\n```\n\nEstimativa: de **1,2 GB** para cerca de **300 MB** parado, sem perder velocidade na busca.";

pub fn convs() -> Vec<Conv> {
    use ChatMsg::*;
    let tool = |kind: &str, value: &str, time: &str| Tool { kind: kind.into(), value: value.into(), time: time.into() };
    vec![
        Conv {
            id: "ram".into(),
            title: "Orion RAM otimização".into(),
            proj: "Orion".into(),
            origin: None,
            day: "Hoje".into(),
            state: ConvState::Normal,
            ctx: 30,
            branch: Some("claude/funny-hamilton-jb1svh".into()),
            pinned: true,
            note: Some(Note { path: "03 Recursos/Orion/Memória/2026-10-15 - Orion RAM otimização.md".into(), at: "09:13".into(), commit: "a3f9c12".into() }),
            pin: Some("Meta do projeto: RAM parada abaixo de 300 MB".into()),
            banner: None,
            msgs: vec![
                User("O Orion está usando 1,2 GB parado. Dá pra baixar sem perder velocidade?".into()),
                Orion(OrionMsg {
                    time: "09:12".into(),
                    via: "Claude Sonnet".into(),
                    out: "Anthropic (Claude Code CLI): pergunta + trechos de 3 arquivos .rs".into(),
                    tools_label: "Leu 4 arquivos, executou 1 comando".into(),
                    tools: vec![
                        tool("Comando".into(), "cargo bloat --release --crates".into(), "2,1 s".into()),
                        tool("Leu".into(), "src/memory/index.rs".into(), "0,1 s".into()),
                        tool("Leu".into(), "src/voice/wake.rs".into(), "0,1 s".into()),
                        tool("Leu".into(), "src/embed/cache.rs".into(), "0,1 s".into()),
                        tool("Vault".into(), "Orion · Orion - Arquitetura · Rust".into(), "0,2 s".into()),
                    ],
                    body: RAM_BODY.into(),
                    saved: true,
                    approval: Some(Approval {
                        id: String::new(),
                        text: "Aplicar as três mudanças na branch `claude/funny-hamilton-jb1svh` e rodar `cargo test`. Escreve 3 arquivos no projeto Orion.".into(),
                        state: ApprState::Open,
                        diff_button: true,
                    }),
                }),
            ],
        },
        Conv {
            id: "rust".into(),
            title: "Orion para Rust".into(),
            proj: "Orion".into(),
            origin: None,
            day: "Hoje".into(),
            state: ConvState::Running,
            ctx: 54,
            branch: Some("claude/rust-port".into()),
            pinned: false,
            note: Some(Note { path: "03 Recursos/Orion/Memória/2026-10-15 - Orion para Rust.md".into(), at: "08:41".into(), commit: "77be0d4".into() }),
            pin: None,
            banner: Some("Groq sem cota; usando Gemini.".into()),
            msgs: vec![
                User("Qual biblioteca de interface faz mais sentido para o front em Rust?".into()),
                Orion(OrionMsg {
                    time: "08:40".into(),
                    via: "Orion · Groq".into(),
                    out: "Groq: pergunta, sem dados pessoais".into(),
                    tools_label: "".into(),
                    tools: vec![],
                    body: "Para um app de desktop com barra de título própria, atalhos e lista longa de conversas, o **iced** é o mais próximo do que o front atual faz: arquitetura Elm, estilo por tema e boa renderização de texto.\n\nO grafo da memória vira um widget `shader` (wgpu) em 3D, com o céu do `sky.js` ao fundo.".into(),
                    saved: true,
                    approval: None,
                }),
            ],
        },
        Conv {
            id: "plano".into(),
            title: "Plano rodada 2".into(),
            proj: "Orion".into(),
            origin: None,
            day: "Hoje".into(),
            state: ConvState::Pending,
            ctx: 41,
            branch: Some("claude/quirky-dijkstra-65d6gq".into()),
            pinned: false,
            note: Some(Note { path: "03 Recursos/Orion/Memória/2026-10-15 - Plano rodada 2.md".into(), at: "07:59".into(), commit: "c41d2aa".into() }),
            pin: None,
            banner: None,
            msgs: vec![
                User("Qual o status da reescrita e o que falta para a fase 6?".into()),
                Orion(OrionMsg {
                    time: "07:58".into(),
                    via: "Gemini Flash".into(),
                    out: "Google (Gemini CLI): pergunta + Orion - Estado e plano.md".into(),
                    tools_label: "".into(),
                    tools: vec![],
                    body: "A fase 6 tem o front redesenhado e a voz por clique. Falta validar a palavra de ativação com microfone real.\n\n- Voz por clique: pronta no código.\n- Voz ao vivo: pronta, sem teste de áudio.\n- Palavra “Orion”: falta testar com o seu microfone.\n\nQuer que eu envie a branch para revisão?".into(),
                    saved: true,
                    approval: Some(Approval {
                        id: String::new(),
                        text: "Enviar `claude/quirky-dijkstra-65d6gq` para o origin e abrir o PR. É uma escrita fora deste computador.".into(),
                        state: ApprState::Open,
                        diff_button: false,
                    }),
                }),
            ],
        },
        Conv {
            id: "brief".into(),
            title: "Briefing da manhã".into(),
            proj: "Rotina".into(),
            origin: Some(Origin::Rotina),
            day: "Hoje".into(),
            state: ConvState::Unread,
            ctx: 0,
            branch: None,
            pinned: false,
            note: None,
            pin: None,
            banner: None,
            msgs: vec![Orion(OrionMsg {
                time: "08:45".into(),
                via: "Orion · Gemini".into(),
                out: "".into(),
                tools_label: "".into(),
                tools: vec![],
                body: "Bom dia. Três eventos hoje, o primeiro às 10:00. Um PR esperando sua revisão no vault. Backup de ontem concluído sem erros.".into(),
                saved: false,
                approval: None,
            })],
        },
        Conv {
            id: "merc".into(),
            title: "Lista do mercado".into(),
            proj: "Sem projeto".into(),
            origin: Some(Origin::Telegram),
            day: "Ontem".into(),
            state: ConvState::Normal,
            ctx: 0,
            branch: None,
            pinned: false,
            note: None,
            pin: None,
            banner: None,
            msgs: vec![
                User("(áudio, 0:12) põe café, leite e pão na lista".into()),
                Orion(OrionMsg {
                    time: "ontem, 19:02".into(),
                    via: "Local · Qwen 7B".into(),
                    out: "".into(),
                    tools_label: "".into(),
                    tools: vec![],
                    body: "Anotei na lista do mercado: café, leite e pão. Mandei a lista atualizada no Telegram.".into(),
                    saved: false,
                    approval: None,
                }),
            ],
        },
        Conv {
            id: "vaultpr".into(),
            title: "Revisão da PR do vault".into(),
            proj: "Vault".into(),
            origin: None,
            day: "Ontem".into(),
            state: ConvState::Normal,
            ctx: 0,
            branch: None,
            pinned: false,
            note: None,
            pin: None,
            banner: None,
            msgs: vec![
                User("Revisa a PR do vault e me diz se pode fazer merge.".into()),
                Orion(OrionMsg {
                    time: "ontem, 18:20".into(),
                    via: "Claude Sonnet".into(),
                    out: "Anthropic (Claude Code CLI): pergunta + diff da PR".into(),
                    tools_label: "".into(),
                    tools: vec![],
                    body: "Duas checagens verdes e nenhum conflito. Um ponto: a nota nova não tem link para nenhum Projeto, o que quebra a regra de “não criar ilhas” do `claude.md`.".into(),
                    saved: false,
                    approval: None,
                }),
            ],
        },
        empty("front".into(), "Análise do frontend Orion".into(), "Orion".into(), "7 dias".into()),
        empty("org".into(), "Organização do vault".into(), "Vault".into(), "7 dias".into()),
        empty("nortis".into(), "Backup e restauração".into(), "Nortis".into(), "Mais antigos".into()),
        empty("lyra".into(), "Avaliação de RAG".into(), "Lyra".into(), "Mais antigos".into()),
    ]
}

fn empty(id: &str, title: &str, proj: &str, day: &str) -> Conv {
    Conv { id: id.into(), title: title.into(), proj: proj.into(), origin: None, day: day.into(), state: ConvState::Normal, ctx: 0, branch: None, pinned: false, note: None, pin: None, banner: None, msgs: vec![] }
}

pub struct Project {
    pub id: String,
    pub n: String,
    pub d: String,
    pub pend: String,
    pub up: String,
    pub repo: String,
    pub ci: bool,
    pub model: String,
    pub pastas: String,
    pub rot: u32,
}

pub fn projects() -> Vec<Project> {
    vec![
    Project { id: String::new(), n: "Orion".into(), d: "Assistente local em Rust. Fase 6: front e voz.".into(), pend: "3 pendências".into(), up: "há 2 h".into(), repo: "antoniossalomao/Orion".into(), ci: true, model: "Claude Sonnet".into(), pastas: "C:\\Projetos\\Orion".into(), rot: 1 },
    Project { id: String::new(), n: "Vault".into(), d: "Segundo cérebro em Obsidian, estrutura PARA.".into(), pend: "1 PR aberto".into(), up: "ontem".into(), repo: "antoniossalomao/vault".into(), ci: true, model: "Gemini Flash".into(), pastas: "C:\\Users\\Antonio\\Vault".into(), rot: 2 },
    Project { id: String::new(), n: "Lyra".into(), d: "Avaliação de RAG com o vault como base.".into(), pend: "pausado".into(), up: "set.".into(), repo: "".into(), ci: false, model: "Auto".into(), pastas: "".into(), rot: 0 },
    Project { id: String::new(), n: "Nortis".into(), d: "Backup e restauração antes de trocar de PC.".into(), pend: "concluído".into(), up: "ago.".into(), repo: "".into(), ci: false, model: "Auto".into(), pastas: "D:\\Backup".into(), rot: 0 },
]
}

pub fn results() -> Vec<(String, String, String, String, String)> {
    let t = |a: &str, b: &str, c: &str, d: &str, e: &str| (a.to_string(), b.to_string(), c.to_string(), d.to_string(), e.to_string());
    vec![
    t("Pesquisa: memória RAM desnecessária", "Índice vetorial, voz residente e cache sem limite.", "15/10", "Pesquisa", "v2"),
    t("Diagnóstico do PR do vault", "Duas checagens verdes, sem conflito.", "14/10", "Revisão", "v1"),
    t("Revisão da fase 5 de voz", "Pendente: calibrar palmas no notebook.", "12/10", "Revisão", "v3"),
]
}

#[derive(Clone)]
pub struct Rot {
    pub id: String,
    pub t: String,
    pub d: String,
    pub next: String,
    pub last: String,
    pub st: String,
    pub on: bool,
    pub news: bool,
    pub crit: bool,
    pub out: String,
    pub runs: String,
}

pub fn rots() -> Vec<Rot> {
    let r = |id: &str, t: &str, d: &str, next: &str, last: &str, st: &str, on: bool, news: bool, crit: bool, out: &str, runs: &str| Rot { id: id.into(), t: t.into(), d: d.into(), next: next.into(), last: last.into(), st: st.into(), on, news, crit, out: out.into(), runs: runs.into() };
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
    pub when: String,
    pub text: String,
    pub kind: String, // ok | warn | err
    pub label: String,
    pub undone: bool,
    pub undoable: bool,
}

pub fn acts() -> Vec<Act> {
    let a = |when: &str, text: &str, kind: &str, label: &str| Act { when: when.into(), text: text.to_string(), kind: kind.into(), label: label.to_string(), undone: false, undoable: false };
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
    pub svc: String,
    pub t: String,
    pub proj: String,
    pub running: bool,
    pub at: String,
    pub link: String,
}
pub fn cloud() -> Vec<CloudTask> {
    vec![
    CloudTask { svc: "Claude".into(), t: "Portar o gateway de provedores para Rust".into(), proj: "Orion".into(), running: true, at: "há 12 min".into(), link: "claude.ai/code · sessão".into() },
    CloudTask { svc: "Codex".into(), t: "Testes do crate orion-ai".into(), proj: "Orion".into(), running: false, at: "há 1 h".into(), link: "PR #31".into() },
    CloudTask { svc: "Claude".into(), t: "Briefing (PC desligado no sábado)".into(), proj: "Rotina".into(), running: false, at: "sábado, 08:45".into(), link: "rotina na nuvem".into() },
]
}

pub struct Fact {
    pub id: i64,
    pub g: String,
    pub t: String,
    pub src: String,
}
pub fn facts() -> Vec<Fact> {
    vec![
    Fact { id: 0, g: "Preferências".into(), t: "Respostas diretas, sem preâmbulo".into(), src: "conversa · 12/09".into() },
    Fact { id: 0, g: "Preferências".into(), t: "Português, tom técnico".into(), src: "conversa · 12/09".into() },
    Fact { id: 0, g: "Projetos".into(), t: "Orion: reescrita em Rust, fase 6".into(), src: "conversa · 14/10".into() },
    Fact { id: 0, g: "Projetos".into(), t: "Vault: segundo cérebro em Obsidian".into(), src: "fato · 03/10".into() },
    Fact { id: 0, g: "Rotina".into(), t: "Briefing da manhã às 08:45 nos dias úteis".into(), src: "rotina · 01/10".into() },
]
}

pub const SRC: &[(&str, &str, &str)] = &[
    ("vault", "Vault do Obsidian", "Lê as notas; grava em 03 Recursos/Orion/Memória."),
    ("repo", "Repositórios dos projetos", "Código do Orion e do vault no GitHub."),
    ("cal", "Calendário", "Só pelo chat e pelo briefing."),
    ("email", "E-mail", "Só pelo chat e pelo briefing."),
    ("web", "Busca na web", "Sem dados pessoais; não pede aprovação."),
];

pub struct Mcp {
    pub t: String,
    pub k: String,
    pub st: String,
    pub ok: bool,
    pub tools: u32,
}
pub fn mcp() -> Vec<Mcp> {
    vec![
    Mcp { t: "GitHub".into(), k: "MCP · HTTP".into(), st: "Conectado".into(), ok: true, tools: 41 },
    Mcp { t: "Google Workspace".into(), k: "MCP · HTTP".into(), st: "Conectado".into(), ok: true, tools: 18 },
    Mcp { t: "Arquivos locais".into(), k: "MCP · stdio".into(), st: "Token expirado".into(), ok: false, tools: 9 },
    Mcp { t: "Telegram".into(), k: "Canal · depois da v1".into(), st: "Conectado · ID permitido".into(), ok: true, tools: 6 },
]
}
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

pub fn arch() -> Vec<(String, String, String)> {
    vec![("Configurar Tailscale no notebook".into(), "Orion".into(), "agosto".into()), ("Exportar tabelas antes de vender o PC".into(), "Nortis".into(), "julho".into())]
}

pub const NOTE_SAMPLE: &str = "---\ntype: conversa\ndate: 2026-10-15\nfonte: orion\nprojeto: \"[[Orion]]\"\ntags: [conversa, orion, rust, memoria]\nresumo: \"RAM parada de 1,2 GB; três causas e plano para ~300 MB.\"\n---\n\n# Orion RAM otimização\n\n> Projeto: [[Orion]] · Conceito: [[Rust]]\n\n## Pergunta\nO Orion está usando 1,2 GB parado. Dá pra baixar sem perder velocidade?\n\n## Achados\n- Índice vetorial carregado inteiro: 610 MB\n- Modelo de voz residente: 280 MB\n- Cache de embeddings sem limite\n\n## Decisão\nAbrir o índice com `mmap`, carregar a voz sob demanda e limitar o cache a **64 MB** com LRU.\n";
