//! Canvas do Maestro: espaços de trabalho com nós de chat, tarefa, nota, página e moldura, ligados por setas.
//!
//! - **Só dado e links.** Nó de página guarda um endereço `http(s)`; o Orion não busca nem abre nada
//!   sozinho (REGRAS.md: ferramentas de web começam desligadas; conteúdo externo é dado).
//! - **Saída de um chat só entra em outro por clique** do Antônio, marcada como dado de outro chat,
//!   nunca como instrução ([`CanvasStore::send_edge`]; REGRAS.md: conteúdo externo é dado).
//! - **O Maestro monta o canvas sozinho** a partir do plano ([`CanvasStore::add_mission_nodes`]): um nó
//!   por subtarefa, notas e páginas que ele pediu. Tudo conferido aqui: tipos, tamanhos e endereços.
//! - **Qual agente é o Maestro** fica na configuração do canvas (`claude`, `codex` ou `orion`; o Gemini
//!   fica de fora porque os termos do Google proíbem usar o login da CLI por terceiros).
//!
//! Tudo fica em `canvas.json` na pasta de dados, gravado por arquivo temporário e renomeação.

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;

pub const MAX_WORKSPACES: usize = 24;
pub const MAX_NODES: usize = 80;
pub const MAX_TEXT: usize = 8_000;
pub const MAX_MESSAGES: usize = 200;
pub const MAX_NOTES: usize = 3;
pub const MAX_PAGES: usize = 3;
/// Quem pode ser o Maestro.
pub const MAESTRO_OPTIONS: [&str; 3] = ["claude", "codex", "orion"];
pub const KINDS: [&str; 5] = ["chat", "tarefa", "nota", "pagina", "moldura"];
/// Agentes de um nó de chat.
pub const CHAT_AGENTS: [&str; 3] = ["orion", "claude", "codex"];

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Message {
    /// user | ai | dado | erro (falha ou interrupção do turno: aparece na tela, nunca entra em prompt)
    #[serde(rename = "papel")]
    pub role: String,
    #[serde(rename = "texto")]
    pub text: String,
    #[serde(default, rename = "hora")]
    pub time: f64,
    /// Para `dado`: título do chat de onde veio.
    #[serde(default, rename = "de")]
    pub from: String,
    #[serde(default, rename = "modelo")]
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Node {
    pub id: String,
    #[serde(rename = "tipo")]
    pub kind: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    #[serde(rename = "titulo")]
    pub title: String,
    /// Texto da nota.
    #[serde(default, rename = "texto")]
    pub text: String,
    /// Endereço da página (`http` ou `https`).
    #[serde(default)]
    pub url: String,
    #[serde(default, rename = "agente")]
    pub agent: String,
    /// Chave do modelo: `opus`, `sonnet`, `haiku` (Claude) ou vazio.
    #[serde(default, rename = "modelo")]
    pub model: String,
    #[serde(default, rename = "esforco")]
    pub effort: String,
    #[serde(default, rename = "mensagens")]
    pub messages: Vec<Message>,
    /// Nó de tarefa: missão e subtarefa do Maestro a que ele mostra o estado.
    #[serde(default)]
    pub missao: String,
    #[serde(default)]
    pub subtarefa: String,
    /// Criado pelo Maestro, não pelo Antônio.
    #[serde(default, rename = "do_maestro")]
    pub by_maestro: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Edge {
    pub id: String,
    pub de: String,
    pub para: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Workspace {
    pub id: String,
    pub nome: String,
    #[serde(default)]
    pub nos: Vec<Node>,
    #[serde(default)]
    pub setas: Vec<Edge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Data {
    espacos: Vec<Workspace>,
    maestro: String,
}

impl Default for Data {
    fn default() -> Self {
        Data { espacos: vec![Workspace { id: "principal".into(), nome: "Maestro".into(), ..Default::default() }], maestro: "claude".into() }
    }
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("{message}")]
pub struct CanvasError {
    pub message: String,
    pub status: u16,
}

fn refuse<T>(m: impl Into<String>, status: u16) -> Result<T, CanvasError> {
    Err(CanvasError { message: m.into(), status })
}

fn clip(s: &str, n: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n).collect::<String>() + "…"
    }
}

/// Aceita só `http` e `https`, sem espaço nem caractere de controle e com no máximo 300 caracteres.
pub fn valid_url(u: &str) -> Option<String> {
    let u = u.trim();
    let lower = u.to_lowercase();
    let ok = (lower.starts_with("https://") || lower.starts_with("http://")) && u.len() <= 300 && !u.chars().any(|c| c.is_whitespace() || c.is_control());
    let host = u.split("://").nth(1).unwrap_or("").split(['/', '?', '#']).next().unwrap_or("");
    (ok && host.contains('.') && !host.starts_with('.')).then(|| u.to_string())
}

fn default_size(kind: &str) -> (f64, f64) {
    match kind {
        "chat" => (420.0, 360.0),
        "tarefa" => (320.0, 210.0),
        "nota" => (260.0, 170.0),
        "pagina" => (320.0, 150.0),
        _ => (560.0, 380.0),
    }
}

fn min_size(kind: &str) -> (f64, f64) {
    match kind {
        "chat" => (320.0, 260.0),
        "tarefa" => (240.0, 150.0),
        "nota" => (180.0, 110.0),
        "pagina" => (240.0, 120.0),
        _ => (220.0, 140.0),
    }
}

pub struct CanvasStore {
    path: PathBuf,
    d: Mutex<Data>,
}

/// Pedido para criar um nó. Campos fora do tipo são ignorados.
#[derive(Debug, Clone, Default)]
pub struct NodeInput {
    pub kind: String,
    pub x: f64,
    pub y: f64,
    pub title: String,
    pub text: String,
    pub url: String,
    pub agent: String,
    pub model: String,
    pub by_maestro: bool,
}

/// Uma subtarefa do plano do Maestro, como o canvas precisa dela.
#[derive(Debug, Clone)]
pub struct TaskSeed {
    pub subtask: String,
    pub title: String,
    pub agent: String,
    pub effort: String,
    pub depends_on: Vec<String>,
}

impl CanvasStore {
    pub fn open(data_dir: &std::path::Path) -> Self {
        let path = data_dir.join("canvas.json");
        let mut d: Data = match std::fs::read(&path) {
            Ok(b) => serde_json::from_slice(&b).unwrap_or_else(|e| {
                // arquivo ilegível: guarda a cópia ao lado em vez de apagar o canvas sem avisar
                let backup = path.with_extension(format!("json.corrompido-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|t| t.as_secs()).unwrap_or(0)));
                let _ = std::fs::rename(&path, &backup);
                tracing::warn!("canvas.json ilegível ({e}); cópia em {}", backup.display());
                Data::default()
            }),
            Err(_) => Data::default(),
        };
        if d.espacos.is_empty() {
            d.espacos = Data::default().espacos;
        }
        if !MAESTRO_OPTIONS.contains(&d.maestro.as_str()) {
            d.maestro = "claude".into();
        }
        CanvasStore { path, d: Mutex::new(d) }
    }

    /// Grava por arquivo temporário e renomeação; o erro volta ao chamador para a tela não mostrar como salvo o que não foi.
    fn save(&self, d: &Data) -> Result<(), CanvasError> {
        let fail = |e: String| CanvasError { message: format!("não consegui gravar o canvas: {e}"), status: 500 };
        let b = serde_json::to_vec_pretty(d).map_err(|e| fail(e.to_string()))?;
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, b).map_err(|e| fail(e.to_string()))?;
        std::fs::rename(&tmp, &self.path).map_err(|e| fail(e.to_string()))
    }

    /// Muda sob a trava e grava. `f` devolve o resultado ou o motivo da recusa.
    /// A gravação acontece com a trava ainda tomada: duas mudanças simultâneas nunca gravam fora de ordem
    /// nem escrevem o mesmo arquivo temporário ao mesmo tempo.
    fn edit<T>(&self, f: impl FnOnce(&mut Data) -> Result<T, CanvasError>) -> Result<T, CanvasError> {
        let mut g = self.d.lock();
        let r = f(&mut g)?;
        self.save(&g)?;
        Ok(r)
    }

    // ── Maestro (qual agente planeja) ──

    pub fn maestro_agent(&self) -> String {
        self.d.lock().maestro.clone()
    }

    pub fn set_maestro_agent(&self, agent: &str) -> Result<(), CanvasError> {
        if !MAESTRO_OPTIONS.contains(&agent) {
            return refuse(format!("o Maestro pode ser: {}", MAESTRO_OPTIONS.join(", ")), 422);
        }
        self.edit(|d| {
            d.maestro = agent.to_string();
            Ok(())
        })
    }

    // ── Espaços ──

    /// Id do espaço pedido, ou o primeiro quando vazio ou inexistente.
    pub fn resolve(&self, id: &str) -> String {
        let g = self.d.lock();
        g.espacos.iter().find(|w| w.id == id).or(g.espacos.first()).map(|w| w.id.clone()).unwrap_or_default()
    }

    pub fn list(&self) -> Vec<Value> {
        self.d.lock().espacos.iter().map(|w| json!({"id": w.id, "nome": w.nome, "nos": w.nos.len(), "chats": w.nos.iter().filter(|n| n.kind == "chat" || n.kind == "tarefa").count(), "do_maestro": w.nos.iter().filter(|n| n.by_maestro).count()})).collect()
    }

    pub fn get(&self, id: &str) -> Result<Workspace, CanvasError> {
        match self.d.lock().espacos.iter().find(|w| w.id == id) {
            Some(w) => Ok(w.clone()),
            None => refuse("espaço não encontrado", 404),
        }
    }

    pub fn create_workspace(&self, name: &str) -> Result<Workspace, CanvasError> {
        let name = clip(name, 60);
        if name.is_empty() {
            return refuse("o espaço precisa de um nome", 422);
        }
        self.edit(|d| {
            if d.espacos.len() >= MAX_WORKSPACES {
                return refuse(format!("no máximo {MAX_WORKSPACES} espaços"), 409);
            }
            let w = Workspace { id: orion_store::uuid_hex()[..10].to_string(), nome: name, ..Default::default() };
            d.espacos.push(w.clone());
            Ok(w)
        })
    }

    pub fn rename_workspace(&self, id: &str, name: &str) -> Result<(), CanvasError> {
        let name = clip(name, 60);
        if name.is_empty() {
            return refuse("o espaço precisa de um nome", 422);
        }
        self.edit(|d| match d.espacos.iter_mut().find(|w| w.id == id) {
            Some(w) => {
                w.nome = name;
                Ok(())
            }
            None => refuse("espaço não encontrado", 404),
        })
    }

    pub fn delete_workspace(&self, id: &str) -> Result<(), CanvasError> {
        self.edit(|d| {
            if d.espacos.len() <= 1 {
                return refuse("o último espaço não pode ser apagado", 409);
            }
            let before = d.espacos.len();
            d.espacos.retain(|w| w.id != id);
            if d.espacos.len() == before {
                return refuse("espaço não encontrado", 404);
            }
            Ok(())
        })
    }

    // ── Nós ──

    fn build_node(input: &NodeInput) -> Result<Node, CanvasError> {
        if !KINDS.contains(&input.kind.as_str()) {
            return refuse(format!("tipo de nó desconhecido (use {})", KINDS.join(", ")), 422);
        }
        let (w, h) = default_size(&input.kind);
        let mut n = Node { id: orion_store::uuid_hex()[..10].to_string(), kind: input.kind.clone(), x: input.x.clamp(-50_000.0, 50_000.0), y: input.y.clamp(-50_000.0, 50_000.0), w, h, by_maestro: input.by_maestro, ..Default::default() };
        n.title = clip(&input.title, 80);
        match n.kind.as_str() {
            "chat" | "tarefa" => {
                n.agent = if CHAT_AGENTS.contains(&input.agent.as_str()) { input.agent.clone() } else { "orion".into() };
                n.model = if n.agent == "claude" && ["opus", "sonnet", "haiku"].contains(&input.model.as_str()) { input.model.clone() } else { String::new() };
                if n.title.is_empty() {
                    n.title = "Novo chat".into();
                }
            }
            "nota" => {
                n.text = clip(&input.text, MAX_TEXT);
                if n.title.is_empty() {
                    n.title = "Nota".into();
                }
            }
            "pagina" => {
                let Some(u) = valid_url(&input.url) else { return refuse("endereço inválido: só http ou https", 422) };
                n.url = u;
                if n.title.is_empty() {
                    n.title = n.url.split("://").nth(1).unwrap_or("Página").split('/').next().unwrap_or("Página").to_string();
                }
            }
            _ => {
                if n.title.is_empty() {
                    n.title = "Moldura".into();
                }
            }
        }
        Ok(n)
    }

    pub fn add_node(&self, ws: &str, input: NodeInput) -> Result<Node, CanvasError> {
        let n = Self::build_node(&input)?;
        self.edit(|d| {
            let w = d.espacos.iter_mut().find(|w| w.id == ws).ok_or(CanvasError { message: "espaço não encontrado".into(), status: 404 })?;
            if w.nos.len() >= MAX_NODES {
                return refuse(format!("no máximo {MAX_NODES} nós por espaço"), 409);
            }
            w.nos.push(n.clone());
            Ok(n)
        })
    }

    /// Muda só o que veio e é válido: posição, tamanho, título, texto, endereço, agente e modelo.
    pub fn update_node(&self, ws: &str, id: &str, patch: &Value) -> Result<Node, CanvasError> {
        self.edit(|d| {
            let w = d.espacos.iter_mut().find(|w| w.id == ws).ok_or(CanvasError { message: "espaço não encontrado".into(), status: 404 })?;
            let n = w.nos.iter_mut().find(|n| n.id == id).ok_or(CanvasError { message: "nó não encontrado".into(), status: 404 })?;
            let num = |k: &str| patch.get(k).and_then(Value::as_f64).filter(|v| v.is_finite());
            if let Some(v) = num("x") {
                n.x = v.clamp(-50_000.0, 50_000.0);
            }
            if let Some(v) = num("y") {
                n.y = v.clamp(-50_000.0, 50_000.0);
            }
            let (mw, mh) = min_size(&n.kind);
            if let Some(v) = num("w") {
                n.w = v.clamp(mw, 2_400.0);
            }
            if let Some(v) = num("h") {
                n.h = v.clamp(mh, 2_400.0);
            }
            if let Some(s) = patch.get("titulo").and_then(Value::as_str) {
                n.title = clip(s, 80);
            }
            if n.kind == "nota" {
                if let Some(s) = patch.get("texto").and_then(Value::as_str) {
                    n.text = s.chars().take(MAX_TEXT).collect();
                }
            }
            if n.kind == "pagina" {
                if let Some(s) = patch.get("url").and_then(Value::as_str) {
                    match valid_url(s) {
                        Some(u) => n.url = u,
                        None => return refuse("endereço inválido: só http ou https", 422),
                    }
                }
            }
            if n.kind == "chat" {
                if let Some(a) = patch.get("agente").and_then(Value::as_str).filter(|a| CHAT_AGENTS.contains(a)) {
                    n.agent = a.to_string();
                    if a != "claude" {
                        n.model.clear();
                    }
                }
                if let Some(m) = patch.get("modelo").and_then(Value::as_str) {
                    n.model = if n.agent == "claude" && ["opus", "sonnet", "haiku"].contains(&m) { m.to_string() } else { String::new() };
                }
            }
            // trazer para a frente: o nó vai para o fim da lista (a tela desenha nessa ordem)
            let out = n.clone();
            if patch.get("frente").and_then(Value::as_bool) == Some(true) {
                if let Some(i) = w.nos.iter().position(|n| n.id == id) {
                    let n = w.nos.remove(i);
                    w.nos.push(n);
                }
            }
            Ok(out)
        })
    }

    /// Move vários nós de uma vez (arrastar uma seleção ou uma moldura com o que está dentro): uma gravação só.
    /// Ids desconhecidos são ignorados. Devolve quantos mudaram.
    pub fn move_nodes(&self, ws: &str, moves: &[(String, f64, f64)]) -> Result<usize, CanvasError> {
        self.edit(|d| {
            let w = d.espacos.iter_mut().find(|w| w.id == ws).ok_or(CanvasError { message: "espaço não encontrado".into(), status: 404 })?;
            let mut changed = 0;
            for (id, x, y) in moves.iter().take(MAX_NODES) {
                if !x.is_finite() || !y.is_finite() {
                    continue;
                }
                if let Some(n) = w.nos.iter_mut().find(|n| &n.id == id) {
                    n.x = x.clamp(-50_000.0, 50_000.0);
                    n.y = y.clamp(-50_000.0, 50_000.0);
                    changed += 1;
                }
            }
            Ok(changed)
        })
    }

    /// Cópia de um nó, deslocada. O chat copiado leva agente e modelo, sem as mensagens; tarefa não se copia
    /// (só nasce do plano do Maestro).
    pub fn duplicate_node(&self, ws: &str, id: &str) -> Result<Node, CanvasError> {
        self.edit(|d| {
            let w = d.espacos.iter_mut().find(|w| w.id == ws).ok_or(CanvasError { message: "espaço não encontrado".into(), status: 404 })?;
            if w.nos.len() >= MAX_NODES {
                return refuse(format!("no máximo {MAX_NODES} nós por espaço"), 409);
            }
            let src = w.nos.iter().find(|n| n.id == id).cloned().ok_or(CanvasError { message: "nó não encontrado".into(), status: 404 })?;
            if src.kind == "tarefa" {
                return refuse("nós de tarefa só nascem do plano do Maestro", 422);
            }
            let n = Node { id: orion_store::uuid_hex()[..10].to_string(), x: (src.x + 32.0).clamp(-50_000.0, 50_000.0), y: (src.y + 32.0).clamp(-50_000.0, 50_000.0), messages: vec![], missao: String::new(), subtarefa: String::new(), by_maestro: false, ..src };
            w.nos.push(n.clone());
            Ok(n)
        })
    }

    pub fn remove_node(&self, ws: &str, id: &str) -> Result<(), CanvasError> {
        self.edit(|d| {
            let w = d.espacos.iter_mut().find(|w| w.id == ws).ok_or(CanvasError { message: "espaço não encontrado".into(), status: 404 })?;
            let before = w.nos.len();
            w.nos.retain(|n| n.id != id);
            if w.nos.len() == before {
                return refuse("nó não encontrado", 404);
            }
            w.setas.retain(|e| e.de != id && e.para != id);
            Ok(())
        })
    }

    pub fn node(&self, ws: &str, id: &str) -> Result<Node, CanvasError> {
        self.get(ws)?.nos.into_iter().find(|n| n.id == id).ok_or(CanvasError { message: "nó não encontrado".into(), status: 404 })
    }

    pub fn push_message(&self, ws: &str, id: &str, m: Message) -> Result<(), CanvasError> {
        self.edit(|d| {
            let w = d.espacos.iter_mut().find(|w| w.id == ws).ok_or(CanvasError { message: "espaço não encontrado".into(), status: 404 })?;
            let n = w.nos.iter_mut().find(|n| n.id == id).ok_or(CanvasError { message: "nó não encontrado".into(), status: 404 })?;
            if n.kind != "chat" {
                return refuse("só chats têm mensagens", 409);
            }
            // um aviso de erro substitui o anterior em vez de empilhar
            if m.role == "erro" && n.messages.last().is_some_and(|l| l.role == "erro") {
                n.messages.pop();
            }
            n.messages.push(Message { text: clip(&m.text, 16_000), ..m });
            let extra = n.messages.len().saturating_sub(MAX_MESSAGES);
            n.messages.drain(..extra);
            Ok(())
        })
    }

    // ── Setas ──

    pub fn add_edge(&self, ws: &str, from: &str, to: &str) -> Result<Edge, CanvasError> {
        self.edit(|d| {
            let w = d.espacos.iter_mut().find(|w| w.id == ws).ok_or(CanvasError { message: "espaço não encontrado".into(), status: 404 })?;
            let has = |id: &str| w.nos.iter().any(|n| n.id == id && n.kind != "moldura");
            if from == to || !has(from) || !has(to) {
                return refuse("a seta liga dois nós diferentes (moldura não liga)", 422);
            }
            if let Some(e) = w.setas.iter().find(|e| e.de == from && e.para == to) {
                return Ok(e.clone());
            }
            let e = Edge { id: orion_store::uuid_hex()[..10].to_string(), de: from.into(), para: to.into() };
            w.setas.push(e.clone());
            Ok(e)
        })
    }

    pub fn remove_edge(&self, ws: &str, id: &str) -> Result<(), CanvasError> {
        self.edit(|d| {
            let w = d.espacos.iter_mut().find(|w| w.id == ws).ok_or(CanvasError { message: "espaço não encontrado".into(), status: 404 })?;
            let before = w.setas.len();
            w.setas.retain(|e| e.id != id);
            if w.setas.len() == before {
                return refuse("seta não encontrada", 404);
            }
            Ok(())
        })
    }

    /// Clique do Antônio na seta: a última resposta do chat de origem entra no chat de destino como
    /// **dado** (REGRAS.md: conteúdo externo é dado, nunca instrução). Devolve o tamanho enviado.
    pub fn send_edge(&self, ws: &str, edge: &str, now: f64) -> Result<usize, CanvasError> {
        self.edit(|d| {
            let w = d.espacos.iter_mut().find(|w| w.id == ws).ok_or(CanvasError { message: "espaço não encontrado".into(), status: 404 })?;
            let e = w.setas.iter().find(|e| e.id == edge).cloned().ok_or(CanvasError { message: "seta não encontrada".into(), status: 404 })?;
            let src = w.nos.iter().find(|n| n.id == e.de).cloned().ok_or(CanvasError { message: "nó de origem não existe".into(), status: 404 })?;
            if src.kind != "chat" {
                return refuse("só a resposta de um chat pode ser enviada", 409);
            }
            let Some(last) = src.messages.iter().rev().find(|m| m.role == "ai" && !m.text.trim().is_empty()) else { return refuse("o chat de origem ainda não tem resposta", 409) };
            let text = last.text.clone();
            let dst = w.nos.iter_mut().find(|n| n.id == e.para).ok_or(CanvasError { message: "nó de destino não existe".into(), status: 404 })?;
            if dst.kind != "chat" {
                return refuse("o destino precisa ser um chat", 409);
            }
            dst.messages.push(Message { role: "dado".into(), text: text.clone(), time: now, from: src.title.clone(), model: String::new() });
            let extra = dst.messages.len().saturating_sub(MAX_MESSAGES);
            dst.messages.drain(..extra);
            Ok(text.chars().count())
        })
    }

    // ── O Maestro monta o canvas ──

    /// Cria no espaço os nós do plano: um por subtarefa (em colunas por profundidade de dependência),
    /// as notas e as páginas que o Maestro pediu, e as setas de `depende`. Abaixo do que já existe.
    /// Devolve quantos nós criou.
    pub fn add_mission_nodes(&self, ws: &str, mission: &str, tasks: &[TaskSeed], notes: &[(String, String)], pages: &[(String, String)]) -> Result<usize, CanvasError> {
        self.edit(|d| {
            let w = d.espacos.iter_mut().find(|w| w.id == ws).ok_or(CanvasError { message: "espaço não encontrado".into(), status: 404 })?;
            let room = MAX_NODES.saturating_sub(w.nos.len());
            let notes: Vec<_> = notes.iter().take(MAX_NOTES).collect();
            let pages: Vec<(String, String)> = pages.iter().filter_map(|(t, u)| valid_url(u).map(|u| (t.clone(), u))).take(MAX_PAGES).collect();
            if tasks.len() + notes.len() + pages.len() > room {
                return refuse("o espaço não tem lugar para os nós da missão: apague alguns", 409);
            }
            let (x0, y0) = if w.nos.is_empty() { (0.0, 0.0) } else { (w.nos.iter().map(|n| n.x).fold(f64::MAX, f64::min), w.nos.iter().map(|n| n.y + n.h).fold(f64::MIN, f64::max) + 120.0) };
            let mut created = 0;
            // páginas numa fileira em cima
            let mut y = y0;
            if !pages.is_empty() {
                let (pw, ph) = default_size("pagina");
                for (i, (title, url)) in pages.iter().enumerate() {
                    let mut n = Self::build_node(&NodeInput { kind: "pagina".into(), x: x0 + i as f64 * (pw + 30.0), y: y0, title: title.clone(), url: url.clone(), by_maestro: true, ..Default::default() })?;
                    n.missao = mission.into();
                    w.nos.push(n);
                    created += 1;
                }
                y = y0 + ph + 60.0;
            }
            // tarefas em colunas por profundidade
            let mut depth: Vec<usize> = vec![];
            for (i, t) in tasks.iter().enumerate() {
                let dp = t.depends_on.iter().filter_map(|dep| tasks[..i].iter().position(|x| &x.subtask == dep)).map(|p| depth[p] + 1).max().unwrap_or(0);
                depth.push(dp);
            }
            let (tw, th) = default_size("tarefa");
            let mut row_in_col: std::collections::HashMap<usize, usize> = Default::default();
            let mut ids: Vec<String> = vec![];
            let mut max_col = 0;
            for (i, t) in tasks.iter().enumerate() {
                let col = depth[i];
                max_col = max_col.max(col);
                let row = row_in_col.entry(col).or_insert(0);
                let this_row = *row;
                *row += 1;
                let mut n = Self::build_node(&NodeInput { kind: "tarefa".into(), x: x0 + col as f64 * (tw + 70.0), y: y + this_row as f64 * (th + 40.0), title: t.title.clone(), agent: t.agent.clone(), by_maestro: true, ..Default::default() })?;
                n.effort = t.effort.clone();
                n.missao = mission.into();
                n.subtarefa = t.subtask.clone();
                ids.push(n.id.clone());
                w.nos.push(n);
                created += 1;
            }
            for (i, t) in tasks.iter().enumerate() {
                for dep in &t.depends_on {
                    if let Some(p) = tasks[..i].iter().position(|x| &x.subtask == dep) {
                        w.setas.push(Edge { id: orion_store::uuid_hex()[..10].to_string(), de: ids[p].clone(), para: ids[i].clone() });
                    }
                }
            }
            // notas à direita da última coluna
            for (i, (title, text)) in notes.iter().enumerate() {
                let (nw, nh) = default_size("nota");
                let mut n = Self::build_node(&NodeInput { kind: "nota".into(), x: x0 + (max_col + 1) as f64 * (tw + 70.0), y: y + i as f64 * (nh + 30.0), title: title.clone(), text: text.clone(), by_maestro: true, ..Default::default() })?;
                n.w = nw;
                n.missao = mission.into();
                w.nos.push(n);
                created += 1;
            }
            Ok(created)
        })
    }
    /// Resultado final da missão como nota ao lado dos nós dela (criada pelo Maestro quando termina).
    pub fn add_result_note(&self, ws: &str, mission: &str, title: &str, text: &str) -> Result<(), CanvasError> {
        self.edit(|d| {
            let w = d.espacos.iter_mut().find(|w| w.id == ws).ok_or(CanvasError { message: "espaço não encontrado".into(), status: 404 })?;
            if w.nos.len() >= MAX_NODES {
                return refuse("o espaço está cheio", 409);
            }
            let mine: Vec<&Node> = w.nos.iter().filter(|n| n.missao == mission).collect();
            let (x, y) = if mine.is_empty() { (0.0, 0.0) } else { (mine.iter().map(|n| n.x + n.w).fold(f64::MIN, f64::max) + 70.0, mine.iter().map(|n| n.y).fold(f64::MAX, f64::min)) };
            let mut n = Self::build_node(&NodeInput { kind: "nota".into(), x, y, title: title.into(), text: text.into(), by_maestro: true, ..Default::default() })?;
            n.w = 360.0;
            n.h = 260.0;
            n.missao = mission.into();
            w.nos.push(n);
            Ok(())
        })
    }
}
