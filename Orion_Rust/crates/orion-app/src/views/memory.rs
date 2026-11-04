//! Memória (§16): grafo do vault com filtros, busca, "Lidas agora", zoom, grafo local, detalhes e linha do tempo.
//! O grafo é uma constelação 3D desenhada em Canvas: câmera orbital (arrastar), pan (botão direito ou Shift), zoom (roda),
//! perspectiva, profundidade e brilho estelar. O 3D em wgpu (F16.02) substituirá só a camada de desenho.

use crate::app::{App, Msg, View};
use crate::graph::{Forces, Graph, TIPOS};
use crate::icons::icon;
use crate::sky::{draw_sky, Sky};
use crate::theme as t;
use crate::ui::*;
use crate::views::login::field_style;
use iced::alignment::Vertical;
use iced::mouse;
use iced::widget::canvas::{self, Action, Geometry, Path, Stroke, Text};
use iced::widget::{button, canvas as cv, column, container, row, scrollable, slider, stack, text_input};
use iced::{Background, Border, Color, Event, Length, Point, Rectangle, Renderer, Theme};
use std::time::Instant;

pub const TYPE_COLORS: [Color; 7] = [t::N_PROJETO, t::N_AREA, t::N_CONCEITO, t::N_PESQUISA, t::N_RECURSO, t::N_CONVERSA, t::N_RELATORIO];

/// Tamanho aproximado do palco, para enquadrar e centralizar sem depender do layout.
pub fn stage_size(app: &App) -> (f32, f32) {
    let side = if app.collapsed || app.is_drawer_mode() { 0.0 } else { t::SIDE_W + 1.0 };
    (app.win.width - side - 24.0, app.win.height - 46.0 - 46.0 - 44.0)
}

pub fn update(app: &mut App, m: Msg) {
    let (w, h) = stage_size(app);
    if !matches!(&m, Msg::MemHover(..)) {
        app.mem.cache.clear();
    }
    match m {
        Msg::MemQ(s) => {
            app.mem.q = s.clone();
            app.mem_q = s;
            app.mem.spinning = false;
        }
        Msg::MemFilter(i) => app.mem.filt[i] = !app.mem.filt[i],
        Msg::MemRead => app.mem.read_on = !app.mem.read_on,
        Msg::MemZoom(f) => {
            let g = &mut app.mem;
            g.spinning = false;
            g.cam.0 = (g.cam.0 * f).clamp(0.2, 6.0);
        }
        Msg::MemFit => {
            app.mem.spinning = false;
            app.mem.fit(w, h);
        }
        Msg::MemHalt => app.mem.spinning = false,
        Msg::MemSelect(n) => {
            // Um clique com tremor de poucos pixels também prende a nota; soltar sempre a libera.
            app.mem.pin = None;
            app.mem.sel = n;
            if n.is_none() {
                app.mem.local = None;
            }
            if let Some(i) = n {
                center(app, i);
            }
        }
        Msg::MemHover(n, x, y) => {
            if app.mem.hov != n {
                app.mem.cache.clear();
            }
            app.mem.hov = n;
            app.mem_tip = (x, y);
        }
        Msg::MemPan(dx, dy) => {
            let g = &mut app.mem;
            g.spinning = false;
            g.cam.1 += dx / g.cam.0;
            g.cam.2 += dy / g.cam.0;
        }
        Msg::MemZoomAt(px, py, f) => {
            let g = &mut app.mem;
            g.spinning = false;
            let (wx, wy) = g.to_plane(px, py, w, h);
            g.cam.0 = (g.cam.0 * f).clamp(0.2, 6.0);
            let (nx, ny) = g.to_plane(px, py, w, h);
            g.cam.1 += nx - wx;
            g.cam.2 += ny - wy;
        }
        Msg::MemOrbit(dx, dy) => {
            let g = &mut app.mem;
            g.spinning = false;
            g.rot += dx * 0.0072;
            g.pitch = (g.pitch + dy * 0.0072).clamp(-1.45, 1.45);
        }
        Msg::MemDrag(i, x, y, z) => {
            // A nota fica presa ao cursor e a simulação esquenta, então as vizinhas vêm junto.
            app.mem.spinning = false;
            let n = &mut app.mem.nodes[i];
            (n.x, n.y, n.z) = (x, y, z);
            app.mem.pin = Some(i);
            // Com "reduzir movimento" a nota só acompanha o cursor; o grafo assenta de uma vez ao soltar.
            if !app.p.reduced {
                app.mem.reheat(0.3);
            }
        }
        Msg::MemRelease => {
            app.mem.pin = None;
            if app.p.reduced {
                app.mem.reheat(0.3);
                app.mem.settle_now();
            }
        }
        Msg::MemForces => app.mem.forces_open = !app.mem.forces_open,
        Msg::MemForce(k, v) => {
            app.mem.set_force(k, v);
            resettle(app, w, h);
        }
        Msg::MemForcesReset => {
            app.mem.forces = Forces::default();
            app.mem.reheat(0.6);
            app.mem.refit = true;
            resettle(app, w, h);
        }
        Msg::MemCut(v) => {
            let g = &mut app.mem;
            g.cut_idx = (v.round() as usize).min(g.dates.len().saturating_sub(1));
            g.cut = g.dates[g.cut_idx].clone();
            if let Some(s) = g.sel {
                if !g.visible(s) {
                    g.sel = None;
                    g.local = None;
                }
            }
        }
        Msg::MemOpenNote(i) => {
            let n = &app.mem.nodes[i];
            let path = format!("{}/{}.md", n.folder, n.label);
            let _ = app.update(Msg::EditorOpen(path));
        }
        Msg::MemLocal(i) => {
            let next = match app.mem.local {
                Some((c, 1)) if c == i => Some((i, 2)),
                Some((c, _)) if c == i => None,
                _ => Some((i, 1)),
            };
            app.mem.local = next;
            app.mem.fit(w, h);
            match next {
                Some((_, 1)) => app.toast("Grafo local: vizinhança em 1 salto (clique de novo para 2 saltos)"),
                Some((_, 2)) => app.toast("Grafo local: vizinhança em 2 saltos (clique de novo para mostrar tudo)"),
                _ => app.toast("Grafo completo restaurado"),
            }
        }
        _ => {}
    }
}

/// Com "reduzir movimento" não há animação: assenta na hora e enquadra.
fn resettle(app: &mut App, w: f32, h: f32) {
    if app.p.reduced {
        app.mem.settle_now();
        app.mem.refit = false;
        app.mem.fit(w, h);
    }
}

fn center(app: &mut App, i: usize) {
    let (w, _h) = stage_size(app);
    let g = &mut app.mem;
    g.spinning = false;
    let (rx, ry) = g.rotp(i);
    let k = g.cam.0.max(1.12);
    let panel_shift = if w > 640.0 { -160.0 / k } else { 0.0 };
    let to = (k, -rx + panel_shift, -ry);
    if app.p.reduced {
        g.cam = to;
    } else {
        app.mem_tween = Some((Instant::now(), g.cam, to));
    }
}

pub fn tick(app: &mut App, now: Instant) {
    if app.view != View::Memoria {
        return;
    }
    // Simulação de forças: dois passos por quadro (~48 passos/s) até esfriar; ao assentar depois de mexer num controle, enquadra.
    if app.mem.simulating() {
        app.mem.step();
        app.mem.step();
        app.mem.cache.clear();
        if !app.mem.simulating() && app.mem.refit {
            app.mem.refit = false;
            let (w, h) = stage_size(app);
            app.mem.fit(w, h);
        }
    }
    // Órbita lenta: as notas giram em torno do núcleo até você mexer; respeita "reduzir movimento".
    if app.mem.spinning && !app.p.reduced {
        let dt = app.mem.drift_at.map(|t0| now.duration_since(t0).as_secs_f32()).unwrap_or(0.0);
        if app.mem.drift_at.is_none() || dt >= 0.05 {
            app.mem.orbit += dt.min(0.2) * 0.05;
            app.mem.drift_at = Some(now);
            app.mem.cache.clear();
        }
    } else {
        app.mem.drift_at = None;
    }
    if let Some((t0, from, to)) = app.mem_tween {
        app.mem.cache.clear();
        let p = (now.duration_since(t0).as_secs_f32() / 0.45).min(1.0);
        let e = 1.0 - (1.0 - p).powi(3);
        app.mem.cam = (from.0 + (to.0 - from.0) * e, from.1 + (to.1 - from.1) * e, from.2 + (to.2 - from.2) * e);
        if p >= 1.0 {
            app.mem_tween = None;
        }
    }
}

struct Mem<'a> {
    g: &'a Graph,
}

#[derive(Clone, Copy, PartialEq)]
enum DragMode {
    Orbit,
    Pan,
}

#[derive(Default)]
struct Interaction {
    /// (nó sob o cursor, último ponto, se já se mexeu, o que o arrasto faz no vazio)
    drag: Option<(Option<usize>, Point, bool, DragMode)>,
    shift: bool,
}

impl Default for DragMode {
    fn default() -> Self {
        DragMode::Orbit
    }
}

impl canvas::Program<Msg> for Mem<'_> {
    type State = Interaction;

    fn update(&self, st: &mut Interaction, ev: &Event, b: Rectangle, cursor: mouse::Cursor) -> Option<Action<Msg>> {
        let (w, h) = (b.width, b.height);
        let pos = cursor.position_in(b);
        match ev {
            Event::Keyboard(iced::keyboard::Event::ModifiersChanged(m)) => {
                st.shift = m.shift();
                None
            }
            Event::Mouse(mouse::Event::ButtonPressed(btn @ (mouse::Button::Left | mouse::Button::Right))) => {
                let p = pos?;
                let right = *btn == mouse::Button::Right;
                let n = if right { None } else { self.g.hit(p.x, p.y, w, h) };
                let mode = if right || st.shift { DragMode::Pan } else { DragMode::Orbit };
                st.drag = Some((n, p, false, mode));
                Some(Action::publish(Msg::MemHalt).and_capture())
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let Some((node, last, moved, mode)) = st.drag.as_mut() {
                    let p = cursor.position_in(b).or_else(|| cursor.position().map(|c| Point::new(c.x - b.x, c.y - b.y)))?;
                    let (dx, dy) = (p.x - last.x, p.y - last.y);
                    if dx.abs() + dy.abs() > 2.0 {
                        *moved = true;
                    }
                    *last = p;
                    return Some(match node {
                        Some(i) => {
                            let (x, y, z) = self.g.drag_target(*i, p.x, p.y, w, h);
                            Action::publish(Msg::MemDrag(*i, x, y, z)).and_capture()
                        }
                        None if *mode == DragMode::Pan => Action::publish(Msg::MemPan(dx, dy)).and_capture(),
                        None => Action::publish(Msg::MemOrbit(dx, dy)).and_capture(),
                    });
                }
                match pos {
                    Some(p) => {
                        let n = self.g.hit(p.x, p.y, w, h);
                        if self.g.hov == n && n.is_none() {
                            None
                        } else {
                            Some(Action::publish(Msg::MemHover(n, p.x, p.y)))
                        }
                    }
                    None => {
                        if self.g.hov.is_some() {
                            Some(Action::publish(Msg::MemHover(None, 0.0, 0.0)))
                        } else {
                            None
                        }
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left | mouse::Button::Right)) => {
                let d = st.drag.take()?;
                if !d.2 && d.3 == DragMode::Orbit {
                    let p = cursor.position_in(b).unwrap_or(d.1);
                    return Some(Action::publish(Msg::MemSelect(self.g.hit(p.x, p.y, w, h))).and_capture());
                }
                if d.0.is_some() {
                    return Some(Action::publish(Msg::MemRelease).and_capture());
                }
                Some(Action::capture())
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let p = pos?;
                let dy = match delta {
                    mouse::ScrollDelta::Lines { y, .. } | mouse::ScrollDelta::Pixels { y, .. } => *y,
                };
                Some(Action::publish(Msg::MemZoomAt(p.x, p.y, if dy > 0.0 { 1.12 } else { 1.0 / 1.12 })).and_capture())
            }
            _ => None,
        }
    }

    fn mouse_interaction(&self, st: &Interaction, b: Rectangle, c: mouse::Cursor) -> mouse::Interaction {
        if st.drag.is_some() {
            return mouse::Interaction::Grabbing;
        }
        match c.position_in(b) {
            Some(p) if self.g.hit(p.x, p.y, b.width, b.height).is_some() => mouse::Interaction::Pointer,
            Some(_) => mouse::Interaction::Grab,
            None => mouse::Interaction::None,
        }
    }

    fn draw(&self, _s: &Interaction, r: &Renderer, _th: &Theme, b: Rectangle, _c: mouse::Cursor) -> Vec<Geometry> {
        let g = self.g;
        let (w, h) = (b.width, b.height);
        let geom = g.cache.draw(r, b.size(), |f| {
            // Fundo: radial-gradient(ellipse at 50% 45%, #15192a 0%, #0b0c10 72%)
            f.fill_rectangle(Point::ORIGIN, b.size(), t::ISLAND_BG);
            let (c0, c1) = ((0x15 as f32, 0x19 as f32, 0x2a as f32), (0x0b as f32, 0x0c as f32, 0x10 as f32));
            const STEPS: usize = 18;
            for k in 0..STEPS {
                let u = k as f32 / (STEPS - 1) as f32;
                let col = Color::from_rgb8((c1.0 + (c0.0 - c1.0) * u) as u8, (c1.1 + (c0.1 - c1.1) * u) as u8, (c1.2 + (c0.2 - c1.2) * u) as u8);
                let (rx, ry) = (w * 0.5 * 1.1 * (1.0 - u) * 0.72 * 1.4 + 1.0, h * 0.5 * 1.1 * (1.0 - u) * 0.72 * 1.4 + 1.0);
                f.with_save(|f| {
                    f.translate(iced::Vector::new(w * 0.5, h * 0.45));
                    f.scale_nonuniform(iced::Vector::new(rx, ry));
                    f.fill(&Path::circle(Point::ORIGIN, 1.0), col);
                });
            }
            draw_sky(f, w, h, Sky { alpha: 0.45, constellation: false, intro: 1.0, belt: 0.0 });
            parallax_stars(f, g, w, h);

            let focus = g.sel.or(g.hov);
            let nb: Vec<usize> = focus.map(|i| g.adj[i].clone()).unwrap_or_default();
            let q = g.q.trim();
            let rd = g.read_on;
            let is_nb = |i: usize| nb.binary_search(&i).is_ok();
            let lit = |i: usize| (focus.is_none() || Some(i) == focus || is_nb(i)) && (q.is_empty() || g.matches(i) || Some(i) == focus) && (!rd || g.nodes[i].read || Some(i) == focus);
            // Projeção 3D: (x, y, escala de perspectiva, profundidade) por nó visível.
            let scr: Vec<Option<(f32, f32, f32, f32)>> = (0..g.nodes.len()).map(|i| if g.visible(i) { Some(g.to_screen3(i, w, h)) } else { None }).collect();
            let (mut zmin, mut zmax) = (f32::MAX, f32::MIN);
            for p in scr.iter().flatten() {
                zmin = zmin.min(p.3);
                zmax = zmax.max(p.3);
            }
            let zspan = (zmax - zmin).max(1.0);
            // 0 = perto da câmera, 1 = longe.
            let far = |z: f32| ((z - zmin) / zspan).clamp(0.0, 1.0);
            let kk = g.cam.0.sqrt().clamp(0.72, 2.2);

            nebulae(f, g, &scr, focus.is_some(), kk);
            core_glow(f, g, w, h, kk, focus.is_some());

            // Arestas: três faixas de profundidade (perto mais nítidas, longe quase apagadas), como linhas de constelação.
            let edge_a = if focus.is_some() || !q.is_empty() || rd { 0.026 } else { 0.07 };
            let mut bands = [Path::new(|_| {}), Path::new(|_| {}), Path::new(|_| {})];
            {
                let mut bs: [Vec<(Point, Point)>; 3] = [vec![], vec![], vec![]];
                for &(a, bb) in &g.links {
                    if focus == Some(a) || focus == Some(bb) {
                        continue;
                    }
                    if let (Some(x), Some(y)) = (scr[a], scr[bb]) {
                        let t = far((x.3 + y.3) * 0.5);
                        let k = if t < 0.34 { 0 } else if t < 0.67 { 1 } else { 2 };
                        bs[k].push((Point::new(x.0, x.1), Point::new(y.0, y.1)));
                    }
                }
                for (k, seg) in bs.iter().enumerate() {
                    bands[k] = Path::new(|p| {
                        for (u, v) in seg {
                            p.move_to(*u);
                            p.line_to(*v);
                        }
                    });
                }
            }
            for (k, (mult, wid)) in [(1.6f32, 0.95f32), (1.0, 0.7), (0.5, 0.5)].into_iter().enumerate() {
                f.stroke(&bands[k], Stroke::default().with_width(wid).with_color(t::alpha(t::GRAPH_ARESTA, (edge_a * mult).min(0.5))));
            }

            // Arestas incidentes ao nó focado: opacidade graduada pelo grau para evitar mancha opaca em hubs de >200 conexões
            if let Some(fi) = focus {
                let deg_f = (nb.len().max(1) as f32).sqrt();
                let focus_alpha = (1.05 / deg_f).clamp(0.15, 0.5);
                let focus_w = if nb.len() > 80 { 0.78 } else { 1.1 };
                let fe = Path::new(|p| {
                    for &(a, bb) in &g.links {
                        if a != fi && bb != fi {
                            continue;
                        }
                        if let (Some(x), Some(y)) = (scr[a], scr[bb]) {
                            p.move_to(Point::new(x.0, x.1));
                            p.line_to(Point::new(y.0, y.1));
                        }
                    }
                });
                f.stroke(&fe, Stroke::default().with_width(focus_w).with_color(t::alpha(TYPE_COLORS[g.nodes[fi].tipo], focus_alpha)));
            }

            let mut q_top: Vec<usize> = vec![];
            if !q.is_empty() {
                let mut v: Vec<usize> = (0..g.nodes.len()).filter(|&i| g.visible(i) && g.matches(i)).collect();
                v.sort_by_key(|&i| std::cmp::Reverse(g.nodes[i].deg));
                v.truncate(14);
                q_top = v;
            }
            let mut focus_nb_top: Vec<usize> = vec![];
            if focus.is_some() {
                let mut v: Vec<usize> = nb.iter().copied().filter(|&i| g.visible(i)).collect();
                v.sort_by_key(|&i| std::cmp::Reverse(g.nodes[i].deg));
                v.truncate(22);
                focus_nb_top = v;
            }

            // Estrelas, do fundo para a frente (algoritmo do pintor): halo, núcleo quente e brilho em cruz nos hubs.
            let mut draw_order: Vec<usize> = (0..g.nodes.len()).filter(|&i| scr[i].is_some()).collect();
            draw_order.sort_by(|&a, &bb| scr[bb].map(|p| p.3).partial_cmp(&scr[a].map(|p| p.3)).unwrap_or(std::cmp::Ordering::Equal));
            let mut labels: Vec<(usize, f32, f32, f32)> = vec![];
            for i in draw_order {
                let n = &g.nodes[i];
                let Some((x, y, s, z)) = scr[i] else { continue };
                if x < -40.0 || y < -40.0 || x > w + 40.0 || y > h + 40.0 {
                    continue;
                }
                let on = lit(i);
                let depth = far(z);
                let col = cool_tint(TYPE_COLORS[n.tipo], depth);
                let rr = g.radius(i) * kk * s;
                let is_focus = Some(i) == focus;
                let brightness = 1.0 - 0.52 * depth;
                let star = n.tipo == 0 || n.tipo == 1 || n.hub || is_focus || (rd && n.read);

                if on {
                    if star {
                        let base = if is_focus { 1.0 } else { 0.62 };
                        for (mult, alpha) in [(4.2f32, 0.045f32), (2.7, 0.085), (1.7, 0.16)] {
                            f.fill(&Path::circle(Point::new(x, y), rr * mult * if is_focus { 1.25 } else { 1.0 }), t::alpha(col, alpha * base * brightness * 1.4));
                        }
                    } else if depth < 0.62 {
                        f.fill(&Path::circle(Point::new(x, y), rr * 2.2), t::alpha(col, 0.07 * brightness));
                    }
                }

                let node_r = rr * if is_focus { 1.28 } else { 1.0 };
                let fill_a = if on { 0.97 * brightness.max(0.55) } else { 0.12 };
                f.fill(&Path::circle(Point::new(x, y), node_r), t::alpha(col, fill_a));
                if on {
                    // Núcleo quente: o ponto claro no centro é o que faz o nó parecer uma estrela e não um disco chapado.
                    f.fill(&Path::circle(Point::new(x, y), node_r * 0.46), t::alpha(Color::WHITE, 0.55 * brightness));
                    if star && node_r > 3.0 {
                        let l = node_r * if is_focus { 5.0 } else { 3.6 };
                        let cross = Path::new(|p| {
                            p.move_to(Point::new(x - l, y));
                            p.line_to(Point::new(x + l, y));
                            p.move_to(Point::new(x, y - l));
                            p.line_to(Point::new(x, y + l));
                        });
                        f.stroke(&cross, Stroke::default().with_width(0.6).with_color(t::alpha(Color::WHITE, 0.34 * brightness)));
                    }
                }
                if Some(i) == g.sel || (rd && n.read) {
                    f.stroke(
                        &Path::circle(Point::new(x, y), node_r + 3.2),
                        Stroke::default().with_width(1.35).with_color(t::alpha(t::FG, 0.92)),
                    );
                }

                let show = on
                    && (is_focus
                        || n.tipo == 0
                        || n.tipo == 1
                        || (n.hub && g.cam.0 >= 0.72)
                        || (focus.is_some() && focus_nb_top.contains(&i))
                        || (!q.is_empty() && q_top.contains(&i))
                        || (rd && n.read)
                        || (g.cam.0 > 1.45 && n.deg >= 8)
                        || g.cam.0 > 2.15);
                // Rótulos só para o que está na frente (ou em foco): no fundo viram ruído.
                if show && (depth < 0.78 || is_focus || g.hov == Some(i) || g.sel == Some(i)) {
                    let pri = (if is_focus { 1e6 } else { 0.0 })
                        + (if g.hov == Some(i) { 5e5 } else { 0.0 })
                        + (if (!q.is_empty() && q_top.contains(&i)) || (rd && n.read) { 1e5 } else { 0.0 })
                        + (if n.tipo == 0 { 2e4 } else if n.tipo == 1 { 1e4 } else { 0.0 })
                        + n.deg as f32
                        + (1.0 - depth) * 60.0;
                    labels.push((i, x, y + node_r + 4.5, pri));
                }
            }

            labels.sort_by(|a, b| b.3.partial_cmp(&a.3).unwrap_or(std::cmp::Ordering::Equal));
            let mut placed: Vec<[f32; 4]> = vec![];
            for (i, x, y, _) in labels {
                let n = &g.nodes[i];
                let is_focus = Some(i) == focus;
                let big = is_focus || n.tipo == 0 || n.tipo == 1 || n.hub;
                let max_chars = if is_focus { 42 } else { 28 };
                let txt = if n.label.chars().count() > max_chars {
                    format!("{}…", n.label.chars().take(max_chars - 1).collect::<String>())
                } else {
                    n.label.clone()
                };
                let size = if is_focus { 12.5 } else if big { 11.5 } else { 10.8 };
                let tw = txt.chars().count() as f32 * size * 0.53;
                let pad_x = 5.5;
                let pill_h = size + 5.0;
                let bx = [x - tw / 2.0 - pad_x, y - 1.5, x + tw / 2.0 + pad_x, y + pill_h];
                if bx[0] < 6.0 || bx[2] > w - 6.0 || bx[1] < 6.0 || bx[3] > h - 28.0 {
                    continue;
                }
                if placed.iter().any(|p| bx[0] < p[2] + 4.0 && bx[2] > p[0] - 4.0 && bx[1] < p[3] + 2.0 && bx[3] > p[1] - 2.0) {
                    continue;
                }
                placed.push(bx);

                let pill_rect = Path::rounded_rectangle(
                    Point::new(bx[0], bx[1]),
                    iced::Size::new(bx[2] - bx[0], bx[3] - bx[1]),
                    5.0.into(),
                );
                f.fill(&pill_rect, Color::from_rgba8(0x0c, 0x0e, 0x13, if is_focus { 0.92 } else { 0.76 }));
                if is_focus {
                    f.stroke(&pill_rect, Stroke::default().with_width(1.0).with_color(t::alpha(TYPE_COLORS[n.tipo], 0.55)));
                }
                f.fill_text(Text {
                    content: txt,
                    position: Point::new(x, y + 1.0),
                    color: t::alpha(if big { t::FG } else { t::FG2 }, 0.96),
                    size: size.into(),
                    font: if big { t::UI_MED } else { t::UI },
                    align_x: iced::alignment::Horizontal::Center.into(),
                    align_y: Vertical::Top,
                    ..Text::default()
                });
            }
        });
        vec![geom]
    }
}

/// Tinge de azul-frio o que está longe (perspectiva atmosférica).
fn cool_tint(c: Color, depth: f32) -> Color {
    let k = depth * 0.38;
    let cold = (0.55, 0.66, 0.98);
    Color { r: c.r + (cold.0 - c.r) * k, g: c.g + (cold.1 - c.g) * k, b: c.b + (cold.2 - c.b) * k, a: c.a }
}

/// Estrelas distantes que se deslocam menos que o grafo quando a câmera orbita: dão a sensação de espaço profundo.
fn parallax_stars(f: &mut canvas::Frame, g: &Graph, w: f32, h: f32) {
    const N: u32 = 150;
    for i in 0..N {
        let mut v = i.wrapping_mul(0x9E37_79B1).wrapping_add(0x7F4A_7C15);
        v ^= v << 13;
        v ^= v >> 17;
        v ^= v << 5;
        let (a, b, c) = ((v & 0x3ff) as f32 / 1023.0, ((v >> 10) & 0x3ff) as f32 / 1023.0, ((v >> 20) & 0x3ff) as f32 / 1023.0);
        let layer = 0.12 + 0.55 * a;
        let x = (b * w + g.rot * layer * 260.0 + g.cam.1 * layer * 0.4).rem_euclid(w.max(1.0));
        let y = (c * h + g.pitch * layer * 200.0 + g.cam.2 * layer * 0.4).rem_euclid(h.max(1.0));
        let r = 0.45 + layer * 1.4;
        let tint = if i % 5 == 0 { Color::from_rgb(0.72, 0.8, 1.0) } else if i % 7 == 0 { Color::from_rgb(1.0, 0.9, 0.78) } else { Color::WHITE };
        f.fill(&Path::circle(Point::new(x, y), r), t::alpha(tint, 0.18 + 0.5 * layer));
    }
}

/// Névoas suaves atrás de cada tipo de nota, no centro de massa projetado: os aglomerados parecem nebulosas.
fn nebulae(f: &mut canvas::Frame, g: &Graph, scr: &[Option<(f32, f32, f32, f32)>], focused: bool, kk: f32) {
    let mut acc = [(0.0f32, 0.0f32, 0.0f32, 0usize); 7];
    for (i, p) in scr.iter().enumerate() {
        let Some((x, y, _, _)) = p else { continue };
        let wgt = 1.0 + (g.nodes[i].deg as f32).sqrt();
        let a = &mut acc[g.nodes[i].tipo];
        a.0 += x * wgt;
        a.1 += y * wgt;
        a.2 += wgt;
        a.3 += 1;
    }
    let fade = if focused { 0.45 } else { 1.0 };
    for (kind, (sx, sy, sw, n)) in acc.into_iter().enumerate() {
        if n < 3 || sw <= 0.0 {
            continue;
        }
        let (cx, cy) = (sx / sw, sy / sw);
        let glow_r = (70.0 + (n as f32).sqrt() * 5.0).min(190.0) * kk;
        for k in 0..6 {
            let u = 1.0 - k as f32 / 6.0;
            f.fill(&Path::circle(Point::new(cx, cy), glow_r * u), t::alpha(TYPE_COLORS[kind], 0.014 * fade));
        }
    }
}

/// O "sol" no centro do grafo: o que as notas orbitam. Brilho quente no miolo e halo frio por fora.
fn core_glow(f: &mut canvas::Frame, g: &Graph, w: f32, h: f32, kk: f32, focused: bool) {
    let (x, y, z) = g.view_space(0.0, 0.0, 0.0);
    let s = Graph::persp(z);
    let c = Point::new(w / 2.0 + (x * s + g.cam.1) * g.cam.0, h / 2.0 + (y * s + g.cam.2) * g.cam.0);
    let fade = if focused { 0.5 } else { 1.0 };
    let base = 17.0 * kk * s;
    for (mult, alpha, col) in [
        (7.0f32, 0.020f32, Color::from_rgb(0.55, 0.66, 1.0)),
        (4.6, 0.034, Color::from_rgb(0.62, 0.72, 1.0)),
        (3.0, 0.060, Color::from_rgb(0.85, 0.85, 1.0)),
        (1.9, 0.12, Color::from_rgb(1.0, 0.93, 0.8)),
        (1.0, 0.30, Color::from_rgb(1.0, 0.95, 0.85)),
    ] {
        f.fill(&Path::circle(c, base * mult), t::alpha(col, alpha * fade));
    }
    f.fill(&Path::circle(c, base * 0.34), t::alpha(Color::WHITE, 0.75 * fade));
}

/// Painel de forças (como o do Obsidian): cada controle reacomoda o grafo ao vivo.
fn forces_panel(g: &Graph) -> El<'_> {
    fn slider_row(name: &'static str, hint: &'static str, k: u8, v: f32) -> El<'static> {
        let sl = slider(0.0..=1.0, v, move |x| Msg::MemForce(k, x)).step(0.01f32).style(|_, _| slider::Style {
            rail: slider::Rail { backgrounds: (Background::Color(t::ACCENT), Background::Color(t::LINE3)), width: 4.0, border: Border { radius: 2.0.into(), ..Default::default() } },
            handle: slider::Handle { shape: slider::HandleShape::Circle { radius: 7.0 }, background: Background::Color(t::ACCENT), border_width: 0.0, border_color: Color::TRANSPARENT },
        });
        column![tx(crate::i18n::text(name), 12.0, t::FG), tx(crate::i18n::text(hint), 11.0, t::FG3), sl].spacing(3).into()
    }
    let f = g.forces;
    let body = column![
        row![tx(crate::i18n::text("Forças"), 13.0, t::FG), grow(), btn(crate::i18n::text("Restaurar"), Kind::Normal, true, None, Some(Msg::MemForcesReset))].align_y(Vertical::Center),
        slider_row("Força central", "Puxa as notas para o centro", 0, f.center),
        slider_row("Força de repulsão", "Afasta as notas umas das outras", 1, f.repel),
        slider_row("Força dos links", "Puxa as notas ligadas", 2, f.link),
        slider_row("Distância dos links", "Tamanho ideal de cada link", 3, f.dist),
        tx(crate::i18n::text("Arraste uma nota: as vizinhas vão junto."), 11.0, t::FG3),
    ]
    .spacing(10);
    container(container(body).width(Length::Fixed(250.0)).padding(12).style(|_| container::Style { background: Some(t::mix(t::RAISE, 0.94).into()), border: Border { color: t::LINE2, width: 1.0, radius: t::R_CARD.into() }, shadow: shadow(), ..Default::default() })).into()
}

pub(crate) fn fchip<'a>(label: &str, dot: Option<Color>, ic: Option<&str>, on: bool, msg: Msg) -> El<'a> {
    let fg = if on { t::FG } else { t::FG3 };
    let mut r = row![].spacing(6).align_y(Vertical::Center);
    if let Some(d) = dot {
        r = r.push(dot_el(if on { d } else { t::alpha(d, 0.5) }, 7.0));
    }
    if let Some(i) = ic {
        r = r.push(icon(i, 14.0, fg));
    }
    r = r.push(tx(label.to_string(), 12.5, fg));
    let b = button(container(r).center_y(Length::Fill))
        .height(Length::Fixed(30.0))
        .padding(pad(0.0, 10.0, 0.0, 10.0))
        .on_press(msg)
        .style(move |_, _| button::Style { background: None, text_color: fg, border: Border { color: if on { t::LINE2 } else { Color::TRANSPARENT }, width: 1.0, radius: t::R_PILL.into() }, shadow: Default::default(), snap: true });
    if on { b.into() } else { dashed(b, 15.0, t::alpha(t::LINE2, 0.5)) }
}

fn fmt_n(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push('.');
        }
        out.push(ch);
    }
    out
}

fn dmy(d: &str) -> String {
    d.split('-').rev().collect::<Vec<_>>().join("/")
}

pub fn view(app: &App) -> El<'_> {
    let g = &app.mem;
    if g.nodes.is_empty() && app.api.is_some() {
        let card = column![
            tx(crate::i18n::text("Sua memória ainda está vazia"), 20.0, t::FG),
            tx(crate::i18n::text("As notas do seu vault aparecem aqui como um mapa. Escolha a pasta do vault em Configurações › Memória."), 13.5, t::FG3),
            btn(crate::i18n::text("Abrir configurações de memória"), Kind::Normal, false, None, Some(Msg::OpenSettings(Some(crate::app::Sec::Memoria)))),
        ]
        .spacing(12)
        .align_x(iced::alignment::Horizontal::Center)
        .max_width(420.0);
        return container(card).width(Length::Fill).height(Length::Fill).center_x(Length::Fill).center_y(Length::Fill).into();
    }
    // barra
    let search = stack![
        text_input(crate::i18n::text("Buscar notas, tags ou pastas"), &g.q).on_input(Msg::MemQ).size(13).font(t::UI).padding(pad(8.0, 12.0, 8.0, 34.0)).style(field_style(t::RAISE, t::FIELD_LINE, t::ACCENT, 9.0)),
        container(icon("search", 14.0, t::FG3)).padding(pad(10.0, 0.0, 0.0, 11.0))
    ];
    let mut chips = row![].spacing(4).align_y(Vertical::Center);
    for (i, (_, l, _)) in TIPOS.iter().enumerate() {
        chips = chips.push(fchip(crate::i18n::text(l), Some(TYPE_COLORS[i]), None, g.filt[i], Msg::MemFilter(i)));
    }
    chips = chips.push(fchip(crate::i18n::text("Lidas agora"), None, Some("eye"), g.read_on, Msg::MemRead));
    if app.api.is_some() {
        chips = chips.push(fchip(crate::i18n::text("Sem ligação"), None, None, app.islands_open, Msg::IslandsToggle));
    }
    if let Some((loc_i, depth)) = g.local {
        chips = chips.push(fchip(&format!("Local ({depth}×) ×"), Some(t::ACCENT), Some("graph"), true, Msg::MemLocal(loc_i)));
    }
    let (n, l) = g.stats();
    let total = g.nodes.len();
    let notas = if n < total { crate::i18n::formatted("{} de {} notas", format!("{} de {} notas", fmt_n(n), fmt_n(total))) } else { crate::i18n::formatted("{} notas", format!("{} notas", fmt_n(n))) };
    let pull = if app.api.is_none() { crate::i18n::text(" · git pull às 09:41") } else { "" };
    let stats = tx(crate::i18n::formatted("{notas} · {} links{pull}", format!("{notas} · {} links{pull}", fmt_n(l))), 12.0, t::FG3).wrapping(iced::widget::text::Wrapping::None);
    let avail = app.win.width - if app.collapsed || app.is_drawer_mode() { 0.0 } else { t::SIDE_W + 1.0 };
    let bar: El = if avail < 1280.0 {
        column![row![container(search).width(Length::Fixed(260.0)), chips].spacing(8).align_y(Vertical::Center), row![grow(), stats]].spacing(4).padding(pad(6.0, 16.0, 10.0, 16.0)).into()
    } else {
        row![container(search).width(Length::Fixed(260.0)), chips, grow(), stats].spacing(8).align_y(Vertical::Center).padding(pad(6.0, 16.0, 10.0, 16.0)).into()
    };

    // palco
    let canvas = cv(Mem { g }).width(Length::Fill).height(Length::Fill);
    let mut layers: Vec<El> = vec![canvas.into()];
    if !app.is_drawer_mode() {
        let mut leg = row![].spacing(12);
        for (i, (_, _, s)) in TIPOS.iter().enumerate() {
            leg = leg.push(row![dot_el(TYPE_COLORS[i], 7.0), tx(crate::i18n::text(s), 11.5, t::FG3)].spacing(6).align_y(Vertical::Center));
        }
        layers.push(container(leg).width(Length::Fill).height(Length::Fill).align_y(Vertical::Bottom).padding(pad(0.0, 0.0, 12.0, 12.0)).into());
    }
    let zoom_right = if g.sel.is_some() { 344.0 } else { 12.0 };
    let zoom = container(column![tip(ib("gear", Some(Msg::MemForces), g.forces_open), crate::i18n::text("Forças do grafo")), tip(ib("plus", Some(Msg::MemZoom(1.25)), false), crate::i18n::text("Aproximar")), tip(ib("minus", Some(Msg::MemZoom(0.8)), false), crate::i18n::text("Afastar")), tip(ib_sz("fit", 30.0, 14.0, Some(Msg::MemFit), false), crate::i18n::text("Enquadrar"))].spacing(2)).padding(3).style(|_| container::Style { background: Some(t::mix(t::RAISE, 0.88).into()), border: Border { color: t::LINE2, width: 1.0, radius: t::R_CARD.into() }, ..Default::default() });
    layers.push(container(zoom).width(Length::Fill).height(Length::Fill).align_x(iced::alignment::Horizontal::Right).align_y(Vertical::Bottom).padding(pad(0.0, zoom_right, 12.0, 0.0)).into());
    if app.islands_open {
        let mut list = column![tx(crate::i18n::text("Notas sem link para fora nem vindo de outras notas"), 11.5, t::FG3)].spacing(2);
        match &app.islands {
            None => list = list.push(tx(crate::i18n::text("Carregando…"), 12.5, t::FG3)),
            Some(v) if v.is_empty() => list = list.push(tx(crate::i18n::text("Nenhuma nota solta."), 12.5, t::FG2)),
            Some(v) => {
                for path in v.iter().take(30) {
                    list = list.push(
                        button(tx(ell(path, 300.0, 12.5), 12.5, t::FG))
                            .padding(pad(4.0, 6.0, 4.0, 6.0))
                            .width(Length::Fill)
                            .on_press(Msg::IslandOpen(path.clone()))
                            .style(|_, st| button::Style { background: if matches!(st, button::Status::Hovered) { Some(Background::Color(t::HOVER)) } else { None }, text_color: t::FG, ..Default::default() }),
                    );
                }
                if v.len() > 30 {
                    list = list.push(tx(crate::i18n::formatted("… e mais {}", format!("… e mais {}", v.len() - 30)), 11.5, t::FG3));
                }
            }
        }
        layers.push(container(container(scrollable(list)).width(Length::Fixed(340.0)).max_height(320.0).padding(8).style(|_| container::Style { background: Some(t::RAISE.into()), border: Border { color: t::LINE2, width: 1.0, radius: t::R_CARD.into() }, ..Default::default() })).width(Length::Fill).height(Length::Fill).padding(pad(12.0, 0.0, 0.0, 12.0)).into());
    }
    if g.forces_open {
        layers.push(container(forces_panel(g)).width(Length::Fill).height(Length::Fill).padding(pad(12.0, 0.0, 0.0, 12.0)).into());
    }
    if let Some(i) = g.hov.filter(|i| g.sel != Some(*i)) {
        let n = &g.nodes[i];
        let tip = container(column![tx(ell(&n.label, 260.0, 12.5), 12.5, t::FG), tx(g.meta(i), 11.0, t::FG3)]).padding(pad(6.0, 10.0, 6.0, 10.0)).max_width(280.0).style(|_| container::Style { background: Some(t::RAISE.into()), border: Border { color: t::LINE2, width: 1.0, radius: t::R_CTRL.into() }, shadow: shadow(), ..Default::default() });
        let (w, h) = stage_size(app);
        let (x, y) = ((app.mem_tip.0 + 14.0).max(8.0).min(w - 290.0), (app.mem_tip.1 + 14.0).min(h - 50.0));
        layers.push(container(tip).padding(pad(y, 0.0, 0.0, x)).into());
    }
    if let Some(s) = g.sel {
        layers.push(detail(app, s));
    }
    let stage = container(stack(layers)).width(Length::Fill).height(Length::Fill).clip(true).style(|_| container::Style { border: Border { color: t::LINE, width: 1.0, radius: t::R_PANEL.into() }, ..Default::default() });

    // linha do tempo
    let d0 = g.dates.first().cloned().unwrap_or_default();
    let max = (g.dates.len().saturating_sub(1)).max(1) as f32;
    let sl = slider(0.0..=max, g.cut_idx as f32, Msg::MemCut).step(1.0f32).style(|_, st| {
        let _ = st;
        slider::Style { rail: slider::Rail { backgrounds: (Background::Color(t::ACCENT), Background::Color(t::LINE3)), width: 4.0, border: Border { radius: 2.0.into(), ..Default::default() } }, handle: slider::Handle { shape: slider::HandleShape::Circle { radius: 7.0 }, background: Background::Color(t::ACCENT), border_width: 0.0, border_color: Color::TRANSPARENT } }
    });
    let time = row![tx(dmy(&d0), 12.0, t::FG3), container(sl).width(Length::Fill), tx(crate::i18n::formatted("até {}", format!("até {}", dmy(&g.cut))), 12.0, t::FG3)].spacing(12).align_y(Vertical::Center).padding(pad(10.0, 16.0, 12.0, 16.0));

    let mut root: Vec<El> = vec![column![bar, container(stage).height(Length::Fill).padding(pad(0.0, 12.0, 0.0, 12.0)), time].into()];
    // resultados da busca
    let q = g.q.trim().to_lowercase();
    if !q.is_empty() {
        let mut hits: Vec<usize> = (0..g.nodes.len()).filter(|&i| g.visible(i) && g.matches(i)).collect();
        hits.sort_by_key(|&i| std::cmp::Reverse(g.nodes[i].deg));
        hits.truncate(8);
        let mut list = column![].spacing(0);
        if hits.is_empty() {
            list = list.push(container(tx(crate::i18n::formatted("Nada com “{}”.", format!("Nada com “{}”.", g.q)), 13.0, t::FG3)).padding(pad(8.0, 10.0, 8.0, 10.0)));
        }
        for (k, i) in hits.iter().enumerate() {
            list = list.push(
                button(row![dot_el(TYPE_COLORS[g.nodes[*i].tipo], 7.0), container(tx(ell(&g.nodes[*i].label, 200.0, 13.0), 13.0, t::FG)).width(Length::Fill), tx(crate::i18n::text(TIPOS[g.nodes[*i].tipo].2), 11.5, t::FG3)].spacing(10).align_y(Vertical::Center))
                    .width(Length::Fill)
                    .padding(pad(6.0, 10.0, 6.0, 10.0))
                    .on_press(Msg::MemSelect(Some(*i)))
                    .style(move |_, st| button::Style { background: if k == 0 || matches!(st, button::Status::Hovered) { Some(Background::Color(t::HOVER)) } else { None }, text_color: t::FG, border: Border { radius: t::R_CTRL.into(), ..Default::default() }, shadow: Default::default(), snap: true }),
            );
        }
        let res = container(list).width(Length::Fixed(320.0)).padding(4).style(|_| container::Style { background: Some(t::RAISE.into()), border: Border { color: t::LINE2, width: 1.0, radius: t::R_CARD.into() }, shadow: shadow(), ..Default::default() });
        root.push(container(res).padding(pad(44.0, 0.0, 0.0, 16.0)).into());
    }
    stack(root).width(Length::Fill).height(Length::Fill).into()
}

fn detail(app: &App, i: usize) -> El<'_> {
    let g = &app.mem;
    let n = &g.nodes[i];
    let mut all: Vec<usize> = g.adj[i].iter().copied().filter(|&x| g.visible(x)).collect();
    all.sort_by_key(|&x| std::cmp::Reverse(g.nodes[x].deg));
    let file = format!("{}/{}.md", n.folder, n.label);
    let mut head = row![container(row![dot_el(TYPE_COLORS[n.tipo], 7.0), txf(crate::i18n::text(TIPOS[n.tipo].2), 11.0, t::FG2, t::UI_MED)].spacing(5).align_y(Vertical::Center)).padding(pad(1.0, 8.0, 1.0, 8.0)).style(surface(None, Some(t::LINE2), 999.0))].spacing(8).align_y(Vertical::Center);
    if !n.status.is_empty() {
        head = head.push(badge(&n.status, Tone::Plain));
    }
    head = head.push(grow()).push(tip(ib("close", Some(Msg::MemSelect(None)), false), crate::i18n::text("Fechar detalhes")));
    let mut body = column![txf(n.label.clone(), 20.0, t::FG, t::SERIF), tx(g.meta(i), 12.0, t::FG3), txf(file, 11.5, t::FG3, t::MONO)].spacing(10);
    if !n.tags.is_empty() {
        let mut tg = row![].spacing(4);
        for tag in n.tags.split(' ').take(6) {
            tg = tg.push(badge(&format!("#{tag}"), Tone::Plain));
        }
        body = body.push(tg.wrap().vertical_spacing(4));
    }
    let local_lbl = match g.local {
        Some((c, 1)) if c == i => "Grafo local (1×)",
        Some((c, 2)) if c == i => "Grafo local (2×)",
        _ => crate::i18n::text("Grafo local"),
    };
    let local_kind = if g.local.is_some_and(|(c, _)| c == i) { Kind::Primary } else { Kind::Normal };
    body = body.push(row![btn(crate::i18n::text("Abrir e editar"), Kind::Normal, true, Some("edit"), Some(Msg::MemOpenNote(i))), btn(local_lbl, local_kind, true, Some("graph"), Some(Msg::MemLocal(i)))].spacing(6));
    body = body.push(container(txf(crate::i18n::formatted("Ligada a · {}", format!("Ligada a · {}", all.len())), 11.5, t::FG3, t::UI_MED)).padding(pad(6.0, 0.0, 2.0, 0.0)));
    let mut nl = column![].spacing(1);
    for x in all.iter().take(60) {
        nl = nl.push(
            button(row![dot_el(TYPE_COLORS[g.nodes[*x].tipo], 7.0), tx(ell(&g.nodes[*x].label, 230.0, 13.0), 13.0, t::FG)].spacing(10).align_y(Vertical::Center))
                .width(Length::Fill)
                .padding(pad(6.0, 8.0, 6.0, 8.0))
                .on_press(Msg::MemSelect(Some(*x)))
                .style(|_, st| button::Style { background: if matches!(st, button::Status::Hovered) { Some(Background::Color(t::HOVER)) } else { None }, text_color: t::FG, border: Border { radius: t::R_CTRL.into(), ..Default::default() }, shadow: Default::default(), snap: true }),
        );
    }
    if all.len() > 60 {
        nl = nl.push(container(tx(crate::i18n::formatted("e mais {}", format!("e mais {}", all.len() - 60)), 12.0, t::FG3)).padding(pad(6.0, 8.0, 0.0, 8.0)));
    }
    body = body.push(nl);
    let panel = container(column![container(head).padding(pad(10.0, 10.0, 0.0, 12.0)), scrollable(container(body).padding(pad(8.0, 16.0, 16.0, 16.0))).direction(scrollable::Direction::Vertical(thin_bar())).style(thin_scroll).height(Length::Fill)])
        .width(Length::Fixed(320.0))
        .height(Length::Fill)
        .style(|_| container::Style { background: Some(t::mix(t::RAISE, 0.92).into()), border: Border { color: t::LINE2, width: 1.0, radius: t::R_PANEL.into() }, shadow: shadow(), ..Default::default() });
    container(panel).width(Length::Fill).height(Length::Fill).align_x(iced::alignment::Horizontal::Right).padding(12).into()
}
