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

/// Zoom contínuo entre [`ZOOM_MIN`] e 100%. Só em 100% os nós mostram o conteúdo (chat, nota); abaixo
/// disso a tela vira mapa de cartões. Os botões andam por estes degraus.
pub const ZOOM_STEPS: [f32; 7] = [1.0, 0.85, 0.7, 0.55, 0.4, 0.3, 0.2];
pub const ZOOM_MIN: f32 = 0.2;
/// Grade a que posição e tamanho se alinham ao arrastar.
pub const GRID: f32 = 8.0;
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
    /// Câmera: deslocamento em pixels da tela e fator de zoom.
    pub pan: (f32, f32),
    pub zoom: f32,
    /// Nós selecionados (Shift+clique soma; Shift+arrastar no vazio seleciona por área).
    pub sel: Vec<String>,
    pub sel_edge: Option<String>,
    /// Tamanho real da área do canvas, informado pelo próprio widget.
    pub stage: (f32, f32),
    /// Nós esperando confirmação para apagar (têm conteúdo).
    pub confirm: Option<Vec<String>>,
    /// Nome do espaço atual em edição.
    pub rename: Option<String>,
    pub confirm_space: bool,
    /// Nós com posição ou tamanho ainda sendo gravados: a tela mantém o que o Antônio deixou.
    pub moving: HashSet<String>,
    /// Chats com pedido de parar já enviado.
    pub stopping: HashSet<String>,
    /// Arrasto em andamento: nó principal, posição inicial dele e a dos que vão junto.
    pub drag: Option<(String, f32, f32, Vec<(String, f32, f32)>)>,
    pub drafts: HashMap<String, String>,
    pub busy: HashSet<String>,
    pub notes: HashMap<String, text_editor::Content>,
    pub dirty: HashSet<String>,
    /// Títulos editados e ainda não gravados (gravados juntos, sem uma chamada por tecla).
    pub dirty_title: HashSet<String>,
    pub mission_in: String,
    pub url_in: Option<String>,
    /// Quem é o Maestro (`claude`, `codex` ou `orion`) e as opções vindas do servidor.
    pub maestro: String,
    pub options: Vec<Value>,
    pub fitted: bool,
    pub dragging: bool,
    pub loaded: bool,
    /// M18: cantos já pedidos ao núcleo e que ainda não voltaram; dois cliques rápidos (Nota, Chat)
    /// não caem no mesmo ponto.
    pub pending_spots: Vec<(f32, f32, f32, f32)>,
}

impl Default for CanvasUi {
    fn default() -> Self {
        CanvasUi {
            spaces: vec![],
            cur: String::new(),
            pending_spots: vec![],
            nodes: vec![],
            edges: vec![],
            missions: vec![],
            pan: (40.0, 40.0),
            zoom: 1.0,
            sel: vec![],
            sel_edge: None,
            stage: (0.0, 0.0),
            confirm: None,
            rename: None,
            confirm_space: false,
            moving: HashSet::new(),
            stopping: HashSet::new(),
            drag: None,
            drafts: HashMap::new(),
            busy: HashSet::new(),
            notes: HashMap::new(),
            dirty: HashSet::new(),
            dirty_title: HashSet::new(),
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
        self.zoom.clamp(ZOOM_MIN, 1.0)
    }
    pub fn detail(&self) -> bool {
        self.zoom >= 0.999
    }
    pub fn selected(&self, id: &str) -> bool {
        self.sel.iter().any(|s| s == id)
    }
    /// Apagar pede confirmação quando há conteúdo a perder ou mais de um nó.
    fn needs_confirm(&self, ids: &[String]) -> bool {
        ids.len() > 1 || ids.iter().filter_map(|id| self.node(id)).any(|n| (n.kind == "chat" && !n.msgs.is_empty()) || (n.kind == "nota" && !n.text.trim().is_empty()) || n.kind == "tarefa")
    }
    pub fn node(&self, id: &str) -> Option<&CNode> {
        self.nodes.iter().find(|n| n.id == id)
    }
    fn node_mut(&mut self, id: &str) -> Option<&mut CNode> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }
    /// Missão do Maestro ainda em andamento ou esperando o Antônio.
    pub fn active_missions(&self) -> impl Iterator<Item = &CMission> {
        self.missions.iter().filter(|m| ["planejando", "aguarda_aprovacao", "rodando", "integrando", "interrompida"].contains(&m.state.as_str()))
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
    /// Clique num nó (ou no vazio): troca a seleção.
    Select(Option<String>),
    /// Shift+clique: soma ou tira o nó da seleção.
    Toggle(String),
    SelectEdge(String),
    /// Seleção por área (Shift+arrastar no vazio).
    Marquee(Vec<String>),
    SelectAll,
    Escape,
    Drag(String, f32, f32),
    Resize(String, f32, f32),
    Commit(String),
    Committed(Vec<String>, Result<(), String>),
    Pan(f32, f32),
    /// Degrau do zoom pelos botões (+1 afasta, -1 aproxima), em torno do centro.
    Zoom(i32),
    /// Zoom pela roda (Ctrl+roda): fator e o ponto da tela que fica parado.
    ZoomAt(f32, f32, f32),
    /// Duplo clique num cartão do mapa: 100% com o nó no centro.
    ZoomTo(String),
    Zoom100,
    Fit,
    /// Centraliza a câmera num ponto do mundo (clique no minimapa).
    Center(f32, f32),
    Stage(f32, f32),
    /// Novo nó num ponto do mundo (duplo clique no vazio).
    AddAt(&'static str, f32, f32),
    /// × do nó ou Delete: apaga, pedindo confirmação se houver conteúdo.
    AskDelete(Vec<String>),
    DeleteSel,
    ConfirmDelete,
    CancelDelete,
    Duplicate,
    Stop(String),
    RenameOpen,
    RenameIn(String),
    RenameGo,
    SpaceDelAsk,
    SpaceDelGo,
    SpaceDeleted(Result<(), String>),
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

/// Zoom dentro dos limites; perto de 100% gruda em 100% (é onde o conteúdo aparece).
fn snap_zoom(z: f32) -> f32 {
    let z = if z.is_finite() { z } else { 1.0 };
    if z > 0.96 {
        1.0
    } else {
        z.clamp(ZOOM_MIN, 1.0)
    }
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
            c.zoom = 1.0;
            c.pan = (40.0, 40.0);
            return;
        }
        let (x0, y0) = c.nodes.iter().fold((f32::MAX, f32::MAX), |a, n| (a.0.min(n.x), a.1.min(n.y)));
        let (x1, y1) = c.nodes.iter().fold((f32::MIN, f32::MIN), |a, n| (a.0.max(n.x + n.w), a.1.max(n.y + n.h)));
        let (w, h) = ((x1 - x0).max(1.0), (y1 - y0).max(1.0));
        let z = snap_zoom(((sw - 96.0) / w).min((sh - 96.0) / h));
        c.zoom = z;
        c.pan = (((sw - w * z) / 2.0 - x0 * z).round(), ((sh - h * z) / 2.0 - y0 * z).round());
    }

    /// Tamanho da área do canvas: o que o widget informou ou, antes do primeiro desenho, uma estimativa
    /// (janela menos barra lateral, topo e barras do canvas).
    pub fn canvas_stage(&self) -> (f32, f32) {
        if self.canvas.stage.0 > 1.0 && self.canvas.stage.1 > 1.0 {
            return self.canvas.stage;
        }
        let side = if self.collapsed || self.is_drawer_mode() { 0.0 } else { crate::theme::SIDE_W + 1.0 };
        ((self.win.width - side).max(320.0), (self.win.height - 46.0 - 92.0).max(240.0))
    }

    fn world_center(&self) -> (f32, f32) {
        let (sw, sh) = self.canvas_stage();
        let z = self.canvas.z();
        ((sw / 2.0 - self.canvas.pan.0) / z, (sh / 2.0 - self.canvas.pan.1) / z)
    }

    /// Muda o zoom mantendo parado o ponto `(ax, ay)` da tela.
    fn canvas_zoom_to(&mut self, new: f32, ax: f32, ay: f32) {
        let old = self.canvas.z();
        let new = snap_zoom(new);
        self.canvas.zoom = new;
        self.canvas.pan = ((ax - (ax - self.canvas.pan.0) * new / old).round(), (ay - (ay - self.canvas.pan.1) * new / old).round());
    }

    /// M18: posição livre para um nó novo. Se o retângulo dele (com uma folga) bate num nó existente ou
    /// num pedido que ainda não voltou do núcleo, ele vai para a direita do que atrapalha, ao lado e não
    /// por cima (antes descia 40 px em diagonal e o chat novo cobria a nota inteira).
    fn canvas_free_spot(&self, mut x: f32, mut y: f32, w: f32, h: f32) -> (f32, f32) {
        const GAP: f32 = 24.0;
        x = (x / GRID).round() * GRID;
        y = (y / GRID).round() * GRID;
        let overlaps = |x: f32, y: f32, (ox, oy, ow, oh): (f32, f32, f32, f32)| {
            x < ox + ow + GAP && ox < x + w + GAP && y < oy + oh + GAP && oy < y + h + GAP
        };
        for _ in 0..40 {
            let blocker = self
                .canvas
                .nodes
                .iter()
                // moldura agrupa: o nó novo pode nascer dentro dela; só os nós de dentro contam
                .filter(|n| n.kind != "moldura")
                .map(|n| (n.x, n.y, n.w, n.h))
                .chain(self.canvas.pending_spots.iter().copied())
                .filter(|&r| overlaps(x, y, r))
                .map(|(ox, _, ow, _)| ox + ow)
                .fold(None, |m: Option<f32>, r| Some(m.map_or(r, |m| m.max(r))));
            match blocker {
                Some(right) => x = ((right + GAP) / GRID).ceil() * GRID,
                None => break,
            }
        }
        (x, y)
    }

    /// Tamanho com que o núcleo cria cada tipo de nó.
    fn new_node_size(kind: &str) -> (f32, f32) {
        if kind == "chat" {
            (420.0, 360.0)
        } else if kind == "nota" {
            (260.0, 170.0)
        } else {
            (560.0, 380.0)
        }
    }

    fn canvas_add(&mut self, kind: &'static str, x: f32, y: f32) -> Task<Msg> {
        let Some(api) = self.api.clone() else { return Task::none() };
        let (w, h) = Self::new_node_size(kind);
        let (x, y) = self.canvas_free_spot(x, y, w, h);
        self.canvas.pending_spots.push((x, y, w, h));
        let ws = self.canvas.cur.clone();
        Task::perform(async move { api.post(&format!("/canvas/{ws}/nos"), json!({"tipo": kind, "x": x, "y": y})).await }, |r| Msg::Canvas(Cv::Added(r)))
    }

    /// Apaga os nós na tela e no servidor.
    fn canvas_delete(&mut self, ids: Vec<String>) -> Task<Msg> {
        let c = &mut self.canvas;
        c.nodes.retain(|n| !ids.contains(&n.id));
        c.edges.retain(|e| !ids.contains(&e.a) && !ids.contains(&e.b));
        c.sel.retain(|s| !ids.contains(s));
        for id in &ids {
            c.notes.remove(id);
            c.dirty.remove(id);
            c.dirty_title.remove(id);
            c.drafts.remove(id);
            c.moving.remove(id);
        }
        let Some(api) = self.api.clone() else { return Task::none() };
        let ws = self.canvas.cur.clone();
        Task::batch(ids.into_iter().map(|id| {
            let (a, ws) = (api.clone(), ws.clone());
            Task::perform(async move { a.delete(&format!("/canvas/{ws}/nos/{id}")).await.map(|_| String::new()) }, |r| Msg::Canvas(Cv::Done(r)))
        }))
    }

    /// Grava notas e títulos editados, um pedido por nó, com os dois campos juntos (nada por tecla).
    fn canvas_flush(&mut self) -> Task<Msg> {
        let Some(api) = self.api.clone() else { return Task::none() };
        let mut ids: Vec<String> = self.canvas.dirty.iter().chain(self.canvas.dirty_title.iter()).cloned().collect();
        ids.sort();
        ids.dedup();
        let (notes, titles) = (std::mem::take(&mut self.canvas.dirty), std::mem::take(&mut self.canvas.dirty_title));
        let ws = self.canvas.cur.clone();
        Task::batch(ids.into_iter().filter_map(|id| {
            let n = self.canvas.node(&id)?;
            let mut body = serde_json::Map::new();
            if notes.contains(&id) {
                body.insert("texto".into(), json!(n.text));
            }
            if titles.contains(&id) {
                body.insert("titulo".into(), json!(n.title));
            }
            let (a, ws) = (api.clone(), ws.clone());
            Some(Task::perform(async move { a.put(&format!("/canvas/{ws}/nos/{id}"), Value::Object(body)).await }, |r| match r {
                Ok(_) => Msg::Noop,
                Err(e) => Msg::Canvas(Cv::Done(Err(e))),
            }))
        }))
    }

    /// Fica ligada na tela do Maestro e, enquanto sobrar edição sem gravar, em qualquer tela.
    pub fn canvas_subscription_active(&self) -> bool {
        self.api.is_some() && self.logged && (self.view == View::Maestro || !self.canvas.dirty.is_empty() || !self.canvas.dirty_title.is_empty())
    }

    pub fn update_canvas(&mut self, m: Cv) -> Task<Msg> {
        let api = self.api.clone();
        match m {
            Cv::Poll => {
                let Some(api) = api else { return Task::none() };
                if self.canvas.dragging || self.canvas.cur.is_empty() {
                    return Task::none();
                }
                // fora da tela do Maestro só grava o que sobrou sem salvar
                if self.view != View::Maestro {
                    return self.canvas_flush();
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
                        if self.canvas.moving.contains(&n.id) {
                            (n.x, n.y, n.w, n.h) = (o.x, o.y, o.w, o.h);
                        }
                        if self.canvas.dirty.contains(&n.id) {
                            n.text = o.text.clone();
                        }
                        if self.canvas.dirty_title.contains(&n.id) {
                            n.title = o.title.clone();
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
                self.canvas.sel.retain(|id| merged.iter().any(|n| &n.id == id));
                if self.canvas.confirm.as_ref().is_some_and(|ids| !ids.iter().all(|id| merged.iter().any(|n| &n.id == id))) {
                    self.canvas.confirm = None;
                }
                self.canvas.nodes = merged;
                self.canvas.edges = v["setas"].as_array().map(|l| l.iter().map(|e| CEdge { id: s(e, "id"), a: s(e, "de"), b: s(e, "para") }).collect()).unwrap_or_default();
                self.canvas.missions = v["missoes"].as_array().map(|l| l.iter().map(|m| CMission { id: s(m, "id"), request: s(m, "pedido"), state: s(m, "estado"), summary: s(m, "resumo"), error: s(m, "erro") }).collect()).unwrap_or_default();
                if self.canvas.sel_edge.as_ref().is_some_and(|id| !self.canvas.edges.iter().any(|e| &e.id == id)) {
                    self.canvas.sel_edge = None;
                }
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
            Cv::Spaces(Err(e)) | Cv::Space(Err(e)) | Cv::Config(Err(e)) | Cv::Added(Err(e)) | Cv::Done(Err(e)) | Cv::SpaceDeleted(Err(e)) => {
                self.canvas.pending_spots.clear();
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
                self.canvas.sel.clear();
                self.canvas.sel_edge = None;
                self.canvas.confirm = None;
                self.canvas.rename = None;
                self.canvas.confirm_space = false;
                self.canvas.moving.clear();
                self.canvas.drag = None;
                self.canvas.dragging = false;
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
                if kind == "pagina" {
                    return self.update_canvas(Cv::UrlOpen);
                }
                let (cx, cy) = self.world_center();
                let (w, h) = Self::new_node_size(kind);
                self.canvas_add(kind, cx - w / 2.0, cy - h / 2.0)
            }
            Cv::AddAt(kind, x, y) => self.canvas_add(kind, x, y),
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
                let (x, y) = self.canvas_free_spot(cx - 160.0, cy - 75.0, 320.0, 150.0);
                // o Added libera um canto pendente: a página também entra na lista
                self.canvas.pending_spots.push((x, y, 320.0, 150.0));
                let (a, ws) = (api.clone(), self.canvas.cur.clone());
                Task::perform(async move { a.post(&format!("/canvas/{ws}/nos"), json!({"tipo": "pagina", "x": x, "y": y, "url": url})).await }, |r| Msg::Canvas(Cv::Added(r)))
            }
            Cv::Added(Ok(v)) => {
                let n = parse_node(&v);
                // o nó chegou: libera só o canto reservado para ele (pedidos podem voltar fora de ordem, e
                // duplicar não reserva canto)
                if let Some(i) = self.canvas.pending_spots.iter().position(|&(x, y, _, _)| (x - n.x).abs() < 0.5 && (y - n.y).abs() < 0.5) {
                    self.canvas.pending_spots.remove(i);
                }
                if n.id.is_empty() || self.canvas.node(&n.id).is_some() {
                    return Task::none();
                }
                if n.kind == "nota" {
                    self.canvas.notes.insert(n.id.clone(), text_editor::Content::with_text(&n.text));
                }
                self.canvas.sel = vec![n.id.clone()];
                self.canvas.sel_edge = None;
                self.canvas.nodes.push(n);
                Task::none()
            }
            Cv::Select(id) => {
                let flush = self.canvas_flush();
                self.canvas.sel_edge = None;
                self.canvas.confirm = None;
                let Some(id) = id else {
                    self.canvas.sel.clear();
                    return flush;
                };
                // clicar num nó de uma seleção múltipla mantém o grupo (para arrastar junto)
                if !(self.canvas.sel.len() > 1 && self.canvas.selected(&id)) {
                    self.canvas.sel = vec![id.clone()];
                }
                // o nó clicado vem para a frente; molduras ficam sempre atrás
                let last = self.canvas.nodes.iter().rev().find(|n| n.kind != "moldura").map(|n| n.id.clone());
                let is_frame = self.canvas.node(&id).is_none_or(|n| n.kind == "moldura");
                if is_frame || last.as_deref() == Some(id.as_str()) {
                    return flush;
                }
                if let Some(i) = self.canvas.nodes.iter().position(|n| n.id == id) {
                    let n = self.canvas.nodes.remove(i);
                    self.canvas.nodes.push(n);
                }
                let Some(api) = api else { return flush };
                let ws = self.canvas.cur.clone();
                let front = Task::perform(async move { api.put(&format!("/canvas/{ws}/nos/{id}"), json!({"frente": true})).await }, |r| match r {
                    Ok(_) => Msg::Noop,
                    Err(e) => Msg::Canvas(Cv::Done(Err(e))),
                });
                Task::batch([flush, front])
            }
            Cv::Toggle(id) => {
                self.canvas.sel_edge = None;
                if self.canvas.selected(&id) {
                    self.canvas.sel.retain(|s| *s != id);
                } else {
                    self.canvas.sel.push(id);
                }
                Task::none()
            }
            Cv::SelectEdge(id) => {
                self.canvas.sel.clear();
                self.canvas.sel_edge = Some(id);
                Task::none()
            }
            Cv::Marquee(ids) => {
                self.canvas.sel = ids;
                self.canvas.sel_edge = None;
                Task::none()
            }
            Cv::SelectAll => {
                self.canvas.sel = self.canvas.nodes.iter().map(|n| n.id.clone()).collect();
                self.canvas.sel_edge = None;
                Task::none()
            }
            Cv::Escape => {
                let c = &mut self.canvas;
                if c.confirm.take().is_some() || c.rename.take().is_some() || c.url_in.take().is_some() {
                    return Task::none();
                }
                if c.confirm_space {
                    c.confirm_space = false;
                    return Task::none();
                }
                c.sel.clear();
                c.sel_edge = None;
                Task::none()
            }
            Cv::Drag(id, x, y) => {
                let c = &mut self.canvas;
                c.dragging = true;
                if c.drag.as_ref().is_none_or(|d| d.0 != id) {
                    let Some(n) = c.node(&id).cloned() else { return Task::none() };
                    // vão junto: os outros selecionados e, se for moldura, o que está inteiro dentro dela
                    let mut group: Vec<String> = if c.selected(&id) { c.sel.iter().filter(|s| **s != id).cloned().collect() } else { vec![] };
                    let frames: Vec<CNode> = c.nodes.iter().filter(|f| f.kind == "moldura" && (f.id == id || group.contains(&f.id))).cloned().collect();
                    for f in &frames {
                        for m in &c.nodes {
                            let inside = m.x >= f.x && m.y >= f.y && m.x + m.w <= f.x + f.w && m.y + m.h <= f.y + f.h;
                            if inside && m.id != f.id && m.id != id && !group.contains(&m.id) {
                                group.push(m.id.clone());
                            }
                        }
                    }
                    let others = group.iter().filter_map(|g| c.node(g).map(|m| (m.id.clone(), m.x, m.y))).collect();
                    c.drag = Some((id.clone(), n.x, n.y, others));
                }
                let Some((_, ox, oy, others)) = c.drag.clone() else { return Task::none() };
                // sozinho, o nó encaixa na grade; em grupo, o que encaixa é o deslocamento (ninguém sai do lugar relativo)
                let snap = |v: f32| (v / GRID).round() * GRID;
                let (nx, ny) = if others.is_empty() { (snap(x), snap(y)) } else { (ox + snap(x - ox), oy + snap(y - oy)) };
                let (dx, dy) = (nx - ox, ny - oy);
                if let Some(n) = c.node_mut(&id) {
                    n.x = nx;
                    n.y = ny;
                }
                for (oid, x0, y0) in others {
                    if let Some(n) = c.node_mut(&oid) {
                        n.x = x0 + dx;
                        n.y = y0 + dy;
                    }
                }
                Task::none()
            }
            Cv::Resize(id, w, h) => {
                self.canvas.dragging = true;
                if let Some(n) = self.canvas.node_mut(&id) {
                    n.w = (w / GRID).round() * GRID;
                    n.h = (h / GRID).round() * GRID;
                }
                Task::none()
            }
            Cv::Commit(id) => {
                self.canvas.dragging = false;
                let group = self.canvas.drag.take().map(|d| d.3).unwrap_or_default();
                let (Some(api), Some(n)) = (api, self.canvas.node(&id).cloned()) else { return Task::none() };
                let ws = self.canvas.cur.clone();
                let mut ids = vec![id.clone()];
                ids.extend(group.iter().map(|g| g.0.clone()));
                self.canvas.moving.extend(ids.iter().cloned());
                let done = ids.clone();
                if group.is_empty() {
                    return Task::perform(async move { api.put(&format!("/canvas/{ws}/nos/{id}"), json!({"x": n.x, "y": n.y, "w": n.w, "h": n.h})).await.map(|_| ()) }, move |r| Msg::Canvas(Cv::Committed(done.clone(), r)));
                }
                let moves: Vec<Value> = ids.iter().filter_map(|i| self.canvas.node(i)).map(|m| json!({"id": m.id, "x": m.x, "y": m.y})).collect();
                Task::perform(async move { api.put(&format!("/canvas/{ws}/posicoes"), json!({"nos": moves})).await.map(|_| ()) }, move |r| Msg::Canvas(Cv::Committed(done.clone(), r)))
            }
            Cv::Committed(ids, r) => {
                for id in &ids {
                    self.canvas.moving.remove(id);
                }
                if let Err(e) = r {
                    self.toast(&e);
                }
                Task::none()
            }
            Cv::Pan(dx, dy) => {
                self.canvas.pan.0 = (self.canvas.pan.0 + dx).round();
                self.canvas.pan.1 = (self.canvas.pan.1 + dy).round();
                Task::none()
            }
            Cv::Zoom(step) => {
                // degrau seguinte na direção pedida, em torno do centro da área
                let (sw, sh) = self.canvas_stage();
                let z = self.canvas.z();
                let next = if step > 0 { ZOOM_STEPS.iter().copied().find(|s| *s < z - 0.01) } else { ZOOM_STEPS.iter().rev().copied().find(|s| *s > z + 0.01) };
                if let Some(n) = next {
                    self.canvas_zoom_to(n, sw / 2.0, sh / 2.0);
                }
                Task::none()
            }
            Cv::ZoomAt(factor, x, y) => {
                let z = self.canvas.z();
                // saindo de 100% a roda sempre dá um passo; o encaixe em 100% só vale na volta
                let target = if factor < 1.0 && z >= 0.999 { z * factor.min(0.92) } else { z * factor };
                self.canvas_zoom_to(target, x, y);
                Task::none()
            }
            Cv::Zoom100 => {
                let (sw, sh) = self.canvas_stage();
                self.canvas_zoom_to(1.0, sw / 2.0, sh / 2.0);
                Task::none()
            }
            Cv::ZoomTo(id) => {
                let (sw, sh) = self.canvas_stage();
                let Some(n) = self.canvas.node(&id).cloned() else { return Task::none() };
                self.canvas.zoom = 1.0;
                let py = if n.h > sh - 48.0 { 24.0 - n.y } else { sh / 2.0 - (n.y + n.h / 2.0) };
                let px = if n.w > sw - 48.0 { 24.0 - n.x } else { sw / 2.0 - (n.x + n.w / 2.0) };
                self.canvas.pan = (px.round(), py.round());
                self.canvas.sel = vec![id];
                self.canvas.sel_edge = None;
                Task::none()
            }
            Cv::Fit => {
                self.canvas_fit();
                Task::none()
            }
            Cv::Center(wx, wy) => {
                let (sw, sh) = self.canvas_stage();
                let z = self.canvas.z();
                self.canvas.pan = ((sw / 2.0 - wx * z).round(), (sh / 2.0 - wy * z).round());
                Task::none()
            }
            Cv::Stage(w, h) => {
                let first = self.canvas.stage.0 <= 1.0;
                self.canvas.stage = (w, h);
                // o primeiro enquadramento usou a estimativa: refaz com o tamanho real
                if first && self.canvas.loaded && !self.canvas.dragging {
                    self.canvas_fit();
                }
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
            Cv::AskDelete(ids) => {
                let ids: Vec<String> = ids.into_iter().filter(|i| self.canvas.node(i).is_some()).collect();
                if ids.is_empty() {
                    return Task::none();
                }
                if self.canvas.needs_confirm(&ids) {
                    self.canvas.confirm = Some(ids);
                    return Task::none();
                }
                self.canvas_delete(ids)
            }
            Cv::DeleteSel => {
                if let Some(e) = self.canvas.sel_edge.take() {
                    return self.update_canvas(Cv::DelEdge(e));
                }
                let ids = self.canvas.sel.clone();
                self.update_canvas(Cv::AskDelete(ids))
            }
            Cv::ConfirmDelete => match self.canvas.confirm.take() {
                Some(ids) => self.canvas_delete(ids),
                None => Task::none(),
            },
            Cv::CancelDelete => {
                self.canvas.confirm = None;
                Task::none()
            }
            Cv::DelNode(id) => self.canvas_delete(vec![id]),
            Cv::Duplicate => {
                let Some(api) = api else { return Task::none() };
                let ws = self.canvas.cur.clone();
                let ids: Vec<String> = self.canvas.sel.iter().filter(|i| self.canvas.node(i).is_some_and(|n| n.kind != "tarefa")).cloned().collect();
                Task::batch(ids.into_iter().map(|id| {
                    let (a, ws) = (api.clone(), ws.clone());
                    Task::perform(async move { a.post(&format!("/canvas/{ws}/nos/{id}/duplicar"), json!({})).await }, |r| Msg::Canvas(Cv::Added(r)))
                }))
            }
            Cv::Stop(id) => {
                let Some(api) = api else { return Task::none() };
                if !self.canvas.busy.contains(&id) || !self.canvas.stopping.insert(id.clone()) {
                    return Task::none();
                }
                let ws = self.canvas.cur.clone();
                Task::perform(async move { api.post(&format!("/canvas/{ws}/nos/{id}/parar"), json!({})).await }, |r| match r {
                    Ok(_) => Msg::Noop,
                    Err(e) => Msg::Canvas(Cv::Done(Err(e))),
                })
            }
            Cv::RenameOpen => {
                let name = self.canvas.spaces.iter().find(|w| w.0 == self.canvas.cur).map(|w| w.1.clone()).unwrap_or_default();
                self.canvas.rename = Some(name);
                self.canvas.confirm_space = false;
                Task::none()
            }
            Cv::RenameIn(t) => {
                self.canvas.rename = Some(t.chars().take(60).collect());
                Task::none()
            }
            Cv::RenameGo => {
                let Some(name) = self.canvas.rename.take().map(|n| n.trim().to_string()).filter(|n| !n.is_empty()) else { return Task::none() };
                let cur = self.canvas.cur.clone();
                if let Some(w) = self.canvas.spaces.iter_mut().find(|w| w.0 == cur) {
                    if w.1 == name {
                        return Task::none();
                    }
                    w.1 = name.clone();
                }
                let Some(api) = api else { return Task::none() };
                Task::perform(async move { api.patch(&format!("/canvas/{cur}"), json!({"nome": name})).await }, |r| match r {
                    Ok(_) => Msg::Noop,
                    Err(e) => Msg::Canvas(Cv::Done(Err(e))),
                })
            }
            Cv::SpaceDelAsk => {
                self.canvas.confirm_space = !self.canvas.confirm_space;
                self.canvas.rename = None;
                Task::none()
            }
            Cv::SpaceDelGo => {
                self.canvas.confirm_space = false;
                let Some(api) = api else { return Task::none() };
                let cur = self.canvas.cur.clone();
                Task::perform(async move { api.delete(&format!("/canvas/{cur}")).await.map(|_| ()) }, |r| Msg::Canvas(Cv::SpaceDeleted(r)))
            }
            Cv::SpaceDeleted(Ok(())) => {
                // a lista volta sem o espaço apagado e o primeiro vira o atual
                let c = &mut self.canvas;
                c.cur.clear();
                c.nodes.clear();
                c.edges.clear();
                c.missions.clear();
                c.sel.clear();
                c.sel_edge = None;
                c.loaded = false;
                match api {
                    Some(api) => fetch(&api),
                    None => Task::none(),
                }
            }
            Cv::DelEdge(id) => {
                self.canvas.edges.retain(|e| e.id != id);
                if self.canvas.sel_edge.as_deref() == Some(id.as_str()) {
                    self.canvas.sel_edge = None;
                }
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
                    n.title = t;
                }
                self.canvas.dirty_title.insert(id);
                Task::none()
            }
            Cv::Draft(id, t) => {
                self.canvas.drafts.insert(id, t);
                Task::none()
            }
            Cv::Send(id) => {
                let Some(api) = api else { return Task::none() };
                // nó respondendo: o rascunho fica onde está até a resposta chegar
                if self.canvas.busy.contains(&id) || self.canvas.drafts.get(&id).is_none_or(|d| d.trim().is_empty()) {
                    return Task::none();
                }
                let text = self.canvas.drafts.remove(&id).unwrap_or_default();
                if let Some(n) = self.canvas.node_mut(&id) {
                    n.msgs.push(CMsg { role: "user".into(), text: text.clone(), ..Default::default() });
                }
                self.canvas.busy.insert(id.clone());
                let (a, ws, nid) = (api.clone(), self.canvas.cur.clone(), id.clone());
                Task::perform(async move { a.post_long(&format!("/canvas/{ws}/nos/{nid}/chat"), json!({"texto": text})).await }, move |r| Msg::Canvas(Cv::Reply(id.clone(), r)))
            }
            Cv::Reply(id, r) => {
                self.canvas.busy.remove(&id);
                self.canvas.stopping.remove(&id);
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
