//! Tela "Maestro" (§15): canvas infinito de chats, tarefas, notas, páginas e molduras ligados por setas.
//!
//! O canvas desenha fundo, setas, molduras e cabeçalhos e trata arrastar, redimensionar e ligar; o
//! conteúdo dos nós (chat, nota, página) são widgets fixados por cima (`pin`) quando o zoom é 100%.
//! Abaixo disso a tela vira mapa: cada nó é um cartão com título e resumo.
//!
//! (REGRAS.md: a saída de um chat só entra em outro por clique na seta, como dado; o chat do nó só
//! lê e propõe; página é só link, nada é aberto sozinho; aprovação só por botão.)

use crate::app::{App, Msg, Sec};
use crate::canvas_ui::{CMsg, CNode, Cv, AGENTS, CLAUDE_MODELS, HEAD, ZOOMS};
use crate::md;
use crate::sky::spinner;
use crate::theme as t;
use crate::ui::*;
use crate::views::login::field_style;
use iced::alignment::{Horizontal, Vertical};
use iced::mouse;
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
}

#[derive(Clone)]
struct Board {
    geos: Vec<Geo>,
    edges: Vec<(String, String)>,
    vx: f32,
    vy: f32,
    z: f32,
    detail: bool,
}

#[derive(Default)]
pub struct Interaction {
    drag: Option<Drag>,
    ctrl: bool,
    cursor: Option<Point>,
}

enum Drag {
    Pan(Point),
    Node { id: String, off: Vector },
    Resize { id: String, from: Point, w: f32, h: f32, min: (f32, f32) },
    Link { from: String },
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

impl Board {
    fn rect(&self, g: &Geo) -> Rectangle {
        Rectangle { x: g.x * self.z + self.vx, y: g.y * self.z + self.vy, width: g.w * self.z, height: g.h * self.z }
    }
    fn geo(&self, id: &str) -> Option<&Geo> {
        self.geos.iter().find(|g| g.id == id)
    }
    /// Pontos da seta em coordenadas do mundo e se ela sai por baixo (destino abaixo da origem).
    fn edge_pts(&self, a: &Geo, b: &Geo) -> (Point, Point, bool) {
        if b.y > a.y + a.h * 0.6 {
            (Point::new(a.x + a.w / 2.0, a.y + a.h), Point::new(b.x + b.w / 2.0, b.y), true)
        } else {
            (Point::new(a.x + a.w, a.y + a.h / 2.0), Point::new(b.x, b.y + b.h / 2.0), false)
        }
    }
    fn to_screen(&self, p: Point) -> Point {
        Point::new(p.x * self.z + self.vx, p.y * self.z + self.vy)
    }
    /// Nó sob o ponto (de cima para baixo). Molduras só respondem no cabeçalho.
    fn hit(&self, p: Point) -> Option<&Geo> {
        self.geos.iter().rev().find(|g| {
            let r = self.rect(g);
            if g.kind == "moldura" {
                Rectangle { height: HEAD * self.z, ..r }.contains(p)
            } else {
                r.contains(p)
            }
        })
    }
}

impl canvas::Program<Msg> for Board {
    type State = Interaction;

    fn update(&self, st: &mut Interaction, ev: &Event, b: Rectangle, cursor: mouse::Cursor) -> Option<Action<Msg>> {
        let pos = cursor.position_in(b);
        match ev {
            Event::Keyboard(iced::keyboard::Event::ModifiersChanged(m)) => {
                st.ctrl = m.control();
                None
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let p = pos?;
                st.cursor = Some(p);
                if let Some(g) = self.hit(p) {
                    let r = self.rect(g);
                    let head = Rectangle { height: HEAD * self.z, ..r };
                    // × do cabeçalho
                    let x_btn = Rectangle { x: r.x + r.width - 30.0 * self.z.max(0.6), y: r.y + 6.0 * self.z, width: 24.0 * self.z.max(0.6), height: 24.0 * self.z.max(0.6) };
                    if x_btn.contains(p) && head.contains(p) {
                        return Some(Action::publish(cv(Cv::DelNode(g.id.clone()))).and_capture());
                    }
                    // porta de saída (meio da borda direita) para ligar
                    let port = Point::new(r.x + r.width, r.y + r.height / 2.0);
                    if g.kind != "moldura" && ((p.x - port.x).powi(2) + (p.y - port.y).powi(2)).sqrt() <= 10.0 {
                        st.drag = Some(Drag::Link { from: g.id.clone() });
                        return Some(Action::publish(cv(Cv::Select(Some(g.id.clone())))).and_capture());
                    }
                    // canto de redimensionar
                    if (Rectangle { x: r.x + r.width - 18.0, y: r.y + r.height - 18.0, width: 18.0, height: 18.0 }).contains(p) {
                        st.drag = Some(Drag::Resize { id: g.id.clone(), from: p, w: g.w, h: g.h, min: min_size(&g.kind) });
                        return Some(Action::publish(cv(Cv::Select(Some(g.id.clone())))).and_capture());
                    }
                    if head.contains(p) || !self.detail {
                        let wp = Point::new((p.x - self.vx) / self.z, (p.y - self.vy) / self.z);
                        st.drag = Some(Drag::Node { id: g.id.clone(), off: Vector::new(wp.x - g.x, wp.y - g.y) });
                    }
                    return Some(Action::publish(cv(Cv::Select(Some(g.id.clone())))).and_capture());
                }
                st.drag = Some(Drag::Pan(p));
                Some(Action::publish(cv(Cv::Select(None))).and_capture())
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let p = cursor.position_in(b).or_else(|| cursor.position().map(|c| Point::new(c.x - b.x, c.y - b.y)))?;
                st.cursor = Some(p);
                match st.drag.as_mut()? {
                    Drag::Pan(last) => {
                        let (dx, dy) = (p.x - last.x, p.y - last.y);
                        *last = p;
                        Some(Action::publish(cv(Cv::Pan(dx, dy))).and_capture())
                    }
                    Drag::Node { id, off } => {
                        let wp = Point::new((p.x - self.vx) / self.z, (p.y - self.vy) / self.z);
                        Some(Action::publish(cv(Cv::Drag(id.clone(), wp.x - off.x, wp.y - off.y))).and_capture())
                    }
                    Drag::Resize { id, from, w, h, min } => {
                        let nw = (*w + (p.x - from.x) / self.z).max(min.0);
                        let nh = (*h + (p.y - from.y) / self.z).max(min.1);
                        Some(Action::publish(cv(Cv::Resize(id.clone(), nw, nh))).and_capture())
                    }
                    Drag::Link { .. } => Some(Action::request_redraw()),
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
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
                    Drag::Pan(_) => None,
                }
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                pos?;
                if st.ctrl {
                    let y = match delta {
                        mouse::ScrollDelta::Lines { y, .. } | mouse::ScrollDelta::Pixels { y, .. } => *y,
                    };
                    return Some(Action::publish(cv(Cv::Zoom(if y > 0.0 { -1 } else { 1 }))).and_capture());
                }
                let (dx, dy) = match delta {
                    mouse::ScrollDelta::Lines { x, y } => (x * 40.0, y * 40.0),
                    mouse::ScrollDelta::Pixels { x, y } => (*x, *y),
                };
                Some(Action::publish(cv(Cv::Pan(dx, dy))).and_capture())
            }
            _ => None,
        }
    }

    fn draw(&self, st: &Interaction, r: &Renderer, _th: &Theme, b: Rectangle, _c: mouse::Cursor) -> Vec<Geometry> {
        let mut f = Frame::new(r, b.size());
        let dash = [5.0f32, 6.0];
        // setas
        for (a, bb) in &self.edges {
            let (Some(ga), Some(gb)) = (self.geo(a), self.geo(bb)) else { continue };
            let (p0, p1, vert) = self.edge_pts(ga, gb);
            let (s0, s1) = (self.to_screen(p0), self.to_screen(p1));
            let path = if vert {
                let dy = ((s1.y - s0.y).abs() / 2.0).max(50.0 * self.z);
                Path::new(|p| {
                    p.move_to(s0);
                    p.bezier_curve_to(Point::new(s0.x, s0.y + dy), Point::new(s1.x, s1.y - dy), s1);
                })
            } else {
                let dx = ((s1.x - s0.x).abs() / 2.0).max(70.0 * self.z);
                Path::new(|p| {
                    p.move_to(s0);
                    p.bezier_curve_to(Point::new(s0.x + dx, s0.y), Point::new(s1.x - dx, s1.y), s1);
                })
            };
            f.stroke(&path, Stroke { width: 1.4, style: t::alpha(t::STAR, 0.30).into(), line_dash: LineDash { segments: &dash, offset: 0 }, ..Stroke::default() });
        }
        // ligação em andamento
        if let (Some(Drag::Link { from }), Some(cur)) = (&st.drag, st.cursor) {
            if let Some(g) = self.geo(from) {
                let r = self.rect(g);
                let s0 = Point::new(r.x + r.width, r.y + r.height / 2.0);
                let dx = ((cur.x - s0.x).abs() / 2.0).max(40.0);
                let path = Path::new(|p| {
                    p.move_to(s0);
                    p.bezier_curve_to(Point::new(s0.x + dx, s0.y), Point::new(cur.x - dx, cur.y), cur);
                });
                f.stroke(&path, Stroke { width: 1.6, style: t::ACCENT.into(), ..Stroke::default() });
            }
        }
        // nós
        for g in &self.geos {
            let r = self.rect(g);
            let (pos, size) = (Point::new(r.x, r.y), Size::new(r.width, r.height));
            let radius = (14.0 * self.z).max(5.0);
            let frame_kind = g.kind == "moldura";
            let body = Path::rounded_rectangle(pos, size, radius.into());
            if !frame_kind {
                let bg = if g.kind == "nota" { t::alpha(t::SIGNAL, 0.11) } else { t::RAISE };
                f.fill(&body, bg);
            }
            let border = if g.sel { t::ACCENT } else if g.kind == "nota" { t::alpha(t::SIGNAL, 0.34) } else if frame_kind { t::LINE3 } else { t::LINE2 };
            let mut stroke = Stroke { width: if g.sel { 1.8 } else { 1.0 }, style: border.into(), ..Stroke::default() };
            if frame_kind {
                stroke.line_dash = LineDash { segments: &dash, offset: 0 };
            }
            f.stroke(&body, stroke);
            if g.maestro && !frame_kind && !self.detail {
                // marca do Maestro no mapa: um ponto lavanda no canto
                f.fill(&Path::circle(Point::new(r.x + r.width - 9.0 * self.z.max(0.5), r.y + 9.0 * self.z.max(0.5)), 2.6 * self.z.max(0.6)), t::ACCENT);
            }
            if self.detail {
                // cabeçalho: ponto de estado, rótulo e ×
                if !frame_kind {
                    f.stroke(&Path::line(Point::new(r.x, r.y + HEAD), Point::new(r.x + r.width, r.y + HEAD)), Stroke { width: 1.0, style: t::LINE.into(), ..Stroke::default() });
                }
                if !frame_kind {
                    f.fill(&Path::circle(Point::new(r.x + 16.0, r.y + HEAD / 2.0), 3.5), g.dot);
                    f.fill_text(Text { content: g.label.clone(), position: Point::new(r.x + 28.0, r.y + HEAD / 2.0), color: t::FG3, size: 11.5.into(), font: t::MONO, align_y: Vertical::Center, max_width: (r.width - 80.0).max(40.0), ..Text::default() });
                }
                let c = Point::new(r.x + r.width - 18.0, r.y + HEAD / 2.0);
                for (a, bb) in [((-4.0, -4.0), (4.0, 4.0)), ((4.0, -4.0), (-4.0, 4.0))] {
                    f.stroke(&Path::line(Point::new(c.x + a.0, c.y + a.1), Point::new(c.x + bb.0, c.y + bb.1)), Stroke { width: 1.4, style: t::FG3.into(), line_cap: canvas::LineCap::Round, ..Stroke::default() });
                }
                if g.maestro && !frame_kind {
                    f.fill_text(Text { content: "Maestro".into(), position: Point::new(r.x + r.width - 34.0, r.y + HEAD / 2.0), color: t::ACCENT, size: 10.5.into(), font: t::UI_MED, align_x: iced::advanced::text::Alignment::Right, align_y: Vertical::Center, ..Text::default() });
                }
                if frame_kind {
                    f.fill_text(Text { content: g.title.to_uppercase(), position: Point::new(r.x + 14.0, r.y + HEAD / 2.0), color: t::FG3, size: 11.0.into(), font: t::MONO_MED, align_y: Vertical::Center, max_width: (r.width - 60.0).max(40.0), ..Text::default() });
                }
            } else if !frame_kind {
                // mapa: título, rótulo e resumo, cada um numa linha ou nas que cabem
                let pad = (14.0 * self.z).max(8.0);
                let tw = (r.width - pad * 2.0).max(20.0);
                let (ts, ls, ss) = ((20.0 * self.z).max(10.5), (15.0 * self.z).max(9.0), (17.0 * self.z).max(9.5));
                f.fill(&Path::circle(Point::new(r.x + pad + 3.0, r.y + pad + ts * 0.55), 3.2), g.dot);
                f.fill_text(Text { content: ell(&g.title, tw - 14.0, ts), position: Point::new(r.x + pad + 12.0, r.y + pad), color: t::FG, size: ts.into(), font: t::UI_MED, max_width: tw - 12.0, ..Text::default() });
                f.fill_text(Text { content: ell(&g.label, tw, ls), position: Point::new(r.x + pad, r.y + pad + ts * 1.5), color: t::FG3, size: ls.into(), font: t::MONO, max_width: tw, ..Text::default() });
                let top = pad + ts * 1.5 + ls * 1.6;
                let lines = (((r.height - top - pad) / (ss * 1.35)).floor() as usize).max(1);
                let per_line = ((tw / (ss * 0.52)) as usize).max(8);
                let sum: String = g.summary.chars().take(per_line * lines).collect();
                if lines >= 1 && r.height > top + ss {
                    f.fill_text(Text { content: sum, position: Point::new(r.x + pad, r.y + top), color: t::FG2, size: ss.into(), font: t::SERIF, max_width: tw, ..Text::default() });
                }
            } else {
                f.fill_text(Text { content: g.title.to_uppercase(), position: Point::new(r.x + 10.0, r.y + 8.0), color: t::FG3, size: 10.5.into(), font: t::MONO_MED, max_width: (r.width - 20.0).max(20.0), ..Text::default() });
            }
            // portas e canto de redimensionar
            if !frame_kind {
                let port = Point::new(r.x + r.width, r.y + r.height / 2.0);
                f.fill(&Path::circle(port, 5.5), if g.sel { t::ACCENT } else { t::RAISE });
                f.stroke(&Path::circle(port, 5.5), Stroke { width: 1.6, style: if g.sel { t::ACCENT } else { t::FG4 }.into(), ..Stroke::default() });
                let inp = Point::new(r.x, r.y + r.height / 2.0);
                f.fill(&Path::circle(inp, 5.0), t::RAISE);
                f.stroke(&Path::circle(inp, 5.0), Stroke { width: 1.6, style: t::FG4.into(), ..Stroke::default() });
            }
            for k in [4.0f32, 9.0] {
                f.stroke(&Path::line(Point::new(r.x + r.width - 3.0, r.y + r.height - 3.0 - k), Point::new(r.x + r.width - 3.0 - k, r.y + r.height - 3.0)), Stroke { width: 1.0, style: t::alpha(t::FG3, 0.7).into(), ..Stroke::default() });
            }
        }
        vec![f.into_geometry()]
    }

    fn mouse_interaction(&self, st: &Interaction, b: Rectangle, c: mouse::Cursor) -> mouse::Interaction {
        match &st.drag {
            Some(Drag::Pan(_)) | Some(Drag::Node { .. }) => return mouse::Interaction::Grabbing,
            Some(Drag::Link { .. }) => return mouse::Interaction::Crosshair,
            Some(Drag::Resize { .. }) => return mouse::Interaction::ResizingDiagonallyDown,
            None => {}
        }
        let Some(p) = c.position_in(b) else { return mouse::Interaction::None };
        match self.hit(p) {
            Some(g) => {
                let r = self.rect(g);
                let port = Point::new(r.x + r.width, r.y + r.height / 2.0);
                if ((p.x - port.x).powi(2) + (p.y - port.y).powi(2)).sqrt() <= 10.0 && g.kind != "moldura" {
                    mouse::Interaction::Crosshair
                } else if (Rectangle { x: r.x + r.width - 18.0, y: r.y + r.height - 18.0, width: 18.0, height: 18.0 }).contains(p) {
                    mouse::Interaction::ResizingDiagonallyDown
                } else if p.y - r.y <= HEAD * self.z {
                    mouse::Interaction::Grab
                } else {
                    mouse::Interaction::None
                }
            }
            None => mouse::Interaction::Grab,
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
            if m.model.is_empty() { body } else { column![body, tx(m.model.clone(), 11.0, t::FG4)].spacing(t::SP_1).into() }
        }
        "dado" => {
            let head = txf(format!("{} {} · {}", crate::i18n::text("DADO DE"), m.from.to_uppercase(), crate::i18n::text("NÃO É INSTRUÇÃO")), 10.0, t::FG3, t::MONO);
            dashed(container(column![head, tx(m.text.chars().take(1200).collect::<String>(), 13.0, t::FG2)].spacing(t::SP_1)).padding(pad(t::SP_2, t::SP_3, t::SP_2, t::SP_3)).width(Length::Fill), 12.0, t::LINE3)
        }
        _ => tx(m.text.clone(), 12.5, t::DANGER).into(),
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
        msgs = msgs.push(row![spinner(app.spin, 12.0), tx(crate::i18n::text("pensando…"), 12.5, t::FG3)].spacing(t::SP_2).align_y(Vertical::Center));
    }
    let list = scrollable(container(msgs).padding(pad(t::SP_2, t::SP_35, t::SP_2, t::SP_35))).direction(scrollable::Direction::Vertical(thin_bar())).style(thin_scroll).anchor_bottom().height(Length::Fill).width(Length::Fill);
    let draft = c.drafts.get(&n.id).cloned().unwrap_or_default();
    let (id1, id2) = (n.id.clone(), n.id.clone());
    let input = text_input(crate::i18n::text("Responder ao Orion"), &draft).on_input(move |s| cv(Cv::Draft(id1.clone(), s))).on_submit(cv(Cv::Send(id2.clone()))).size(13.5).font(t::UI).padding(pad(t::SP_25, t::SP_3, t::SP_25, t::SP_3)).style(field_style(t::RAISE2, t::LINE2, t::LINE3, 12.0));
    let send: El = if busy { spinner(app.spin, 14.0) } else { ib("up", if draft.trim().is_empty() { None } else { Some(cv(Cv::Send(n.id.clone()))) }, !draft.trim().is_empty()) };
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
        container(column![txf(host, 12.5, t::FG2, t::MONO), txf(crate::i18n::text("Só o link: o Orion não abre nem busca sozinho."), 10.5, t::FG4, t::MONO)].spacing(t::SP_1)).padding(pad(t::SP_1, t::SP_4, t::SP_2, t::SP_4)),
        container(btn(crate::i18n::text("Abrir no navegador"), Kind::Normal, true, Some("out"), Some(Msg::OpenUrl(n.url.clone())))).padding(pad(0.0, t::SP_4, t::SP_3, t::SP_4)),
    ]
    .into()
}

fn task_body<'a>(n: &'a CNode) -> El<'a> {
    let waiting = n.state == "pendente" && n.mission_state == "aguarda_aprovacao";
    let out: String = if n.out.is_empty() {
        if waiting {
            crate::i18n::text("Aguardando você aprovar o plano.").to_string()
        } else if n.state == "rodando" {
            crate::i18n::text("O agente está trabalhando…").to_string()
        } else {
            crate::i18n::text("Aguardando as dependências.").to_string()
        }
    } else {
        n.out.clone()
    };
    let meta = row![badge(&n.agent, Tone::Info), tx(format!("{} {}", crate::i18n::text("esforço"), n.effort), 11.5, t::FG3), grow(), txf(crate::i18n::text(state_label(&n.state)), 11.5, state_color(&n.state), t::UI_MED)].spacing(t::SP_2).align_y(Vertical::Center);
    column![
        container(tx(n.title.clone(), 15.0, t::FG)).padding(pad(t::SP_2, t::SP_4, t::SP_1, t::SP_4)),
        container(meta).padding(pad(0.0, t::SP_4, t::SP_2, t::SP_4)),
        scrollable(container(txf(out, 13.5, t::FG2, t::SERIF)).padding(pad(0.0, t::SP_4, t::SP_3, t::SP_4)).width(Length::Fill)).direction(scrollable::Direction::Vertical(thin_bar())).style(thin_scroll).height(Length::Fill),
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

fn geos(app: &App) -> Vec<Geo> {
    let c = &app.canvas;
    c.nodes
        .iter()
        .map(|n| Geo {
            id: n.id.clone(),
            kind: n.kind.clone(),
            x: n.x,
            y: n.y,
            w: n.w,
            h: n.h,
            label: label_of(n),
            title: n.title.clone(),
            summary: summary(n),
            dot: if n.kind == "tarefa" { state_color(&n.state) } else if c.busy.contains(&n.id) { t::ACCENT } else { t::FG4 },
            sel: c.sel.as_deref() == Some(n.id.as_str()),
            maestro: n.by_maestro,
        })
        .collect()
}

fn empty_state(app: &App) -> El<'_> {
    let sphere = container(iced::widget::canvas(crate::presence::Sphere { engine: &app.presence }).width(Length::Fixed(240.0)).height(Length::Fixed(200.0))).width(Length::Fill).center_x(Length::Fill);
    container(
        column![
            sphere,
            txf(crate::i18n::text("SEU CANVAS COM ORION"), t::TEXT_EYEBROW, t::FG3, t::UI_MED),
            txf(crate::i18n::text("Peça uma missão ao Maestro ou crie um chat."), 26.0, t::FG, t::SERIF),
            tx(crate::i18n::text("O Maestro divide o pedido, escolhe quem faz cada parte e monta os nós aqui."), t::TEXT_BASE, t::FG3),
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

fn board(app: &App) -> El<'_> {
    let c = &app.canvas;
    let z = c.z();
    let b = Board { geos: geos(app), edges: c.edges.iter().map(|e| (e.a.clone(), e.b.clone())).collect(), vx: c.pan.0, vy: c.pan.1, z, detail: c.detail() };
    let mut layers: Vec<El> = vec![iced::widget::canvas(b.clone()).width(Length::Fill).height(Length::Fill).into()];
    if c.nodes.is_empty() && c.loaded {
        layers.push(empty_state(app));
    }
    if c.detail() {
        for n in &c.nodes {
            if let Some(body) = node_body(app, n) {
                let w = n.w;
                let h = (n.h - HEAD - 2.0).max(40.0);
                layers.push(pin(container(body).width(Length::Fixed(w - 2.0)).height(Length::Fixed(h))).x(n.x * z + c.pan.0 + 1.0).y((n.y + HEAD) * z + c.pan.1 + 1.0).into());
            }
        }
    }
    // botões das setas: enviar a última resposta (só entre chats) e remover
    for e in &c.edges {
        let (Some(ga), Some(gb)) = (b.geo(&e.a), b.geo(&e.b)) else { continue };
        let (p0, p1, _) = b.edge_pts(ga, gb);
        let (s0, s1) = (b.to_screen(p0), b.to_screen(p1));
        let mid = Point::new((s0.x + s1.x) / 2.0, (s0.y + s1.y) / 2.0);
        let both_chat = c.node(&e.a).is_some_and(|n| n.kind == "chat") && c.node(&e.b).is_some_and(|n| n.kind == "chat");
        let mut r = row![].spacing(t::SP_05).align_y(Vertical::Center);
        if both_chat {
            r = r.push(btn(crate::i18n::text("Enviar última resposta"), Kind::Ghost, true, None, Some(cv(Cv::SendEdge(e.id.clone())))));
        }
        r = r.push(ib_sz("close", 22.0, 12.0, Some(cv(Cv::DelEdge(e.id.clone()))), false));
        let width = if both_chat { 218.0 } else { 26.0 };
        let pill = container(r).padding(pad(0.0, t::SP_05, 0.0, t::SP_05)).style(surface(Some(t::RAISE), Some(t::LINE2), t::R_PILL));
        layers.push(pin(pill).x(mid.x - width / 2.0).y(mid.y - 14.0).into());
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
        spaces = spaces.push(space_chip(name, *id == c.cur, id.clone()));
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
    let zoom = row![
        tip(ib("minus", Some(cv(Cv::Zoom(1))), false), crate::i18n::text("Afastar")),
        txf(format!("{}%", (c.z() * 100.0).round()), 12.0, t::FG2, t::MONO),
        tip(ib("plus", Some(cv(Cv::Zoom(-1))), false), crate::i18n::text("Aproximar")),
        tip(ib("fit", Some(cv(Cv::Fit)), false), crate::i18n::text("Ajustar ao conteúdo")),
    ]
    .spacing(t::SP_05)
    .align_y(Vertical::Center);
    let mut r = row![spaces, vline(18.0), tools].spacing(t::SP_3).align_y(Vertical::Center);
    if let Some(u) = &c.url_in {
        let input = text_input("https://…", u).on_input(|s| cv(Cv::UrlIn(s))).on_submit(cv(Cv::UrlGo)).size(13).font(t::UI).width(Length::Fixed(240.0)).padding(pad(t::SP_15, t::SP_25, t::SP_15, t::SP_25)).style(field_style(t::RAISE, t::LINE2, t::ACCENT, 8.0));
        r = r.push(input).push(btn(crate::i18n::text("Adicionar"), Kind::Primary, true, None, Some(cv(Cv::UrlGo))));
    }
    r = r.push(grow());
    r = r.push(chip(format!("{} {}", crate::i18n::text("Maestro:"), maestro_name), None, Some("bolt"), true, Msg::OpenSettings(Some(Sec::Terminais))));
    r = r.push(zoom);
    container(r).padding(pad(t::SP_2, t::SP_4, t::SP_2, t::SP_4)).width(Length::Fill).into()
}

fn mission_state(s: &str) -> (&'static str, Color) {
    match s {
        "planejando" => ("planejando", t::ACCENT),
        "aguarda_aprovacao" => ("aguardando você", t::SIGNAL),
        "rodando" => ("rodando", t::ACCENT),
        "integrando" => ("juntando o resultado", t::ACCENT),
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
        if m.state == "aguarda_aprovacao" {
            r = r.push(btn(crate::i18n::text("Aprovar plano"), Kind::Primary, true, None, Some(Msg::MaestroApprove(m.id.clone()))));
        }
        r = r.push(btn(crate::i18n::text("Cancelar"), Kind::Ghost, true, None, Some(Msg::MaestroCancel(m.id.clone()))));
        col = col.push(container(r).padding(pad(0.0, t::SP_4, t::SP_15, t::SP_4)));
    }
    col.into()
}

pub fn view(app: &App) -> El<'_> {
    let top = column![toolbar(app), mission_bar(app)].width(Length::Fill);
    column![top, hline(), board(app)].width(Length::Fill).height(Length::Fill).into()
}

#[allow(dead_code)]
fn _zooms() -> usize {
    ZOOMS.len()
}
