//! Tela "Maestro" (§15): canvas infinito de chats, tarefas, notas, páginas e molduras ligados por setas.
//!
//! O canvas desenha grade, setas, molduras e cabeçalhos e trata selecionar, arrastar, redimensionar e
//! ligar; o conteúdo dos nós (chat, nota, página) são widgets fixados por cima (`pin`) quando o zoom é
//! 100%. Abaixo disso a tela vira mapa: cada nó é um cartão com título e resumo.
//!
//! Mouse e teclado: arrastar no vazio move a câmera (botão do meio também); Ctrl+roda dá zoom no ponto
//! do cursor; Shift+clique soma à seleção e Shift+arrastar seleciona por área; duplo clique no vazio
//! cria uma nota e, no mapa, abre o nó em 100%; Delete apaga a seleção (com confirmação se houver
//! conteúdo); Esc limpa; Ctrl+A seleciona tudo; Ctrl+D duplica; Ctrl+0 ajusta ao conteúdo.
//!
//! (REGRAS.md: a saída de um chat só entra em outro por clique na seta, como dado; o chat do nó só
//! lê e propõe; página é só link, nada é aberto sozinho; aprovação só por botão.)

use crate::app::{App, Msg, Sec};
use crate::canvas_ui::{CMsg, CNode, Cv, AGENTS, CLAUDE_MODELS, HEAD};
use crate::md;
use crate::sky::spinner;
use crate::theme as t;
use crate::ui::*;
use crate::views::login::field_style;
use iced::alignment::{Horizontal, Vertical};
use iced::mouse;
use iced::keyboard::{self, key::Named, Key};
use iced::widget::canvas::{self, path::Path, Action, Event, Frame, Geometry, LineDash, Stroke, Text};
use iced::widget::{column, container, pin, row, scrollable, stack, text_editor, text_input};
use iced::{Border, Color, Length, Point, Rectangle, Renderer, Size, Theme, Vector};

fn cv(m: Cv) -> Msg {
    Msg::Canvas(m)
}

fn agent_label(n: &CNode) -> String {
    let base = AGENTS.iter().find(|a| a.0 == n.agent).map(|a| a.1).unwrap_or("Orion");
    match (n.agent.as_str(), n.model.as_str()) {
        ("claude", m) => format!("{base} {}", CLAUDE_MODELS.iter().find(|c| c.0 == m).map(|c| c.1).unwrap_or("Sonnet")),
        _ => base.to_string(),
    }
}

fn state_label(s: &str) -> &'static str {
    match s {
        "ok" => "pronto",
        "rodando" => "rodando",
        "falhou" => "falhou",
        "pulada" => "pulada",
        _ => "pendente",
    }
}

fn state_color(s: &str) -> Color {
    match s {
        "ok" => t::OK,
        "rodando" => t::ACCENT,
        "falhou" => t::DANGER,
        _ => t::FG4,
    }
}

/// Primeira linha útil do nó, para o mapa e o cabeçalho de resumo.
fn summary(n: &CNode) -> String {
    let raw = match n.kind.as_str() {
        "chat" => n.msgs.iter().rev().find(|m| m.role == "ai" || m.role == "user").map(|m| m.text.clone()).unwrap_or_default(),
        "tarefa" => if n.out.is_empty() { String::new() } else { n.out.clone() },
        "nota" => n.text.clone(),
        "pagina" => n.url.clone(),
        _ => String::new(),
    };
    raw.replace(['*', '`', '#'], "").chars().take(220).collect()
}

// ─────────────────────────────── geometria para o canvas ───────────────────────────────

#[derive(Clone)]
struct Geo {
    id: String,
    kind: String,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    label: String,
    title: String,
    summary: String,
    dot: Color,
    sel: bool,
    maestro: bool,
    busy: bool,
}

#[derive(Clone)]
struct Board {
    /// Molduras primeiro: ficam sempre atrás dos outros nós.
    geos: Vec<Geo>,
    /// (id, origem, destino)
    edges: Vec<(String, String, String)>,
    sel_edge: Option<String>,
    vx: f32,
    vy: f32,
    z: f32,
    detail: bool,
}

#[derive(Default)]
pub struct Interaction {
    drag: Option<Drag>,
    mods: keyboard::Modifiers,
    cursor: Option<Point>,
    hover: Option<String>,
    last_click: Option<(std::time::Instant, Point)>,
    size: Size,
    grid: canvas::Cache,
    grid_key: std::cell::Cell<(i32, i32, i32, i32, i32)>,
}

enum Drag {
    Pan(Point),
    Node { id: String, off: Vector },
    Resize { id: String, from: Point, w: f32, h: f32, min: (f32, f32) },
    Link { from: String },
    Marquee(Point),
}

fn min_size(kind: &str) -> (f32, f32) {
    match kind {
        "chat" => (320.0, 260.0),
        "tarefa" => (240.0, 150.0),
        "nota" => (180.0, 110.0),
        "pagina" => (240.0, 120.0),
        _ => (220.0, 140.0),
    }
}

/// Cor opaca de `fg` com opacidade `a` sobre `bg` (nota sobre sombra não pode ser translúcida).
fn over(bg: Color, fg: Color, a: f32) -> Color {
    Color { r: bg.r + (fg.r - bg.r) * a, g: bg.g + (fg.g - bg.g) * a, b: bg.b + (fg.b - bg.b) * a, a: 1.0 }
}

fn bezier(c: &[Point; 4], t: f32) -> Point {
    let u = 1.0 - t;
    let (a, b, d, e) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    Point::new(a * c[0].x + b * c[1].x + d * c[2].x + e * c[3].x, a * c[0].y + b * c[1].y + d * c[2].y + e * c[3].y)
}

fn dist(a: Point, b: Point) -> f32 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt()
}

const DOUBLE_CLICK: std::time::Duration = std::time::Duration::from_millis(400);

impl Board {
    fn rect(&self, g: &Geo) -> Rectangle {
        Rectangle { x: g.x * self.z + self.vx, y: g.y * self.z + self.vy, width: g.w * self.z, height: g.h * self.z }
    }
    fn geo(&self, id: &str) -> Option<&Geo> {
        self.geos.iter().find(|g| g.id == id)
    }
    fn to_world(&self, p: Point) -> Point {
        Point::new((p.x - self.vx) / self.z, (p.y - self.vy) / self.z)
    }
    /// Curva da seta na tela (início, dois controles, fim). Sai pelo lado que olha para o destino.
    fn curve(&self, a: &Geo, b: &Geo) -> [Point; 4] {
        let (ra, rb) = (self.rect(a), self.rect(b));
        let k = |len: f32| (len.abs() / 2.0).clamp(36.0 * self.z, 240.0);
        let (acy, bcy) = (ra.y + ra.height / 2.0, rb.y + rb.height / 2.0);
        let (acx, bcx) = (ra.x + ra.width / 2.0, rb.x + rb.width / 2.0);
        if rb.x >= ra.x + ra.width {
            let (p0, p1) = (Point::new(ra.x + ra.width, acy), Point::new(rb.x, bcy));
            let d = k(p1.x - p0.x);
            [p0, Point::new(p0.x + d, p0.y), Point::new(p1.x - d, p1.y), p1]
        } else if rb.x + rb.width <= ra.x {
            let (p0, p1) = (Point::new(ra.x, acy), Point::new(rb.x + rb.width, bcy));
            let d = k(p1.x - p0.x);
            [p0, Point::new(p0.x - d, p0.y), Point::new(p1.x + d, p1.y), p1]
        } else if rb.y >= ra.y + ra.height {
            let (p0, p1) = (Point::new(acx, ra.y + ra.height), Point::new(bcx, rb.y));
            let d = k(p1.y - p0.y);
            [p0, Point::new(p0.x, p0.y + d), Point::new(p1.x, p1.y - d), p1]
        } else if rb.y + rb.height <= ra.y {
            let (p0, p1) = (Point::new(acx, ra.y), Point::new(bcx, rb.y + rb.height));
            let d = k(p1.y - p0.y);
            [p0, Point::new(p0.x, p0.y - d), Point::new(p1.x, p1.y + d), p1]
        } else {
            // nós sobrepostos: da direita de um para a esquerda do outro
            let (p0, p1) = (Point::new(ra.x + ra.width, acy), Point::new(rb.x, bcy));
            let d = 60.0 * self.z;
            [p0, Point::new(p0.x + d, p0.y), Point::new(p1.x - d, p1.y), p1]
        }
    }
    /// Nó sob o ponto (de cima para baixo). Molduras só respondem no cabeçalho.
    fn hit(&self, p: Point) -> Option<&Geo> {
        self.geos.iter().rev().find(|g| {
            let r = self.rect(g);
            if g.kind == "moldura" {
                Rectangle { height: HEAD * self.z.max(0.5), ..r }.contains(p)
            } else {
                r.contains(p)
            }
        })
    }
    /// Nó cujo canto de redimensionar está sob o ponto (vale também para molduras).
    fn corner(&self, p: Point) -> Option<&Geo> {
        self.geos.iter().rev().find(|g| {
            let r = self.rect(g);
            (Rectangle { x: r.x + r.width - 18.0, y: r.y + r.height - 18.0, width: 18.0, height: 18.0 }).contains(p)
        })
    }
    fn port(&self, g: &Geo) -> Point {
        let r = self.rect(g);
        Point::new(r.x + r.width, r.y + r.height / 2.0)
    }
    /// Seta sob o ponto (a até 7 px da curva).
    fn hit_edge(&self, p: Point) -> Option<&str> {
        self.edges.iter().find_map(|(id, a, b)| {
            let c = self.curve(self.geo(a)?, self.geo(b)?);
            (0..=24).any(|i| dist(bezier(&c, i as f32 / 24.0), p) <= 7.0).then_some(id.as_str())
        })
    }
    fn close_zone(&self, g: &Geo) -> Rectangle {
        let r = self.rect(g);
        Rectangle { x: r.x + r.width - 32.0, y: r.y + 7.0, width: 26.0, height: 26.0 }
    }
}

impl canvas::Program<Msg> for Board {
    type State = Interaction;

    fn update(&self, st: &mut Interaction, ev: &Event, b: Rectangle, cursor: mouse::Cursor) -> Option<Action<Msg>> {
        let pos = cursor.position_in(b);
        match ev {
            // o tamanho real da área vai para o app: zoom e enquadramento usam o centro certo
            Event::Window(iced::window::Event::RedrawRequested(_)) => {
                if (st.size.width - b.width).abs() > 0.5 || (st.size.height - b.height).abs() > 0.5 {
                    st.size = b.size();
                    return Some(Action::publish(cv(Cv::Stage(b.width, b.height))));
                }
                None
            }
            Event::Keyboard(keyboard::Event::ModifiersChanged(m)) => {
                st.mods = *m;
                None
            }
            // teclas só chegam aqui quando nenhum campo de texto as capturou
            Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                let has_sel = self.sel_edge.is_some() || self.geos.iter().any(|g| g.sel);
                match key.as_ref() {
                    Key::Named(Named::Delete) if has_sel => Some(Action::publish(cv(Cv::DeleteSel)).and_capture()),
                    Key::Named(Named::Escape) => Some(Action::publish(cv(Cv::Escape))),
                    Key::Character("a") if modifiers.command() => Some(Action::publish(cv(Cv::SelectAll)).and_capture()),
                    Key::Character("d") if modifiers.command() && has_sel => Some(Action::publish(cv(Cv::Duplicate)).and_capture()),
                    Key::Character("0") if modifiers.command() => Some(Action::publish(cv(Cv::Fit)).and_capture()),
                    _ => None,
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Middle)) => {
                let p = pos?;
                st.drag = Some(Drag::Pan(p));
                Some(Action::capture())
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let p = pos?;
                st.cursor = Some(p);
                let now = std::time::Instant::now();
                let double = st.last_click.is_some_and(|(t, q)| now.duration_since(t) <= DOUBLE_CLICK && dist(p, q) <= 6.0);
                st.last_click = if double { None } else { Some((now, p)) };
                let shift = st.mods.shift();
                // canto de redimensionar (inclui molduras, que só respondem no cabeçalho)
                if let Some(g) = self.corner(p) {
                    st.drag = Some(Drag::Resize { id: g.id.clone(), from: p, w: g.w, h: g.h, min: min_size(&g.kind) });
                    return Some(Action::publish(cv(Cv::Select(Some(g.id.clone())))).and_capture());
                }
                if let Some(g) = self.hit(p) {
                    let r = self.rect(g);
                    // × do cabeçalho: só existe com o conteúdo à vista (no mapa não há × desenhado)
                    if self.detail && self.close_zone(g).contains(p) {
                        return Some(Action::publish(cv(Cv::AskDelete(vec![g.id.clone()]))).and_capture());
                    }
                    // porta de saída (meio da borda direita) para ligar
                    if g.kind != "moldura" && dist(p, self.port(g)) <= 10.0 {
                        st.drag = Some(Drag::Link { from: g.id.clone() });
                        return Some(Action::publish(cv(Cv::Select(Some(g.id.clone())))).and_capture());
                    }
                    if shift {
                        return Some(Action::publish(cv(Cv::Toggle(g.id.clone()))).and_capture());
                    }
                    if double && !self.detail && g.kind != "moldura" {
                        return Some(Action::publish(cv(Cv::ZoomTo(g.id.clone()))).and_capture());
                    }
                    if p.y - r.y <= HEAD * self.z.max(0.5) || !self.detail {
                        let wp = self.to_world(p);
                        st.drag = Some(Drag::Node { id: g.id.clone(), off: Vector::new(wp.x - g.x, wp.y - g.y) });
                    }
                    return Some(Action::publish(cv(Cv::Select(Some(g.id.clone())))).and_capture());
                }
                if let Some(id) = self.hit_edge(p) {
                    return Some(Action::publish(cv(Cv::SelectEdge(id.to_string()))).and_capture());
                }
                if double {
                    let wp = self.to_world(p);
                    return Some(Action::publish(cv(Cv::AddAt("nota", wp.x - 130.0, wp.y - 20.0))).and_capture());
                }
                if shift {
                    st.drag = Some(Drag::Marquee(p));
                    return Some(Action::capture());
                }
                st.drag = Some(Drag::Pan(p));
                Some(Action::publish(cv(Cv::Select(None))).and_capture())
            }
            Event::Mouse(mouse::Event::CursorLeft) => {
                st.hover.take().map(|_| Action::request_redraw())
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                // durante um arrasto o cursor pode sair da área ou passar sobre um widget
                let inside = cursor.position_in(b);
                let p = inside.or_else(|| cursor.position().map(|c| Point::new(c.x - b.x, c.y - b.y)));
                let Some(drag) = st.drag.as_mut() else {
                    let hover = inside.and_then(|p| self.corner(p).or_else(|| self.hit(p))).map(|g| g.id.clone());
                    if hover != st.hover {
                        st.hover = hover;
                        return Some(Action::request_redraw());
                    }
                    return None;
                };
                let p = p.or(st.cursor)?;
                st.cursor = Some(p);
                match drag {
                    Drag::Pan(last) => {
                        let (dx, dy) = (p.x - last.x, p.y - last.y);
                        *last = p;
                        Some(Action::publish(cv(Cv::Pan(dx, dy))).and_capture())
                    }
                    Drag::Node { id, off } => {
                        let wp = self.to_world(p);
                        Some(Action::publish(cv(Cv::Drag(id.clone(), wp.x - off.x, wp.y - off.y))).and_capture())
                    }
                    Drag::Resize { id, from, w, h, min } => {
                        let nw = (*w + (p.x - from.x) / self.z).clamp(min.0, 2400.0);
                        let nh = (*h + (p.y - from.y) / self.z).clamp(min.1, 2400.0);
                        Some(Action::publish(cv(Cv::Resize(id.clone(), nw, nh))).and_capture())
                    }
                    Drag::Link { .. } | Drag::Marquee(_) => Some(Action::request_redraw().and_capture()),
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left | mouse::Button::Middle)) => {
                let d = st.drag.take()?;
                match d {
                    Drag::Node { id, .. } | Drag::Resize { id, .. } => Some(Action::publish(cv(Cv::Commit(id))).and_capture()),
                    Drag::Link { from } => {
                        let p = st.cursor?;
                        let to = self.hit(p).filter(|g| g.id != from && g.kind != "moldura").map(|g| g.id.clone());
                        Some(match to {
                            Some(to) => Action::publish(cv(Cv::Link(from, to))).and_capture(),
                            None => Action::request_redraw(),
                        })
                    }
                    Drag::Marquee(from) => {
                        let p = st.cursor?;
                        let area = Rectangle { x: from.x.min(p.x), y: from.y.min(p.y), width: (from.x - p.x).abs(), height: (from.y - p.y).abs() };
                        let ids = self.geos.iter().filter(|g| self.rect(g).intersects(&area)).map(|g| g.id.clone()).collect();
                        Some(Action::publish(cv(Cv::Marquee(ids))).and_capture())
                    }
                    Drag::Pan(_) => None,
                }
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let p = pos?;
                if st.mods.command() {
                    let y = match delta {
                        mouse::ScrollDelta::Lines { y, .. } => *y,
                        mouse::ScrollDelta::Pixels { y, .. } => *y / 40.0,
                    };
                    if y == 0.0 {
                        return None;
                    }
                    return Some(Action::publish(cv(Cv::ZoomAt(1.1f32.powf(y.clamp(-4.0, 4.0)), p.x, p.y))).and_capture());
                }
                let (dx, dy) = match delta {
                    mouse::ScrollDelta::Lines { x, y } => (x * 48.0, y * 48.0),
                    mouse::ScrollDelta::Pixels { x, y } => (*x, *y),
                };
                // Shift+roda anda de lado
                let (dx, dy) = if st.mods.shift() && dx == 0.0 { (dy, 0.0) } else { (dx, dy) };
                Some(Action::publish(cv(Cv::Pan(dx, dy))).and_capture())
            }
            _ => None,
        }
    }

    fn draw(&self, st: &Interaction, r: &Renderer, _th: &Theme, b: Rectangle, cur: mouse::Cursor) -> Vec<Geometry> {
        // grade de pontos: guardada e só refeita quando a câmera ou o tamanho mudam
        let key = (self.vx.round() as i32, self.vy.round() as i32, (self.z * 1000.0) as i32, b.width as i32, b.height as i32);
        if st.grid_key.get() != key {
            st.grid.clear();
            st.grid_key.set(key);
        }
        let grid = st.grid.draw(r, b.size(), |f| {
            let mut step = 24.0 * self.z;
            while step < 16.0 {
                step *= 2.0;
            }
            let color = t::alpha(t::STAR, 0.075);
            let (ox, oy) = (self.vx.rem_euclid(step), self.vy.rem_euclid(step));
            let mut y = oy;
            while y < b.height {
                let mut x = ox;
                while x < b.width {
                    f.fill_rectangle(Point::new(x - 0.75, y - 0.75), Size::new(1.5, 1.5), color);
                    x += step;
                }
                y += step;
            }
        });

        let mut f = Frame::new(r, b.size());
        let dash = [5.0f32, 6.0];
        let hover = if st.drag.is_none() { cur.position_in(b).and_then(|p| self.corner(p).or_else(|| self.hit(p))).map(|g| g.id.as_str()) } else { None };
        let linking = match &st.drag {
            Some(Drag::Link { from }) => Some(from.as_str()),
            _ => None,
        };
        let link_target = linking.and_then(|from| st.cursor.and_then(|p| self.hit(p)).filter(|g| g.id != from && g.kind != "moldura").map(|g| g.id.as_str()));
        let grip = |f: &mut Frame, r: Rectangle| {
            for k in [4.0f32, 9.0] {
                f.stroke(&Path::line(Point::new(r.x + r.width - 3.0, r.y + r.height - 3.0 - k), Point::new(r.x + r.width - 3.0 - k, r.y + r.height - 3.0)), Stroke { width: 1.0, style: t::alpha(t::FG3, 0.8).into(), ..Stroke::default() });
            }
        };
        let cross = |f: &mut Frame, c: Point| {
            for (a, bb) in [((-4.0, -4.0), (4.0, 4.0)), ((4.0, -4.0), (-4.0, 4.0))] {
                f.stroke(&Path::line(Point::new(c.x + a.0, c.y + a.1), Point::new(c.x + bb.0, c.y + bb.1)), Stroke { width: 1.4, style: t::FG3.into(), line_cap: canvas::LineCap::Round, ..Stroke::default() });
            }
        };

        // molduras: atrás de tudo
        for g in self.geos.iter().filter(|g| g.kind == "moldura") {
            let r = self.rect(g);
            let active = g.sel || hover == Some(g.id.as_str());
            let body = Path::rounded_rectangle(Point::new(r.x, r.y), Size::new(r.width, r.height), (14.0 * self.z).max(5.0).into());
            f.fill(&body, t::alpha(t::STAR, 0.018));
            f.stroke(&body, Stroke { width: if g.sel { 1.6 } else { 1.0 }, style: if g.sel { t::ACCENT } else { t::LINE3 }.into(), line_dash: LineDash { segments: &dash, offset: 0 }, ..Stroke::default() });
            let ts = if self.detail { 11.0 } else { (15.0 * self.z).max(9.5) };
            let ty = if self.detail { HEAD / 2.0 } else { (HEAD * self.z.max(0.5)) / 2.0 };
            f.fill_text(Text { content: ell(&g.title.to_uppercase(), (r.width - 60.0).max(40.0), ts), position: Point::new(r.x + 14.0f32.min(r.width * 0.1), r.y + ty), color: if active { t::FG2 } else { t::FG3 }, size: ts.into(), font: t::MONO_MED, align_y: Vertical::Center, ..Text::default() });
            if self.detail && active {
                cross(&mut f, Point::new(r.x + r.width - 18.0, r.y + HEAD / 2.0));
            }
            if active {
                grip(&mut f, r);
            }
        }

        // setas, com ponta no destino
        for (id, a, bb) in &self.edges {
            let (Some(ga), Some(gb)) = (self.geo(a), self.geo(bb)) else { continue };
            let c = self.curve(ga, gb);
            let on = self.sel_edge.as_deref() == Some(id.as_str());
            let color = if on { t::ACCENT } else { t::alpha(t::STAR, 0.34) };
            let path = Path::new(|p| {
                p.move_to(c[0]);
                p.bezier_curve_to(c[1], c[2], c[3]);
            });
            f.stroke(&path, Stroke { width: if on { 2.0 } else { 1.4 }, style: color.into(), line_cap: canvas::LineCap::Round, ..Stroke::default() });
            // direção de chegada: do último controle para o fim
            let (dx, dy) = (c[3].x - c[2].x, c[3].y - c[2].y);
            let len = (dx * dx + dy * dy).sqrt().max(0.001);
            let (ux, uy) = (dx / len, dy / len);
            let s = (8.0 * self.z).max(5.0);
            let base = Point::new(c[3].x - ux * s, c[3].y - uy * s);
            let head = Path::new(|p| {
                p.move_to(c[3]);
                p.line_to(Point::new(base.x - uy * s * 0.55, base.y + ux * s * 0.55));
                p.line_to(Point::new(base.x + uy * s * 0.55, base.y - ux * s * 0.55));
                p.close();
            });
            f.fill(&head, if on { t::ACCENT } else { t::alpha(t::STAR, 0.55) });
            f.fill(&Path::circle(c[0], (2.6 * self.z).max(1.6)), color);
        }

        // nós
        for g in self.geos.iter().filter(|g| g.kind != "moldura") {
            let r = self.rect(g);
            if r.x > b.width || r.y > b.height || r.x + r.width < 0.0 || r.y + r.height < 0.0 {
                continue;
            }
            let (pos, size) = (Point::new(r.x, r.y), Size::new(r.width, r.height));
            let radius = (14.0 * self.z).max(5.0);
            let active = g.sel || hover == Some(g.id.as_str());
            let target = link_target == Some(g.id.as_str());
            // sombra curta: dois retângulos escuros deslocados
            for (dy, grow, a) in [(8.0f32, 3.0f32, 0.16f32), (2.0, 0.5, 0.22)] {
                let (dy, grow) = (dy * self.z, grow * self.z);
                f.fill(&Path::rounded_rectangle(Point::new(r.x - grow, r.y - grow + dy), Size::new(r.width + grow * 2.0, r.height + grow * 2.0), (radius + grow).into()), Color { a, ..Color::BLACK });
            }
            let body = Path::rounded_rectangle(pos, size, radius.into());
            f.fill(&body, if g.kind == "nota" { over(t::RAISE, t::SIGNAL, 0.10) } else { t::RAISE });
            if g.sel || target {
                // halo da seleção
                f.stroke(&Path::rounded_rectangle(Point::new(r.x - 2.0, r.y - 2.0), Size::new(r.width + 4.0, r.height + 4.0), (radius + 2.0).into()), Stroke { width: 4.0, style: t::alpha(t::ACCENT, 0.18).into(), ..Stroke::default() });
            }
            let border = if g.sel || target { t::ACCENT } else if g.busy { t::alpha(t::ACCENT, 0.55) } else if g.kind == "nota" { t::alpha(t::SIGNAL, if active { 0.5 } else { 0.34 }) } else if active { t::LINE3 } else { t::LINE2 };
            f.stroke(&body, Stroke { width: if g.sel || target { 1.6 } else { 1.0 }, style: border.into(), ..Stroke::default() });
            if self.detail {
                // cabeçalho: ponto de estado, rótulo, marca do Maestro e ×
                f.stroke(&Path::line(Point::new(r.x + 1.0, r.y + HEAD), Point::new(r.x + r.width - 1.0, r.y + HEAD)), Stroke { width: 1.0, style: t::LINE.into(), ..Stroke::default() });
                f.fill(&Path::circle(Point::new(r.x + 16.0, r.y + HEAD / 2.0), 3.5), g.dot);
                if g.busy {
                    f.stroke(&Path::circle(Point::new(r.x + 16.0, r.y + HEAD / 2.0), 6.0), Stroke { width: 1.0, style: t::alpha(g.dot, 0.45).into(), ..Stroke::default() });
                }
                f.fill_text(Text { content: ell(&g.label, (r.width - if g.maestro { 150.0 } else { 80.0 }).max(40.0), 11.5), position: Point::new(r.x + 28.0, r.y + HEAD / 2.0), color: t::FG3, size: 11.5.into(), font: t::MONO, align_y: Vertical::Center, ..Text::default() });
                if g.maestro {
                    f.fill_text(Text { content: "Maestro".into(), position: Point::new(r.x + r.width - 36.0, r.y + HEAD / 2.0), color: t::ACCENT, size: 10.5.into(), font: t::UI_MED, align_x: iced::advanced::text::Alignment::Right, align_y: Vertical::Center, ..Text::default() });
                }
                let close = self.close_zone(g);
                if cur.position_in(b).is_some_and(|p| close.contains(p)) {
                    f.fill(&Path::rounded_rectangle(Point::new(close.x, close.y), close.size(), 7.0.into()), t::HOVER);
                }
                if active {
                    cross(&mut f, Point::new(close.x + close.width / 2.0, close.y + close.height / 2.0));
                } else {
                    f.fill(&Path::circle(Point::new(close.x + close.width / 2.0, close.y + close.height / 2.0), 1.5), t::FG4);
                }
            } else {
                // mapa: título, rótulo e resumo, cada um numa linha ou nas que cabem
                let pad = (14.0 * self.z).max(8.0);
                let tw = (r.width - pad * 2.0).max(20.0);
                let (ts, ls, ss) = ((20.0 * self.z).max(10.5), (15.0 * self.z).max(9.0), (17.0 * self.z).max(9.5));
                f.fill(&Path::circle(Point::new(r.x + pad + 3.0, r.y + pad + ts * 0.55), 3.2), g.dot);
                f.fill_text(Text { content: ell(&g.title, tw - 14.0, ts), position: Point::new(r.x + pad + 12.0, r.y + pad), color: t::FG, size: ts.into(), font: t::UI_MED, max_width: tw - 12.0, ..Text::default() });
                if g.maestro {
                    // marca do Maestro no mapa: um ponto lavanda no canto
                    f.fill(&Path::circle(Point::new(r.x + r.width - 9.0 * self.z.max(0.5), r.y + 9.0 * self.z.max(0.5)), 2.6 * self.z.max(0.6)), t::ACCENT);
                }
                if r.height > pad * 2.0 + ts * 1.5 + ls {
                    f.fill_text(Text { content: ell(&g.label, tw, ls), position: Point::new(r.x + pad, r.y + pad + ts * 1.5), color: t::FG3, size: ls.into(), font: t::MONO, max_width: tw, ..Text::default() });
                }
                let top = pad + ts * 1.5 + ls * 1.6;
                if r.height > top + ss * 1.35 + pad * 0.5 {
                    let lines = (((r.height - top - pad) / (ss * 1.35)).floor() as usize).max(1);
                    let per_line = ((tw / (ss * 0.52)) as usize).max(8);
                    let sum: String = g.summary.chars().take(per_line * lines).collect();
                    f.fill_text(Text { content: sum, position: Point::new(r.x + pad, r.y + top), color: t::FG2, size: ss.into(), font: t::SERIF, max_width: tw, ..Text::default() });
                }
            }
            // porta de saída e canto de redimensionar: só no nó ativo (ou em todos enquanto se liga)
            if active || linking.is_some() {
                let port = self.port(g);
                let from_here = linking == Some(g.id.as_str());
                let near = cur.position_in(b).is_some_and(|p| dist(p, port) <= 10.0);
                let lit = g.sel || from_here || near;
                f.fill(&Path::circle(port, if near { 6.5 } else { 5.0 }), if lit { t::ACCENT } else { t::RAISE });
                f.stroke(&Path::circle(port, if near { 6.5 } else { 5.0 }), Stroke { width: 1.6, style: if lit { t::ACCENT } else { t::FG3 }.into(), ..Stroke::default() });
            }
            if active {
                grip(&mut f, r);
            }
        }

        // ligação em andamento
        if let (Some(from), Some(c)) = (linking, st.cursor) {
            if let Some(g) = self.geo(from) {
                let s0 = self.port(g);
                let dx = ((c.x - s0.x).abs() / 2.0).max(40.0);
                let path = Path::new(|p| {
                    p.move_to(s0);
                    p.bezier_curve_to(Point::new(s0.x + dx, s0.y), Point::new(c.x - dx, c.y), c);
                });
                f.stroke(&path, Stroke { width: 1.8, style: t::ACCENT.into(), line_cap: canvas::LineCap::Round, ..Stroke::default() });
                f.fill(&Path::circle(c, 3.5), t::ACCENT);
            }
        }
        // seleção por área
        if let (Some(Drag::Marquee(from)), Some(p)) = (&st.drag, st.cursor) {
            let (pos, size) = (Point::new(from.x.min(p.x), from.y.min(p.y)), Size::new((from.x - p.x).abs(), (from.y - p.y).abs()));
            f.fill_rectangle(pos, size, t::alpha(t::ACCENT, 0.08));
            f.stroke(&Path::rectangle(pos, size), Stroke { width: 1.0, style: t::alpha(t::ACCENT, 0.7).into(), ..Stroke::default() });
        }
        vec![grid, f.into_geometry()]
    }

    fn mouse_interaction(&self, st: &Interaction, b: Rectangle, c: mouse::Cursor) -> mouse::Interaction {
        match &st.drag {
            Some(Drag::Pan(_)) | Some(Drag::Node { .. }) => return mouse::Interaction::Grabbing,
            Some(Drag::Link { .. }) | Some(Drag::Marquee(_)) => return mouse::Interaction::Crosshair,
            Some(Drag::Resize { .. }) => return mouse::Interaction::ResizingDiagonallyDown,
            None => {}
        }
        let Some(p) = c.position_in(b) else { return mouse::Interaction::None };
        if self.corner(p).is_some() {
            return mouse::Interaction::ResizingDiagonallyDown;
        }
        match self.hit(p) {
            Some(g) => {
                let r = self.rect(g);
                if self.detail && self.close_zone(g).contains(p) {
                    mouse::Interaction::Pointer
                } else if g.kind != "moldura" && dist(p, self.port(g)) <= 10.0 {
                    mouse::Interaction::Crosshair
                } else if p.y - r.y <= HEAD * self.z.max(0.5) || !self.detail {
                    mouse::Interaction::Grab
                } else {
                    mouse::Interaction::None
                }
            }
            None if self.hit_edge(p).is_some() => mouse::Interaction::Pointer,
            None if st.mods.shift() => mouse::Interaction::Crosshair,
            None => mouse::Interaction::Grab,
        }
    }
}

// ───────────────────────────────────── minimapa ─────────────────────────────────────

const MINI: (f32, f32) = (184.0, 124.0);

/// Visão geral no canto: todos os nós e o retângulo da câmera. Clique ou arraste para levar a câmera.
struct Mini {
    /// (x, y, w, h, cor, moldura) no mundo.
    rects: Vec<(f32, f32, f32, f32, Color, bool)>,
    /// Área visível no mundo.
    view: (f32, f32, f32, f32),
}

impl Mini {
    /// Escala e origem que encaixam nós e câmera no minimapa.
    fn fit(&self, b: Size) -> (f32, f32, f32) {
        let (mut x0, mut y0, mut x1, mut y1) = (self.view.0, self.view.1, self.view.0 + self.view.2, self.view.1 + self.view.3);
        for r in &self.rects {
            x0 = x0.min(r.0);
            y0 = y0.min(r.1);
            x1 = x1.max(r.0 + r.2);
            y1 = y1.max(r.1 + r.3);
        }
        let (w, h) = ((x1 - x0).max(1.0), (y1 - y0).max(1.0));
        let k = ((b.width - 16.0) / w).min((b.height - 16.0) / h);
        (k, (b.width - w * k) / 2.0 - x0 * k, (b.height - h * k) / 2.0 - y0 * k)
    }
}

impl canvas::Program<Msg> for Mini {
    type State = bool;

    fn update(&self, pressed: &mut bool, ev: &Event, b: Rectangle, cursor: mouse::Cursor) -> Option<Action<Msg>> {
        let go = |p: Point| {
            let (k, ox, oy) = self.fit(b.size());
            Action::publish(cv(Cv::Center((p.x - ox) / k, (p.y - oy) / k))).and_capture()
        };
        match ev {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let p = cursor.position_in(b)?;
                *pressed = true;
                Some(go(p))
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) if *pressed => Some(go(cursor.position_in(b)?)),
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                *pressed = false;
                None
            }
            _ => None,
        }
    }

    fn draw(&self, _: &bool, r: &Renderer, _th: &Theme, b: Rectangle, _c: mouse::Cursor) -> Vec<Geometry> {
        let mut f = Frame::new(r, b.size());
        let (k, ox, oy) = self.fit(b.size());
        for (x, y, w, h, color, frame) in &self.rects {
            let (pos, size) = (Point::new(x * k + ox, y * k + oy), Size::new((w * k).max(2.0), (h * k).max(2.0)));
            if *frame {
                f.stroke(&Path::rectangle(pos, size), Stroke { width: 1.0, style: t::LINE3.into(), ..Stroke::default() });
            } else {
                f.fill(&Path::rounded_rectangle(pos, size, 1.5.into()), *color);
            }
        }
        let (pos, size) = (Point::new(self.view.0 * k + ox, self.view.1 * k + oy), Size::new(self.view.2 * k, self.view.3 * k));
        f.fill_rectangle(pos, size, t::alpha(t::ACCENT, 0.10));
        f.stroke(&Path::rounded_rectangle(pos, size, 2.0.into()), Stroke { width: 1.2, style: t::ACCENT.into(), ..Stroke::default() });
        vec![f.into_geometry()]
    }

    fn mouse_interaction(&self, _: &bool, b: Rectangle, c: mouse::Cursor) -> mouse::Interaction {
        if c.is_over(b) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::None
        }
    }
}

// ─────────────────────────────────────── widgets ───────────────────────────────────────

fn msg_view<'a>(m: &'a CMsg) -> El<'a> {
    match m.role.as_str() {
        "user" => container(container(tx(m.text.clone(), 13.5, t::FG)).padding(pad(t::SP_2, t::SP_3, t::SP_2, t::SP_3)).max_width(300.0).style(surface(Some(t::RAISE2), None, 16.0))).width(Length::Fill).align_x(Horizontal::Right).into(),
        "ai" => {
            let blocks = md::cached_parse(&m.text);
            let body = md::render_with(&blocks, md::Style::chat(t::SERIF, t::SERIF_SEMI, 14.5), Some(Msg::OpenUrl), Some(Msg::CopyText));
            let foot = row![tx(m.model.clone(), 11.0, t::FG4), tip(ib_sz("copy", 22.0, 12.0, Some(Msg::CopyText(m.text.clone())), false), crate::i18n::text("Copiar resposta"))].spacing(t::SP_1).align_y(Vertical::Center);
            column![body, foot].spacing(t::SP_05).into()
        }
        "dado" => {
            let head = txf(format!("{} {} · {}", crate::i18n::text("DADO DE"), m.from.to_uppercase(), crate::i18n::text("NÃO É INSTRUÇÃO")), 10.0, t::FG3, t::MONO);
            dashed(container(column![head, tx(m.text.chars().take(1200).collect::<String>(), 13.0, t::FG2)].spacing(t::SP_1)).padding(pad(t::SP_2, t::SP_3, t::SP_2, t::SP_3)).width(Length::Fill), 12.0, t::LINE3)
        }
        // aviso de falha ou de resposta interrompida
        _ => container(row![dot_el(t::DANGER, 6.0), tx(m.text.clone(), 12.5, t::DANGER)].spacing(t::SP_2).align_y(Vertical::Center)).padding(pad(t::SP_15, t::SP_25, t::SP_15, t::SP_25)).width(Length::Fill).style(surface(Some(t::DANGER_SOFT), None, 10.0)).into(),
    }
}

fn title_input<'a>(n: &'a CNode) -> El<'a> {
    let id = n.id.clone();
    text_input(crate::i18n::text("Nome"), &n.title).on_input(move |s| cv(Cv::Title(id.clone(), s))).size(13.5).font(t::UI_MED).padding(pad(t::SP_1, t::SP_15, t::SP_1, t::SP_15)).style(field_style(Color::TRANSPARENT, Color::TRANSPARENT, t::LINE3, 7.0)).into()
}

fn chat_body<'a>(app: &'a App, n: &'a CNode) -> El<'a> {
    let c = &app.canvas;
    let busy = c.busy.contains(&n.id);
    let mut msgs = column![].spacing(t::SP_35).width(Length::Fill);
    if n.msgs.is_empty() {
        msgs = msgs.push(container(tx(crate::i18n::text("Chat vazio. Escreva abaixo ou ligue outro chat a este."), 12.5, t::FG3)).padding(pad(t::SP_8, t::SP_4, 0.0, t::SP_4)).center_x(Length::Fill));
    }
    for m in &n.msgs {
        msgs = msgs.push(msg_view(m));
    }
    if busy {
        let label = if c.stopping.contains(&n.id) { crate::i18n::text("parando…") } else { crate::i18n::text("pensando…") };
        msgs = msgs.push(row![spinner(app.spin, 12.0), tx(label, 12.5, t::FG3)].spacing(t::SP_2).align_y(Vertical::Center));
    }
    let list = scrollable(container(msgs).padding(pad(t::SP_2, t::SP_35, t::SP_2, t::SP_35))).direction(scrollable::Direction::Vertical(thin_bar())).style(thin_scroll).anchor_bottom().height(Length::Fill).width(Length::Fill);
    let draft = c.drafts.get(&n.id).cloned().unwrap_or_default();
    let (id1, id2) = (n.id.clone(), n.id.clone());
    let who = AGENTS.iter().find(|a| a.0 == n.agent).map(|a| a.1).unwrap_or("Orion");
    let input = text_input(&crate::i18n::formatted("Mensagem para {}", format!("Mensagem para {who}")), &draft).on_input(move |s| cv(Cv::Draft(id1.clone(), s))).on_submit(cv(Cv::Send(id2.clone()))).size(13.5).font(t::UI).padding(pad(t::SP_25, t::SP_3, t::SP_25, t::SP_3)).style(field_style(t::RAISE2, t::LINE2, t::LINE3, 12.0));
    // respondendo: o botão de enviar vira Parar
    let send: El = if busy {
        tip(ib("close", if c.stopping.contains(&n.id) { None } else { Some(cv(Cv::Stop(n.id.clone()))) }, false), crate::i18n::text("Parar a resposta"))
    } else {
        ib("up", if draft.trim().is_empty() { None } else { Some(cv(Cv::Send(n.id.clone()))) }, !draft.trim().is_empty())
    };
    let composer = container(row![input, send].spacing(t::SP_1).align_y(Vertical::Center)).padding(pad(t::SP_1, t::SP_3, t::SP_1, t::SP_3));
    let idx = AGENTS.iter().position(|a| a.0 == n.agent).unwrap_or(0);
    let idn = n.id.clone();
    let names: Vec<&str> = AGENTS.iter().map(|a| a.1).collect();
    let mut foot = row![seg(&names, idx, move |i| cv(Cv::Agent(idn.clone(), i)))].spacing(t::SP_2).align_y(Vertical::Center);
    if n.agent == "claude" {
        let mi = CLAUDE_MODELS.iter().position(|m| m.0 == n.model).unwrap_or(0);
        let idm = n.id.clone();
        let mn: Vec<&str> = CLAUDE_MODELS.iter().map(|m| m.1).collect();
        foot = foot.push(seg(&mn, mi, move |i| cv(Cv::Model(idm.clone(), i))));
    }
    let via = match n.agent.as_str() {
        "orion" => crate::i18n::text("APIs gratuitas · só leitura"),
        _ => crate::i18n::text("Assinatura pela CLI oficial · só texto"),
    };
    column![title_input(n), list, composer, container(foot).padding(pad(t::SP_1, t::SP_3, t::SP_05, t::SP_3)), container(txf(via, 10.0, t::FG4, t::MONO)).padding(pad(0.0, t::SP_4, t::SP_25, t::SP_4))].into()
}

fn note_body<'a>(app: &'a App, n: &'a CNode) -> El<'a> {
    let content = app.canvas.notes.get(&n.id);
    let id = n.id.clone();
    let ed: El = match content {
        Some(c) => text_editor(c)
            .placeholder(crate::i18n::text("Escreva aqui"))
            .on_action(move |a| cv(Cv::Note(id.clone(), a)))
            .font(t::SERIF)
            .size(15.0)
            .padding(pad(t::SP_1, t::SP_3, t::SP_3, t::SP_3))
            .height(Length::Fill)
            .style(|_, _| text_editor::Style { background: iced::Background::Color(Color::TRANSPARENT), border: Border::default(), placeholder: t::FG3, value: t::FG, selection: t::ACCENT_SOFT })
            .into(),
        None => container(tx(n.text.clone(), 15.0, t::FG)).padding(pad(t::SP_1, t::SP_4, t::SP_3, t::SP_4)).into(),
    };
    column![title_input(n), ed].into()
}

fn page_body<'a>(n: &'a CNode) -> El<'a> {
    let host = n.url.split("://").nth(1).unwrap_or(&n.url).to_string();
    column![
        title_input(n),
        container(column![txf(ell(&host, n.w - 40.0, 12.5), 12.5, t::FG2, t::MONO), txf(crate::i18n::text("Só o link: o Orion não abre nem busca sozinho."), 10.5, t::FG4, t::MONO)].spacing(t::SP_1)).padding(pad(t::SP_1, t::SP_4, t::SP_2, t::SP_4)),
        container(row![btn(crate::i18n::text("Abrir no navegador"), Kind::Normal, true, Some("out"), Some(Msg::OpenUrl(n.url.clone()))), tip(ib("copy", Some(Msg::CopyText(n.url.clone())), false), crate::i18n::text("Copiar link"))].spacing(t::SP_1).align_y(Vertical::Center)).padding(pad(0.0, t::SP_4, t::SP_3, t::SP_4)),
    ]
    .into()
}

fn task_body<'a>(n: &'a CNode) -> El<'a> {
    let waiting = n.state == "pendente" && n.mission_state == "aguarda_aprovacao";
    let out: El = if n.out.is_empty() {
        let hint = if waiting {
            crate::i18n::text("Aguardando você aprovar o plano.")
        } else if n.state == "rodando" {
            crate::i18n::text("O agente está trabalhando…")
        } else if n.mission_state == "cancelada" || n.state == "pulada" {
            crate::i18n::text("Não rodou.")
        } else {
            crate::i18n::text("Aguardando as dependências.")
        };
        txf(hint, 13.5, t::FG3, t::SERIF).into()
    } else if n.state == "falhou" {
        txf(n.out.clone(), 13.0, t::DANGER, t::UI).into()
    } else {
        // a saída do agente é Markdown, como no chat
        md::render_with(&md::cached_parse(&n.out), md::Style::chat(t::SERIF, t::SERIF_SEMI, 13.5), Some(Msg::OpenUrl), Some(Msg::CopyText))
    };
    let mut meta = row![badge(&n.agent, Tone::Info), tx(format!("{} {}", crate::i18n::text("esforço"), n.effort), 11.5, t::FG3), grow(), txf(crate::i18n::text(state_label(&n.state)), 11.5, state_color(&n.state), t::UI_MED)].spacing(t::SP_2).align_y(Vertical::Center);
    if !n.out.is_empty() {
        meta = meta.push(tip(ib_sz("copy", 22.0, 12.0, Some(Msg::CopyText(n.out.clone())), false), crate::i18n::text("Copiar resposta")));
    }
    column![
        container(tx(n.title.clone(), 15.0, t::FG)).padding(pad(t::SP_2, t::SP_4, t::SP_1, t::SP_4)),
        container(meta).padding(pad(0.0, t::SP_4, t::SP_2, t::SP_4)),
        scrollable(container(out).padding(pad(0.0, t::SP_4, t::SP_3, t::SP_4)).width(Length::Fill)).direction(scrollable::Direction::Vertical(thin_bar())).style(thin_scroll).height(Length::Fill),
    ]
    .into()
}

fn node_body<'a>(app: &'a App, n: &'a CNode) -> Option<El<'a>> {
    Some(match n.kind.as_str() {
        "chat" => chat_body(app, n),
        "nota" => note_body(app, n),
        "pagina" => page_body(n),
        "tarefa" => task_body(n),
        _ => return None,
    })
}

fn label_of(n: &CNode) -> String {
    match n.kind.as_str() {
        "chat" => format!("{} · {}", crate::i18n::text("Chat"), agent_label(n)),
        "tarefa" => format!("{} · {}", n.agent, n.effort),
        "nota" => crate::i18n::text("Nota").to_string(),
        "pagina" => crate::i18n::text("Página").to_string(),
        _ => crate::i18n::text("Moldura").to_string(),
    }
}

/// Cor do ponto do nó: estado da tarefa, chat respondendo ou a cor do tipo.
fn dot_of(n: &CNode, busy: bool) -> Color {
    match n.kind.as_str() {
        "tarefa" => state_color(&n.state),
        "chat" if busy => t::ACCENT,
        "nota" => t::SIGNAL,
        "pagina" => t::N_PESQUISA,
        "chat" if n.msgs.iter().any(|m| m.role == "ai") => t::FG3,
        _ => t::FG4,
    }
}

fn geos(app: &App) -> Vec<Geo> {
    let c = &app.canvas;
    let mut v: Vec<Geo> = c
        .nodes
        .iter()
        .map(|n| {
            let busy = c.busy.contains(&n.id) || (n.kind == "tarefa" && n.state == "rodando");
            Geo { id: n.id.clone(), kind: n.kind.clone(), x: n.x, y: n.y, w: n.w, h: n.h, label: label_of(n), title: n.title.clone(), summary: summary(n), dot: dot_of(n, busy), sel: c.selected(&n.id), maestro: n.by_maestro, busy }
        })
        .collect();
    // molduras atrás, mantendo a ordem do resto
    v.sort_by_key(|g| g.kind != "moldura");
    v
}

fn empty_state(app: &App) -> El<'_> {
    let sphere = container(iced::widget::canvas(crate::presence::Sphere { engine: &app.presence }).width(Length::Fixed(240.0)).height(Length::Fixed(200.0))).width(Length::Fill).center_x(Length::Fill);
    container(
        column![
            sphere,
            txf(crate::i18n::text("SEU CANVAS COM ORION"), t::TEXT_EYEBROW, t::FG3, t::UI_MED),
            txf(crate::i18n::text("Peça uma missão ao Maestro ou crie um chat."), 26.0, t::FG, t::SERIF),
            tx(crate::i18n::text("O Maestro divide o pedido, escolhe quem faz cada parte e monta os nós aqui."), t::TEXT_BASE, t::FG3),
            tx(crate::i18n::text("Duplo clique no vazio cria uma nota."), t::TEXT_LABEL, t::FG4),
        ]
        .spacing(t::SP_3)
        .align_x(Horizontal::Center),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .into()
}

/// Camada flutuante presa a um canto da área, sem tirar o mouse do canvas no resto dela.
fn corner<'a>(el: impl Into<El<'a>>, x: Horizontal, y: Vertical) -> El<'a> {
    container(el).width(Length::Fill).height(Length::Fill).align_x(x).align_y(y).padding(t::SP_3).into()
}

fn floating<'a>(el: impl Into<El<'a>>) -> container::Container<'a, Msg> {
    container(el).style(|_| container::Style { background: Some(t::RAISE.into()), border: Border { color: t::LINE2, width: 1.0, radius: 12.0.into() }, shadow: shadow(), ..Default::default() })
}

/// Controles de zoom no canto inferior esquerdo.
fn zoom_hud(app: &App) -> El<'_> {
    let c = &app.canvas;
    let pct = iced::widget::button(container(txf(format!("{}%", (c.z() * 100.0).round()), 12.0, t::FG2, t::MONO)).center_x(Length::Fill).center_y(Length::Fill)).width(Length::Fixed(48.0)).height(Length::Fixed(30.0)).padding(0).on_press(cv(Cv::Zoom100)).style(|_, st| iced::widget::button::Style { background: matches!(st, iced::widget::button::Status::Hovered).then(|| t::HOVER.into()), border: Border { radius: 8.0.into(), ..Default::default() }, ..Default::default() });
    let help = crate::i18n::text("Ctrl+roda: zoom · arrastar o vazio: mover · Shift: selecionar vários · Delete: apagar · duplo clique: nota");
    let r = row![
        tip(ib("minus", (c.z() > crate::canvas_ui::ZOOM_MIN + 0.01).then(|| cv(Cv::Zoom(1))), false), crate::i18n::text("Afastar")),
        tip(pct, crate::i18n::text("Voltar a 100%")),
        tip(ib("plus", (!c.detail()).then(|| cv(Cv::Zoom(-1))), false), crate::i18n::text("Aproximar")),
        vline(16.0),
        tip(ib("fit", Some(cv(Cv::Fit)), false), crate::i18n::text("Ajustar ao conteúdo (Ctrl+0)")),
        tip(container(crate::icons::icon("keys", 15.0, t::FG3)).width(Length::Fixed(30.0)).height(Length::Fixed(30.0)).center_x(Length::Fixed(30.0)).center_y(Length::Fixed(30.0)), help),
    ]
    .spacing(t::SP_05)
    .align_y(Vertical::Center);
    floating(r).padding(t::SP_05).into()
}

fn minimap(app: &App) -> El<'_> {
    let c = &app.canvas;
    let (sw, sh) = app.canvas_stage();
    let z = c.z();
    let rects = c.nodes.iter().map(|n| (n.x, n.y, n.w, n.h, if c.selected(&n.id) { t::ACCENT } else if n.kind == "nota" { t::alpha(t::SIGNAL, 0.55) } else { t::alpha(t::FG3, 0.55) }, n.kind == "moldura")).collect();
    let mini = Mini { rects, view: (-c.pan.0 / z, -c.pan.1 / z, sw / z, sh / z) };
    floating(iced::widget::canvas(mini).width(Length::Fixed(MINI.0)).height(Length::Fixed(MINI.1))).clip(true).into()
}

/// Barra contextual embaixo, no centro: confirmação de apagar ou ações da seleção.
fn context_bar(app: &App) -> Option<El<'_>> {
    let c = &app.canvas;
    if let Some(ids) = &c.confirm {
        let what = if ids.len() == 1 {
            let title = c.node(&ids[0]).map(|n| n.title.clone()).unwrap_or_default();
            crate::i18n::formatted("Apagar “{}” e o que há nele?", format!("Apagar “{}” e o que há nele?", ell(&title, 220.0, 13.0)))
        } else {
            crate::i18n::formatted("Apagar {} nós e o que há neles?", format!("Apagar {} nós e o que há neles?", ids.len()))
        };
        let r = row![tx(what, 13.0, t::FG), btn(crate::i18n::text("Apagar"), Kind::Danger, true, Some("trash"), Some(cv(Cv::ConfirmDelete))), btn(crate::i18n::text("Cancelar"), Kind::Ghost, true, None, Some(cv(Cv::CancelDelete)))].spacing(t::SP_2).align_y(Vertical::Center);
        return Some(floating(r).padding(pad(t::SP_15, t::SP_2, t::SP_15, t::SP_35)).into());
    }
    if c.sel.len() > 1 {
        let can_dup = c.sel.iter().any(|i| c.node(i).is_some_and(|n| n.kind != "tarefa"));
        let r = row![
            txf(crate::i18n::formatted("{} selecionados", format!("{} selecionados", c.sel.len())), 12.5, t::FG2, t::UI_MED),
            vline(16.0),
            btn(crate::i18n::text("Duplicar"), Kind::Ghost, true, Some("copy"), can_dup.then(|| cv(Cv::Duplicate))),
            btn(crate::i18n::text("Apagar"), Kind::Ghost, true, Some("trash"), Some(cv(Cv::DeleteSel))),
        ]
        .spacing(t::SP_2)
        .align_y(Vertical::Center);
        return Some(floating(r).padding(pad(t::SP_1, t::SP_15, t::SP_1, t::SP_35)).into());
    }
    None
}

fn board(app: &App) -> El<'_> {
    let c = &app.canvas;
    let z = c.z();
    let b = Board { geos: geos(app), edges: c.edges.iter().map(|e| (e.id.clone(), e.a.clone(), e.b.clone())).collect(), sel_edge: c.sel_edge.clone(), vx: c.pan.0, vy: c.pan.1, z, detail: c.detail() };
    let mut layers: Vec<El> = vec![iced::widget::canvas(b.clone()).width(Length::Fill).height(Length::Fill).into()];
    if c.nodes.is_empty() && c.loaded {
        layers.push(empty_state(app));
    }
    if c.detail() {
        let (sw, sh) = app.canvas_stage();
        for n in &c.nodes {
            // fora da área visível não monta o conteúdo: o custo acompanha o que está na tela. Fica um
            // espaço vazio no lugar para a posição das camadas não mudar (o foco de um campo é guardado por posição)
            let (x, y) = (n.x + c.pan.0, n.y + c.pan.1);
            if x > sw || y > sh || x + n.w < 0.0 || y + n.h < 0.0 {
                if n.kind != "moldura" {
                    layers.push(sp_w(0.0));
                }
                continue;
            }
            if let Some(body) = node_body(app, n) {
                let h = (n.h - HEAD - 2.0).max(40.0);
                layers.push(pin(container(body).width(Length::Fixed(n.w - 2.0)).height(Length::Fixed(h)).clip(true)).x(x + 1.0).y(y + HEAD + 1.0).into());
            }
        }
    }
    // ações da seta selecionada: enviar a última resposta (só entre chats) e remover
    if let Some(e) = c.sel_edge.as_ref().and_then(|id| c.edges.iter().find(|e| &e.id == id)) {
        if let (Some(ga), Some(gb)) = (b.geo(&e.a), b.geo(&e.b)) {
            let mid = bezier(&b.curve(ga, gb), 0.5);
            let both_chat = c.node(&e.a).is_some_and(|n| n.kind == "chat") && c.node(&e.b).is_some_and(|n| n.kind == "chat");
            let mut r = row![].spacing(t::SP_05).align_y(Vertical::Center);
            if both_chat {
                r = r.push(btn(crate::i18n::text("Enviar última resposta"), Kind::Ghost, true, Some("send2"), Some(cv(Cv::SendEdge(e.id.clone())))));
            }
            r = r.push(tip(ib_sz("trash", 28.0, 14.0, Some(cv(Cv::DelEdge(e.id.clone()))), false), crate::i18n::text("Remover a seta")));
            let width = if both_chat { 232.0 } else { 34.0 };
            layers.push(pin(floating(r).padding(t::SP_05)).x((mid.x - width / 2.0).max(4.0)).y((mid.y - 17.0).max(4.0)).into());
        }
    }
    layers.push(corner(zoom_hud(app), Horizontal::Left, Vertical::Bottom));
    if !c.nodes.is_empty() {
        layers.push(corner(minimap(app), Horizontal::Right, Vertical::Bottom));
    }
    if let Some(bar) = context_bar(app) {
        layers.push(corner(bar, Horizontal::Center, Vertical::Bottom));
    }
    container(stack(layers).width(Length::Fill).height(Length::Fill)).width(Length::Fill).height(Length::Fill).clip(true).into()
}

fn space_chip<'a>(name: &str, cur: bool, id: String) -> El<'a> {
    btn(name, if cur { Kind::Normal } else { Kind::Ghost }, true, None, Some(cv(Cv::SelectSpace(id))))
}

fn toolbar(app: &App) -> El<'_> {
    let c = &app.canvas;
    let mut spaces = row![].spacing(t::SP_1).align_y(Vertical::Center);
    for (id, name, _) in &c.spaces {
        let cur = *id == c.cur;
        match (&c.rename, cur) {
            (Some(text), true) => {
                let input = text_input(crate::i18n::text("Nome do espaço"), text).on_input(|s| cv(Cv::RenameIn(s))).on_submit(cv(Cv::RenameGo)).size(12.5).font(t::UI_MED).width(Length::Fixed(160.0)).padding(pad(t::SP_15, t::SP_25, t::SP_15, t::SP_25)).style(field_style(t::RAISE, t::LINE2, t::ACCENT, 8.0));
                spaces = spaces.push(input).push(tip(ib("check", Some(cv(Cv::RenameGo)), false), crate::i18n::text("Salvar o nome")));
            }
            _ => spaces = spaces.push(space_chip(name, cur, id.clone())),
        }
    }
    if c.rename.is_none() && !c.cur.is_empty() {
        spaces = spaces.push(tip(ib_sz("edit", 26.0, 13.0, Some(cv(Cv::RenameOpen)), false), crate::i18n::text("Renomear o espaço")));
        if c.spaces.len() > 1 {
            spaces = spaces.push(tip(ib_sz("trash", 26.0, 13.0, Some(cv(Cv::SpaceDelAsk)), c.confirm_space), crate::i18n::text("Apagar o espaço")));
        }
    }
    spaces = spaces.push(tip(ib("plus", Some(cv(Cv::NewSpace)), false), crate::i18n::text("Novo espaço")));
    let tools = row![
        btn(crate::i18n::text("Chat"), Kind::Normal, true, Some("chat"), Some(cv(Cv::Add("chat")))),
        btn(crate::i18n::text("Nota"), Kind::Normal, true, Some("file"), Some(cv(Cv::Add("nota")))),
        btn(crate::i18n::text("Página"), Kind::Normal, true, Some("globe"), Some(cv(Cv::Add("pagina")))),
        btn(crate::i18n::text("Moldura"), Kind::Normal, true, Some("fit"), Some(cv(Cv::Add("moldura")))),
    ]
    .spacing(t::SP_1);
    let maestro_name = match c.maestro.as_str() {
        "codex" => "Codex",
        "orion" => "Orion",
        _ => "Claude",
    };
    let mut r = row![scrollable(spaces).direction(scrollable::Direction::Horizontal(scrollable::Scrollbar::new().width(0).scroller_width(0))).width(Length::Shrink), vline(18.0), tools].spacing(t::SP_3).align_y(Vertical::Center);
    if c.confirm_space {
        let name = c.spaces.iter().find(|w| w.0 == c.cur).map(|w| w.1.clone()).unwrap_or_default();
        r = r.push(tx(crate::i18n::formatted("Apagar “{}” e tudo nele?", format!("Apagar “{}” e tudo nele?", ell(&name, 160.0, 12.5))), 12.5, t::FG)).push(btn(crate::i18n::text("Apagar"), Kind::Danger, true, None, Some(cv(Cv::SpaceDelGo)))).push(btn(crate::i18n::text("Cancelar"), Kind::Ghost, true, None, Some(cv(Cv::SpaceDelAsk))));
    } else if let Some(u) = &c.url_in {
        let input = text_input("https://…", u).on_input(|s| cv(Cv::UrlIn(s))).on_submit(cv(Cv::UrlGo)).size(13).font(t::UI).width(Length::Fixed(240.0)).padding(pad(t::SP_15, t::SP_25, t::SP_15, t::SP_25)).style(field_style(t::RAISE, t::LINE2, t::ACCENT, 8.0));
        r = r.push(input).push(btn(crate::i18n::text("Adicionar"), Kind::Primary, true, None, (!u.trim().is_empty()).then(|| cv(Cv::UrlGo)))).push(ib("close", Some(cv(Cv::Escape)), false));
    }
    r = r.push(grow());
    r = r.push(chip(format!("{} {}", crate::i18n::text("Maestro:"), maestro_name), None, Some("bolt"), true, Msg::OpenSettings(Some(Sec::Terminais))));
    container(r).padding(pad(t::SP_2, t::SP_4, t::SP_2, t::SP_4)).width(Length::Fill).into()
}

fn mission_state(s: &str) -> (&'static str, Color) {
    match s {
        "planejando" => ("planejando", t::ACCENT),
        "aguarda_aprovacao" => ("aguardando você", t::SIGNAL),
        "rodando" => ("rodando", t::ACCENT),
        "integrando" => ("juntando o resultado", t::ACCENT),
        "interrompida" => ("interrompida", t::SIGNAL),
        _ => ("", t::FG3),
    }
}

fn mission_bar(app: &App) -> El<'_> {
    let c = &app.canvas;
    let input = text_input(crate::i18n::text("Peça uma missão ao Maestro: ele planeja, distribui e monta os nós"), &c.mission_in)
        .on_input(|s| cv(Cv::MissionIn(s)))
        .on_submit(cv(Cv::MissionGo))
        .size(13.5)
        .font(t::UI)
        .padding(pad(t::SP_25, t::SP_35, t::SP_25, t::SP_35))
        .style(field_style(t::RAISE, t::LINE2, t::LINE3, 12.0));
    let go = btn(crate::i18n::text("Planejar"), Kind::Primary, true, Some("bolt"), if c.mission_in.trim().is_empty() { None } else { Some(cv(Cv::MissionGo)) });
    let mut col = column![container(row![input, go].spacing(t::SP_2).align_y(Vertical::Center)).padding(pad(0.0, t::SP_4, t::SP_2, t::SP_4))].width(Length::Fill);
    // a missão que falhou mais recente fica visível com o motivo, até o Antônio apagar
    if let Some(m) = c.missions.iter().find(|m| m.state == "falhou") {
        let reason = if m.error.is_empty() { crate::i18n::text("falhou").to_string() } else { m.error.clone() };
        let r = row![
            dot_el(t::DANGER, 7.0),
            tx(ell(&m.request, 360.0, 13.0), 13.0, t::FG),
            txf(ell(&reason, 520.0, 12.0), 12.0, t::DANGER, t::UI_MED),
            grow(),
            btn(crate::i18n::text("Apagar"), Kind::Ghost, true, None, Some(Msg::MaestroDelete(m.id.clone()))),
        ]
        .spacing(t::SP_2)
        .align_y(Vertical::Center);
        col = col.push(container(r).padding(pad(0.0, t::SP_4, t::SP_15, t::SP_4)));
    }
    for m in c.active_missions().take(3) {
        let (label, color) = mission_state(&m.state);
        let mut r = row![dot_el(color, 7.0), tx(ell(&m.request, 520.0, 13.0), 13.0, t::FG)].spacing(t::SP_2).align_y(Vertical::Center);
        r = r.push(txf(crate::i18n::text(label), 12.0, color, t::UI_MED)).push(grow());
        match m.state.as_str() {
            "aguarda_aprovacao" => r = r.push(btn(crate::i18n::text("Aprovar plano"), Kind::Primary, true, None, Some(Msg::MaestroApprove(m.id.clone())))).push(btn(crate::i18n::text("Cancelar"), Kind::Ghost, true, None, Some(Msg::MaestroCancel(m.id.clone())))),
            // parou quando o Orion fechou: só recomeça por clique (REGRAS.md: aprovação só por canal autenticado)
            "interrompida" => r = r.push(btn(crate::i18n::text("Retomar"), Kind::Normal, true, None, Some(Msg::MaestroApprove(m.id.clone())))).push(btn(crate::i18n::text("Apagar"), Kind::Ghost, true, None, Some(Msg::MaestroDelete(m.id.clone())))),
            _ => r = r.push(btn(crate::i18n::text("Cancelar"), Kind::Ghost, true, None, Some(Msg::MaestroCancel(m.id.clone())))),
        }
        col = col.push(container(r).padding(pad(0.0, t::SP_4, t::SP_15, t::SP_4)));
    }
    col.into()
}

pub fn view(app: &App) -> El<'_> {
    let top = column![toolbar(app), mission_bar(app)].width(Length::Fill);
    column![top, hline(), board(app)].width(Length::Fill).height(Length::Fill).into()
}
