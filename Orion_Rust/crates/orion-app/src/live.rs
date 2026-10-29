//! Liga o app ao servidor local (`orion serve`): converte o JSON da API nos dados das telas e dispara as chamadas.
//! Sem `Api`, o app roda com os dados de exemplo do protótipo (testes e fotos).

use crate::app::{App, Msg, Panic, Sec, View};
use crate::data::{self, Act, ApprState, Approval, ChatMsg, Conv, ConvState, Fact, Origin, OrionMsg, Project, Rot, Tool};
use crate::net::{self, arr, s, Api, Chat, LoginResult};
use iced::Task;
use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub enum Lm {
    Boot(Result<bool, String>),
    Initialized(Result<(), String>),
    LanguageSaved(Result<(), String>),
    ToolPermissions(std::time::Instant, Result<Value, String>),
    Attached(u64, Result<Option<Value>, String>),
    Conflitos(Result<Value, String>),
    ConflitoAberto(Result<Value, String>),
    ConflitoResolvido(Result<String, String>),
    ArtifactList(String, Result<Value, String>),
    ArtifactSaved(Result<Value, String>),
    ArtifactDetail(String, Result<Value, String>),
    Login(Result<LoginResult, String>),
    /// Resultado de abrir o banco cifrado com a senha digitada.
    Desbloqueado(Result<(), String>),
    TwoFactor(Result<(), String>),
    Sessions(Result<Value, String>),
    Opened(String, Result<Value, String>),
    Created(Result<Value, String>),
    Approvals(Result<Value, String>),
    Rotinas(Result<Value, String>),
    Projects(Result<Value, String>),
    ProjectCreated(u64, Result<Value, String>),
    Renamed(u64, String, Result<Value, String>),
    Keys(Result<Value, String>),
    KeySaved(Result<Value, String>),
    /// Imagem gerada baixada do núcleo: (URL `/imagens/…`, bytes).
    Imagem(String, Result<Vec<u8>, String>),
    Facts(Result<Value, String>),
    Painel(Result<Value, String>),
    Budget(Result<Value, String>),
    Modo(Result<Value, String>),
    Grafo(Result<Value, String>),
    GraphReady(Result<crate::graph::Prepared, String>),
    Mcp(Result<Value, String>),
    Skills(Result<Value, String>),
    Tarefas(Result<Value, String>),
    /// Lista da nuvem; com a nuvem desligada (403) fica vazia, sem erro na tela.
    Nuvem(Result<Value, String>),
    Modelos(Result<Value, String>),
    Chat(Chat),
    /// Resultado de uma ação simples: mensagem de sucesso ou de erro (vira toast) e, se `true`, recarrega as listas.
    Acao(Result<String, String>, bool),
}

// ── relógio sem dependências ──
/// Deslocamento do fuso do Windows em segundos (o horário local do PC, com horário de verão se houver).
pub fn tz_offset() -> f64 {
    chrono::Local::now().offset().local_minus_utc() as f64
}

/// `escrever_arquivo` → `Escrever arquivo` (o rótulo curto da linha de ferramenta).
fn humanizar(nome: &str) -> String {
    let t = nome.replace('_', " ");
    let mut c = t.chars();
    match c.next() {
        Some(p) => p.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

fn now_secs() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

/// `2026-10-19T14:47:44.69+00:00` → segundos desde 1970 (UTC).
pub fn iso_to_secs(iso: &str) -> Option<f64> {
    let (d, t) = iso.split_once('T')?;
    let mut dp = d.split('-').map(|x| x.parse::<i64>().ok());
    let (y, m, dd) = (dp.next()??, dp.next()??, dp.next()??);
    let t = t.split(['+', 'Z']).next()?;
    let mut tp = t.split(':');
    let h: f64 = tp.next()?.parse().ok()?;
    let mi: f64 = tp.next()?.parse().ok()?;
    let sec: f64 = tp.next().and_then(|x| x.parse().ok()).unwrap_or(0.0);
    // dias desde 1970 (algoritmo civil de Howard Hinnant)
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + dd - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some(days as f64 * 86400.0 + h * 3600.0 + mi * 60.0 + sec)
}

/// "Hoje", "Ontem", "7 dias" ou "Mais antigos", pelo dia local.
pub fn day_group(secs: f64, now: f64) -> &'static str {
    let day = |t: f64| ((t + tz_offset()) / 86400.0).floor();
    let d = day(now) - day(secs);
    if d <= 0.0 {
        "Hoje"
    } else if d <= 1.0 {
        "Ontem"
    } else if d <= 7.0 {
        "7 dias"
    } else {
        "Mais antigos"
    }
}

/// HH:MM no fuso local de um instante em segundos.
pub fn hm(secs: f64) -> String {
    let s = (secs + tz_offset()).rem_euclid(86400.0) as i64;
    format!("{:02}:{:02}", s / 3600, (s % 3600) / 60)
}

fn rel(secs: f64, now: f64) -> String {
    let d = (now - secs).max(0.0);
    if d < 3600.0 {
        format!("há {} min", (d / 60.0).max(1.0) as i64)
    } else if d < 86400.0 {
        format!("há {} h", (d / 3600.0) as i64)
    } else if d < 2.0 * 86400.0 {
        "ontem".into()
    } else {
        format!("há {} dias", (d / 86400.0) as i64)
    }
}

// ── mapeamentos puros (testáveis) ──
pub fn map_sessions(v: &Value, old: &[Conv], projects: &[Project], now: f64) -> (Vec<Conv>, Option<String>) {
    let ativa = v.get("ativa").and_then(Value::as_str).map(String::from);
    let mut out: Vec<(f64, Conv)> = vec![];
    for x in arr(v, "sessoes") {
        if x.get("arquivada").and_then(Value::as_bool).unwrap_or(false) {
            continue;
        }
        let id = s(x, "sessao_id").to_string();
        let last = iso_to_secs(s(x, "ultima_atividade")).unwrap_or(0.0);
        let proj = x.get("project_id").and_then(Value::as_str).and_then(|p| projects.iter().find(|pr| pr.id == p)).map(|p| p.n.clone()).unwrap_or_else(|| "Sem projeto".into());
        let origin = match s(x, "canal") {
            "telegram" => Some(Origin::Telegram),
            "rotina" => Some(Origin::Rotina),
            _ => None,
        };
        let prev = old.iter().find(|c| c.id == id);
        let conv = Conv {
            id,
            title: s(x, "titulo").to_string(),
            proj,
            origin,
            day: day_group(last, now).into(),
            state: prev.map(|p| p.state).unwrap_or(ConvState::Normal),
            ctx: 0,
            branch: None,
            pinned: x.get("favorita").and_then(Value::as_bool).unwrap_or(false),
            note: None,
            pin: None,
            banner: None,
            msgs: prev.map(|p| p.msgs.clone()).unwrap_or_default(),
        };
        out.push((last, conv));
    }
    out.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    (out.into_iter().map(|c| c.1).collect(), ativa)
}

pub fn map_history(v: &Value) -> Vec<ChatMsg> {
    let mut out = vec![];
    for m in arr(v, "mensagens") {
        let role = s(m, "role");
        let content = s(m, "content").to_string();
        let ts = iso_to_secs(s(m, "timestamp")).map(hm).unwrap_or_default();
        match role {
            "user" => out.push(ChatMsg::User(content)),
            "assistant" => {
                let prov = m.get("provenance").cloned().unwrap_or(Value::Null);
                let via = ["model", "modelo", "endpoint", "tier"].iter().find_map(|k| prov.get(*k).and_then(Value::as_str)).unwrap_or("").to_string();
                out.push(ChatMsg::Orion(OrionMsg { time: ts, via, out: String::new(), tools_label: String::new(), tools: vec![], body: content, saved: false, approval: None }));
            }
            _ => {}
        }
    }
    // o servidor devolve do mais novo para o mais antigo? normaliza pela ordem de id
    let ids: Vec<i64> = arr(v, "mensagens").iter().filter(|m| matches!(s(m, "role"), "user" | "assistant")).filter_map(|m| m.get("id").and_then(Value::as_i64)).collect();
    if ids.len() == out.len() && ids.windows(2).any(|w| w[0] > w[1]) {
        out.reverse();
    }
    out
}

pub fn map_approval(a: &Value) -> Approval {
    let args: Vec<(String, String)> = match a.get("args") {
        Some(Value::Object(o)) => o.iter().map(|(k, v)| (k.clone(), v.as_str().map(String::from).unwrap_or_else(|| v.to_string()))).collect(),
        _ => vec![],
    };
    let (tool, reason) = (s(a, "tool"), s(a, "reason"));
    // os argumentos vão inteiros no corpo do cartão: aqui só o motivo e a ferramenta
    let text = if reason.is_empty() { format!("`{tool}`") } else { format!("{reason} `{tool}`") };
    Approval {
        id: s(a, "id").to_string(),
        text,
        state: ApprState::Open,
        diff_button: false,
        tool: tool.to_string(),
        risk: s(a, "risk").to_string(),
        args,
        truncated: a.get("args_truncated").and_then(Value::as_bool).unwrap_or(false),
    }
}

pub fn map_rotinas(v: &Value) -> Vec<Rot> {
    arr(v, "rotinas")
        .iter()
        .map(|r| {
            let nome = s(r, "nome").to_string();
            let estado = s(r, "estado");
            let gat: Vec<String> = arr(r, "gatilhos").iter().map(|g| match g.get("valor").and_then(Value::as_str) { Some(v) => format!("{}: {v}", s(g, "tipo")), None => s(g, "tipo").to_string() }).collect();
            let mut d = s(r, "descricao").to_string();
            if !gat.is_empty() {
                d = if d.is_empty() { gat.join(", ") } else { format!("{d} · {}", gat.join(", ")) };
            }
            let st = match estado {
                "concedida" => "concedida",
                "mudou" => "mudou",
                "sem_concessao" => "sem concessão",
                e => e,
            };
            Rot { id: nome.clone(), t: nome, d, next: String::new(), last: String::new(), st: st.into(), on: estado == "concedida", news: false, crit: false, out: String::new(), runs: String::new() }
        })
        .collect()
}

pub fn map_projects(v: &Value, now: f64) -> Vec<Project> {
    v.as_array()
        .map(|a| a.as_slice())
        .unwrap_or(&[])
        .iter()
        .filter(|p| !p.get("archived").and_then(Value::as_bool).unwrap_or(false))
        .map(|p| Project {
            id: s(p, "id").to_string(),
            n: s(p, "name").to_string(),
            d: s(p, "instructions").lines().next().unwrap_or("").chars().take(140).collect(),
            pend: String::new(),
            up: rel(p.get("updated_at").and_then(Value::as_f64).unwrap_or(0.0), now),
            repo: String::new(),
            ci: false,
            model: "Auto".into(),
            pastas: s(p, "root").to_string(),
            rot: 0,
            vault: p.get("vault_note").and_then(Value::as_bool).unwrap_or(false),
        })
        .collect()
}

pub fn map_facts(v: &Value) -> Vec<Fact> {
    v.as_array()
        .map(|a| a.as_slice())
        .unwrap_or(&[])
        .iter()
        .filter(|f| !f.get("old").and_then(Value::as_bool).unwrap_or(false))
        .map(|f| {
            let src = s(f, "source");
            let g = match src.split([':', ' ', '·']).next().unwrap_or("") {
                "conversa" | "chat" => "Conversas",
                "vault" | "nota" => "Vault",
                "rotina" => "Rotina",
                "" => "Fatos",
                o => o,
            };
            let d = f.get("updated_at").and_then(Value::as_f64).map(|t| format!(" · {}", rel(t, now_secs()))).unwrap_or_default();
            Fact { id: f.get("id").and_then(Value::as_i64).unwrap_or(0), g: g.to_string(), t: s(f, "text").to_string(), src: format!("{src}{d}") }
        })
        .collect()
}

pub fn map_mcp(v: &Value) -> Vec<data::Mcp> {
    arr(v, "servidores")
        .iter()
        .map(|m| {
            let nome = s(m, "nome").to_string();
            let estado = s(m, "estado").to_lowercase();
            data::Mcp {
                t: nome.clone(),
                nome,
                k: format!("MCP · {}", if s(m, "transporte") == "remoto" { "HTTP" } else { "stdio" }),
                st: { let st = s(m, "status"); if st.is_empty() { s(m, "estado").to_string() } else { st.chars().take(60).collect() } },
                ok: estado.starts_with("conect") || estado == "ok",
                tools: arr(m, "ferramentas").len() as u32,
            }
        })
        .collect()
}

pub fn map_skills(v: &Value) -> Vec<data::SkillRow> {
    v.as_array().map(|a| a.as_slice()).unwrap_or(&[]).iter().map(|k| data::SkillRow { id: s(k, "id").to_string(), name: if s(k, "name").is_empty() { s(k, "id").to_string() } else { s(k, "name").to_string() }, desc: { let o = s(k, "origin"); let d = s(k, "description"); if d.is_empty() { o.to_string() } else { format!("{d} · {o}") } }, on: k.get("enabled").and_then(Value::as_bool).unwrap_or(false), ligavel: k.get("ligavel").and_then(Value::as_bool).unwrap_or(false) }).collect()
}

pub fn map_tarefas(v: &Value) -> Vec<data::CloudTask> {
    arr(v, "tarefas")
        .iter()
        .map(|t| {
            let status = s(t, "status").to_string();
            let criado = t.get("created_at").and_then(Value::as_f64).unwrap_or(0.0);
            let relatorio: String = s(t, "report").split_whitespace().collect::<Vec<_>>().join(" ").chars().take(160).collect();
            data::CloudTask { id: t.get("id").and_then(Value::as_i64).unwrap_or(0), svc: "Orion".into(), t: s(t, "objective").to_string(), running: status == "rodando", proj: status, at: if criado > 0.0 { format!("{} · {}", day_label(criado), hm(criado)) } else { String::new() }, link: relatorio }
        })
        .collect()
}

pub fn map_modelos(v: &Value) -> Vec<data::LocalModel> {
    arr(v, "modelos")
        .iter()
        .map(|m| data::LocalModel {
            id: s(m, "id").to_string(),
            nome: s(m, "nome").to_string(),
            tipo: s(m, "tipo").to_string(),
            bytes: m.get("bytes").and_then(Value::as_u64).unwrap_or(0),
            instalado: m.get("instalado").and_then(Value::as_bool).unwrap_or(false),
            ativo: m.get("ativo").and_then(Value::as_bool).unwrap_or(false),
            baixando: m.get("baixando").and_then(Value::as_bool).unwrap_or(false),
            baixados: m.get("baixados").and_then(Value::as_u64).unwrap_or(0),
            erro: m.get("erro").and_then(Value::as_str).unwrap_or("").to_string(),
        })
        .collect()
}

/// O que cada seção das configurações precisa buscar no núcleo.
pub fn pull_sec(api: &Api, sec: Sec) -> Task<Msg> {
    let get = |path: &'static str, f: fn(Result<Value, String>) -> Lm| {
        let a = api.clone();
        Task::perform(async move { a.get(path).await }, move |r| Msg::L(f(r)))
    };
    match sec {
        Sec::Tools => pull_tool_permissions(api),
        Sec::Conectores => Task::batch([get("/mcp/servidores", Lm::Mcp), get("/skills", Lm::Skills), crate::resources::load(api),crate::resources::load_management(api,&["mcp","accounts","calendar","plugins"])]),
        Sec::Automation=>Task::batch([crate::resources::load(api),crate::resources::load_management(api,&["routines","downloads"])]),
        Sec::Voz | Sec::Camera | Sec::Memoria | Sec::Fontes | Sec::Privacidade | Sec::Sistema => crate::resources::load(api),
        Sec::Locais => get("/modelos-locais", Lm::Modelos),
        Sec::Painel => Task::batch([get("/painel", Lm::Painel), get("/budget", Lm::Budget)]),
        Sec::Modelos => Task::batch([get("/painel", Lm::Painel), get("/budget", Lm::Budget), get("/config/chaves", Lm::Keys)]),
        _ => Task::none(),
    }
}

fn day_label(secs: f64) -> String {
    let d = ((secs + tz_offset()) / 86400.0).floor() as i64 + 719468;
    let era = d.div_euclid(146097);
    let doe = d - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{day:02}/{month:02}")
}

pub fn map_decisions(v: &Value) -> Vec<Act> {
    arr(&v["decisoes"], "recentes")
        .iter()
        .map(|d| {
            let tool = ["tool", "ferramenta", "action", "acao"].iter().find_map(|k| d.get(*k).and_then(Value::as_str)).unwrap_or("ação");
            let dec = ["decision", "decisao", "action", "status"].iter().find_map(|k| d.get(*k).and_then(Value::as_str)).unwrap_or("");
            let when = d.get("ts").or(d.get("at")).and_then(Value::as_f64).map(hm).unwrap_or_default();
            let (kind, label) = match dec {
                "allow" | "approved" | "aprovada" => ("ok", "concluído"),
                "deny" | "denied" | "negada" => ("err", "negada"),
                "confirm" | "pending" => ("warn", "aguardando"),
                o => ("ok", if o.is_empty() { "registrado" } else { o }),
            };
            Act { when, text: tool.to_string(), kind: kind.into(), label: label.into(), undone: false, undoable: false }
        })
        .collect()
}

/// Caminho da rota que lê um conflito; o caminho da nota vai codificado na consulta.
fn caminho_conflito(proposta: &str) -> String {
    let mut url = reqwest::Url::parse("http://127.0.0.1/vault/conflitos/ver").expect("URL fixa");
    url.query_pairs_mut().append_pair("path", proposta);
    format!("{}?{}", url.path(), url.query().unwrap_or(""))
}

// ── tarefas ──
fn pull_tool_permissions(api: &Api) -> Task<Msg> {
    let api = api.clone();
    let started = std::time::Instant::now();
    Task::perform(async move { api.get("/config/tools").await }, move |result| Msg::L(Lm::ToolPermissions(started, result)))
}

pub fn pull_basic(api: &Api) -> Task<Msg> {
    Task::batch([
        { let a = api.clone(); Task::perform(async move { a.get("/sessoes?limite=100").await }, |r| Msg::L(Lm::Sessions(r))) },
        { let a = api.clone(); Task::perform(async move { a.get("/approvals").await }, |r| Msg::L(Lm::Approvals(r))) },
        { let a = api.clone(); Task::perform(async move { a.get("/projects").await }, |r| Msg::L(Lm::Projects(r))) },
        { let a = api.clone(); Task::perform(async move { a.get("/modo").await }, |r| Msg::L(Lm::Modo(r))) },
        { let a = api.clone(); Task::perform(async move { a.get("/vault/conflitos").await }, |r| Msg::L(Lm::Conflitos(r))) },
        pull_tool_permissions(api),
        crate::resources::load(api),
    ])
}

pub fn pull_page(api: &Api, v: View) -> Task<Msg> {
    let get = |path: &'static str, f: fn(Result<Value, String>) -> Lm| {
        let a = api.clone();
        Task::perform(async move { a.get(path).await }, move |r| Msg::L(f(r)))
    };
    match v {
        View::Rotinas => get("/rotinas", Lm::Rotinas),
        View::Conhecimento => get("/facts", Lm::Facts),
        View::Memoria => get("/grafo/completo", Lm::Grafo),
        View::Projetos => get("/projects", Lm::Projects),
        View::Nuvem => Task::batch([get("/tarefas-bg", Lm::Tarefas), get("/nuvem", Lm::Nuvem)]),
        View::Aprovacoes => Task::batch([get("/painel", Lm::Painel), get("/approvals", Lm::Approvals)]),
        _ => Task::none(),
    }
}

fn acao(api: &Api, f: impl std::future::Future<Output = Result<Value, String>> + Send + 'static, ok: &'static str, reload: bool) -> Task<Msg> {
    let _ = api;
    Task::perform(f, move |r| Msg::L(Lm::Acao(r.map(|_| ok.to_string()), reload)))
}

impl App {
    pub fn is_live(&self) -> bool {
        self.api.is_some()
    }

    /// Entra no modo real: nada de dados de exemplo, tudo vem do servidor.
    pub fn live(api: Api) -> (Self, Task<Msg>) {
        let mut a = App::new();
        a.persist_ui = true;
        crate::keys::enable_persistence();
        let (scale, compact) = crate::uiprefs::load();
        if let Some(s) = scale {
            a.p.scale = s;
        }
        if let Some(c) = compact {
            a.p.compact = c;
        }
        a.convs.clear();
        a.projects.clear();
        a.cloud.clear();
        a.mcp.clear();
        a.skills.clear();
        a.locais.clear();
        a.facts.clear();
        a.rots.clear();
        a.acts.clear();
        a.archived.clear();
        a.rot_pend = false;
        a.term_out.clear();
        a.p.proj = 99;
        a.mem = crate::graph::Graph::empty();
        if let Some(profile) = api.perfil() { a.user = s(&profile, "user").to_string(); a.p.english = s(&profile, "language") == "en"; }
        if let Some(defaults) = api.primeiro_uso() {
            a.onboarding = Some(crate::onboarding::Form::new(defaults));
            a.api = Some(api);
            return (a, Task::none());
        }
        a.loading = Some("Preparando o Orion…".into());
        let t = { let api2 = api.clone(); Task::perform(async move { api2.initialize().await }, |r| Msg::L(Lm::Initialized(r))) };
        if api.bloqueado() {
            a.db_locked = true;
            a.gate_err = "Digite a senha do Orion para abrir.".into();
            a.api = Some(api);
            return (a, t);
        }
        a.api = Some(api);
        a.gate_err = "Conectando ao servidor…".into();
        (a, t)
    }

    fn artifact_query(&self) -> String {
        let Some(project) = self.conv.and_then(|i| self.convs.get(i)).and_then(|c| self.projects.iter().find(|p| p.n == c.proj)) else { return String::new() };
        let mut url = reqwest::Url::parse("http://127.0.0.1").unwrap();
        url.query_pairs_mut().append_pair("project_id", &project.id);
        format!("?{}", url.query().unwrap_or(""))
    }
    pub fn session_id(&self) -> Option<String> {
        self.conv.and_then(|i| self.convs.get(i)).map(|c| c.id.clone())
    }

    /// Mensagens que precisam de chamada ao servidor. `true` = depois ainda roda a lógica local.
    pub fn live_before(&mut self, msg: &Msg) -> Option<(Task<Msg>, bool)> {
        let api = self.api.clone()?;
        match msg {
            Msg::Toggle("boot")=>{let on=!self.p.boot;Some((Task::perform(async move{tokio::task::spawn_blocking(move||crate::onboarding::set_autostart(on).map(|_|on)).await.map_err(|_|"Não foi possível atualizar o início automático.".to_string()).and_then(|v|v)},|r|Msg::Resources(crate::resources::Event::StartupDone(r))),false))},
            Msg::Toggle("shortcut")=>{
                let on=!self.p.shortcut;
                let result=self.bandeja.as_ref().ok_or("Bandeja indisponível.".to_string()).and_then(|b|b.set_shortcut_enabled(on));
                match result{Ok(())=>{self.p.shortcut=on;if let Err(e)=crate::uiprefs::save_shortcut(on){self.toast(&e.to_string());}},Err(e)=>self.toast(&e)}
                Some((Task::none(),false))
            },
            Msg::Mic=>Some((self.update_resources(if self.mic_rec{crate::resources::Event::AudioStop(false)}else{crate::resources::Event::AudioStart(false)}),false)),
            Msg::Live=>Some((self.update_resources(if self.voice{crate::resources::Event::AudioStop(true)}else{crate::resources::Event::AudioStart(true)}),false)),
            Msg::TogA("src",i)=>{let key=["vault","repo","calendar","email","web"].get(*i).copied()?; Some((self.update_resources(crate::resources::Event::Source(key)),false))},
            Msg::TermRun=>{
                if self.resources.busy || self.term_in.trim().is_empty(){return Some((Task::none(),false));}
                let command=std::mem::take(&mut self.term_in);self.term_out.push(format!("PS> {command}"));self.resources.busy=true;
                Some((Task::perform(async move{api.post("/terminal/run",json!({"command":command})).await},|r|Msg::Resources(crate::resources::Event::TerminalDone(r))),false))
            },
            Msg::Pick("lang", value) => {
                let language = if *value == 1 { "en" } else { "pt-BR" };
                Some((Task::perform(async move { api.post("/config/language", json!({"language":language})).await.map(|_| ()) }, |result| Msg::L(Lm::LanguageSaved(result))), true))
            }
            Msg::LoginCancel => {
                if let Some(ch)=self.challenge.take(){api.cancelar_entrada(&ch);}
                self.pw.clear();self.gate_err.clear();
                Some((Task::none(),false))
            }
            Msg::Hello => {
                self.loading = Some("Confirmando Windows Hello…".into());
                self.gate_err="Confirme no Windows Hello…".into();
                Some((Task::perform(async move{api.hello().await},|r|Msg::L(Lm::Desbloqueado(r))),false))
            }
            Msg::Login => {
                if self.pw.trim().is_empty() {
                    return None;
                }
                let pw = std::mem::take(&mut self.pw);
                if self.db_locked && self.challenge.is_none() {
                    self.loading = Some("Verificando senha e abrindo os dados…".into());
                    self.gate_err = "Abrindo…".into();
                    let usuario = self.user.clone();
                    return Some((Task::perform(async move { api.entrar_usuario(usuario, pw).await }, |r| Msg::L(Lm::Login(r))), false));
                }
                self.gate_err = "Entrando…".into();
                if let Some(ch) = self.challenge.clone() {
                    return Some((Task::perform(async move { api.second_factor(&ch, &pw).await }, |r| Msg::L(Lm::TwoFactor(r))), false));
                }
                let usuario = self.user.clone();
                Some((Task::perform(async move { api.login(&usuario, &pw).await }, |r| Msg::L(Lm::Login(r))), false))
            }
            Msg::Logout => {
                self.project_form = None;
                self.rename_form = None;
                Some((Task::perform(async move { let _=api.post("/resources/audio/cancel",json!({})).await;api.logout().await }, |_| Msg::Noop), true))
            }
            // O modo real nunca usa amostras nem recibos de gravação do protótipo
            // (REGRAS.md: você é o administrador).
            Msg::Attach => {
                if self.attaching || self.streaming.is_some() || self.attachment_ids.len() >= 8 { return Some((Task::none(), false)); }
                self.attaching = true;
                let generation = self.attachment_generation;
                let imagens_anexadas = self.attach.iter().filter(|a| a.1 == "imagem").count();
                let project = self.conv.and_then(|index| self.convs.get(index)).and_then(|conversation| self.projects.iter().find(|project| project.n == conversation.proj)).or_else(|| self.conv.is_none().then(|| self.projects.get(self.p.proj)).flatten()).map(|project| project.id.clone()).filter(|id| !id.is_empty());
                Some((Task::perform(async move {
                    // Documento (6 MB), imagem (10 MB, F05.11) ou áudio (20 MB, F05.12): a classe vem
                    // da extensão, mas o núcleo confere de novo pela assinatura dos bytes.
                    let selected = tokio::task::spawn_blocking(move || {
                        use std::io::Read;
                        let Some(path) = crate::onboarding::choose(crate::onboarding::Field::Attachment)? else { return Ok(None) };
                        let path = std::path::PathBuf::from(path);
                        let name = path.file_name().ok_or("Nome de arquivo inválido")?.to_string_lossy().to_string();
                        let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
                        let (classe, limite) = match ext.as_str() {
                            "txt" | "md" | "pdf" => ("documento", 6usize * 1024 * 1024),
                            "png" | "jpg" | "jpeg" | "webp" => ("imagem", 10 * 1024 * 1024),
                            "wav" | "mp3" | "m4a" | "ogg" | "opus" | "flac" | "webm" => ("audio", 20 * 1024 * 1024),
                            _ => return Err("Tipo de arquivo não suportado. Use texto, Markdown, PDF, imagem (PNG, JPEG, WebP) ou áudio.".to_string()),
                        };
                        let file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
                        if !file.metadata().map_err(|e| e.to_string())?.is_file() { return Err("Escolha um arquivo.".to_string()); }
                        let mut bytes = vec![];
                        file.take(limite as u64 + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
                        if bytes.is_empty() || bytes.len() > limite { return Err(format!("Arquivo vazio ou maior que {} MB.", limite / (1024 * 1024))); }
                        let miniatura = if classe == "imagem" {
                            // Assinatura conferida aqui também: extensão não prova que é imagem.
                            let ok = bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) || bytes.starts_with(&[0xFF, 0xD8, 0xFF]) || (bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP");
                            if !ok { return Err("O arquivo não é uma imagem PNG, JPEG ou WebP válida.".to_string()); }
                            let img = image::load_from_memory(&bytes).map_err(|_| "Não foi possível abrir a imagem.".to_string())?;
                            let mut png = std::io::Cursor::new(Vec::new());
                            img.thumbnail(96, 96).write_to(&mut png, image::ImageFormat::Png).map_err(|e| e.to_string())?;
                            png.into_inner()
                        } else { vec![] };
                        Ok(Some((name, bytes, classe, miniatura)))
                    }).await.map_err(|e| e.to_string())??;
                    let Some((name, bytes, classe, miniatura)) = selected else { return Ok(None) };
                    if classe == "imagem" {
                        // O servidor aceita 4 imagens por mensagem: avisa antes de guardar a quinta.
                        if imagens_anexadas >= 4 { return Err("Até 4 imagens por mensagem.".into()); }
                        let value = api.upload_image(name, bytes).await?;
                        let id = s(&value, "id").to_string();
                        if id.is_empty() { return Err("Resposta de upload inválida.".into()); }
                        return Ok(Some(json!({"id": id, "name": s(&value, "name"), "kind": "imagem", "thumb": miniatura})));
                    }
                    // Áudio: com Whisper local nada sai do computador. Sem ele, só vai ao provedor se você confirmar aqui.
                    let mut remote = false;
                    if classe == "audio" {
                        let modo = api.get("/chat/audios/modo").await?;
                        if !modo["local"].as_bool().unwrap_or(false) {
                            if !modo["remoto"].as_bool().unwrap_or(false) { return Err("A transcrição de áudio não está configurada. Ligue o Whisper local ou defina a chave de transcrição.".into()); }
                            let provedor = s(&modo, "provedor").to_string();
                            let pergunta = crate::i18n::formatted("Este áudio vai sair do seu computador e será enviado a {} para ser transcrito. Enviar?", format!("Este áudio vai sair do seu computador e será enviado a {provedor} para ser transcrito. Enviar?"));
                            let aceitou = tokio::task::spawn_blocking(move || crate::onboarding::confirm(&pergunta)).await.map_err(|e| e.to_string())?;
                            if !aceitou { return Ok(None); }
                            remote = true;
                        }
                    }
                    let mut value = if classe == "audio" { api.upload_audio(name, project.clone(), remote, bytes).await? } else { api.upload_document(name, project.clone(), bytes).await? };
                    let id = s(&value, "id").to_string();
                    if id.is_empty() { return Err("Resposta de upload inválida.".into()); }
                    let mut detail = reqwest::Url::parse(&format!("http://127.0.0.1/documents/{id}")).map_err(|error| error.to_string())?;
                    if let Some(project) = project { detail.query_pairs_mut().append_pair("project_id", &project); }
                    let detail = format!("{}{}", detail.path(), detail.query().map(|query| format!("?{query}")).unwrap_or_default());
                    // Transcrever leva mais que extrair texto: até 10 minutos, sem travar a janela.
                    let tentativas = if classe == "audio" { 600 } else { 120 };
                    // Confirma a extração antes do recibo (REGRAS.md: conteúdo externo é dado).
                    for attempt in 0..=tentativas {
                        match s(&value, "status") {
                            "ready" => return Ok(Some(value)),
                            "failed" => return Err("Não foi possível extrair o documento.".into()),
                            "error" if classe == "audio" => return Err(format!("Não foi possível transcrever o áudio: {}", s(&value, "error"))),
                            _ => {},
                        }
                        if attempt == tentativas { break; }
                        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                        value = api.get(&detail).await?;
                    }
                    Err("O anexo ainda está sendo processado. Tente anexá-lo novamente em instantes.".into())
                }, move |r| Msg::L(Lm::Attached(generation, r))), false))
            }
            Msg::RecarregarConfig => Some((Task::perform(async move {
                let r = api.post("/config/recarregar", json!({})).await?;
                let n = |k: &str| r[k].as_array().map(|a| a.len()).unwrap_or(0);
                Ok(format!("Configurações recarregadas: {} aplicada(s) na hora, {} exigem reiniciar o Orion.", n("aplicados"), n("exigem_reinicio")))
            }, |r| Msg::L(Lm::Acao(r, false))), false)),
            Msg::VoceFezExportar => Some((Task::perform(async move {
                let r = api.post("/painel/voce-fez/exportar", json!({})).await?;
                Ok(if r["unchanged"].as_bool().unwrap_or(false) { "O relatório de hoje já estava no vault; nada foi sobrescrito.".to_string() } else { format!("Relatório gravado no vault: {}", s(&r, "file_path")) })
            }, |r| Msg::L(Lm::Acao(r, false))), false)),
            Msg::ConflitoAbrir(i) => {
                let Some((_, proposta, _)) = self.conflitos.get(*i).cloned() else { return Some((Task::none(), false)) };
                Some((Task::perform(async move { api.get(&caminho_conflito(&proposta)).await }, |r| Msg::L(Lm::ConflitoAberto(r))), false))
            }
            Msg::ConflitoSalvar => {
                let Some(c) = self.conflito.as_mut().filter(|c| !c.busy) else { return Some((Task::none(), false)) };
                c.busy = true;
                let corpo = json!({"path": c.proposed_path, "original_sha256": c.sha, "text": c.content.text()});
                Some((Task::perform(async move { api.post("/vault/conflitos/resolver", corpo).await.map(|_| "Nota do vault atualizada. Revise e faça o commit quando quiser.".to_string()) }, |r| Msg::L(Lm::ConflitoResolvido(r))), false))
            }
            Msg::ConflitoDescartar => {
                let Some(c) = self.conflito.as_mut().filter(|c| !c.busy) else { return Some((Task::none(), false)) };
                c.busy = true;
                let corpo = json!({"path": c.proposed_path});
                Some((Task::perform(async move { api.post("/vault/conflitos/descartar", corpo).await.map(|_| "Proposta descartada. A nota do vault ficou como estava.".to_string()) }, |r| Msg::L(Lm::ConflitoResolvido(r))), false))
            }
            Msg::EditorOpen(_) | Msg::EditorSave => {
                self.toast("O editor do vault ainda não está ligado ao núcleo. Nenhuma nota foi alterada.");
                Some((Task::none(), false))
            }
            Msg::Undo(_) => {
                self.toast("Desfazer ainda não está disponível nesta tela. Nenhuma ação foi revertida.");
                Some((Task::none(), false))
            }
            Msg::OpenConv(i) => {
                let c = self.convs.get(*i)?;
                let sid = c.id.clone();
                let loaded = !c.msgs.is_empty();
                let a2 = api.clone();
                let sid2 = sid.clone();
                let t = Task::perform(
                    async move {
                        let _ = a2.post("/sessoes/ativar", json!({"sessao_id": sid2})).await;
                        a2.get(&format!("/historico?sessao={sid2}&limite=100")).await
                    },
                    move |r| Msg::L(Lm::Opened(sid.clone(), r)),
                );
                let _ = loaded;
                Some((t, true))
            }
            Msg::CtxTab(crate::app::CtxTab::Artefatos) => {
                let sid = self.session_id()?;
                self.artifact_detail = None;
                let query = self.artifact_query();
                Some((Task::perform(async move { api.get(&format!("/artifacts{query}")).await }, move |r| Msg::L(Lm::ArtifactList(sid.clone(), r))), true))
            }
            Msg::ArtifactRead(id, version) => {
                let session = self.session_id()?;
                let query = self.artifact_query();
                let path = format!("/artifacts/{id}{query}{}version={version}", if query.is_empty() { "?" } else { "&" });
                Some((Task::perform(async move { api.get(&path).await }, move |r| Msg::L(Lm::ArtifactDetail(session.clone(), r))), false))
            }
            Msg::ArtifactRecord(index) | Msg::ArtifactUpdate(index, _) => {
                if self.artifact_busy { return Some((Task::none(), false)); }
                let c = self.conv.and_then(|i| self.convs.get(i))?;
                let (lang, body) = crate::views::ctx::artifacts(c).into_iter().nth(*index)?;
                let title = format!("Artefato {}", index + 1);
                let existing = self.artifact_records.iter().find(|a| a["session_id"] == c.id && match msg { Msg::ArtifactUpdate(_, id) => a["id"] == *id, _ => a["title"] == title });
                let title = existing.and_then(|a| a["title"].as_str()).unwrap_or(&title).to_string();
                let (path, expected) = match existing { Some(a) => (format!("/artifacts/{}/versions", s(a,"id")), a["version"].as_i64()), None => ("/artifacts".into(), None) };
                let kind = if matches!(lang.as_str(), "md" | "markdown" | "") { "markdown" } else { "code" };
                let payload = json!({"title":title,"kind":kind,"content":body,"session_id":c.id,"language":lang,"expected_version":expected});
                let path = format!("{path}{}", self.artifact_query());
                self.artifact_busy = true;
                Some((Task::perform(async move { api.post(&path, payload).await }, |r| Msg::L(Lm::ArtifactSaved(r))), false))
            }
            Msg::Go(v) => Some((pull_page(&api, *v), true)),
            Msg::Poll => {
                let extra = if self.view == View::Nuvem { pull_page(&api, View::Nuvem) } else if self.set_open && self.sec == Sec::Locais { pull_sec(&api, Sec::Locais) } else { Task::none() };
                Some((Task::batch([pull_basic(&api), extra]), false))
            }
            Msg::OpenSettings(Some(sec)) | Msg::SetSec(sec) => Some((pull_sec(&api, *sec), true)),
            Msg::NuvemTrazer(i) => {
                let id = self.nuvem.get(*i)?.id.clone();
                let a2 = api.clone();
                Some((Task::perform(async move { a2.post(&format!("/nuvem/{id}/trazer"), json!({})).await }, |r| {
                    let msg = r.map(|v| match v.get("comando").and_then(Value::as_str) {
                        Some(c) => format!("Rode no terminal, num clone do repositório: {c}"),
                        None => "Diff do Codex aplicado na pasta do projeto".to_string(),
                    });
                    Msg::L(Lm::Acao(msg, false))
                }), false))
            }
            Msg::NuvemDescartar(i) => {
                let id = self.nuvem.get(*i)?.id.clone();
                let a2 = api.clone();
                self.nuvem.remove(*i);
                Some((acao(&api, async move { a2.delete(&format!("/nuvem/{id}")).await }, "Saiu da lista; para parar a execução use o link da tarefa", false), false))
            }
            Msg::CloudCancel(i) => {
                let id = self.cloud.get(*i)?.id;
                let a2 = api.clone();
                Some((acao(&api, async move { a2.post(&format!("/tarefas-bg/{id}/cancelar"), json!({})).await }, "Tarefa cancelada", true), false))
            }
            Msg::McpTest(i) => {
                let nome = self.mcp.get(*i)?.nome.clone();
                let a2 = api.clone();
                let rotulo = nome.clone();
                Some((Task::perform(async move { a2.post(&format!("/mcp/servidores/{nome}/testar"), json!({})).await }, move |r| {
                    Msg::L(Lm::Acao(r.and_then(|v| if v.get("ok").and_then(Value::as_bool).unwrap_or(false) { Ok(format!("{rotulo}: respondeu ({} ferramentas)", v.get("ferramentas").and_then(Value::as_u64).unwrap_or(0))) } else { Err(format!("{rotulo}: {}", v.get("erro").and_then(Value::as_str).unwrap_or("não respondeu"))) }), true))
                }), false))
            }
            Msg::SkillToggle(i) => {
                let k = self.skills.get(*i)?;
                if !k.ligavel {
                    return None;
                }
                let (id, ativo, a2) = (k.id.clone(), !k.on, api.clone());
                Some((Task::batch([acao(&api, async move { a2.post("/skills/ligar", json!({"id": id, "ativo": ativo})).await }, if ativo { "Skill ligada" } else { "Skill desligada" }, false), pull_sec(&api, Sec::Conectores)]), false))
            }
            Msg::ModelGet(i) => {
                let id = self.locais.get(*i)?.id.clone();
                let a2 = api.clone();
                Some((acao(&api, async move { a2.post(&format!("/modelos-locais/{id}/baixar"), json!({})).await }, "Download iniciado", false), false))
            }
            Msg::ModelDel(i) => {
                let m = self.locais.get(*i)?;
                if m.ativo {
                    return None;
                }
                let id = m.id.clone();
                let a2 = api.clone();
                Some((Task::batch([acao(&api, async move { a2.delete(&format!("/modelos-locais/{id}")).await }, "Modelo removido", false), pull_sec(&api, Sec::Locais)]), false))
            }
            Msg::Toggle("auto") => {
                let novo = !self.p.auto;
                let a2 = api.clone();
                Some((Task::perform(async move { a2.post("/modo/autonomo", json!({"ativo": novo})).await }, |r| Msg::L(Lm::Modo(r))), true))
            }
            Msg::ToolPermission(index) => {
                if self.p.tools_busy { return Some((Task::none(), false)); }
                let Some(mut values) = self.p.tools else { return Some((pull_sec(&api, Sec::Tools), false)); };
                if *index == 3 { values = [!values.iter().all(|v| *v); 3]; }
                else if let Some(value) = values.get_mut(*index) { *value = !*value; }
                else { return Some((Task::none(), false)); }
                self.p.tools_busy = true;
                let started = std::time::Instant::now();
                self.p.tools_changed_at = Some(started);
                let a = api.clone();
                Some((Task::perform(async move {
                    a.put("/config/tools", json!({"desktop_tools":values[0], "web_tools":values[1], "without_confirmation":values[2]})).await
                }, move |result| Msg::L(Lm::ToolPermissions(started, result))), false))
            }
            Msg::FactSave => {
                let (i, texto) = self.edit_fact.clone()?;
                let f = self.facts.get(i)?;
                if f.id == 0 || texto.trim().is_empty() {
                    return None;
                }
                let (id, a2) = (f.id, api.clone());
                self.edit_fact = None;
                self.facts[i].t = texto.clone();
                self.facts[i].src = "corrigido por você · hoje".into();
                Some((acao(&api, async move { a2.patch(&format!("/facts/{id}"), json!({"texto": texto})).await }, "Fato corrigido", true), false))
            }
            Msg::Refresh => Some((pull_sec(&api, Sec::Painel), true)),
            Msg::Approve(ci, mi) | Msg::Deny(ci, mi) => {
                let ok = matches!(msg, Msg::Approve(..));
                let ChatMsg::Orion(o) = self.convs.get(*ci)?.msgs.get(*mi)? else { return None };
                let id = o.approval.as_ref()?.id.clone();
                if id.is_empty() {
                    return None;
                }
                let a2 = api.clone();
                let id2 = id.clone();
                let ci2 = *ci;
                let t = Task::perform(async move { a2.post(&format!("/approvals/{id2}/decide"), json!({"approved": ok})).await }, move |r| match r {
                    Ok(_) if ok => Msg::L(Lm::Acao(Ok(format!("resume:{ci2}:{id}")), false)),
                    Ok(_) => Msg::L(Lm::Acao(Ok("Negado. Nada foi alterado.".into()), true)),
                    Err(e) => Msg::L(Lm::Acao(Err(e), true)),
                });
                Some((t, true))
            }
            Msg::RotRun(i) => {
                let nome = self.rots.get(*i)?.id.clone();
                let a2 = api.clone();
                Some((Task::perform(async move { a2.post(&format!("/rotinas/{nome}/executar"), json!({})).await }, |r| match r {
                    Ok(v) => Msg::L(Lm::Acao(Ok(format!("Rotina: {}", s(&v, "status").replace('_', " "))), true)),
                    Err(e) => Msg::L(Lm::Acao(Err(e), false)),
                }), false))
            }
            Msg::RotToggle(i) => {
                let r = self.rots.get(*i)?;
                let (nome, on) = (r.id.clone(), r.on);
                let a2 = api.clone();
                let path = if on { format!("/rotinas/{nome}/revogar") } else { format!("/rotinas/{nome}/conceder") };
                Some((acao(&api, async move { a2.post(&path, json!({})).await }, if on { "Rotina pausada" } else { "Rotina concedida" }, true), false))
            }
            Msg::FactForget(i) => {
                let f = self.facts.get(*i)?;
                if f.id == 0 {
                    return None;
                }
                let id = f.id;
                let a2 = api.clone();
                Some((acao(&api, async move { a2.delete(&format!("/facts/{id}")).await }, "Fato esquecido; nota no vault atualizada", true), false))
            }
            Msg::PinToggle => {
                let c = self.convs.get(self.conv?)?;
                let (id, fav) = (c.id.clone(), !c.pinned);
                let a2 = api.clone();
                Some((acao(&api, async move { a2.patch(&format!("/sessoes/{id}"), json!({"favorita": fav})).await }, if fav { "Conversa fixada" } else { "Conversa desafixada" }, true), true))
            }
            Msg::ArchiveConv => {
                let id = self.session_id()?;
                let a2 = api.clone();
                Some((acao(&api, async move { a2.patch(&format!("/sessoes/{id}"), json!({"arquivada": true})).await }, "Conversa arquivada", true), false))
            }
            Msg::DeleteConv => {
                let id = self.session_id()?;
                let a2 = api.clone();
                Some((acao(&api, async move {
                    // só apaga arquivada: arquiva antes
                    let _ = a2.patch(&format!("/sessoes/{id}"), json!({"arquivada": true})).await;
                    a2.delete(&format!("/sessoes/{id}")).await
                }, "Conversa apagada", true), false))
            }
            Msg::WipeConfirm => {
                let a2 = api.clone();
                Some((acao(&api, async move { a2.delete("/historico").await }, "Histórico apagado", true), true))
            }
            Msg::PanicConfirm => {
                let a2 = api.clone();
                Some((Task::perform(async move { a2.post("/modo/panico", json!({"ativo": true})).await }, |r| match r {
                    Ok(_) => Msg::L(Lm::Acao(Ok("Tudo pausado. Nenhuma ferramenta de escrita, rede ou execução roda.".into()), true)),
                    Err(e) => Msg::L(Lm::Acao(Err(e), true)),
                }), true))
            }
            Msg::PanicResume => {
                let a2 = api.clone();
                let senha = std::mem::take(&mut self.resume_pw);
                Some((Task::perform(async move { a2.post("/modo/panico", json!({"ativo": false, "senha": senha})).await }, |r| match r {
                    Ok(_) => Msg::L(Lm::Acao(Ok("Retomado.".into()), true)),
                    Err(e) => Msg::L(Lm::Acao(Err(e), true)),
                }), true))
            }
            Msg::KeySave => {
                let Some(f) = self.key_form.as_mut().filter(|f| !f.busy) else { return Some((Task::none(), false)) };
                if f.value.trim().is_empty() {
                    f.error = "Cole a chave de API.".into();
                    return Some((Task::none(), false));
                }
                f.busy = true;
                let (nome, valor) = (f.nome.clone(), f.value.trim().to_string());
                let a2 = api.clone();
                Some((Task::perform(async move { a2.post("/config/chave", json!({"nome": nome, "valor": valor})).await }, |r| Msg::L(Lm::KeySaved(r))), false))
            }
            Msg::VaultPull => {
                let a2 = api.clone();
                Some((acao(&api, async move { a2.post("/vault/pull", json!({})).await }, "Vault atualizado (git pull)", true), true))
            }
            Msg::QcapSave => {
                let q = self.qcap.as_ref()?;
                let texto = q.text.trim().to_string();
                if texto.is_empty() {
                    return None;
                }
                let a2 = api.clone();
                Some((acao(&api, async move { a2.post("/captura", json!({"texto": texto})).await }, "Salvo em 00 Inbox · commit na main", true), true))
            }
            Msg::Stop => {
                if let Some(h) = self.abort.take() {
                    h.abort();
                }
                // F05.01: parar a resposta não deixa a fila presa; ela segue na ordem.
                self.streaming = None;
                let queued_task = self.next_queued();
                Some((queued_task, true))
            }
            _ => None,
        }
    }

    /// Envia o texto para o servidor (cria a conversa se preciso) e abre o fluxo SSE.
    pub fn live_send(&mut self, text: String) -> Task<Msg> {
        if self.pending_send.is_some() { return Task::none(); }
        if self.attaching { self.toast("Espere o documento terminar de anexar."); return Task::none(); }
        let Some(api) = self.api.clone() else { return Task::none() };
        match self.conv.filter(|_| self.view == View::Chat) {
            Some(ci) => self.start_chat(ci, text),
            None => {
                self.pending_send = Some(text);
                let pid = self.projects.get(self.p.proj).map(|p| p.id.clone()).filter(|i| !i.is_empty());
                Task::perform(async move { api.post("/sessoes", json!({"project_id": pid})).await }, |r| Msg::L(Lm::Created(r)))
            }
        }
    }

    fn start_chat(&mut self, ci: usize, text: String) -> Task<Msg> {
        let Some(api) = self.api.clone() else { return Task::none() };
        let mut shown = text.clone();
        if !self.attach.is_empty() {
            shown.push_str("\n\nAnexos: ");
            shown.push_str(&self.attach.iter().map(|a| a.0.clone()).collect::<Vec<_>>().join(", "));
        }
        let via = crate::views::chat::via_label(&self.p);
        let c = &mut self.convs[ci];
        c.msgs.push(ChatMsg::User(shown));
        c.msgs.push(ChatMsg::Orion(OrionMsg { time: hm(now_secs()), via, out: String::new(), tools_label: String::new(), tools: vec![], body: String::new(), saved: false, approval: None }));
        c.state = ConvState::Running;
        self.streaming = Some(crate::app::Stream { conv: ci, words: 0, total: usize::MAX });
        self.composer = iced::widget::text_editor::Content::new();
        let ids = std::mem::take(&mut self.attachment_ids);
        let (imagens, attachments): (Vec<_>, Vec<_>) = ids.into_iter().zip(self.attach.iter().map(|a| a.1 == "imagem")).partition(|(_, imagem)| *imagem);
        let (imagens, attachments): (Vec<String>, Vec<String>) = (imagens.into_iter().map(|i| i.0).collect(), attachments.into_iter().map(|i| i.0).collect());
        self.attach.clear();
        self.attach_thumbs.clear();
        let (task, handle) = Task::run(api.stream("/chat", json!({"texto": text, "canal": "web", "anexos": attachments, "imagens": imagens})), |e| Msg::L(Lm::Chat(e))).abortable();
        self.abort = Some(handle);
        task
    }

    fn resume_chat(&mut self, ci: usize, id: String) -> Task<Msg> {
        let Some(api) = self.api.clone() else { return Task::none() };
        let c = &mut self.convs[ci];
        c.msgs.push(ChatMsg::Orion(OrionMsg { time: hm(now_secs()), via: String::new(), out: String::new(), tools_label: String::new(), tools: vec![], body: String::new(), saved: false, approval: None }));
        c.state = ConvState::Running;
        self.streaming = Some(crate::app::Stream { conv: ci, words: 0, total: usize::MAX });
        let (task, handle) = Task::run(api.stream(&format!("/approvals/{id}/resume"), json!({"canal": "web"})), |e| Msg::L(Lm::Chat(e))).abortable();
        self.abort = Some(handle);
        task
    }

    fn sync_session_state(&mut self) {
        // o índice da conversa aberta acompanha o id depois de recarregar a lista
        if let (Some(cur), Some(id)) = (self.conv, self.cur_sid.clone()) {
            let _ = cur;
            self.conv = self.convs.iter().position(|c| c.id == id).or(self.conv.filter(|i| *i < self.convs.len()));
        }
    }

    pub fn lm(&mut self, m: Lm) -> Task<Msg> {
        let now = now_secs();
        if matches!(&m, Lm::Login(_) | Lm::Desbloqueado(_) | Lm::TwoFactor(_)) { self.loading = None; }
        match m {
            Lm::LanguageSaved(result) => { if let Err(error) = result { self.toast(error); } },
            Lm::ToolPermissions(started, result) => {
                // Um GET iniciado antes da alteração não pode sobrescrever o estado recém-salvo.
                if self.p.tools_changed_at.is_some_and(|changed| started < changed) { return Task::none(); }
                if self.p.tools_busy && self.p.tools_changed_at != Some(started) { return Task::none(); }
                self.p.tools_busy = false;
                self.p.tools_changed_at = Some(std::time::Instant::now());
                match result {
                    Ok(value) => self.p.tools = Some([value["desktop_tools"] == true, value["web_tools"] == true, value["without_confirmation"] == true]),
                    Err(error) => self.toast(error),
                }
            },
            Lm::ArtifactList(session, result) => {
                if self.session_id().as_deref() != Some(session.as_str()) { return Task::none(); }
                match result { Ok(value) => self.artifact_records = value.as_array().cloned().unwrap_or_default(), Err(error) => self.toast(error) }
            }
            Lm::ArtifactSaved(result) => {
                self.artifact_busy = false;
                match result {
                    Ok(value) => { self.artifact_records.retain(|a| a["id"] != value["id"]); self.artifact_records.push(value); self.toast("Versão do artefato guardada."); },
                    Err(error) => self.toast(error),
                }
            }
            Lm::ArtifactDetail(session, result) => {
                if self.session_id().as_deref() != Some(session.as_str()) { return Task::none(); }
                match result { Ok(value) => self.artifact_detail = Some(value), Err(error) => self.toast(error) }
            },
            Lm::Attached(generation, result) => {
                self.attaching = false;
                if generation != self.attachment_generation {
                    return self.next_queued();
                }
                match result {
                    Ok(Some(value)) => {
                        let id = s(&value, "id").to_string();
                        if s(&value, "kind") == "imagem" {
                            let thumb: Vec<u8> = value["thumb"].as_array().map(|a| a.iter().filter_map(|n| n.as_u64().map(|n| n as u8)).collect()).unwrap_or_default();
                            if !thumb.is_empty() { self.attach_thumbs.insert(id.clone(), iced::widget::image::Handle::from_bytes(thumb)); }
                            self.toast("Imagem anexada. Ela vai com sua próxima mensagem e não fica guardada.");
                        } else { self.toast("Anexo pronto. O texto será enviado com sua próxima mensagem."); }
                        self.attachment_ids.push(id);
                        self.attach.push((s(&value, "name").into(), s(&value, "kind").into()));
                    }
                    Ok(None) => {}, Err(error) => self.toast(error),
                }
            }
            Lm::Initialized(result) => {
                self.loading = None;
                if let Err(error) = result { self.gate_err = error; return Task::none(); }
                if self.db_locked { return Task::none(); }
                if let Some(api) = self.api.clone() { return Task::perform(async move { api.health().await?; Ok(api.auth_ok().await) }, |r| Msg::L(Lm::Boot(r))); }
            }
            Lm::Boot(r) => match r {
                Ok(true) => {
                    self.logged = true;
                    self.sync_profile();
                    self.gate_err.clear();
                    if let Some(api) = self.api.clone() {
                        return pull_basic(&api);
                    }
                }
                Ok(false) => self.gate_err.clear(),
                Err(e) => self.gate_err = e,
            },
            Lm::Desbloqueado(r) => match r {
                Ok(()) => {
                    self.db_locked = false;
                    self.gate_err = "Conectando ao núcleo…".into();
                    if let Some(api) = self.api.clone() {
                        return Task::perform(async move { api.health().await?; Ok(api.auth_ok().await) }, |r| Msg::L(Lm::Boot(r)));
                    }
                }
                Err(e) => self.gate_err = e,
            },
            Lm::Login(r) => match r {
                Ok(LoginResult::Ok) => {
                    self.db_locked=false;
                    self.logged = true;
                    self.sync_profile();
                    self.gate_err.clear();
                    self.challenge = None;
                    self.view = View::Home;
                    if let Some(api) = self.api.clone() {
                        return pull_basic(&api);
                    }
                }
                Ok(LoginResult::TwoFactor(ch)) => {
                    self.challenge = Some(ch);
                    self.gate_err = "Digite o código enviado ao seu Telegram.".into();
                }
                Err(e) => self.gate_err = e,
            },
            Lm::TwoFactor(r) => match r {
                Ok(()) => {
                    self.db_locked=false;
                    self.logged = true;
                    self.sync_profile();
                    self.challenge = None;
                    self.gate_err.clear();
                    self.view = View::Home;
                    if let Some(api) = self.api.clone() {
                        return pull_basic(&api);
                    }
                }
                Err(e) => self.gate_err = e,
            },
            Lm::Sessions(r) => match r {
                Ok(v) => {
                    let cur = self.cur_sid.clone();
                    let (list, _) = map_sessions(&v, &self.convs, &self.projects, now);
                    self.convs = list;
                    if self.view == View::Chat {
                        self.conv = cur.and_then(|id| self.convs.iter().position(|c| c.id == id));
                        if self.conv.is_none() {
                            self.view = View::Home;
                        }
                    }
                    self.sync_session_state();
                }
                Err(e) => self.net_fail(e),
            },
            Lm::Opened(sid, r) => match r {
                Ok(v) => {
                    if let Some(i) = self.convs.iter().position(|c| c.id == sid) {
                        let keep: Vec<ChatMsg> = self.convs[i].msgs.iter().filter(|m| matches!(m, ChatMsg::Orion(o) if o.approval.as_ref().is_some_and(|a| a.state == ApprState::Open))).cloned().collect();
                        let streaming_here = self.streaming.as_ref().is_some_and(|s| s.conv == i);
                        if !streaming_here {
                            let mut h = map_history(&v);
                            h.extend(keep);
                            self.convs[i].msgs = h;
                        }
                        self.cur_sid = Some(sid);
                        self.conv = Some(i);
                    }
                    return self.buscar_imagens();
                }
                Err(e) => self.net_fail(e),
            },
            Lm::Created(r) => match r {
                Ok(v) => {
                    let id = s(&v, "sessao_id").to_string();
                    let proj = v.get("project_id").and_then(Value::as_str).and_then(|p| self.projects.iter().find(|x| x.id == p)).map(|p| p.n.clone()).unwrap_or_else(|| "Sem projeto".into());
                    self.convs.insert(0, Conv { id: id.clone(), title: "Nova conversa".into(), proj, origin: None, day: "Hoje".into(), state: ConvState::Normal, ctx: 0, branch: None, pinned: false, note: None, pin: None, banner: None, msgs: vec![] });
                    for h in self.hist.iter_mut().chain(self.fwd.iter_mut()) {
                        if let Some(c) = &mut h.1 {
                            *c += 1;
                        }
                    }
                    self.cur_sid = Some(id);
                    self.view = View::Chat;
                    self.conv = Some(0);
                    if let Some(t) = self.pending_send.take() {
                        return self.start_chat(0, t);
                    }
                }
                Err(e) => {
                    self.pending_send = None;
                    self.net_fail(e)
                }
            },
            Lm::Approvals(r) => match r {
                Ok(v) => self.apply_approvals(&v),
                Err(e) => self.net_fail(e),
            },
            Lm::Rotinas(r) => match r {
                Ok(v) => self.rots = map_rotinas(&v),
                Err(e) => self.net_fail(e),
            },
            Lm::ProjectCreated(request_id, r) => {
                if self.project_form.as_ref().is_none_or(|f| !f.busy || f.request_id != request_id) { return Task::none(); }
                match r {
                    Ok(v) if !s(&v, "id").is_empty() => {
                        for project in map_projects(&json!([v]), now_secs()) {
                            if !self.projects.iter().any(|p| p.id == project.id) { self.projects.push(project); }
                        }
                        self.proj_q.clear();
                        self.project_form = None;
                        let nota = &v["vault_note"];
                        let msg = if nota["ok"].as_bool().unwrap_or(false) {
                            if nota["resultado"]["unchanged"].as_bool().unwrap_or(false) { "Projeto criado. A nota do projeto já existia no vault e não foi alterada." } else { "Projeto criado e nota gravada no vault." }
                        } else { "Projeto criado, mas a nota não foi gravada no vault. Confira a pasta do vault em Configurações › Memória." };
                        self.toast(msg);
                    }
                    result => {
                        if let Some(form) = self.project_form.as_mut() {
                            form.busy = false;
                            form.error = result.err().unwrap_or_else(|| "Resposta inesperada. Confira a lista de projetos antes de tentar de novo.".into());
                        }
                    }
                }
            }
            Lm::Keys(r) => {
                if let Ok(v) = r {
                    if let Some(o) = v.as_object() {
                        self.keys = o.iter().map(|(k, x)| (k.clone(), x.as_bool().unwrap_or(false))).collect();
                    }
                }
            }
            Lm::KeySaved(r) => match r {
                Ok(value) => {
                    if let Some(f) = self.key_form.take() {
                        self.keys.insert(f.nome.clone(), true);
                        self.toast(if value["reiniciar"] == true { format!("Chave de {} guardada no cofre. Reabra o Orion para usar esta integração.", f.label) } else { format!("Chave de {} guardada no cofre e disponível para uso.", f.label) });
                    }
                }
                Err(e) => {
                    if let Some(f) = self.key_form.as_mut() {
                        f.busy = false;
                        f.error = e;
                    }
                }
            },
            Lm::Renamed(request_id, sid, r) => {
                if !self.rename_form.as_ref().is_some_and(|f| f.session_id == sid && f.busy && f.request_id == request_id) { return Task::none(); }
                match r {
                    Ok(v) if !s(&v, "titulo").trim().is_empty() => {
                        if let Some(c) = self.convs.iter_mut().find(|c| c.id == sid) { c.title = s(&v, "titulo").to_string(); }
                        self.rename_form = None;
                        self.toast("Conversa renomeada.");
                    }
                    result => {
                        if let Some(form) = self.rename_form.as_mut() {
                            form.busy = false;
                            form.error = result.err().unwrap_or_else(|| "O núcleo não confirmou o novo título.".into());
                        }
                    }
                }
            }
            Lm::Projects(r) => match r {
                Ok(v) => self.projects = map_projects(&v, now),
                Err(e) => self.net_fail(e),
            },
            Lm::Conflitos(r) => {
                // Vault não configurado ou sem pasta de memória: não há conflito para mostrar, e isso não é erro.
                self.conflitos = r.ok().map(|v| v.as_array().map(|a| a.as_slice()).unwrap_or(&[]).iter().map(|c| (s(c, "original_path").to_string(), s(c, "proposed_path").to_string(), c["original_exists"].as_bool().unwrap_or(false))).collect()).unwrap_or_default();
            }
            Lm::ConflitoAberto(r) => match r {
                Ok(v) => {
                    let (original, proposal) = (s(&v, "original").to_string(), s(&v, "proposal").to_string());
                    self.conflito = Some(crate::app::ConflitoState {
                        proposed_path: s(&v, "proposed_path").into(),
                        original_path: s(&v, "original_path").into(),
                        sha: s(&v, "original_sha256").into(),
                        content: iced::widget::text_editor::Content::with_text(&original),
                        original,
                        proposal,
                        busy: false,
                    });
                    let agora = std::time::Instant::now();
                    self.at.modal = Some(agora);
                    self.at.scrim = Some(agora);
                }
                Err(e) => self.toast(e),
            },
            Lm::ConflitoResolvido(r) => match r {
                Ok(msg) => {
                    self.conflito = None;
                    self.toast(msg);
                    if let Some(api) = self.api.clone() { return Task::perform(async move { api.get("/vault/conflitos").await }, |r| Msg::L(Lm::Conflitos(r))); }
                }
                Err(e) => { if let Some(c) = &mut self.conflito { c.busy = false; } self.toast(e); }
            },
            Lm::Facts(r) => match r {
                Ok(v) => self.facts = map_facts(&v),
                Err(e) => self.net_fail(e),
            },
            Lm::Painel(r) => match r {
                Ok(v) => {
                    self.acts = map_decisions(&v);
                    self.painel = Some(v);
                }
                Err(e) => self.net_fail(e),
            },
            Lm::Budget(r) => match r {
                Ok(v) => self.budget = Some(v),
                Err(e) => self.net_fail(e),
            },
            Lm::Modo(r) => match r {
                Ok(v) => {
                    let panico = v.get("panico").and_then(Value::as_bool).unwrap_or(false);
                    self.panic = if panico { Panic::Paused } else if self.panic == Panic::Paused { Panic::Idle } else { self.panic };
                    self.p.dnd = v.get("nao_perturbe").and_then(Value::as_bool).unwrap_or(false);
                    if let Some(b) = v.get("autonomo").and_then(Value::as_bool) {
                        self.p.auto = b;
                    }
                }
                Err(e) => self.net_fail(e),
            },
            Lm::Mcp(r) => match r {
                Ok(v) => self.mcp = map_mcp(&v),
                Err(e) => self.net_fail(e),
            },
            Lm::Skills(r) => match r {
                Ok(v) => self.skills = map_skills(&v),
                Err(e) => self.net_fail(e),
            },
            Lm::Nuvem(r) => {
                self.nuvem = match r {
                    Ok(v) => arr(&v, "tarefas")
                        .iter()
                        .map(|t| data::NuvemItem { id: s(t, "id").into(), svc: if s(t, "servico") == "codex" { "Codex".into() } else { "Claude".into() }, t: s(t, "titulo").into(), estado: s(t, "estado").into(), url: s(t, "url").into() })
                        .collect(),
                    Err(_) => vec![],
                };
            }
            Lm::Tarefas(r) => match r {
                Ok(v) => self.cloud = map_tarefas(&v),
                Err(e) => self.net_fail(e),
            },
            Lm::Modelos(r) => match r {
                Ok(v) => self.locais = map_modelos(&v),
                Err(e) => self.net_fail(e),
            },
            Lm::GraphReady(result) => match result { Ok(graph) => self.mem = crate::graph::Graph::from_prepared(graph), Err(error) => self.toast(error) },
            Lm::Grafo(r) => match r {
                Ok(v) => return Task::perform(async move { tokio::task::spawn_blocking(move || crate::graph::Graph::prepare(&v)).await.map_err(|e| e.to_string()) }, |result| Msg::L(Lm::GraphReady(result))),
                Err(e) => self.net_fail(e),
            },
            Lm::Acao(r, reload) => match r {
                Ok(m) => {
                    if let Some(rest) = m.strip_prefix("resume:") {
                        let mut it = rest.splitn(2, ':');
                        let ci: usize = it.next().and_then(|x| x.parse().ok()).unwrap_or(0);
                        let id = it.next().unwrap_or("").to_string();
                        if ci < self.convs.len() {
                            return self.resume_chat(ci, id);
                        }
                    } else {
                        self.toast(m);
                    }
                    if reload {
                        return self.reload_all();
                    }
                }
                Err(e) => {
                    self.net_fail(e);
                    if reload {
                        return self.reload_all();
                    }
                }
            },
            Lm::Chat(ev) => return self.on_chat(ev),
            Lm::Imagem(url, r) => {
                // `imgs_pedidas` guarda só o que está a caminho: ao terminar (bem ou mal) a URL sai, e uma imagem
                // que o cache apagou depois pode ser baixada de novo
                self.imgs_pedidas.remove(&url);
                if let Ok(bytes) = r {
                    let _ = crate::imgcache::guardar_url(&url, &bytes);
                }
            }
        }
        Task::none()
    }

    /// Baixa do núcleo as imagens geradas (`![alt](/imagens/img-….png)`) citadas na conversa aberta, uma vez cada.
    fn buscar_imagens(&mut self) -> Task<Msg> {
        let Some(api) = self.api.clone() else { return Task::none() };
        let Some(c) = self.conv.and_then(|i| self.convs.get(i)) else { return Task::none() };
        let mut urls: Vec<String> = vec![];
        for m in &c.msgs {
            if let ChatMsg::Orion(o) = m {
                for b in crate::md::parse(&o.body) {
                    if let crate::md::Block::Image(_, src) = b {
                        if crate::imgcache::url_do_nucleo(&src) && crate::imgcache::da_url(&src).is_none() && !urls.contains(&src) {
                            urls.push(src);
                        }
                    }
                }
            }
        }
        let tarefas: Vec<Task<Msg>> = urls
            .into_iter()
            .filter(|u| self.imgs_pedidas.insert(u.clone()))
            .map(|u| {
                let a = api.clone();
                let u2 = u.clone();
                Task::perform(async move { a.bytes(&u2).await }, move |r| Msg::L(Lm::Imagem(u.clone(), r)))
            })
            .collect();
        Task::batch(tarefas)
    }

    fn reload_all(&mut self) -> Task<Msg> {
        match self.api.clone() {
            Some(api) => Task::batch([pull_basic(&api), pull_page(&api, self.view)]),
            None => Task::none(),
        }
    }

    fn net_fail(&mut self, e: String) {
        if e.contains("Acesso negado") && self.logged {
            // sessão expirou: volta para a entrada
            self.logged = false;
            self.gate_err = "Sua sessão expirou. Entre de novo.".into();
            return;
        }
        self.toast(e);
    }

    fn apply_approvals(&mut self, v: &Value) {
        let list = v.as_array().map(|a| a.as_slice()).unwrap_or(&[]);
        let open: Vec<&Value> = list.iter().collect();
        // some o que já não está pendente
        for c in self.convs.iter_mut() {
            for m in c.msgs.iter_mut() {
                if let ChatMsg::Orion(o) = m {
                    if let Some(a) = &mut o.approval {
                        if a.state == ApprState::Open && !a.id.is_empty() && !open.iter().any(|p| s(p, "id") == a.id) {
                            a.state = ApprState::Approved("Resolvida.".into());
                        }
                    }
                }
            }
            if c.state == ConvState::Pending && !c.msgs.iter().any(|m| matches!(m, ChatMsg::Orion(o) if o.approval.as_ref().is_some_and(|a| a.state == ApprState::Open))) {
                c.state = ConvState::Normal;
            }
        }
        for p in open {
            let sid = s(p, "session_id");
            let id = s(p, "id");
            let Some(c) = self.convs.iter_mut().find(|c| c.id == sid) else { continue };
            let exists = c.msgs.iter().any(|m| matches!(m, ChatMsg::Orion(o) if o.approval.as_ref().is_some_and(|a| a.id == id)));
            if !exists {
                c.msgs.push(ChatMsg::Orion(OrionMsg { time: "pendente".into(), via: String::new(), out: String::new(), tools_label: String::new(), tools: vec![], body: String::new(), saved: false, approval: Some(map_approval(p)) }));
            }
            if c.state != ConvState::Running {
                c.state = ConvState::Pending;
            }
        }
        self.rot_pend = false;
    }

    fn on_chat(&mut self, ev: Chat) -> Task<Msg> {
        let Some(ci) = self.streaming.as_ref().map(|s| s.conv) else { return Task::none() };
        let Some(conv) = self.convs.get_mut(ci) else { return Task::none() };
        let Some(ChatMsg::Orion(o)) = conv.msgs.last_mut() else { return Task::none() };
        match ev {
            Chat::Text(t) => o.body.push_str(&t),
            Chat::Model(m) => o.via = m,
            Chat::Tool(v) => {
                let name = ["name", "label"].iter().find_map(|k| v.get(*k).and_then(Value::as_str)).unwrap_or("ferramenta");
                let label = v.get("label").and_then(Value::as_str).unwrap_or(name);
                let summary = v.get("summary").and_then(Value::as_str).unwrap_or("");
                let estado = match v.get("state").and_then(Value::as_str).unwrap_or("") {
                    "completed" | "ok" | "done" => "ok",
                    "failed" | "error" => "falhou",
                    "running" => "rodando",
                    "waiting_approval" | "pending_approval" => "aguardando aprovação",
                    "denied" => "negada",
                    outro => outro,
                };
                let call = v.get("call_id").and_then(Value::as_str).unwrap_or("");
                if let Some(t) = o.tools.iter_mut().find(|t| !call.is_empty() && t.id == call) {
                    t.time = estado.to_string();
                    if !summary.is_empty() {
                        t.value = summary.to_string();
                    }
                } else {
                    let kind = humanizar(label);
                    let value = if summary.is_empty() { name.to_string() } else { summary.to_string() };
                    o.tools.push(Tool { id: call.to_string(), kind, value, time: estado.to_string() });
                }
                o.tools_label = if o.tools.len() == 1 { "Usou 1 ferramenta".into() } else { format!("Usou {} ferramentas", o.tools.len()) };
            }
            Chat::Approval { id, tool, risk, reason, args, truncated } => {
                let a = json!({"id": id, "tool": tool, "risk": risk, "reason": reason, "args": args, "args_truncated": truncated});
                o.approval = Some(map_approval(&a));
                conv.state = ConvState::Pending;
            }
            Chat::Error(e) if e == "confirmar_envio_privado" => {
                // F02.26: o último pedido do usuário espera a confirmação; nada é enviado a Google ou Mistral sem ela.
                self.private_confirm = conv.msgs.iter().rev().find_map(|m| if let ChatMsg::User(s) = m { Some(s.clone()) } else { None });
            }
            Chat::Error(e) => {
                if o.body.is_empty() {
                    o.via.clear(); // ninguém respondeu: não atribui a resposta a um provedor
                } else {
                    o.body.push_str("\n\n");
                }
                o.body.push_str(&format!("**{}:** {}", crate::i18n::text("Erro"), crate::i18n::status(&e)));
                self.toast(e);
            }
            Chat::Done { .. } => {
                o.saved = true;
                if conv.state == ConvState::Running {
                    conv.state = ConvState::Normal;
                }
                self.streaming = None;
                self.abort = None;
                if o.body.is_empty() && o.approval.is_none() && o.tools.is_empty() {
                    conv.msgs.pop();
                }
                let queued_task = self.next_queued();
                return Task::batch([self.reload_all(), self.buscar_imagens(), queued_task]);
            }
        }
        Task::none()
    }
}

#[allow(dead_code)]
fn _use(_: &net::Chat) {
    let _ = data::OBJ;
}

/// `AAAA-MM-DD` (UTC−3) de um instante em segundos.
pub fn iso_date(secs: f64) -> String {
    let d = ((secs + tz_offset()) / 86400.0).floor() as i64 + 719468;
    let era = d.div_euclid(146097);
    let doe = d - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{:04}-{:02}-{:02}", if month <= 2 { y + 1 } else { y }, month, day)
}
