//! Dynamic Island do Face ID: cartão flutuante no topo central (REGRAS.md: o Face ID é opt-in, só
//! vale na janela nativa e a senha continua sempre como alternativa).
//!
//! Segue a ilha de voz (`overlays::island`): fundo `ISLAND_BG`, borda `LINE2`, pílula, Geist e as
//! estrelas do cinturão de Orion como glifo. Cores só do tema: `ACCENT` lendo, `OK` no sucesso,
//! `DANGER` no erro. A forma muda por interpolação de tamanho com a curva `ease` do protótipo; o
//! `canvas` só é redesenhado enquanto a ilha anima.

use crate::app::{App, Msg};
use crate::island::{Phase, FaceMsg, Purpose, ERROR_MS, MORPH_MS, SUCCESS_MS};
use crate::net::FaceHint;
use crate::theme as t;
use crate::ui::*;
use iced::alignment::Vertical;
use iced::mouse;
use iced::widget::canvas::{self, path::Arc, Frame, Geometry, LineCap, Path, Stroke};
use iced::widget::{button, canvas as cv, column, container, row};
use iced::{Border, Color, Length, Point, Radians, Rectangle, Renderer, Shadow, Theme, Vector};
use std::f32::consts::{PI, TAU};

/// Mistura linear de duas cores.
fn lerp(a: Color, b: Color, k: f32) -> Color {
    let k = k.clamp(0.0, 1.0);
    Color { r: a.r + (b.r - a.r) * k, g: a.g + (b.g - a.g) * k, b: a.b + (b.b - a.b) * k, a: a.a + (b.a - a.a) * k }
}

/// Estrela no estilo da mini barra de voz: halo de alfa baixo e núcleo.
fn star(f: &mut Frame, c: Point, radius: f32, color: Color, glow: f32) {
    f.fill(&Path::circle(c, radius * 3.0), t::alpha(color, 0.18 * glow));
    f.fill(&Path::circle(c, radius), t::alpha(color, 0.35 + 0.65 * glow));
}

/// O glifo da ilha: o cinturão de Orion (três estrelas) dentro do anel de leitura.
struct Glyph {
    phase: Phase,
    /// Segundos desde o início da fase.
    t: f32,
    hint: FaceHint,
    reduced: bool,
}

impl canvas::Program<Msg> for Glyph {
    type State = ();
    fn draw(&self, _: &(), r: &Renderer, _: &Theme, b: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        let mut f = Frame::new(r, b.size());
        let c = Point::new(b.width / 2.0, b.height / 2.0);
        let radius = b.width.min(b.height) / 2.0 - 2.0;
        let t = if self.reduced { 0.0 } else { self.t };
        let stroke_arc = |f: &mut Frame, from: f32, to: f32, color: Color, width: f32| {
            let p = Path::new(|p| p.arc(Arc { center: c, radius: radius, start_angle: Radians(from), end_angle: Radians(to) }));
            f.stroke(&p, Stroke::default().with_color(color).with_width(width).with_line_cap(LineCap::Round));
        };
        // cinturão inclinado como na marca (28,17) (36,16) (44,15) da mini barra
        let spacing = radius * 0.36;
        let belt = [Point::new(c.x - spacing, c.y + spacing * 0.14), c, Point::new(c.x + spacing, c.y - spacing * 0.14)];
        match self.phase {
            Phase::Standby => {
                for p in belt {
                    star(&mut f, p, 1.5, t::ACCENT, 1.0);
                }
            }
            Phase::Reading | Phase::Enrolling => {
                // trilho discreto e um arco de acento girando, com cauda que some
                stroke_arc(&mut f, 0.0, TAU, t::LINE2, 1.5);
                let color = if self.hint == FaceHint::LowLight { t::SIGNAL } else { t::ACCENT };
                let base = t * 3.6;
                let segments = 24;
                for i in 0..segments {
                    let k = i as f32 / segments as f32;
                    let a0 = base + k * 1.9;
                    stroke_arc(&mut f, a0, a0 + 1.9 / segments as f32 + 0.01, t::alpha(color, 0.08 + 0.92 * k), 1.8);
                }
                // as estrelas acendem em sequência, como um varrimento
                for (n, p) in belt.into_iter().enumerate() {
                    let phase = (t * 2.2 - n as f32 * 0.5).sin() * 0.5 + 0.5;
                    star(&mut f, p, 1.7, t::STAR, 0.35 + 0.65 * phase);
                }
            }
            Phase::Success => {
                let p = crate::anim::ease((t / 0.42).min(1.0));
                stroke_arc(&mut f, -PI / 2.0, -PI / 2.0 + TAU * p, t::OK, 1.8);
                let q = crate::anim::ease(((t - 0.18) / 0.36).clamp(0.0, 1.0));
                if q > 0.0 {
                    let (a, m, z) = (Point::new(c.x - radius * 0.40, c.y + radius * 0.02), Point::new(c.x - radius * 0.1, c.y + radius * 0.32), Point::new(c.x + radius * 0.44, c.y - radius * 0.30));
                    let check = Path::new(|pb| {
                        pb.move_to(a);
                        if q < 0.4 {
                            let k = q / 0.4;
                            pb.line_to(Point::new(a.x + (m.x - a.x) * k, a.y + (m.y - a.y) * k));
                        } else {
                            pb.line_to(m);
                            let k = (q - 0.4) / 0.6;
                            pb.line_to(Point::new(m.x + (z.x - m.x) * k, m.y + (z.y - m.y) * k));
                        }
                    });
                    f.stroke(&check, Stroke::default().with_color(t::OK).with_width(2.0).with_line_cap(LineCap::Round));
                }
            }
            Phase::Error => {
                stroke_arc(&mut f, 0.0, TAU, t::alpha(t::DANGER, 0.8), 1.8);
                let s = radius * 0.32;
                let x = Path::new(|p| {
                    p.move_to(Point::new(c.x - s, c.y - s));
                    p.line_to(Point::new(c.x + s, c.y + s));
                    p.move_to(Point::new(c.x + s, c.y - s));
                    p.line_to(Point::new(c.x - s, c.y + s));
                });
                f.stroke(&x, Stroke::default().with_color(t::DANGER).with_width(1.8).with_line_cap(LineCap::Round));
            }
        }
        vec![f.into_geometry()]
    }
}

fn hint_text(d: FaceHint) -> &'static str {
    match d {
        FaceHint::Starting => "Ligando a câmera…",
        FaceHint::NoFace => "Olhe para a câmera",
        FaceHint::LowLight => "Pouca luz: aproxime-se de uma luz",
        FaceHint::Reading => "Mantenha o rosto parado",
    }
}

/// A ilha. Fica por cima de tudo, centrada no topo; o resto da janela continua clicável.
pub fn view(app: &App) -> El<'_> {
    let i = &app.island;
    let (now, reduced) = (app.now, app.p.reduced);
    let elapsed = now.saturating_duration_since(i.since).as_secs_f32();
    let (mut w, h) = i.current_size(now, reduced);
    // sucesso: pulso curto de expansão; erro: um tremor discreto ao abrir
    let mut dx = 0.0;
    if !reduced {
        match i.phase {
            Phase::Success if elapsed < SUCCESS_MS as f32 / 1000.0 => w += 12.0 * (elapsed / (SUCCESS_MS as f32 / 1000.0) * PI).sin(),
            Phase::Error if elapsed < 0.42 => dx = 5.0 * (elapsed * 46.0).sin() * (1.0 - elapsed / 0.42),
            _ => {}
        }
    }
    let p = crate::anim::progress(Some(i.since), now, MORPH_MS, reduced);
    let open = i.phase != Phase::Standby;
    let name = if app.user.is_empty() { "você".to_string() } else { app.user.clone() };

    let side = if open { 36.0 } else { 20.0 };
    let glyph = cv(Glyph { phase: i.phase, t: elapsed, hint: i.hint, reduced }).width(Length::Fixed(side)).height(Length::Fixed(side));
    // o texto aparece quando a forma já abriu o bastante (sem texto espremido na transição)
    let k = ((p - 0.35) / 0.65).clamp(0.0, 1.0);
    let lines = |a: String, b: String, color_b: Color| column![tx(a, 13.0, t::alpha(t::FG, k)), tx(b, 12.0, t::alpha(color_b, k))].spacing(0).width(Length::Fill);
    let content: El = match i.phase {
        Phase::Standby => row![glyph, tx(crate::i18n::text("Face ID"), 12.0, t::FG2)].spacing(8).align_y(Vertical::Center).into(),
        Phase::Reading | Phase::Enrolling => {
            let title = if i.phase == Phase::Enrolling {
                crate::i18n::text("A cadastrar o seu rosto…").to_string()
            } else {
                crate::i18n::formatted("A verificar {v}…", format!("A verificar {name}…"))
            };
            let request = match (i.phase, i.purpose) {
                (Phase::Enrolling, _) => crate::i18n::text("Vire o rosto devagar"),
                (_, Purpose::Login) => crate::i18n::text(hint_text(i.hint)),
                (_, Purpose::Autonomous) => crate::i18n::text("Confirmar: ligar o modo autônomo"),
                (_, Purpose::Panic) => crate::i18n::text("Confirmar: sair do modo pânico"),
            };
            let request = if i.phase == Phase::Reading && i.purpose != Purpose::Login && i.hint == FaceHint::LowLight { crate::i18n::text(hint_text(i.hint)) } else { request };
            let close_btn = tip(ib("close", Some(Msg::Face(FaceMsg::Cancel)), false), crate::i18n::text("Usar a senha"));
            row![glyph, lines(title, request.into(), t::FG3), close_btn].spacing(12).align_y(Vertical::Center).into()
        }
        Phase::Success => {
            let (a, b) = match i.purpose {
                Purpose::Login => (crate::i18n::formatted("Olá, {v}", format!("Olá, {name}")), crate::i18n::text("A abrir o Orion").to_string()),
                _ => (crate::i18n::text("Confirmado").to_string(), crate::i18n::text("Rosto conferido").to_string()),
            };
            row![glyph, lines(a, b, t::OK)].spacing(12).align_y(Vertical::Center).into()
        }
        Phase::Error => {
            let reason = i.failure.as_ref().map(crate::island::failure_text).unwrap_or("Rosto não reconhecido");
            row![glyph, lines(crate::i18n::text(reason).into(), crate::i18n::text("Digite a senha mestre").into(), t::DANGER)].spacing(12).align_y(Vertical::Center).into()
        }
    };

    // sombra da casa; o acento só tinge de leve a borda quando algo acontece
    let border_color = match i.phase {
        Phase::Reading | Phase::Enrolling => lerp(t::LINE2, t::alpha(t::ACCENT, 0.35), 0.5 + 0.5 * (elapsed * 2.4).sin()),
        Phase::Success => lerp(t::LINE2, t::alpha(t::OK, 0.6), 1.0 - (elapsed / 0.9).min(1.0)),
        Phase::Error => lerp(t::LINE2, t::alpha(t::DANGER, 0.55), 1.0 - (elapsed / (ERROR_MS as f32 / 1000.0)).min(1.0)),
        Phase::Standby => t::LINE2,
    };
    let base = shadow();
    let radius = h / 2.0;
    let pill = container(content)
        .width(Length::Fixed(w))
        .height(Length::Fixed(h))
        .padding(pad(0.0, if open { 8.0 } else { 14.0 }, 0.0, if open { 14.0 } else { 10.0 }))
        .center_y(Length::Fixed(h))
        .clip(true)
        .style(move |_| container::Style {
            background: Some(t::ISLAND_BG.into()),
            border: Border { color: border_color, width: 1.0, radius: radius.into() },
            shadow: Shadow { color: base.color, offset: base.offset, blur_radius: base.blur_radius },
            ..Default::default()
        });
    // em espera, a pílula inteira é o botão de ler de novo (só na tela de entrada)
    let island_el: El = if i.phase == Phase::Standby && !app.logged {
        tip(button(pill).padding(0).on_press(Msg::Face(FaceMsg::Start(Purpose::Login))).style(|_, _| button::Style::default()), crate::i18n::text("Entrar com Face ID"))
    } else {
        pill.into()
    };
    let top = if app.logged { 10.0 } else { t::BAR_H + 8.0 };
    let island_el = iced::widget::float(island_el).translate(move |_, _| Vector::new(dx, 0.0));
    // HUD da câmera logo abaixo da ilha (login e cadastro), entrando com a mesma curva
    let hud: El = match (&i.preview, i.shows_hud()) {
        (Some((preview, handle)), true) => {
            let tone = match i.phase {
                Phase::Success => t::OK,
                Phase::Error => t::DANGER,
                _ => t::ACCENT,
            };
            let tag = (i.phase == Phase::Success).then(|| name.to_uppercase());
            rise(fade(crate::views::camera_hud::view(preview, handle, tone, tag, elapsed, reduced), k, t::BG), k, 8.0)
        }
        _ => sp_h(0.0),
    };
    container(column![island_el, hud].spacing(10).align_x(iced::alignment::Horizontal::Center))
        .width(Length::Fill)
        .center_x(Length::Fill)
        .padding(pad(top, 0.0, 0.0, 0.0))
        .into()
}

/// Pedido de senha para cadastrar o Face ID (o cadastro guarda a senha no cofre do sistema).
pub fn enroll_password_modal(app: &App) -> El<'_> {
    let v = app.island.enroll_password.as_deref().unwrap_or("");
    let input = iced::widget::text_input(crate::i18n::text("Sua senha"), v)
        .id("faceid-password")
        .on_input(|s| Msg::Face(FaceMsg::EnrollPassword(s)))
        .on_submit(Msg::Face(FaceMsg::EnrollSubmit))
        .secure(true)
        .size(t::snap_size(15.0))
        .font(t::UI)
        .padding(pad(12.0, 12.0, 12.0, 12.0));
    let body = column![
        tx(crate::i18n::text("Ativar o Face ID"), 15.0, t::FG),
        tx(crate::i18n::text("Digite a senha. Depois olhe para a câmera por alguns segundos. O rosto vira só um vetor cifrado neste computador; a senha fica no cofre do sistema e o Face ID a libera ao reconhecer você. Uma foto boa do seu rosto pode enganar uma câmera comum: use onde só você tem acesso."), 12.0, t::FG3),
        input,
        row![grow(), btn(crate::i18n::text("Cancelar"), Kind::Normal, true, None, Some(Msg::Face(FaceMsg::EnrollClose))), btn(crate::i18n::text("Cadastrar"), Kind::Primary, !v.is_empty(), None, Some(Msg::Face(FaceMsg::EnrollSubmit)))].spacing(8),
    ]
    .spacing(12);
    crate::views::overlays::password_modal(app, body.into(), Msg::Face(FaceMsg::EnrollClose))
}
