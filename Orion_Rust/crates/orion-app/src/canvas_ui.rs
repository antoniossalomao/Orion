//! Estado e mensagens do canvas do Maestro (tela "Maestro"): espaços, nós e setas vêm do servidor
//! (`/canvas`); o app guarda a câmera, o que está sendo arrastado e os rascunhos.
//!
//! (REGRAS.md: a saída de um chat só entra em outro por clique na seta, como dado; chat do nó só lê e
//! propõe; aprovação só por canal autenticado.)

use crate::app::{App, Msg, View};
use crate::net::Api;
use iced::widget::text_editor;
use iced::Task;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

/// Níveis de zoom: só o primeiro mostra o conteúdo dos nós (chat, nota); os outros viram mapa.
pub const ZOOMS: [f32; 4] = [1.0, 0.7, 0.5, 0.35];
/// Altura do cabeçalho do nó (desenhado no canvas, em pixels do mundo).
pub const HEAD: f32 = 40.0;
pub const POLL: Duration = Duration::from_secs(2);

/// Agentes que um chat pode usar, na ordem do seletor.
pub const AGENTS: [(&str, &str); 3] = [("orion", "Orion"), ("claude", "Claude"), ("codex", "Codex")];
pub const CLAUDE_MODELS: [(&str, &str); 3] = [("sonnet", "Sonnet"), ("opus", "Opus"), ("haiku", "Haiku")];

#[derive(Debug, Clone, Default)]
pub struct CMsg {
    /// user | ai | dado | erro
    pub role: String,
    pub text: String,
    pub from: String,
    pub model: String,
}

#[derive(Debug, Clone, Default)]
pub struct CNode {
    pub id: String,
    /// chat | tarefa | nota | pagina | moldura
    pub kind: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub title: String,
    pub text: String,
    pub url: String,
    pub agent: String,
    pub model: String,
    pub effort: String,
    /// Estado da subtarefa do Maestro (nó de tarefa).
    pub state: String,
    pub out: String,
    pub mission: String,
    pub mission_state: String,
    pub by_maestro: bool,
    pub msgs: Vec<CMsg>,
}

#[derive(Debug, Clone)]
pub struct CEdge {
    pub id: String,
    pub a: String,
    pub b: String,
}

#[derive(Debug, Clone, Default)]
pub struct CMission {
    pub id: String,
    pub request: String,
    pub state: String,
    pub summary: String,
    pub error: String,
}

pub struct CanvasUi {
    pub spaces: Vec<(String, String, usize)>,
    pub cur: String,
    pub nodes: Vec<CNode>,
    pub edges: Vec<CEdge>,
    pub missions: Vec<CMission>,
    /// Câmera: deslocamento em pixels da tela e índice em [`ZOOMS`].
    pub pan: (f32, f32),
    pub zoom: usize,
    pub sel: Option<String>,
    pub drafts: HashMap<String, String>,
    pub busy: HashSet<String>,
    pub notes: HashMap<String, text_editor::Content>,
    pub dirty: HashSet<String>,
    pub mission_in: String,
    pub url_in: Option<String>,
    /// Quem é o Maestro (`claude`, `codex` ou `orion`) e as opções vindas do servidor.
    pub maestro: String,
    pub options: Vec<Value>,
    pub fitted: bool,
    pub dragging: bool,
    pub loaded: bool,
}

impl Default for CanvasUi {
    fn default() -> Self {
        CanvasUi {
            spaces: vec![],
            cur: String::new(),
            nodes: vec![],
            edges: vec![],
            missions: vec![],
            pan: (40.0, 40.0),
            zoom: 0,
            sel: None,
            drafts: HashMap::new(),
            busy: HashSet::new(),
            notes: HashMap::new(),
            dirty: HashSet::new(),
            mission_in: String::new(),
            url_in: None,
            maestro: "claude".into(),
            options: vec![],
            fitted: false,
            dragging: false,
            loaded: false,
        }
    }
}

impl CanvasUi {
    pub fn z(&self) -> f32 {
        ZOOMS[self.zoom.min(ZOOMS.len() - 1)]
    }
    pub fn detail(&self) -> bool {
        self.zoom == 0
    }
    pub fn node(&self, id: &str) -> Option<&CNode> {
        self.nodes.iter().find(|n| n.id == id)
    }
    fn node_mut(&mut self, id: &str) -> Option<&mut CNode> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }
    /// Missão do Maestro ainda em andamento ou esperando o Antônio.
    pub fn active_missions(&self) -> impl Iterator<Item = &CMission> {
        self.missions.iter().filter(|m| ["planejando", "aguarda_aprovacao", "rodando", "integrando"].contains(&m.state.as_str()))
    }
    pub fn needs_poll(&self) -> bool {
        self.active_missions().next().is_some() || !self.busy.is_empty()
    }
}

#[derive(Debug, Clone)]
pub enum Cv {
    /// A cada 2 s com a tela aberta: grava notas editadas e atualiza o espaço.
    Poll,
    Spaces(Result<Value, String>),
    SpacesOnly(Value),
    Space(Result<Value, String>),
    Config(Result<Value, String>),
    SelectSpace(String),
    NewSpace,
    Add(&'static str),
    UrlOpen,
    UrlIn(String),
    UrlGo,
    Added(Result<Value, String>),
    Select(Option<String>),
    Drag(String, f32, f32),
    Resize(String, f32, f32),
    Commit(String),
    Pan(f32, f32),
    Zoom(i32),
    Fit,
    Link(String, String),
    DelNode(String),
    DelEdge(String),
    SendEdge(String),
    Title(String, String),
    Draft(String, String),
    Send(String),
    Reply(String, Result<Value, String>),
    Note(String, text_editor::Action),
    Agent(String, usize),
    Model(String, usize),
    MissionIn(String),
    MissionGo,
    SetMaestro(String),
    /// Resposta de uma ação sem retorno útil: recarrega o espaço.
    Done(Result<String, String>),
}

fn s(v: &Value, k: &str) -> String {
    v[k].as_str().unwrap_or("").to_string()
}

fn parse_node(v: &Value) -> CNode {
    let f = |k: &str, d: f64| v[k].as_f64().unwrap_or(d) as f32;
    CNode {
        id: s(v, "id"),
        kind: s(v, "tipo"),
        x: f("x", 0.0),
        y: f("y", 0.0),
        w: f("w", 320.0),
        h: f("h", 240.0),
        title: s(v, "titulo"),
        text: s(v, "texto"),
        url: s(v, "url"),
        agent: s(v, "agente"),
        model: s(v, "modelo"),
        effort: s(v, "esforco"),
        state: s(v, "estado"),
        out: s(v, "saida"),
        mission: s(v, "missao"),
        mission_state: s(v, "missao_estado"),
        by_maestro: v["do_maestro"].as_bool().unwrap_or(false),
        msgs: v["mensagens"].as_array().map(|l| l.iter().map(|m| CMsg { role: s(m, "papel"), text: s(m, "texto"), from: s(m, "de"), model: s(m, "modelo") }).collect()).unwrap_or_default(),
    }
}

/// Carrega a lista de espaços (e, em cadeia, o espaço atual e a configuração).
pub fn fetch(api: &Api) -> Task<Msg> {
    let a = api.clone();
    Task::perform(async move { a.get("/canvas").await }, |r| Msg::Canvas(Cv::Spaces(r)))
}

/// Só a lista de espaços, sem recarregar o espaço atual.
fn fetch_list(api: &Api) -> Task<Msg> {
    let a = api.clone();
    Task::perform(async move { a.get("/canvas").await }, |r| match r {
        Ok(v) => Msg::Canvas(Cv::SpacesOnly(v)),
        Err(e) => Msg::Canvas(Cv::Done(Err(e))),
    })
}

pub fn fetch_space(api: &Api, id: &str) -> Task<Msg> {
    let (a, id) = (api.clone(), id.to_string());
    Task::perform(async move { a.get(&format!("/canvas/{id}")).await }, |r| Msg::Canvas(Cv::Space(r)))
}

pub fn fetch_config(api: &Api) -> Task<Msg> {
    let a = api.clone();
    Task::perform(async move { a.get("/canvas-config").await }, |r| Msg::Canvas(Cv::Config(r)))
}

impl App {
    /// Ajusta a câmera para mostrar todos os nós: o maior zoom em que tudo cabe.
    fn canvas_fit(&mut self) {
        let (sw, sh) = self.canvas_stage();
        let c = &mut self.canvas;
        if c.nodes.is_empty() {
            c.zoom = 0;
            c.pan = (40.0, 40.0);
            return;
        }
        let (x0, y0) = c.nodes.iter().fold((f32::MAX, f32::MAX), |a, n| (a.0.min(n.x), a.1.min(n.y)));
        let (x1, y1) = c.nodes.iter().fold((f32::MIN, f32::MIN), |a, n| (a.0.max(n.x + n.w), a.1.max(n.y + n.h)));
        let (w, h) = (x1 - x0, y1 - y0);
        let idx = ZOOMS.iter().position(|z| w * z + 80.0 <= sw && h * z + 80.0 <= sh).unwrap_or(ZOOMS.len() - 1);
        c.zoom = idx;
        let z = ZOOMS[idx];
        c.pan = ((sw - w * z) / 2.0 - x0 * z, (sh - h * z) / 2.0 - y0 * z);
    }

    /// Tamanho aproximado da área do canvas (janela menos barra lateral, topo e barras do canvas).
    pub fn canvas_stage(&self) -> (f32, f32) {
        let side = if self.collapsed || self.is_drawer_mode() { 0.0 } else { crate::theme::SIDE_W + 1.0 };
        ((self.win.width - side).max(320.0), (self.win.height - 46.0 - 92.0).max(240.0))
    }

    fn world_center(&self) -> (f32, f32) {
        let (sw, sh) = self.canvas_stage();
        let z = self.canvas.z();
        ((sw / 2.0 - self.canvas.pan.0) / z, (sh / 2.0 - self.canvas.pan.1) / z)
    }

    fn canvas_flush(&mut self) -> Task<Msg> {
        let Some(api) = self.api.clone() else { return Task::none() };
        let ids: Vec<String> = self.canvas.dirty.drain().collect();
        let ws = self.canvas.cur.clone();
        Task::batch(ids.into_iter().filter_map(|id| {
            let text = self.canvas.node(&id)?.text.clone();
            let (a, ws) = (api.clone(), ws.clone());
            Some(Task::perform(async move { a.put(&format!("/canvas/{ws}/nos/{id}"), json!({"texto": text})).await.map(|_| String::new()) }, |r| Msg::Canvas(Cv::Done(r.map(|_| String::new())))))
        }))
    }

    pub fn canvas_subscription_active(&self) -> bool {
        self.view == View::Maestro && self.api.is_some() && self.logged
    }

    pub fn update_canvas(&mut self, m: Cv) -> Task<Msg> {
        let api = self.api.clone();
        match m {
            Cv::Poll => {
                let Some(api) = api else { return Task::none() };
                if self.canvas.dragging || self.canvas.cur.is_empty() {
                    return Task::none();
                }
                // grava as notas editadas e só depois busca o espaço: a busca nunca lê o texto antigo
                let flush = self.canvas_flush();
                flush.chain(fetch_space(&api, &self.canvas.cur))
            }
            Cv::Spaces(Ok(v)) => {
                self.canvas.spaces = v["espacos"].as_array().map(|l| l.iter().map(|w| (s(w, "id"), s(w, "nome"), w["nos"].as_u64().unwrap_or(0) as usize)).collect()).unwrap_or_default();
                if let Some(m) = v["maestro"].as_str() {
                    self.canvas.maestro = m.to_string();
                }
                if !self.canvas.spaces.iter().any(|w| w.0 == self.canvas.cur) {
                    self.canvas.cur = self.canvas.spaces.first().map(|w| w.0.clone()).unwrap_or_default();
                    self.canvas.fitted = false;
                }
                let Some(api) = api else { return Task::none() };
                Task::batch([fetch_space(&api, &self.canvas.cur), fetch_config(&api)])
            }
            Cv::SpacesOnly(v) => {
                self.canvas.spaces = v["espacos"].as_array().map(|l| l.iter().map(|w| (s(w, "id"), s(w, "nome"), w["nos"].as_u64().unwrap_or(0) as usize)).collect()).unwrap_or_default();
                Task::none()
            }
            Cv::Space(Ok(v)) => {
                if self.canvas.dragging || v["id"].as_str() != Some(self.canvas.cur.as_str()) {
                    return Task::none();
                }
                let incoming: Vec<CNode> = v["nos"].as_array().map(|l| l.iter().map(parse_node).collect()).unwrap_or_default();
                let old = std::mem::take(&mut self.canvas.nodes);
                let busy = self.canvas.busy.clone();
                let mut merged = vec![];
                for mut n in incoming {
                    // chat respondendo: o servidor ainda não gravou a resposta; mantém o que a tela já mostra
                    if let Some(o) = old.iter().find(|o| o.id == n.id) {
                        if busy.contains(&n.id) {
                            n.msgs = o.msgs.clone();
                        }
                        if self.canvas.dirty.contains(&n.id) {
                            n.text = o.text.clone();
                        }
                    }
                    if n.kind == "nota" && !self.canvas.notes.contains_key(&n.id) {
                        self.canvas.notes.insert(n.id.clone(), text_editor::Content::with_text(&n.text));
                    }
                    merged.push(n);
                }
                self.canvas.notes.retain(|k, _| merged.iter().any(|n| &n.id == k));
                // nota mudada em outro lugar (ex.: pelo Maestro) e sem edição local pendente
                for n in &merged {
                    if n.kind == "nota" && !self.canvas.dirty.contains(&n.id) {
                        if let Some(c) = self.canvas.notes.get(&n.id) {
                            if c.text().trim_end_matches('\n') != n.text.trim_end_matches('\n') {
                                self.canvas.notes.insert(n.id.clone(), text_editor::Content::with_text(&n.text));
                            }
                        }
                    }
                }
                self.canvas.nodes = merged;
                self.canvas.edges = v["setas"].as_array().map(|l| l.iter().map(|e| CEdge { id: s(e, "id"), a: s(e, "de"), b: s(e, "para") }).collect()).unwrap_or_default();
                self.canvas.missions = v["missoes"].as_array().map(|l| l.iter().map(|m| CMission { id: s(m, "id"), request: s(m, "pedido"), state: s(m, "estado"), summary: s(m, "resumo"), error: s(m, "erro") }).collect()).unwrap_or_default();
                self.canvas.loaded = true;
                if !self.canvas.fitted {
                    self.canvas.fitted = true;
                    self.canvas_fit();
                }
                Task::none()
            }
            Cv::Config(Ok(v)) => {
                if let Some(m) = v["maestro"].as_str() {
                    self.canvas.maestro = m.to_string();
                }
                self.canvas.options = v["opcoes"].as_array().cloned().unwrap_or_default();
                Task::none()
            }
            Cv::Spaces(Err(e)) | Cv::Space(Err(e)) | Cv::Config(Err(e)) | Cv::Added(Err(e)) | Cv::Done(Err(e)) => {
                self.toast(&e);
                Task::none()
            }
            Cv::SelectSpace(id) => {
                let flush = self.canvas_flush();
                self.canvas.cur = id.clone();
                self.canvas.fitted = false;
                self.canvas.loaded = false;
                self.canvas.nodes.clear();
                self.canvas.edges.clear();
                self.canvas.missions.clear();
                self.canvas.sel = None;
                let Some(api) = api else { return flush };
                // espaço recém-criado ainda não está na lista da barra
                let list = if self.canvas.spaces.iter().any(|w| w.0 == id) { Task::none() } else { fetch_list(&api) };
                Task::batch([flush, fetch_space(&api, &id), list])
            }
            Cv::NewSpace => {
                let Some(api) = api else { return Task::none() };
                let name = format!("{} {}", crate::i18n::text("Espaço"), self.canvas.spaces.len() + 1);
                let a = api.clone();
                Task::perform(async move { a.post("/canvas", json!({"nome": name})).await.map(|v| s(&v, "id")) }, |r| match r {
                    Ok(id) => Msg::Canvas(Cv::SelectSpace(id)),
                    Err(e) => Msg::Canvas(Cv::Done(Err(e))),
                })
            }
            Cv::Add(kind) => {
                let Some(api) = api else { return Task::none() };
                if kind == "pagina" {
                    return self.update_canvas(Cv::UrlOpen);
                }
                let (cx, cy) = self.world_center();
                let (a, ws) = (api.clone(), self.canvas.cur.clone());
                let (w, h) = if kind == "chat" { (420.0, 360.0) } else if kind == "nota" { (260.0, 170.0) } else { (560.0, 380.0) };
                let body = json!({"tipo": kind, "x": cx - w / 2.0, "y": cy - h / 2.0});
                Task::perform(async move { a.post(&format!("/canvas/{ws}/nos"), body).await }, |r| Msg::Canvas(Cv::Added(r)))
            }
            Cv::UrlOpen => {
                self.canvas.url_in = Some(String::new());
                Task::none()
            }
            Cv::UrlIn(t) => {
                self.canvas.url_in = Some(t);
                Task::none()
            }
            Cv::UrlGo => {
                let Some(api) = api else { return Task::none() };
                let url = self.canvas.url_in.take().unwrap_or_default();
                if url.trim().is_empty() {
                    return Task::none();
                }
                let url = if url.contains("://") { url } else { format!("https://{url}") };
                let (cx, cy) = self.world_center();
                let (a, ws) = (api.clone(), self.canvas.cur.clone());
                Task::perform(async move { a.post(&format!("/canvas/{ws}/nos"), json!({"tipo": "pagina", "x": cx - 160.0, "y": cy - 75.0, "url": url})).await }, |r| Msg::Canvas(Cv::Added(r)))
            }
            Cv::Added(Ok(v)) => {
                let n = parse_node(&v);
                if n.kind == "nota" {
                    self.canvas.notes.insert(n.id.clone(), text_editor::Content::new());
                }
                self.canvas.sel = Some(n.id.clone());
                self.canvas.nodes.push(n);
                Task::none()
            }
            Cv::Select(id) => {
                let flush = self.canvas_flush();
                self.canvas.sel = id;
                flush
            }
            Cv::Drag(id, x, y) => {
                self.canvas.dragging = true;
                if let Some(n) = self.canvas.node_mut(&id) {
                    n.x = x;
                    n.y = y;
                }
                Task::none()
            }
            Cv::Resize(id, w, h) => {
                self.canvas.dragging = true;
                if let Some(n) = self.canvas.node_mut(&id) {
                    n.w = w;
                    n.h = h;
                }
                Task::none()
            }
            Cv::Commit(id) => {
                self.canvas.dragging = false;
                let (Some(api), Some(n)) = (api, self.canvas.node(&id).cloned()) else { return Task::none() };
                let (a, ws) = (api.clone(), self.canvas.cur.clone());
                Task::perform(async move { a.put(&format!("/canvas/{ws}/nos/{id}"), json!({"x": n.x, "y": n.y, "w": n.w, "h": n.h})).await.map(|_| String::new()) }, |r| match r {
                    Ok(_) => Msg::Noop,
                    Err(e) => Msg::Canvas(Cv::Done(Err(e))),
                })
            }
            Cv::Pan(dx, dy) => {
                self.canvas.pan.0 += dx;
                self.canvas.pan.1 += dy;
                Task::none()
            }
            Cv::Zoom(step) => {
                // zoom em torno do centro da área
                let (sw, sh) = self.canvas_stage();
                let old = self.canvas.z();
                let idx = (self.canvas.zoom as i32 + step).clamp(0, ZOOMS.len() as i32 - 1) as usize;
                self.canvas.zoom = idx;
                let new = self.canvas.z();
                let (cx, cy) = (sw / 2.0, sh / 2.0);
                self.canvas.pan = (cx - (cx - self.canvas.pan.0) * new / old, cy - (cy - self.canvas.pan.1) * new / old);
                Task::none()
            }
            Cv::Fit => {
                self.canvas_fit();
                Task::none()
            }
            Cv::Link(from, to) => {
                let Some(api) = api else { return Task::none() };
                if from == to || self.canvas.edges.iter().any(|e| e.a == from && e.b == to) {
                    return Task::none();
                }
                let (a, ws) = (api.clone(), self.canvas.cur.clone());
                Task::perform(async move { a.post(&format!("/canvas/{ws}/setas"), json!({"de": from, "para": to})).await.map(|_| String::new()) }, |r| Msg::Canvas(Cv::Done(r)))
            }
            Cv::DelNode(id) => {
                self.canvas.nodes.retain(|n| n.id != id);
                self.canvas.edges.retain(|e| e.a != id && e.b != id);
                self.canvas.notes.remove(&id);
                self.canvas.dirty.remove(&id);
                if self.canvas.sel.as_deref() == Some(id.as_str()) {
                    self.canvas.sel = None;
                }
                let Some(api) = api else { return Task::none() };
                let (a, ws) = (api.clone(), self.canvas.cur.clone());
                Task::perform(async move { a.delete(&format!("/canvas/{ws}/nos/{id}")).await.map(|_| String::new()) }, |r| Msg::Canvas(Cv::Done(r)))
            }
            Cv::DelEdge(id) => {
                self.canvas.edges.retain(|e| e.id != id);
                let Some(api) = api else { return Task::none() };
                let (a, ws) = (api.clone(), self.canvas.cur.clone());
                Task::perform(async move { a.delete(&format!("/canvas/{ws}/setas/{id}")).await.map(|_| String::new()) }, |r| Msg::Canvas(Cv::Done(r)))
            }
            Cv::SendEdge(id) => {
                let Some(api) = api else { return Task::none() };
                let (a, ws) = (api.clone(), self.canvas.cur.clone());
                Task::perform(async move { a.post(&format!("/canvas/{ws}/setas/{id}/enviar"), json!({})).await.map(|v| format!("{} caracteres enviados como dado", v["caracteres"])) }, |r| Msg::Canvas(Cv::Done(r)))
            }
            Cv::Title(id, t) => {
                if let Some(n) = self.canvas.node_mut(&id) {
                    n.title = t.clone();
                }
                let Some(api) = api else { return Task::none() };
                let (a, ws) = (api.clone(), self.canvas.cur.clone());
                // grava sem recarregar: o texto digitado já está na tela
                Task::perform(async move { a.put(&format!("/canvas/{ws}/nos/{id}"), json!({"titulo": t})).await.map(|_| String::new()) }, |r| match r {
                    Ok(_) => Msg::Noop,
                    Err(e) => Msg::Canvas(Cv::Done(Err(e))),
                })
            }
            Cv::Draft(id, t) => {
                self.canvas.drafts.insert(id, t);
                Task::none()
            }
            Cv::Send(id) => {
                let Some(api) = api else { return Task::none() };
                let text = self.canvas.drafts.remove(&id).unwrap_or_default();
                if text.trim().is_empty() || self.canvas.busy.contains(&id) {
                    return Task::none();
                }
                if let Some(n) = self.canvas.node_mut(&id) {
                    n.msgs.push(CMsg { role: "user".into(), text: text.clone(), ..Default::default() });
                }
                self.canvas.busy.insert(id.clone());
                let (a, ws, nid) = (api.clone(), self.canvas.cur.clone(), id.clone());
                Task::perform(async move { a.post(&format!("/canvas/{ws}/nos/{nid}/chat"), json!({"texto": text})).await }, move |r| Msg::Canvas(Cv::Reply(id.clone(), r)))
            }
            Cv::Reply(id, r) => {
                self.canvas.busy.remove(&id);
                match r {
                    Ok(v) => {
                        let n = parse_node(&v);
                        if let Some(o) = self.canvas.node_mut(&id) {
                            o.msgs = n.msgs;
                        }
                    }
                    Err(e) => {
                        if let Some(n) = self.canvas.node_mut(&id) {
                            n.msgs.push(CMsg { role: "erro".into(), text: e.clone(), ..Default::default() });
                        }
                        self.toast(&e);
                    }
                }
                Task::none()
            }
            Cv::Note(id, action) => {
                let edit = action.is_edit();
                if let Some(c) = self.canvas.notes.get_mut(&id) {
                    c.perform(action);
                    if edit {
                        let t = c.text();
                        if let Some(n) = self.canvas.node_mut(&id) {
                            n.text = t;
                        }
                        self.canvas.dirty.insert(id);
                    }
                }
                Task::none()
            }
            Cv::Agent(id, i) => {
                let Some((agent, _)) = AGENTS.get(i) else { return Task::none() };
                if let Some(n) = self.canvas.node_mut(&id) {
                    n.agent = agent.to_string();
                    if *agent != "claude" {
                        n.model.clear();
                    }
                }
                let Some(api) = api else { return Task::none() };
                let (a, ws, agent) = (api.clone(), self.canvas.cur.clone(), agent.to_string());
                Task::perform(async move { a.put(&format!("/canvas/{ws}/nos/{id}"), json!({"agente": agent})).await.map(|_| String::new()) }, |r| match r {
                    Ok(_) => Msg::Noop,
                    Err(e) => Msg::Canvas(Cv::Done(Err(e))),
                })
            }
            Cv::Model(id, i) => {
                let Some((model, _)) = CLAUDE_MODELS.get(i) else { return Task::none() };
                if let Some(n) = self.canvas.node_mut(&id) {
                    n.model = model.to_string();
                }
                let Some(api) = api else { return Task::none() };
                let (a, ws, model) = (api.clone(), self.canvas.cur.clone(), model.to_string());
                Task::perform(async move { a.put(&format!("/canvas/{ws}/nos/{id}"), json!({"modelo": model})).await.map(|_| String::new()) }, |r| match r {
                    Ok(_) => Msg::Noop,
                    Err(e) => Msg::Canvas(Cv::Done(Err(e))),
                })
            }
            Cv::MissionIn(t) => {
                self.canvas.mission_in = t;
                Task::none()
            }
            Cv::MissionGo => {
                let Some(api) = api else { return Task::none() };
                let pedido = self.canvas.mission_in.trim().to_string();
                if pedido.is_empty() {
                    return Task::none();
                }
                self.canvas.mission_in.clear();
                let (a, ws) = (api.clone(), self.canvas.cur.clone());
                Task::perform(async move { a.post(&format!("/canvas/{ws}/missao"), json!({"pedido": pedido})).await.map(|_| "O Maestro está planejando: os nós aparecem aqui".to_string()) }, |r| Msg::Canvas(Cv::Done(r)))
            }
            Cv::SetMaestro(agent) => {
                let Some(api) = api else { return Task::none() };
                self.canvas.maestro = agent.clone();
                let a = api.clone();
                Task::perform(async move { a.put("/canvas-config", json!({"maestro": agent})).await }, |r| Msg::Canvas(Cv::Config(r)))
            }
            Cv::Done(Ok(msg)) => {
                if !msg.is_empty() {
                    self.toast(&msg);
                }
                match api {
                    Some(api) if !self.canvas.cur.is_empty() => fetch_space(&api, &self.canvas.cur),
                    _ => Task::none(),
                }
            }
        }
    }
}
