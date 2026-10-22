//! Grafo do vault (§16): dados do snapshot de 15/10/2026 e a lógica de câmera, foco e rótulos do protótipo.

use crate::data::READ_NOTES;
use serde::Deserialize;

pub const TIPOS: [(&str, &str, &str); 7] = [
    ("projeto", "Projetos", "Projeto"),
    ("area", "Áreas", "Área"),
    ("conceito", "Conceitos", "Conceito"),
    ("pesquisa", "Pesquisas", "Pesquisa"),
    ("recurso", "Recursos", "Recurso"),
    ("conversa", "Conversas", "Conversa com IA"),
    ("relatorio", "Relatórios", "Relatório"),
];

#[derive(Deserialize)]
struct Raw {
    t: Vec<String>,
    f: Vec<String>,
    n: Vec<serde_json::Value>,
    e: Vec<usize>,
}

pub struct Node {
    pub label: String,
    pub tipo: usize,
    pub folder: String,
    pub date: String,
    pub status: String,
    pub tags: String,
    pub x: f32,
    pub y: f32,
    pub r: f32,
    pub deg: usize,
    pub hub: bool,
    pub read: bool,
}

pub struct Graph {
    pub nodes: Vec<Node>,
    pub links: Vec<(usize, usize)>,
    pub adj: Vec<Vec<usize>>,
    pub dates: Vec<String>,
    pub filt: [bool; 7],
    pub q: String,
    pub sel: Option<usize>,
    pub hov: Option<usize>,
    pub cam: (f32, f32, f32), // k, x, y
    pub cut: String,
    pub cut_idx: usize,
    pub read_on: bool,
    pub rot: f32,
    pub spinning: bool,
    pub fitted: bool,
    pub size: (f32, f32),
    pub cache: iced::widget::canvas::Cache,
}

impl Graph {
    /// Grafo vazio (modo real, antes de carregar do servidor).
    pub fn empty() -> Self {
        Self::build(vec![], vec![])
    }

    /// Converte `/grafo/completo` (`nodes`/`links`) e calcula posições por força (Fruchterman–Reingold).
    pub fn from_server(v: &serde_json::Value) -> Self {
        let nodes_raw = v.get("nodes").and_then(|n| n.as_array()).cloned().unwrap_or_default();
        let mut ids: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        let mut nodes: Vec<Node> = vec![];
        for n in &nodes_raw {
            let id = n.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let tipo_raw = n.get("tipo").and_then(|x| x.as_str()).unwrap_or("");
            let pasta = n.get("pasta").and_then(|x| x.as_str()).unwrap_or("").to_string();
            let tipo = tipo_de_servidor(tipo_raw, &pasta);
            let base = match tipo {
                0 | 1 => 4.6,
                2 => 3.1,
                _ => 2.2,
            };
            let ts = n.get("ts").and_then(|x| x.as_f64()).unwrap_or(0.0) / 1000.0;
            let date = if ts > 0.0 { crate::live::iso_date(ts) } else { String::new() };
            ids.insert(id, nodes.len());
            nodes.push(Node { label: n.get("label").and_then(|x| x.as_str()).unwrap_or("").to_string(), tipo, folder: pasta, date, status: String::new(), tags: String::new(), x: 0.0, y: 0.0, r: base, deg: 0, hub: false, read: false });
        }
        let mut links = vec![];
        for l in v.get("links").and_then(|n| n.as_array()).cloned().unwrap_or_default() {
            let (a, b) = (l.get("source").and_then(|x| x.as_str()).and_then(|k| ids.get(k)), l.get("target").and_then(|x| x.as_str()).and_then(|k| ids.get(k)));
            if let (Some(a), Some(b)) = (a, b) {
                if a != b {
                    links.push((*a, *b));
                }
            }
        }
        layout_forca(&mut nodes, &links);
        Self::build(nodes, links)
    }

    fn build(mut nodes: Vec<Node>, links: Vec<(usize, usize)>) -> Self {
        let mut adj = vec![vec![]; nodes.len()];
        for (a, b) in &links {
            adj[*a].push(*b);
            adj[*b].push(*a);
        }
        for (i, n) in nodes.iter_mut().enumerate() {
            n.deg = adj[i].len();
            n.hub = n.deg >= 45;
        }
        let mut dates: Vec<String> = nodes.iter().filter(|n| !n.date.is_empty()).map(|n| n.date.clone()).collect();
        dates.sort();
        dates.dedup();
        let cut = dates.last().cloned().unwrap_or_default();
        let cut_idx = dates.len().saturating_sub(1);
        Graph { nodes, links, adj, dates, filt: [true, true, true, true, true, true, false], q: String::new(), sel: None, hov: None, cam: (1.0, 0.0, 0.0), cut, cut_idx, read_on: false, rot: 0.0, spinning: true, fitted: false, size: (0.0, 0.0), cache: iced::widget::canvas::Cache::new() }
    }

    pub fn load() -> Self {
        let raw: Raw = serde_json::from_str(include_str!("../assets/vault.json")).expect("vault.json");
        let tipo_idx = |s: &str| TIPOS.iter().position(|t| t.0 == s).unwrap_or(4);
        let nodes: Vec<Node> = raw
            .n
            .iter()
            .map(|a| {
                let tipo = tipo_idx(&raw.t[a[1].as_u64().unwrap_or(4) as usize]);
                let base = match tipo {
                    0 | 1 => 4.6,
                    2 => 3.1,
                    _ => 2.2,
                };
                let label = a[0].as_str().unwrap_or("").to_string();
                Node {
                    read: READ_NOTES.contains(&label.as_str()),
                    label,
                    tipo,
                    folder: raw.f[a[2].as_u64().unwrap_or(0) as usize].clone(),
                    date: a[3].as_str().unwrap_or("").to_string(),
                    status: a[4].as_str().unwrap_or("").to_string(),
                    tags: a[5].as_str().unwrap_or("").to_string(),
                    x: a[6].as_f64().unwrap_or(0.0) as f32,
                    y: a[7].as_f64().unwrap_or(0.0) as f32,
                    r: base,
                    deg: 0,
                    hub: false,
                }
            })
            .collect();
        let mut links = vec![];
        let mut adj = vec![vec![]; nodes.len()];
        for c in raw.e.chunks(2) {
            if c.len() == 2 && c[0] < nodes.len() && c[1] < nodes.len() {
                links.push((c[0], c[1]));
                adj[c[0]].push(c[1]);
                adj[c[1]].push(c[0]);
            }
        }
        let mut nodes = nodes;
        for (i, n) in nodes.iter_mut().enumerate() {
            n.deg = adj[i].len();
            n.hub = n.deg >= 45;
        }
        let mut dates: Vec<String> = nodes.iter().filter(|n| !n.date.is_empty()).map(|n| n.date.clone()).collect();
        dates.sort();
        dates.dedup();
        let cut = dates.last().cloned().unwrap_or_default();
        let cut_idx = dates.len().saturating_sub(1);
        Graph {
            nodes,
            links,
            adj,
            dates,
            filt: [true, true, true, true, true, true, false],
            q: String::new(),
            sel: None,
            hov: None,
            cam: (1.0, 0.0, 0.0),
            cut,
            cut_idx,
            read_on: false,
            rot: 0.0,
            spinning: true,
            fitted: false,
            size: (0.0, 0.0),
            cache: iced::widget::canvas::Cache::new(),
        }
    }

    pub fn visible(&self, i: usize) -> bool {
        let n = &self.nodes[i];
        self.filt[n.tipo] && (n.date.is_empty() || n.date <= self.cut)
    }
    pub fn matches(&self, i: usize) -> bool {
        let q = self.q.trim().to_lowercase();
        if q.is_empty() {
            return true;
        }
        let n = &self.nodes[i];
        format!("{} {} {}", n.label, n.tags, n.folder).to_lowercase().contains(&q)
    }
    pub fn radius(&self, i: usize) -> f32 {
        let n = &self.nodes[i];
        n.r + (n.deg.max(1) as f32).log2() * 0.55
    }
    pub fn rotp(&self, i: usize) -> (f32, f32) {
        let n = &self.nodes[i];
        let (c, s) = (self.rot.cos(), self.rot.sin());
        (n.x * c - n.y * s, n.x * s + n.y * c)
    }
    pub fn to_screen(&self, i: usize, w: f32, h: f32) -> (f32, f32) {
        let (x, y) = self.rotp(i);
        (w / 2.0 + (x + self.cam.1) * self.cam.0, h / 2.0 + (y + self.cam.2) * self.cam.0)
    }
    pub fn to_world(&self, px: f32, py: f32, w: f32, h: f32) -> (f32, f32) {
        let x = (px - w / 2.0) / self.cam.0 - self.cam.1;
        let y = (py - h / 2.0) / self.cam.0 - self.cam.2;
        let (c, s) = ((-self.rot).cos(), (-self.rot).sin());
        (x * c - y * s, x * s + y * c)
    }
    pub fn fit(&mut self, w: f32, h: f32) {
        let vis: Vec<usize> = (0..self.nodes.len()).filter(|&i| self.visible(i) && self.nodes[i].deg > 0).collect();
        if vis.is_empty() || w <= 0.0 {
            return;
        }
        let xs = vis.iter().map(|&i| self.nodes[i].x);
        let ys = vis.iter().map(|&i| self.nodes[i].y);
        let (x0, x1) = xs.clone().fold((f32::MAX, f32::MIN), |a, v| (a.0.min(v), a.1.max(v)));
        let (y0, y1) = ys.fold((f32::MAX, f32::MIN), |a, v| (a.0.min(v), a.1.max(v)));
        let k = (w / (x1 - x0 + 80.0)).min(h / (y1 - y0 + 80.0));
        self.cam = (k.clamp(0.25, 2.0), -(x0 + x1) / 2.0, -(y0 + y1) / 2.0);
    }
    pub fn hit(&self, px: f32, py: f32, w: f32, h: f32) -> Option<usize> {
        let mut best = None;
        let mut bd = f32::MAX;
        for i in 0..self.nodes.len() {
            if !self.visible(i) {
                continue;
            }
            let (x, y) = self.to_screen(i, w, h);
            let d = ((x - px).powi(2) + (y - py).powi(2)).sqrt();
            let r = (self.radius(i) * self.cam.0.sqrt() + 3.0).max(7.0);
            if d < r && d < bd {
                bd = d;
                best = Some(i);
            }
        }
        best
    }
    pub fn meta(&self, i: usize) -> String {
        let n = &self.nodes[i];
        let mut v = vec![TIPOS[n.tipo].2.to_string()];
        if !n.date.is_empty() {
            let p: Vec<&str> = n.date.split('-').collect();
            if p.len() == 3 {
                v.push(format!("{}/{}/{}", p[2], p[1], p[0]));
            }
        }
        v.push(format!("{} {}", n.deg, if n.deg == 1 { "link" } else { "links" }));
        if n.read {
            v.push("lida na última resposta".into());
        }
        v.join(" · ")
    }
    pub fn stats(&self) -> (usize, usize) {
        let n = (0..self.nodes.len()).filter(|&i| self.visible(i)).count();
        let l = self.links.iter().filter(|(a, b)| self.visible(*a) && self.visible(*b)).count();
        (n, l)
    }
}

/// Tipos do servidor (`nota`, `fato`, `topico`, `evento`) → tipos do protótipo, pela pasta da nota.
fn tipo_de_servidor(tipo: &str, pasta: &str) -> usize {
    match tipo {
        "fato" => 2,
        "topico" => 3,
        "evento" => 5,
        _ => {
            let p = pasta.to_lowercase();
            if p.starts_with("01") || p.contains("projeto") {
                0
            } else if p.starts_with("02") || p.contains("area") {
                1
            } else if p.contains("conceito") {
                2
            } else if p.contains("pesquisa") || p.contains("radar") {
                3
            } else if p.contains("conversas") {
                5
            } else if p.contains("relat") {
                6
            } else {
                4
            }
        }
    }
}

/// Posições iniciais em espiral + 200 passos de repulsão/atração (grafo pequeno ou médio).
fn layout_forca(nodes: &mut [Node], links: &[(usize, usize)]) {
    let n = nodes.len();
    if n == 0 {
        return;
    }
    for (i, nd) in nodes.iter_mut().enumerate() {
        let a = i as f32 * 2.399963;
        let r = 12.0 * (i as f32 + 1.0).sqrt() * 3.0;
        nd.x = r * a.cos();
        nd.y = r * a.sin();
    }
    let k = 38.0f32;
    let steps = if n > 1500 { 60 } else { 200 };
    for it in 0..steps {
        let t = 40.0 * (1.0 - it as f32 / steps as f32) + 1.0;
        let mut dx = vec![0.0f32; n];
        let mut dy = vec![0.0f32; n];
        // repulsão só entre vizinhos próximos (grade) para escalar
        let cell = k * 3.0;
        let mut grid: std::collections::HashMap<(i32, i32), Vec<usize>> = std::collections::HashMap::new();
        for (i, nd) in nodes.iter().enumerate() {
            grid.entry(((nd.x / cell).floor() as i32, (nd.y / cell).floor() as i32)).or_default().push(i);
        }
        for (i, nd) in nodes.iter().enumerate() {
            let (cx, cy) = ((nd.x / cell).floor() as i32, (nd.y / cell).floor() as i32);
            for gx in cx - 1..=cx + 1 {
                for gy in cy - 1..=cy + 1 {
                    if let Some(v) = grid.get(&(gx, gy)) {
                        for &j in v {
                            if i == j {
                                continue;
                            }
                            let (ex, ey) = (nd.x - nodes[j].x, nd.y - nodes[j].y);
                            let d2 = (ex * ex + ey * ey).max(0.01);
                            let f = k * k / d2;
                            dx[i] += ex * f;
                            dy[i] += ey * f;
                        }
                    }
                }
            }
        }
        for (a, b) in links {
            let (ex, ey) = (nodes[*a].x - nodes[*b].x, nodes[*a].y - nodes[*b].y);
            let d = (ex * ex + ey * ey).sqrt().max(0.01);
            let f = d / k;
            dx[*a] -= ex / d * d * f * 0.05;
            dy[*a] -= ey / d * d * f * 0.05;
            dx[*b] += ex / d * d * f * 0.05;
            dy[*b] += ey / d * d * f * 0.05;
        }
        for i in 0..n {
            // gravidade leve para o centro
            dx[i] -= nodes[i].x * 0.01;
            dy[i] -= nodes[i].y * 0.01;
            let m = (dx[i] * dx[i] + dy[i] * dy[i]).sqrt().max(0.01);
            let s = m.min(t) / m;
            nodes[i].x += dx[i] * s;
            nodes[i].y += dy[i] * s;
        }
    }
}
