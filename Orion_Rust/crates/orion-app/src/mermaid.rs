//! Diagrama Mermaid (fluxograma) desenhado no próprio app, sem WebView (ADR 0004).
//! Entende `graph`/`flowchart` (TD, TB, BT, LR, RL), nós `[ ] ( ) { } (( ))`, setas `-->`, `---`, `-.->`, `==>`
//! com rótulo `|texto|` e cadeias `A --> B --> C`. Qualquer outro tipo de diagrama devolve `None` e o código fica visível.

use crate::theme as t;
use iced::alignment::{Horizontal, Vertical};
use iced::mouse;
use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke, Text};
use iced::{Point, Rectangle, Renderer, Size, Theme};

#[derive(Clone, Copy, PartialEq)]
enum Shape {
    Rect,
    Round,
    Diamond,
    Circle,
}

struct Node {
    id: String,
    label: Vec<String>,
    shape: Shape,
    layer: usize,
    pos: (f32, f32), // centro, em coordenadas do diagrama
    size: (f32, f32),
}

pub struct Diagram {
    horizontal: bool,
    nodes: Vec<Node>,
    edges: Vec<(usize, usize, String, bool)>, // de, para, rótulo, tracejada
    /// por aresta: coordenada transversal do desvio, quando a reta passaria por cima de outro nó
    via: Vec<Option<f32>>,
    pub width: f32,
    pub height: f32,
}

const GAP_FLOW: f32 = 54.0;
const GAP_CROSS: f32 = 22.0;
const CHAR_W: f32 = 7.2;
const LINE_H: f32 = 15.5;
const PAD: f32 = 14.0;

fn is_id(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Lê `ID` + forma opcional a partir de `s`; devolve (id, rótulo, forma, resto).
fn node_ref(s: &str) -> Option<(String, Option<String>, Shape, &str)> {
    let s = s.trim_start();
    let end = s.char_indices().find(|(_, c)| !is_id(*c)).map_or(s.len(), |(i, _)| i);
    if end == 0 {
        return None;
    }
    let id = s[..end].to_string();
    let rest = &s[end..];
    let pairs: [(&str, &str, Shape); 7] = [
        ("((", "))", Shape::Circle),
        ("([", "])", Shape::Round),
        ("[[", "]]", Shape::Rect),
        ("[", "]", Shape::Rect),
        ("(", ")", Shape::Round),
        ("{{", "}}", Shape::Diamond),
        ("{", "}", Shape::Diamond),
    ];
    for (open, close, shape) in pairs {
        if let Some(after) = rest.strip_prefix(open) {
            let fim = after.find(close)?;
            let label = after[..fim].trim().trim_matches('"').to_string();
            return Some((id, Some(label), shape, &after[fim + close.len()..]));
        }
    }
    Some((id, None, Shape::Rect, rest))
}

/// Lê uma seta e o rótulo opcional; devolve (rótulo, tracejada, resto).
fn arrow(s: &str) -> Option<(String, bool, &str)> {
    let s = s.trim_start();
    let mut rest;
    let mut dashed = false;
    let mut label = String::new();
    if let Some(r) = s.strip_prefix("-.->") {
        dashed = true;
        rest = r;
    } else if let Some(r) = s.strip_prefix("==>") {
        rest = r;
    } else if let Some(r) = s.strip_prefix("-->") {
        rest = r;
    } else if let Some(r) = s.strip_prefix("---") {
        rest = r;
    } else if let Some(r) = s.strip_prefix("--") {
        // `-- texto -->`
        let fim = r.find("-->")?;
        label = r[..fim].trim().to_string();
        rest = &r[fim + 3..];
    } else {
        return None;
    }
    let r = rest.trim_start();
    if let Some(r) = r.strip_prefix('|') {
        let fim = r.find('|')?;
        label = r[..fim].trim().trim_matches('"').to_string();
        rest = &r[fim + 1..];
    }
    Some((label, dashed, rest))
}

fn split_label(s: &str) -> Vec<String> {
    s.replace("<br/>", "\n").replace("<br>", "\n").replace("<br />", "\n").split('\n').map(|l| l.trim().to_string()).collect()
}

pub fn parse(lines: &[String]) -> Option<Diagram> {
    let mut it = lines.iter().map(|l| l.trim()).filter(|l| !l.is_empty() && !l.starts_with("%%"));
    let head = it.next()?;
    let mut w = head.split_whitespace();
    if !matches!(w.next()?, "graph" | "flowchart") {
        return None;
    }
    let horizontal = matches!(w.next().unwrap_or("TD"), "LR" | "RL");
    let mut nodes: Vec<Node> = Vec::new();
    let mut edges: Vec<(usize, usize, String, bool)> = Vec::new();
    let ensure = |nodes: &mut Vec<Node>, id: String, label: Option<String>, shape: Shape| -> usize {
        if let Some(i) = nodes.iter().position(|n| n.id == id) {
            if let Some(l) = label {
                nodes[i].label = split_label(&l);
                nodes[i].shape = shape;
            }
            return i;
        }
        let label = split_label(&label.unwrap_or_else(|| id.clone()));
        nodes.push(Node { id, label, shape, layer: 0, pos: (0.0, 0.0), size: (0.0, 0.0) });
        nodes.len() - 1
    };
    for line in it {
        let first = line.split_whitespace().next().unwrap_or("");
        if matches!(first, "style" | "classDef" | "class" | "linkStyle" | "click" | "subgraph" | "end" | "direction") {
            continue;
        }
        for stmt in line.split(';') {
            let mut rest = stmt.trim();
            if rest.is_empty() {
                continue;
            }
            // sintaxe que não entendemos (`A & B --> C`, estilos inline...): o código aparece inteiro em vez de um diagrama incompleto
            let Some((id, label, shape, r)) = node_ref(rest) else { return None };
            let mut prev = ensure(&mut nodes, id, label, shape);
            rest = r;
            while let Some((lab, dashed, r)) = arrow(rest) {
                let Some((id, label, shape, r)) = node_ref(r) else { return None };
                let cur = ensure(&mut nodes, id, label, shape);
                edges.push((prev, cur, lab, dashed));
                prev = cur;
                rest = r;
            }
            if !rest.trim().is_empty() {
                return None;
            }
        }
    }
    // laço em si mesmo não é desenhado: devolve None para o código aparecer inteiro
    if nodes.is_empty() || nodes.len() > 60 || edges.iter().any(|e| e.0 == e.1) {
        return None;
    }
    let via = vec![None; edges.len()];
    let mut d = Diagram { horizontal, nodes, edges, via, width: 0.0, height: 0.0 };
    d.layout();
    Some(d)
}

impl Diagram {
    fn layout(&mut self) {
        let n = self.nodes.len();
        // camada = maior caminho a partir das origens (limitado, para não girar em ciclos)
        for _ in 0..n {
            let mut mudou = false;
            for &(a, b, _, _) in &self.edges {
                if a != b && self.nodes[b].layer < self.nodes[a].layer + 1 && self.nodes[a].layer + 1 < n {
                    self.nodes[b].layer = self.nodes[a].layer + 1;
                    mudou = true;
                }
            }
            if !mudou {
                break;
            }
        }
        for nd in &mut self.nodes {
            let cols = nd.label.iter().map(|l| l.chars().count()).max().unwrap_or(1) as f32;
            let (mut w, mut h) = ((cols * CHAR_W + 26.0).max(56.0), nd.label.len() as f32 * LINE_H + 18.0);
            match nd.shape {
                Shape::Diamond => {
                    w = w * 1.45;
                    h = h * 1.5;
                }
                Shape::Circle => {
                    let d = w.max(h) + 6.0;
                    w = d;
                    h = d;
                }
                _ => {}
            }
            nd.size = (w, h);
        }
        let camadas = self.nodes.iter().map(|n| n.layer).max().unwrap_or(0) + 1;
        let mut por: Vec<Vec<usize>> = vec![Vec::new(); camadas];
        for (i, nd) in self.nodes.iter().enumerate() {
            por[nd.layer].push(i);
        }
        // extensão de cada camada ao longo do eixo transversal e espessura ao longo do fluxo
        let cross = |i: usize, s: &Self| if s.horizontal { s.nodes[i].size.1 } else { s.nodes[i].size.0 };
        let flow = |i: usize, s: &Self| if s.horizontal { s.nodes[i].size.0 } else { s.nodes[i].size.1 };
        let ext: Vec<f32> = por.iter().map(|l| l.iter().map(|&i| cross(i, self)).sum::<f32>() + GAP_CROSS * l.len().saturating_sub(1) as f32).collect();
        let larg = ext.iter().cloned().fold(0.0, f32::max);
        let mut off = 0.0;
        for (li, l) in por.iter().enumerate() {
            let espessura = l.iter().map(|&i| flow(i, self)).fold(0.0, f32::max);
            let mut c = (larg - ext[li]) / 2.0;
            for &i in l {
                let cs = cross(i, self);
                let (a, b) = (off + espessura / 2.0, c + cs / 2.0);
                self.nodes[i].pos = if self.horizontal { (a, b) } else { (b, a) };
                c += cs + GAP_CROSS;
            }
            off += espessura + GAP_FLOW;
        }
        let total_flow = off - GAP_FLOW;
        // arestas que pulam camadas e cruzariam outro nó contornam pelo lado
        let cr = |i: usize, s: &Self| if s.horizontal { (s.nodes[i].pos.1, s.nodes[i].pos.0) } else { (s.nodes[i].pos.0, s.nodes[i].pos.1) };
        let mut max_cross = larg;
        for k in 0..self.edges.len() {
            let (a, b, _, _) = self.edges[k];
            let (la, lb) = (self.nodes[a].layer, self.nodes[b].layer);
            if lb <= la + 1 {
                continue;
            }
            let (ca, fa) = cr(a, self);
            let (cb, fb) = cr(b, self);
            let mut bate = false;
            let mut borda_max = ca.max(cb);
            for (i, nd) in self.nodes.iter().enumerate() {
                if nd.layer <= la || nd.layer >= lb {
                    continue;
                }
                let (ci, fi) = cr(i, self);
                let t = if (fb - fa).abs() < 0.01 { 0.5 } else { (fi - fa) / (fb - fa) };
                let x_linha = ca + (cb - ca) * t;
                let meia = if self.horizontal { nd.size.1 } else { nd.size.0 } / 2.0;
                if (x_linha - ci).abs() < meia + 6.0 {
                    bate = true;
                }
                borda_max = borda_max.max(ci + meia);
            }
            if bate {
                let x = borda_max + 24.0;
                max_cross = max_cross.max(x + 10.0);
                self.via[k] = Some(x);
            }
        }
        let larg = max_cross;
        let (w, h) = if self.horizontal { (total_flow, larg) } else { (larg, total_flow) };
        self.width = w + PAD * 2.0;
        self.height = h + PAD * 2.0;
    }
}

/// Ponto onde a reta do centro `c` até `to` cruza a borda do nó (retângulo/losango/círculo aproximados).
fn borda(nd: &Node, toward: (f32, f32)) -> (f32, f32) {
    let (dx, dy) = (toward.0 - nd.pos.0, toward.1 - nd.pos.1);
    if dx == 0.0 && dy == 0.0 {
        return nd.pos;
    }
    let (hw, hh) = (nd.size.0 / 2.0, nd.size.1 / 2.0);
    let k = match nd.shape {
        Shape::Circle => hw / (dx * dx + dy * dy).sqrt(),
        Shape::Diamond => 1.0 / (dx.abs() / hw + dy.abs() / hh),
        _ => 1.0 / (dx.abs() / hw).max(dy.abs() / hh),
    };
    (nd.pos.0 + dx * k, nd.pos.1 + dy * k)
}

impl<M> canvas::Program<M> for Diagram {
    type State = ();
    fn draw(&self, _s: &(), renderer: &Renderer, _th: &Theme, bounds: Rectangle, _c: mouse::Cursor) -> Vec<Geometry> {
        let mut f = Frame::new(renderer, bounds.size());
        let s = (bounds.width / self.width).min(1.0);
        let ox = (bounds.width - self.width * s) / 2.0;
        let p = |x: f32, y: f32| Point::new(ox + (x + PAD) * s, (y + PAD) * s);
        for (k, (a, b, lab, dashed)) in self.edges.iter().enumerate() {
            let (na, nb) = (&self.nodes[*a], &self.nodes[*b]);
            if a == b {
                continue;
            }
            // pontos do traçado no espaço do diagrama: reta ou, se há desvio, três trechos pelo lado
            let pts: Vec<(f32, f32)> = match self.via[k] {
                Some(x) => {
                    let (ia, ib) = if self.horizontal { ((na.pos.0, x), (nb.pos.0, x)) } else { ((x, na.pos.1), (x, nb.pos.1)) };
                    vec![borda(na, ia), ia, ib, borda(nb, ib)]
                }
                None => vec![borda(na, nb.pos), borda(nb, na.pos)],
            };
            let pts: Vec<Point> = pts.iter().map(|q| p(q.0, q.1)).collect();
            let stroke = Stroke::default().with_width(1.4).with_color(t::FG3);
            for w in pts.windows(2) {
                let (pa, pb) = (w[0], w[1]);
                if *dashed {
                    let (dx, dy) = (pb.x - pa.x, pb.y - pa.y);
                    let len = (dx * dx + dy * dy).sqrt().max(1.0);
                    let mut d = 0.0;
                    while d < len {
                        let e = (d + 5.0 * s).min(len);
                        f.stroke(&Path::line(Point::new(pa.x + dx * d / len, pa.y + dy * d / len), Point::new(pa.x + dx * e / len, pa.y + dy * e / len)), stroke);
                        d += 9.0 * s;
                    }
                } else {
                    f.stroke(&Path::line(pa, pb), stroke);
                }
            }
            // ponta da seta, no último trecho
            let (pa, pb) = (pts[pts.len() - 2], pts[pts.len() - 1]);
            let ang = (pb.y - pa.y).atan2(pb.x - pa.x);
            let tam = 8.0 * s;
            let ponta = Path::new(|bd| {
                bd.move_to(pb);
                bd.line_to(Point::new(pb.x - tam * (ang - 0.45).cos(), pb.y - tam * (ang - 0.45).sin()));
                bd.line_to(Point::new(pb.x - tam * (ang + 0.45).cos(), pb.y - tam * (ang + 0.45).sin()));
                bd.close();
            });
            f.fill(&ponta, t::FG3);
            if !lab.is_empty() {
                // com desvio, o rótulo fica no trecho do meio; sem desvio, no meio da reta
                let (m1, m2) = if pts.len() == 4 { (pts[1], pts[2]) } else { (pts[0], pts[1]) };
                let mid = Point::new((m1.x + m2.x) / 2.0, (m1.y + m2.y) / 2.0);
                let w = lab.chars().count() as f32 * 6.4 * s + 10.0 * s;
                f.fill(&Path::rectangle(Point::new(mid.x - w / 2.0, mid.y - 8.0 * s), Size::new(w, 16.0 * s)), t::BG);
                f.fill_text(Text { content: lab.clone(), position: mid, color: t::FG2, size: (11.5 * s).into(), font: t::UI, align_x: Horizontal::Center.into(), align_y: Vertical::Center, ..Text::default() });
            }
        }
        for nd in &self.nodes {
            let c = p(nd.pos.0, nd.pos.1);
            let (w, h) = (nd.size.0 * s, nd.size.1 * s);
            let top = Point::new(c.x - w / 2.0, c.y - h / 2.0);
            let shape = match nd.shape {
                Shape::Rect => Path::rounded_rectangle(top, Size::new(w, h), 6.0.into()),
                Shape::Round => Path::rounded_rectangle(top, Size::new(w, h), (h / 2.0).into()),
                Shape::Circle => Path::circle(c, w / 2.0),
                Shape::Diamond => Path::new(|bd| {
                    bd.move_to(Point::new(c.x, c.y - h / 2.0));
                    bd.line_to(Point::new(c.x + w / 2.0, c.y));
                    bd.line_to(Point::new(c.x, c.y + h / 2.0));
                    bd.line_to(Point::new(c.x - w / 2.0, c.y));
                    bd.close();
                }),
            };
            f.fill(&shape, t::RAISE);
            f.stroke(&shape, Stroke::default().with_width(1.2).with_color(t::LINE2));
            let n = nd.label.len() as f32;
            for (i, l) in nd.label.iter().enumerate() {
                let y = c.y + (i as f32 - (n - 1.0) / 2.0) * LINE_H * s;
                f.fill_text(Text { content: l.clone(), position: Point::new(c.x, y), color: t::FG, size: (12.5 * s).into(), font: t::UI, align_x: Horizontal::Center.into(), align_y: Vertical::Center, ..Text::default() });
            }
        }
        vec![f.into_geometry()]
    }
}
