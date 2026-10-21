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
    Facts(Result<Value, String>),
    Results(Result<Value, String>),
    Painel(Result<Value, String>),
    Budget(Result<Value, String>),
    Modo(Result<Value, String>),
    Grafo(Result<Value, String>),
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

/// "Hoje", "Ontem", "7 dias" ou "Mais antigos", pelo dia local (UTC−3).
pub fn day_group(secs: f64, now: f64) -> &'static str {
    let day = |t: f64| ((t - 3.0 * 3600.0) / 86400.0).floor();
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

/// HH:MM (UTC−3) de um instante em segundos.
pub fn hm(secs: f64) -> String {
    let s = (secs - 3.0 * 3600.0).rem_euclid(86400.0) as i64;
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
    let args = a.get("args").cloned().unwrap_or(Value::Null);
    let resumo = match &args {
        Value::Object(o) if !o.is_empty() => o.iter().take(4).map(|(k, v)| format!("{k}: {}", v.as_str().map(String::from).unwrap_or_else(|| v.to_string()))).collect::<Vec<_>>().join(" · "),
        _ => String::new(),
    };
    let reason = s(a, "reason");
    let mut text = format!("`{}`", s(a, "tool"));
    if !reason.is_empty() {
        text = format!("{reason} {text}");
    }
    if !resumo.is_empty() {
        text = format!("{text}. {resumo}");
    }
    Approval { id: s(a, "id").to_string(), text, state: ApprState::Open, diff_button: false }
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

pub fn map_results(v: &Value) -> Vec<(String, String, String, String, String)> {
    arr(v, "resultados")
        .iter()
        .map(|r| {
            let criado = iso_to_secs(s(r, "criado")).map(|t| format!("{}", day_label(t))).unwrap_or_default();
            let previa = r.get("previa").and_then(Value::as_str).or_else(|| r.get("conversa").and_then(Value::as_str)).unwrap_or("").to_string();
            (s(r, "nome").to_string(), previa, criado, s(r, "tipo").to_string(), format!("v{}", r.get("versao").and_then(Value::as_i64).unwrap_or(1)))
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
        Sec::Conectores => Task::batch([get("/mcp/servidores", Lm::Mcp), get("/skills", Lm::Skills)]),
        Sec::Locais => get("/modelos-locais", Lm::Modelos),
        _ => Task::none(),
    }
}

fn day_label(secs: f64) -> String {
    let d = ((secs - 3.0 * 3600.0) / 86400.0).floor() as i64 + 719468;
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

// ── tarefas ──
pub fn pull_basic(api: &Api) -> Task<Msg> {
    Task::batch([
        { let a = api.clone(); Task::perform(async move { a.get("/sessoes?limite=100").await }, |r| Msg::L(Lm::Sessions(r))) },
        { let a = api.clone(); Task::perform(async move { a.get("/approvals").await }, |r| Msg::L(Lm::Approvals(r))) },
        { let a = api.clone(); Task::perform(async move { a.get("/projects").await }, |r| Msg::L(Lm::Projects(r))) },
        { let a = api.clone(); Task::perform(async move { a.get("/modo").await }, |r| Msg::L(Lm::Modo(r))) },
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
        View::Resultados => get("/resultados", Lm::Results),
        View::Memoria => get("/grafo/completo", Lm::Grafo),
        View::Projetos => get("/projects", Lm::Projects),
        View::Painel => Task::batch([get("/painel", Lm::Painel), get("/budget", Lm::Budget)]),
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
        a.convs.clear();
        a.projects.clear();
        a.cloud.clear();
        a.mcp.clear();
        a.skills.clear();
        a.locais.clear();
        a.results.clear();
        a.facts.clear();
        a.rots.clear();
        a.acts.clear();
        a.archived.clear();
        a.rot_pend = false;
        a.term_out.clear();
        a.p.proj = 99;
        a.mem = crate::graph::Graph::empty();
        if let Some(defaults) = api.primeiro_uso() {
            a.onboarding = Some(crate::onboarding::Form::new(defaults));
            a.api = Some(api);
            return (a, Task::none());
        }
        let t = { let api2 = api.clone(); Task::perform(async move { api2.health().await?; Ok(api2.auth_ok().await) }, |r| Msg::L(Lm::Boot(r))) };
        if api.bloqueado() {
            a.db_locked = true;
            a.gate_err = "Digite a senha do Orion para abrir.".into();
            a.api = Some(api);
            return (a, Task::none());
        }
        a.api = Some(api);
        a.gate_err = "Conectando ao servidor…".into();
        (a, t)
    }

    pub fn session_id(&self) -> Option<String> {
        self.conv.and_then(|i| self.convs.get(i)).map(|c| c.id.clone())
    }

    /// Mensagens que precisam de chamada ao servidor. `true` = depois ainda roda a lógica local.
    pub fn live_before(&mut self, msg: &Msg) -> Option<(Task<Msg>, bool)> {
        let api = self.api.clone()?;
        match msg {
            Msg::LoginCancel => {
                if let Some(ch)=self.challenge.take(){api.cancelar_entrada(&ch);}
                self.pw.clear();self.gate_err.clear();
                Some((Task::none(),false))
            }
            Msg::Hello => {
                self.gate_err="Confirme no Windows Hello…".into();
                Some((Task::perform(async move{api.hello().await},|r|Msg::L(Lm::Desbloqueado(r))),false))
            }
            Msg::Login => {
                if self.pw.trim().is_empty() {
                    return None;
                }
                let pw = std::mem::take(&mut self.pw);
                if self.db_locked && self.challenge.is_none() {
                    self.gate_err = "Abrindo…".into();
                    return Some((Task::perform(async move { api.entrar(pw).await }, |r| Msg::L(Lm::Login(r))), false));
                }
                self.gate_err = "Entrando…".into();
                if let Some(ch) = self.challenge.clone() {
                    return Some((Task::perform(async move { api.second_factor(&ch, &pw).await }, |r| Msg::L(Lm::TwoFactor(r))), false));
                }
                let usuario = self.user.clone();
                Some((Task::perform(async move { api.login(&usuario, &pw).await }, |r| Msg::L(Lm::Login(r))), false))
            }
            Msg::Logout => {
                Some((Task::perform(async move { api.logout().await }, |_| Msg::Noop), true))
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
            Msg::Refresh => Some((pull_page(&api, View::Painel), true)),
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
                None.or(Some((Task::none(), true)))
            }
            _ => None,
        }
    }

    /// Envia o texto para o servidor (cria a conversa se preciso) e abre o fluxo SSE.
    pub fn live_send(&mut self, text: String) -> Task<Msg> {
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
        self.attach.clear();
        let (task, handle) = Task::run(api.stream("/chat", json!({"texto": text, "canal": "web"})), |e| Msg::L(Lm::Chat(e))).abortable();
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
        match m {
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
            Lm::Projects(r) => match r {
                Ok(v) => self.projects = map_projects(&v, now),
                Err(e) => self.net_fail(e),
            },
            Lm::Facts(r) => match r {
                Ok(v) => self.facts = map_facts(&v),
                Err(e) => self.net_fail(e),
            },
            Lm::Results(r) => match r {
                Ok(v) => self.results = map_results(&v),
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
            Lm::Grafo(r) => match r {
                Ok(v) => self.mem = crate::graph::Graph::from_server(&v),
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
        }
        Task::none()
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
                let name = ["label", "name"].iter().find_map(|k| v.get(*k).and_then(Value::as_str)).unwrap_or("ferramenta");
                let summary = v.get("summary").and_then(Value::as_str).unwrap_or("");
                let estado = v.get("state").and_then(Value::as_str).unwrap_or("");
                let call = v.get("call_id").and_then(Value::as_str).unwrap_or("");
                if let Some(t) = o.tools.iter_mut().find(|t| !call.is_empty() && t.kind == format!("#{call}")) {
                    t.time = estado.to_string();
                } else {
                    o.tools.push(Tool { kind: if call.is_empty() { name.to_string() } else { format!("#{call}") }, value: if summary.is_empty() { name.to_string() } else { format!("{name} · {summary}") }, time: estado.to_string() });
                }
                o.tools_label = if o.tools.len() == 1 { "Usou 1 ferramenta".into() } else { format!("Usou {} ferramentas", o.tools.len()) };
            }
            Chat::Approval { id, tool, reason, args } => {
                let a = json!({"id": id, "tool": tool, "reason": reason, "args": args});
                o.approval = Some(map_approval(&a));
                conv.state = ConvState::Pending;
            }
            Chat::Error(e) => {
                if !o.body.is_empty() {
                    o.body.push_str("\n\n");
                }
                o.body.push_str(&format!("**Erro:** {e}"));
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
                return self.reload_all();
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
    let d = ((secs - 3.0 * 3600.0) / 86400.0).floor() as i64 + 719468;
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
