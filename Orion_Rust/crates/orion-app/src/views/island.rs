//! Dynamic Island do Face ID: cartão flutuante no topo central (REGRAS.md: o Face ID é opt-in, só
//! vale na janela nativa e a senha continua sempre como alternativa).
//!
//! Segue a ilha de voz (`overlays::island`): fundo `ISLAND_BG`, borda `LINE2`, pílula, Geist e as
//! estrelas do cinturão de Orion como glifo. Cores só do tema: `ACCENT` lendo, `OK` no sucesso,
//! `DANGER` no erro. A forma muda por interpolação de tamanho com a curva `ease` do protótipo; o
//! `canvas` só é redesenhado enquanto a ilha anima.

use crate::app::{App, Msg};
use crate::island::{Fase, FaceMsg, Proposito, ERRO_MS, MORPH_MS, SUCESSO_MS};
use crate::net::FaceDica;
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
fn estrela(f: &mut Frame, c: Point, raio: f32, cor: Color, brilho: f32) {
    f.fill(&Path::circle(c, raio * 3.0), t::alpha(cor, 0.18 * brilho));
    f.fill(&Path::circle(c, raio), t::alpha(cor, 0.35 + 0.65 * brilho));
}

/// O glifo da ilha: o cinturão de Orion (três estrelas) dentro do anel de leitura.
struct Glifo {
    fase: Fase,
    /// Segundos desde o início da fase.
    t: f32,
    dica: FaceDica,
    reduced: bool,
}

impl canvas::Program<Msg> for Glifo {
    type State = ();
    fn draw(&self, _: &(), r: &Renderer, _: &Theme, b: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        let mut f = Frame::new(r, b.size());
        let c = Point::new(b.width / 2.0, b.height / 2.0);
        let raio = b.width.min(b.height) / 2.0 - 2.0;
        let t = if self.reduced { 0.0 } else { self.t };
        let arco = |f: &mut Frame, de: f32, ate: f32, cor: Color, largura: f32| {
            let p = Path::new(|p| p.arc(Arc { center: c, radius: raio, start_angle: Radians(de), end_angle: Radians(ate) }));
            f.stroke(&p, Stroke::default().with_color(cor).with_width(largura).with_line_cap(LineCap::Round));
        };
        // cinturão inclinado como na marca (28,17) (36,16) (44,15) da mini barra
        let passo = raio * 0.36;
        let cinto = [Point::new(c.x - passo, c.y + passo * 0.14), c, Point::new(c.x + passo, c.y - passo * 0.14)];
        match self.fase {
            Fase::Standby => {
                for p in cinto {
                    estrela(&mut f, p, 1.5, t::ACCENT, 1.0);
                }
            }
            Fase::Lendo | Fase::Cadastro => {
                // trilho discreto e um arco de acento girando, com cauda que some
                arco(&mut f, 0.0, TAU, t::LINE2, 1.5);
                let cor = if self.dica == FaceDica::PoucaLuz { t::SIGNAL } else { t::ACCENT };
                let base = t * 3.6;
                let passos = 24;
                for i in 0..passos {
                    let k = i as f32 / passos as f32;
                    let a0 = base + k * 1.9;
                    arco(&mut f, a0, a0 + 1.9 / passos as f32 + 0.01, t::alpha(cor, 0.08 + 0.92 * k), 1.8);
                }
                // as estrelas acendem em sequência, como um varrimento
                for (n, p) in cinto.into_iter().enumerate() {
                    let fase = (t * 2.2 - n as f32 * 0.5).sin() * 0.5 + 0.5;
                    estrela(&mut f, p, 1.7, t::STAR, 0.35 + 0.65 * fase);
                }
            }
            Fase::Sucesso => {
                let p = crate::anim::ease((t / 0.42).min(1.0));
                arco(&mut f, -PI / 2.0, -PI / 2.0 + TAU * p, t::OK, 1.8);
                let q = crate::anim::ease(((t - 0.18) / 0.36).clamp(0.0, 1.0));
                if q > 0.0 {
                    let (a, m, z) = (Point::new(c.x - raio * 0.40, c.y + raio * 0.02), Point::new(c.x - raio * 0.1, c.y + raio * 0.32), Point::new(c.x + raio * 0.44, c.y - raio * 0.30));
                    let visto = Path::new(|pb| {
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
                    f.stroke(&visto, Stroke::default().with_color(t::OK).with_width(2.0).with_line_cap(LineCap::Round));
                }
            }
            Fase::Erro => {
                arco(&mut f, 0.0, TAU, t::alpha(t::DANGER, 0.8), 1.8);
                let s = raio * 0.32;
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

fn dica_texto(d: FaceDica) -> &'static str {
    match d {
        FaceDica::Iniciando => "Ligando a câmera…",
        FaceDica::SemRosto => "Olhe para a câmera",
        FaceDica::PoucaLuz => "Pouca luz: aproxime-se de uma luz",
        FaceDica::Lendo => "Mantenha o rosto parado",
    }
}

/// A ilha. Fica por cima de tudo, centrada no topo; o resto da janela continua clicável.
pub fn view(app: &App) -> El<'_> {
    let i = &app.island;
    let (now, reduced) = (app.now, app.p.reduced);
    let decorrido = now.saturating_duration_since(i.desde).as_secs_f32();
    let (mut w, h) = i.tamanho_atual(now, reduced);
    // sucesso: pulso curto de expansão; erro: um tremor discreto ao abrir
    let mut dx = 0.0;
    if !reduced {
        match i.fase {
            Fase::Sucesso if decorrido < SUCESSO_MS as f32 / 1000.0 => w += 12.0 * (decorrido / (SUCESSO_MS as f32 / 1000.0) * PI).sin(),
            Fase::Erro if decorrido < 0.42 => dx = 5.0 * (decorrido * 46.0).sin() * (1.0 - decorrido / 0.42),
            _ => {}
        }
    }
    let p = crate::anim::progress(Some(i.desde), now, MORPH_MS, reduced);
    let aberta = i.fase != Fase::Standby;
    let nome = if app.user.is_empty() { "você".to_string() } else { app.user.clone() };

    let lado = if aberta { 36.0 } else { 20.0 };
    let glifo = cv(Glifo { fase: i.fase, t: decorrido, dica: i.dica, reduced }).width(Length::Fixed(lado)).height(Length::Fixed(lado));
    // o texto aparece quando a forma já abriu o bastante (sem texto espremido na transição)
    let k = ((p - 0.35) / 0.65).clamp(0.0, 1.0);
    let linhas = |a: String, b: String, cor_b: Color| column![tx(a, 13.0, t::alpha(t::FG, k)), tx(b, 12.0, t::alpha(cor_b, k))].spacing(0).width(Length::Fill);
    let conteudo: El = match i.fase {
        Fase::Standby => row![glifo, tx(crate::i18n::text("Face ID"), 12.0, t::FG2)].spacing(8).align_y(Vertical::Center).into(),
        Fase::Lendo | Fase::Cadastro => {
            let titulo = if i.fase == Fase::Cadastro {
                crate::i18n::text("A cadastrar o seu rosto…").to_string()
            } else {
                crate::i18n::formatted("A verificar {v}…", format!("A verificar {nome}…"))
            };
            let pedido = match (i.fase, i.proposito) {
                (Fase::Cadastro, _) => crate::i18n::text("Vire o rosto devagar"),
                (_, Proposito::Login) => crate::i18n::text(dica_texto(i.dica)),
                (_, Proposito::Autonomo) => crate::i18n::text("Confirmar: ligar o modo autônomo"),
                (_, Proposito::Panico) => crate::i18n::text("Confirmar: sair do modo pânico"),
            };
            let pedido = if i.fase == Fase::Lendo && i.proposito != Proposito::Login && i.dica == FaceDica::PoucaLuz { crate::i18n::text(dica_texto(i.dica)) } else { pedido };
            let fechar = tip(ib("close", Some(Msg::Face(FaceMsg::Cancelar)), false), crate::i18n::text("Usar a senha"));
            row![glifo, linhas(titulo, pedido.into(), t::FG3), fechar].spacing(12).align_y(Vertical::Center).into()
        }
        Fase::Sucesso => {
            let (a, b) = match i.proposito {
                Proposito::Login => (crate::i18n::formatted("Olá, {v}", format!("Olá, {nome}")), crate::i18n::text("A abrir o Orion").to_string()),
                _ => (crate::i18n::text("Confirmado").to_string(), crate::i18n::text("Rosto conferido").to_string()),
            };
            row![glifo, linhas(a, b, t::OK)].spacing(12).align_y(Vertical::Center).into()
        }
        Fase::Erro => {
            let motivo = i.falha.as_ref().map(crate::island::texto_falha).unwrap_or("Rosto não reconhecido");
            row![glifo, linhas(crate::i18n::text(motivo).into(), crate::i18n::text("Digite a senha mestre").into(), t::DANGER)].spacing(12).align_y(Vertical::Center).into()
        }
    };

    // sombra da casa; o acento só tinge de leve a borda quando algo acontece
    let borda = match i.fase {
        Fase::Lendo | Fase::Cadastro => lerp(t::LINE2, t::alpha(t::ACCENT, 0.35), 0.5 + 0.5 * (decorrido * 2.4).sin()),
        Fase::Sucesso => lerp(t::LINE2, t::alpha(t::OK, 0.6), 1.0 - (decorrido / 0.9).min(1.0)),
        Fase::Erro => lerp(t::LINE2, t::alpha(t::DANGER, 0.55), 1.0 - (decorrido / (ERRO_MS as f32 / 1000.0)).min(1.0)),
        Fase::Standby => t::LINE2,
    };
    let base = shadow();
    let raio = h / 2.0;
    let pilula = container(conteudo)
        .width(Length::Fixed(w))
        .height(Length::Fixed(h))
        .padding(pad(0.0, if aberta { 8.0 } else { 14.0 }, 0.0, if aberta { 14.0 } else { 10.0 }))
        .center_y(Length::Fixed(h))
        .clip(true)
        .style(move |_| container::Style {
            background: Some(t::ISLAND_BG.into()),
            border: Border { color: borda, width: 1.0, radius: raio.into() },
            shadow: Shadow { color: base.color, offset: base.offset, blur_radius: base.blur_radius },
            ..Default::default()
        });
    // em espera, a pílula inteira é o botão de ler de novo (só na tela de entrada)
    let ilha: El = if i.fase == Fase::Standby && !app.logged {
        tip(button(pilula).padding(0).on_press(Msg::Face(FaceMsg::Iniciar(Proposito::Login))).style(|_, _| button::Style::default()), crate::i18n::text("Entrar com Face ID"))
    } else {
        pilula.into()
    };
    let topo = if app.logged { 10.0 } else { t::BAR_H + 8.0 };
    let ilha = iced::widget::float(ilha).translate(move |_, _| Vector::new(dx, 0.0));
    container(ilha)
        .width(Length::Fill)
        .center_x(Length::Fill)
        .padding(pad(topo, 0.0, 0.0, 0.0))
        .into()
}

/// Pedido de senha para cadastrar o Face ID (o cadastro guarda a senha no cofre do sistema).
pub fn cadastro_senha(app: &App) -> El<'_> {
    let v = app.island.senha_cadastro.as_deref().unwrap_or("");
    let input = iced::widget::text_input(crate::i18n::text("Sua senha"), v)
        .id("faceid-password")
        .on_input(|s| Msg::Face(FaceMsg::CadastroSenha(s)))
        .on_submit(Msg::Face(FaceMsg::CadastroOk))
        .secure(true)
        .size(t::snap_size(15.0))
        .font(t::UI)
        .padding(pad(12.0, 12.0, 12.0, 12.0));
    let corpo = column![
        tx(crate::i18n::text("Ativar o Face ID"), 15.0, t::FG),
        tx(crate::i18n::text("Digite a senha. Depois olhe para a câmera por alguns segundos. O rosto vira só um vetor cifrado neste computador; a senha fica no cofre do sistema e o Face ID a libera ao reconhecer você. Uma foto boa do seu rosto pode enganar uma câmera comum: use onde só você tem acesso."), 12.0, t::FG3),
        input,
        row![grow(), btn(crate::i18n::text("Cancelar"), Kind::Normal, true, None, Some(Msg::Face(FaceMsg::CadastroFechar))), btn(crate::i18n::text("Cadastrar"), Kind::Primary, !v.is_empty(), None, Some(Msg::Face(FaceMsg::CadastroOk)))].spacing(8),
    ]
    .spacing(12);
    crate::views::overlays::modal_senha(app, corpo.into(), Msg::Face(FaceMsg::CadastroFechar))
}
