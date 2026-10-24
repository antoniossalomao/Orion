//! Grafo do vault (§16): dados do snapshot de 15/10/2026 e a lógica de câmera, foco, filtro local e distribuição orgânica por forças.

use crate::data::READ_NOTES;
use serde::Deserialize;
use std::sync::OnceLock;

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

#[derive(Debug, Clone)]
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
    /// Filtro de grafo local: `(nó_central, profundidade 1..=2)`.
    pub local: Option<(usize, u8)>,
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

/// Só os dados de layout atravessam a thread; o cache de vídeo nasce na UI.
#[derive(Debug, Clone)]
pub struct Prepared {
    nodes: Vec<Node>,
    links: Vec<(usize, usize)>,
}

static VAULT_COORDS: OnceLock<Vec<(f32, f32)>> = OnceLock::new();

impl Graph {
    /// Grafo vazio (modo real, antes de carregar do servidor).
    pub fn empty() -> Self {
        Self::build(vec![], vec![])
    }

    /// Converte `/grafo/completo` (`nodes`/`links`) e calcula posições por força orgânica.
    pub fn from_server(v: &serde_json::Value) -> Self {
        Self::from_prepared(Self::prepare(v))
    }

    pub fn from_prepared(prepared: Prepared) -> Self {
        Self::build(prepared.nodes, prepared.links)
    }

    pub fn prepare(v: &serde_json::Value) -> Prepared {
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
        Prepared { nodes, links }
    }

    fn build(mut nodes: Vec<Node>, links: Vec<(usize, usize)>) -> Self {
        let mut adj = vec![vec![]; nodes.len()];
        for (a, b) in &links {
            adj[*a].push(*b);
            adj[*b].push(*a);
        }
        for (i, v) in adj.iter_mut().enumerate() {
            v.sort_unstable();
            v.dedup();
            nodes[i].deg = v.len();
            nodes[i].hub = nodes[i].deg >= 38;
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
            local: None,
            cam: (1.0, 0.0, 0.0),
            cut,
            cut_idx,
            read_on: false,
            rot: 0.0,
            spinning: false,
            fitted: false,
            size: (0.0, 0.0),
            cache: iced::widget::canvas::Cache::new(),
        }
    }

    pub fn load() -> Self {
        let raw: Raw = serde_json::from_str(include_str!("../assets/vault.json")).expect("vault.json");
        let tipo_idx = |s: &str| TIPOS.iter().position(|t| t.0 == s).unwrap_or(4);
        let mut nodes: Vec<Node> = raw
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
        for c in raw.e.chunks(2) {
            if c.len() == 2 && c[0] < nodes.len() && c[1] < nodes.len() && c[0] != c[1] {
                links.push((c[0], c[1]));
            }
        }
        let coords = VAULT_COORDS.get_or_init(|| {
            let mut copy = nodes.clone();
            layout_forca(&mut copy, &links);
            copy.iter().map(|n| (n.x, n.y)).collect()
        });
        for (nd, &(x, y)) in nodes.iter_mut().zip(coords.iter()) {
            nd.x = x;
            nd.y = y;
        }
        Self::build(nodes, links)
    }

    pub fn in_local(&self, i: usize) -> bool {
        let Some((center, depth)) = self.local else {
            return true;
        };
        if i == center {
            return true;
        }
        if self.adj.get(center).is_some_and(|nb| nb.binary_search(&i).is_ok()) {
            return true;
        }
        if depth >= 2 {
            if let Some(nb) = self.adj.get(center) {
                return nb.iter().any(|&m| self.adj.get(m).is_some_and(|nb2| nb2.binary_search(&i).is_ok()));
            }
        }
        false
    }

    pub fn visible(&self, i: usize) -> bool {
        let n = &self.nodes[i];
        self.filt[n.tipo] && (n.date.is_empty() || n.date <= self.cut) && self.in_local(i)
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
        (n.r + (n.deg.max(1) as f32).ln() * 0.72).clamp(2.2, 8.6)
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
        self.cache.clear();
        let mut vis: Vec<usize> = (0..self.nodes.len()).filter(|&i| self.visible(i) && self.nodes[i].deg > 0).collect();
        if vis.is_empty() {
            vis = (0..self.nodes.len()).filter(|&i| self.visible(i)).collect();
        }
        if vis.is_empty() || w <= 0.0 || h <= 0.0 {
            return;
        }
        let panel_w = if self.sel.is_some() && w > 640.0 { 332.0 } else { 0.0 };
        let eff_w = (w - panel_w).max(200.0);
        let mut x0 = f32::MAX;
        let mut x1 = f32::MIN;
        let mut y0 = f32::MAX;
        let mut y1 = f32::MIN;
        for &i in &vis {
            let (rx, ry) = self.rotp(i);
            x0 = x0.min(rx);
            x1 = x1.max(rx);
            y0 = y0.min(ry);
            y1 = y1.max(ry);
        }
        let span_x = (x1 - x0).max(60.0) + 64.0;
        let span_y = (y1 - y0).max(60.0) + 64.0;
        let k = (eff_w / span_x).min(h / span_y).clamp(0.28, 2.4);
        let cx = (x0 + x1) * 0.5;
        let cy = (y0 + y1) * 0.5;
        let shift_x = -(panel_w * 0.5) / k;
        self.cam = (k, -cx + shift_x, -cy);
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

/// Motor de forças orgânico com semeadura por topologia, repulsão multiescala,
/// mola ponderada pelo grau (estilo d3-force / Obsidian), gravidade isotrópica,
/// equalização de arredondamento e resolução de colisões circulares.
fn layout_forca(nodes: &mut [Node], links: &[(usize, usize)]) {
    let n = nodes.len();
    if n == 0 {
        return;
    }

    let mut adj = vec![Vec::<usize>::new(); n];
    for &(a, b) in links {
        if a < n && b < n && a != b {
            adj[a].push(b);
            adj[b].push(a);
        }
    }
    for v in &mut adj {
        v.sort_unstable();
        v.dedup();
    }
    let deg: Vec<usize> = adj.iter().map(|v| v.len()).collect();

    // 1. Componentes conexas para distinguir o núcleo conectado das ilhas desconectadas.
    let mut comp = vec![usize::MAX; n];
    let mut comp_sizes = Vec::<usize>::new();
    let mut queue = std::collections::VecDeque::new();
    for i in 0..n {
        if comp[i] != usize::MAX {
            continue;
        }
        let cid = comp_sizes.len();
        let mut sz = 0usize;
        comp[i] = cid;
        queue.push_back(i);
        while let Some(u) = queue.pop_front() {
            sz += 1;
            for &v in &adj[u] {
                if comp[v] == usize::MAX {
                    comp[v] = cid;
                    queue.push_back(v);
                }
            }
        }
        comp_sizes.push(sz);
    }
    let main_comp = comp_sizes
        .iter()
        .enumerate()
        .max_by_key(|&(_, sz)| sz)
        .map(|(cid, _)| cid)
        .unwrap_or(0);

    // 2. Semeadura guiada por topologia: hubs e projetos em espiral de Fermat; folhas junto aos seus hubs.
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| {
        let ma = comp[a] == main_comp;
        let mb = comp[b] == main_comp;
        mb.cmp(&ma)
            .then_with(|| deg[b].cmp(&deg[a]))
            .then_with(|| nodes[a].tipo.cmp(&nodes[b].tipo))
            .then_with(|| a.cmp(&b))
    });

    let golden = 2.399_963_1_f32;
    let mut placed = vec![false; n];
    let mut core_rank = 0usize;
    let mut island_center = std::collections::HashMap::<usize, (f32, f32)>::new();
    let mut island_rank = 0usize;

    for &i in &order {
        if comp[i] != main_comp {
            let cid = comp[i];
            let (cx, cy) = *island_center.entry(cid).or_insert_with(|| {
                let a = (island_rank as f32) * golden + 0.65;
                let r = 175.0 + ((island_rank % 6) as f32) * 10.5;
                island_rank += 1;
                (r * a.cos(), r * a.sin())
            });
            let local_a = (i as f32) * golden;
            let local_r = 5.5 * ((deg[i] as f32) + 1.0).sqrt();
            nodes[i].x = cx + local_r * local_a.cos();
            nodes[i].y = cy + local_r * local_a.sin();
            placed[i] = true;
            continue;
        }

        let mut sx = 0.0f32;
        let mut sy = 0.0f32;
        let mut sw = 0.0f32;
        for &nb in &adj[i] {
            if placed[nb] {
                let w = if deg[nb] > 110 {
                    0.22
                } else if nodes[nb].tipo <= 2 {
                    2.4
                } else {
                    1.0
                };
                sx += nodes[nb].x * w;
                sy += nodes[nb].y * w;
                sw += w;
            }
        }
        if sw > 0.0 && deg[i] <= 18 {
            let bx = sx / sw;
            let by = sy / sw;
            let a = (i as f32) * golden;
            let jitter = 16.0 + ((i % 13) as f32) * 2.8;
            nodes[i].x = bx + jitter * a.cos();
            nodes[i].y = by + jitter * a.sin();
        } else {
            let a = (core_rank as f32) * golden;
            let r = 14.0 * ((core_rank as f32 + 1.0).sqrt());
            nodes[i].x = r * a.cos();
            nodes[i].y = r * a.sin();
            core_rank += 1;
        }
        placed[i] = true;
    }

    // 3. Relaxamento por forças (repulsão multiescala + mola com viés de grau + gravidade radial).
    let k = 28.0f32;
    let steps = if n > 1500 { 65 } else { 110 };
    let charge: Vec<f32> = (0..n)
        .map(|i| {
            let base = 1.0 + (deg[i].min(60) as f32).ln_1p() * 0.26;
            if nodes[i].tipo == 6 {
                base * 0.35
            } else {
                base
            }
        })
        .collect();

    let mut dx = vec![0.0f32; n];
    let mut dy = vec![0.0f32; n];

    for it in 0..steps {
        let progress = it as f32 / steps as f32;
        let temp = 28.0 * (1.0 - progress).powi(2) + 1.2;
        dx.fill(0.0);
        dy.fill(0.0);

        let cell = 84.0f32;
        let mut grid: std::collections::HashMap<(i32, i32), Vec<usize>> = std::collections::HashMap::with_capacity(128);
        let mut cell_com: Vec<(i32, i32, f32, f32, f32)> = Vec::with_capacity(128);

        for (i, nd) in nodes.iter().enumerate() {
            let c = ((nd.x / cell).floor() as i32, (nd.y / cell).floor() as i32);
            grid.entry(c).or_default().push(i);
        }
        for (&(gx, gy), idxs) in &grid {
            let mut cx = 0.0f32;
            let mut cy = 0.0f32;
            let mut qsum = 0.0f32;
            for &i in idxs {
                let q = charge[i];
                cx += nodes[i].x * q;
                cy += nodes[i].y * q;
                qsum += q;
            }
            if qsum > 0.0 {
                cell_com.push((gx, gy, cx / qsum, cy / qsum, qsum));
            }
        }

        // Repulsão local exata (vizinhança 5×5) + repulsão global por centro de massa das células distantes.
        for i in 0..n {
            let (xi, yi, qi) = (nodes[i].x, nodes[i].y, charge[i]);
            let (cx, cy) = ((xi / cell).floor() as i32, (yi / cell).floor() as i32);
            let mut fx = 0.0f32;
            let mut fy = 0.0f32;

            for gx in (cx - 2)..=(cx + 2) {
                for gy in (cy - 2)..=(cy + 2) {
                    if let Some(v) = grid.get(&(gx, gy)) {
                        for &j in v {
                            if i == j {
                                continue;
                            }
                            let (ex, ey) = (xi - nodes[j].x, yi - nodes[j].y);
                            let d2 = ex * ex + ey * ey + 18.0;
                            let inv_d = d2.sqrt().recip();
                            let rep = (k * k * qi * charge[j]) / d2;
                            fx += ex * inv_d * rep;
                            fy += ey * inv_d * rep;
                        }
                    }
                }
            }

            for &(gx, gy, com_x, com_y, qcell) in &cell_com {
                if (gx - cx).abs() <= 2 && (gy - cy).abs() <= 2 {
                    continue;
                }
                let (ex, ey) = (xi - com_x, yi - com_y);
                let d2 = ex * ex + ey * ey + 120.0;
                let inv_d = d2.sqrt().recip();
                let rep = (k * k * qi * qcell * 0.85) / d2;
                fx += ex * inv_d * rep;
                fy += ey * inv_d * rep;
            }

            dx[i] += fx;
            dy[i] += fy;
        }

        // Atração de arestas com viés de grau estilo d3-force / Obsidian.
        for &(a, b) in links {
            if a >= n || b >= n || a == b {
                continue;
            }
            let (da, db) = (deg[a].max(1) as f32, deg[b].max(1) as f32);
            let (dmin, dmax) = (da.min(db), da.max(db));
            let (ex, ey) = (nodes[b].x - nodes[a].x, nodes[b].y - nodes[a].y);
            let d = (ex * ex + ey * ey).sqrt().max(0.1);
            let target = 24.0 + dmin.sqrt() * 3.2 + dmax.sqrt() * 1.15;
            let hub_damp = if dmax > 110.0 { 0.34 } else if dmax > 55.0 { 0.62 } else { 1.0 };
            let strength = (0.16 * hub_damp) / (1.0 + dmin * 0.12);
            let force = (d - target) * strength;
            let (ux, uy) = (ex / d * force, ey / d * force);
            let bias_a = (db + 2.5) / (da + db + 5.0);
            let bias_b = 1.0 - bias_a;
            dx[a] += ux * bias_a;
            dy[a] += uy * bias_a;
            dx[b] -= ux * bias_b;
            dy[b] -= uy * bias_b;
        }

        // Gravidade central isotrópica + contenção elástica suave para ilhas desconectadas.
        for i in 0..n {
            let (xi, yi) = (nodes[i].x, nodes[i].y);
            let r = (xi * xi + yi * yi).sqrt().max(0.1);
            if comp[i] != main_comp {
                let target_r = 165.0 + ((comp[i] * 37 + i * 13) % 65) as f32;
                let radial_f = (target_r - r) * 0.045;
                dx[i] += (xi / r) * radial_f - xi * 0.014;
                dy[i] += (yi / r) * radial_f - yi * 0.014;
            } else {
                let base_g = if deg[i] >= 35 {
                    0.075
                } else if deg[i] <= 2 {
                    0.050
                } else {
                    0.040
                };
                let g = base_g + (r / 270.0).powi(2) * 0.042;
                dx[i] -= xi * g;
                dy[i] -= yi * g;
            }

            let m = (dx[i] * dx[i] + dy[i] * dy[i]).sqrt().max(0.01);
            let step = m.min(temp) / m;
            nodes[i].x += dx[i] * step;
            nodes[i].y += dy[i] * step;
        }
    }

    // 4. Equalização isotrópica (arredondamento orgânico nos 4 quadrantes) centrada nos nós visíveis por padrão.
    equalize_roundness(nodes, &comp, main_comp);

    // 5. Resolução iterativa de colisões circulares para garantir separação nítida entre todos os nós.
    resolve_collisions(nodes, &deg);
}

fn equalize_roundness(nodes: &mut [Node], comp: &[usize], main_comp: usize) {
    let active: Vec<usize> = (0..nodes.len())
        .filter(|&i| nodes[i].tipo != 6 && comp[i] == main_comp)
        .collect();
    if active.len() < 8 {
        return;
    }
    let inv_n = 1.0 / active.len() as f32;
    let cx: f32 = active.iter().map(|&i| nodes[i].x).sum::<f32>() * inv_n;
    let cy: f32 = active.iter().map(|&i| nodes[i].y).sum::<f32>() * inv_n;
    for nd in nodes.iter_mut() {
        nd.x -= cx;
        nd.y -= cy;
    }

    let (mut sxx, mut syy, mut sxy) = (0.0f32, 0.0f32, 0.0f32);
    for &i in &active {
        let (x, y) = (nodes[i].x, nodes[i].y);
        sxx += x * x;
        syy += y * y;
        sxy += x * y;
    }
    sxx *= inv_n;
    syy *= inv_n;
    sxy *= inv_n;

    let tr = sxx + syy;
    let disc = ((sxx - syy).powi(2) + 4.0 * sxy * sxy).sqrt();
    let l1 = ((tr + disc) * 0.5).max(1.0);
    let l2 = ((tr - disc) * 0.5).max(1.0);
    let (vx, vy) = if sxy.abs() > 1e-4 {
        let (ax, ay) = (l1 - syy, sxy);
        let len = (ax * ax + ay * ay).sqrt().max(1e-4);
        (ax / len, ay / len)
    } else if sxx >= syy {
        (1.0, 0.0)
    } else {
        (0.0, 1.0)
    };
    let target_std = (l1.sqrt() * l2.sqrt()).sqrt().clamp(85.0, 135.0);
    let sx = (target_std / l1.sqrt()).clamp(0.72, 1.38);
    let sy = (target_std / l2.sqrt()).clamp(0.72, 1.38);

    for (idx, nd) in nodes.iter_mut().enumerate() {
        let u = nd.x * vx + nd.y * vy;
        let v = -nd.x * vy + nd.y * vx;
        let (u2, v2) = (u * sx, v * sy);
        let (mut x, mut y) = (u2 * vx - v2 * vy, u2 * vy + v2 * vx);
        let r = (x * x + y * y).sqrt();
        let knee = 205.0 + ((idx * 29) % 42) as f32;
        if r > knee {
            let nr = knee + 88.0 * ((r - knee) / 88.0).ln_1p();
            x *= nr / r;
            y *= nr / r;
        }
        nd.x = x;
        nd.y = y;
    }

    let mut mx = 0.0f32;
    let mut my = 0.0f32;
    for &i in &active {
        mx += nodes[i].x;
        my += nodes[i].y;
    }
    mx *= inv_n;
    my *= inv_n;
    for nd in nodes.iter_mut() {
        nd.x -= mx;
        nd.y -= my;
    }
}

fn resolve_collisions(nodes: &mut [Node], deg: &[usize]) {
    let n = nodes.len();
    let radii: Vec<f32> = (0..n)
        .map(|i| (nodes[i].r + (deg[i].max(1) as f32).ln() * 0.72).clamp(2.2, 8.6) + 3.4)
        .collect();
    let cell = 24.0f32;
    for _ in 0..12 {
        let mut grid: std::collections::HashMap<(i32, i32), Vec<usize>> = std::collections::HashMap::with_capacity(n);
        for (i, nd) in nodes.iter().enumerate() {
            grid.entry(((nd.x / cell).floor() as i32, (nd.y / cell).floor() as i32)).or_default().push(i);
        }
        for i in 0..n {
            let (cx, cy) = ((nodes[i].x / cell).floor() as i32, (nodes[i].y / cell).floor() as i32);
            for gx in (cx - 1)..=(cx + 1) {
                for gy in (cy - 1)..=(cy + 1) {
                    let Some(bucket) = grid.get(&(gx, gy)) else { continue };
                    for &j in bucket {
                        if j <= i {
                            continue;
                        }
                        let (ex, ey) = (nodes[i].x - nodes[j].x, nodes[i].y - nodes[j].y);
                        let d2 = ex * ex + ey * ey;
                        let min_d = radii[i] + radii[j];
                        if d2 < min_d * min_d {
                            let d = d2.sqrt().max(0.05);
                            let overlap = (min_d - d) * 0.52;
                            let (ux, uy) = (ex / d * overlap, ey / d * overlap);
                            nodes[i].x += ux;
                            nodes[i].y += uy;
                            nodes[j].x -= ux;
                            nodes[j].y -= uy;
                        }
                    }
                }
            }
        }
    }
}
