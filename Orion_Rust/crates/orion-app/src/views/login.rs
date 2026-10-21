use crate::app::{App, Msg};
use crate::sky::Sky;
use crate::theme as t;
use crate::ui::*;
use iced::alignment::Horizontal;
use iced::widget::{canvas, column, container, row, stack, text_input};
use iced::{Border, Length};

pub fn field_style(bg: iced::Color, line: iced::Color, focus: iced::Color, radius: f32) -> impl Fn(&iced::Theme, text_input::Status) -> text_input::Style {
    move |_, st| text_input::Style {
        background: bg.into(),
        border: Border { color: if matches!(st, text_input::Status::Focused { .. }) { focus } else { line }, width: 1.0, radius: radius.into() },
        icon: t::FG3,
        placeholder: t::FG3,
        value: t::FG,
        selection: t::ACCENT_SOFT,
    }
}

/// Brilhos suaves atrás da entrada (círculos concêntricos de alfa baixo imitam desfoque) e à deriva.
struct Brilho {
    t: f32,
    /// 0 = só o céu (esquerda); 1 = painel (direita).
    lado: u8,
    p: f32,
}

impl canvas::Program<Msg> for Brilho {
    type State = ();

    fn draw(&self, _s: &(), renderer: &iced::Renderer, _th: &iced::Theme, b: iced::Rectangle, _c: iced::mouse::Cursor) -> Vec<canvas::Geometry> {
        let mut f = canvas::Frame::new(renderer, b.size());
        let (w, h) = (b.width, b.height);
        let blobs: [(f32, f32, f32, iced::Color, f32); 3] = if self.lado == 0 {
            [(0.28, 0.24, 0.50, t::SIGNAL, 0.055), (0.55, 0.88, 0.58, t::ACCENT, 0.07), (0.45, 0.52, 0.48, iced::Color::from_rgb(0.55, 0.45, 0.85), 0.05)]
        } else {
            [(0.92, 0.08, 0.70, t::ACCENT, 0.08), (0.10, 0.95, 0.55, t::SIGNAL, 0.04), (0.55, 0.45, 0.50, iced::Color::from_rgb(0.55, 0.45, 0.85), 0.03)]
        };
        for (n, (x, y, r, c, a)) in blobs.into_iter().enumerate() {
            let k = n as f32 * 1.7;
            let cx = (x + 0.03 * (self.t * 0.21 + k).sin()) * w;
            let cy = (y + 0.03 * (self.t * 0.17 + k * 1.3).cos()) * h;
            let raio = r * w.max(h) * (1.0 + 0.04 * (self.t * 0.3 + k).sin());
            for s in 0..44 {
                let q = s as f32 / 44.0;
                f.fill(&canvas::Path::circle(iced::Point::new(cx, cy), raio * (1.0 - q * 0.92)), t::alpha(c, a * self.p * 1.1 / 44.0));
            }
        }
        vec![f.into_geometry()]
    }
}

fn degrade(de: iced::Color, ate: iced::Color, graus: f32) -> iced::Background {
    iced::gradient::Linear::new(iced::Radians(graus.to_radians())).add_stop(0.0, de).add_stop(1.0, ate).into()
}

/// Entrada: céu à esquerda, vidro fosco com os campos à direita, tudo entrando em sequência.
pub fn view(app: &App) -> El<'_> {
    let code = app.challenge.is_some();
    let seg = |ms: u64| crate::anim::progress(Some(app.login_at + std::time::Duration::from_millis(ms)), app.now, 900, app.p.reduced);
    let tempo = app.now.saturating_duration_since(app.login_at).as_secs_f32();
    let (p_ceu, p_card, p_user, p_senha, p_btn) = (seg(0), seg(150), seg(300), seg(420), seg(540));

    let campo = |ph: &'static str, v: &str, f: fn(String) -> Msg, secure: bool| {
        text_input(ph, v)
            .on_input(f)
            .on_submit(Msg::Login)
            .secure(secure)
            .size(14)
            .font(t::UI)
            .padding(pad(12.0, 14.0, 12.0, 14.0))
            .style(field_style(t::alpha(t::BG, 0.55), t::LINE, t::ACCENT, 10.0))
    };
    let neutro = app.gate_err.is_empty() || app.gate_err.starts_with("Conectando") || app.gate_err.starts_with("Entrando") || app.gate_err.starts_with("Abrindo") || app.gate_err.starts_with("Digite a senha");
    let aviso = container(tx(if app.gate_err.starts_with("Digite a senha") { String::new() } else { app.gate_err.clone() }, 12.0, if neutro { t::FG3 } else { t::DANGER })).height(Length::Fixed(16.0));

    let cab = column![belt_fg(false), txf(if code { "Confirme sua identidade" } else { "Bem-vindo de volta" }, 26.0, t::FG, t::SERIF)].spacing(10).align_x(Horizontal::Center);

    let mut campos = column![].spacing(10).width(Length::Fill);
    let bg = t::SIDE;
    if !code {
        campos = campos.push(rise(fade(campo("Usuário", &app.user, Msg::UserChanged, false), p_user, bg), p_user, 10.0));
    }
    campos = campos.push(rise(fade(campo(if code { "Código do Telegram" } else { "Senha" }, &app.pw, Msg::PwChanged, !code), p_senha, bg), p_senha, 10.0));

    let entrar = iced::widget::button(container(row![txf("Entrar", 14.0, t::PRIMARY_INK, t::UI_MED), crate::icons::icon("chevR", 14.0, t::PRIMARY_INK)].spacing(8).align_y(iced::alignment::Vertical::Center)).width(Length::Fill).center_x(Length::Fill).center_y(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fixed(44.0))
        .padding(0)
        .on_press(Msg::Login)
        .style(move |_, st| {
            let hov = matches!(st, iced::widget::button::Status::Hovered | iced::widget::button::Status::Pressed);
            let k = if hov { 1.06 } else { 1.0 };
            let a = iced::Color { r: (t::PRIMARY.r * k).min(1.0), g: (t::PRIMARY.g * k).min(1.0), b: (t::PRIMARY.b * k).min(1.0), a: 1.0 };
            iced::widget::button::Style {
                background: Some(degrade(a, t::mix(t::ACCENT, 0.9), 100.0)),
                text_color: t::PRIMARY_INK,
                border: Border { radius: 10.0.into(), ..Default::default() },
                shadow: iced::Shadow { color: t::alpha(t::ACCENT, if hov { 0.22 } else { 0.10 }), offset: iced::Vector::new(0.0, 4.0), blur_radius: 16.0 },
                snap: true,
            }
        });

    let hello=if app.api.as_ref().is_some_and(|api|api.hello_ativo()) {
        btn("Entrar com Windows Hello",Kind::Normal,false,None,Some(Msg::Hello))
    }else{sp_h(0.0)};
    let cartao = container(column![cab, sp_h(14.0), campos, aviso, rise(fade(entrar, p_btn, bg), p_btn, 10.0),hello,if code{btn("Voltar à senha",Kind::Ghost,true,None,Some(Msg::LoginCancel))}else{sp_h(0.0)}].spacing(8).align_x(Horizontal::Center).width(Length::Fill))
        .padding(pad(30.0, 28.0, 26.0, 28.0))
        .width(Length::Fixed(380.0))
        .style(|_| container::Style {
            background: Some(degrade(t::alpha(iced::Color::WHITE, 0.035), t::alpha(iced::Color::WHITE, 0.012), 160.0)),
            border: Border { color: t::alpha(iced::Color::WHITE, 0.05), width: 1.0, radius: 18.0.into() },
            shadow: iced::Shadow { color: t::alpha(iced::Color::BLACK, 0.25), offset: iced::Vector::new(0.0, 16.0), blur_radius: 40.0 },
            ..Default::default()
        });
    let rodape = txf(if app.db_locked { "Sua senha protege o Orion e suas cópias de segurança" } else { "Tudo fica neste computador" }, 11.0, t::FG3, t::MONO);

    let glow_dir = canvas(Brilho { t: tempo, lado: 1, p: p_ceu }).width(Length::Fill).height(Length::Fill);
    let conteudo = container(column![rise(fade(cartao, p_card, bg), p_card, 18.0), sp_h(18.0), fade(rodape, p_card, bg)].align_x(Horizontal::Center)).width(Length::Fill).height(Length::Fill).center_x(Length::Fill).center_y(Length::Fill);
    let painel = container(stack![glow_dir, conteudo]).width(Length::FillPortion(105)).height(Length::Fill).clip(true).style(|_| container::Style { background: Some(degrade(t::mix(t::SIDE, 1.0), iced::Color::from_rgb(0.075, 0.078, 0.095), 160.0)), ..Default::default() });

    let sky = canvas(Sky { alpha: 1.0, constellation: true, belt: 0.0, zoom: 1.0 }).width(Length::Fill).height(Length::Fill);
    let glow_esq = canvas(Brilho { t: tempo, lado: 0, p: p_ceu }).width(Length::Fill).height(Length::Fill);
    let marca = container(row![column![belt_fg(false), txf("Orion", 24.0, t::FG, t::SERIF)].spacing(4), grow(), txf(concat!("v", env!("CARGO_PKG_VERSION")), 11.0, t::FG3, t::MONO)].align_y(iced::alignment::Vertical::Bottom)).width(Length::Fill).height(Length::Fill).align_bottom(Length::Fill).padding(pad(0.0, 26.0, 24.0, 28.0));
    let esq = container(fade(stack![sky, glow_esq, marca], p_ceu, t::BG)).width(Length::FillPortion(100)).height(Length::Fill).clip(true).style(|_| container::Style { background: Some(t::BG.into()), ..Default::default() });
    let divisor = container(sp_w(1.0)).width(Length::Fixed(1.0)).height(Length::Fill).style(|_| container::Style { background: Some(degrade(t::alpha(iced::Color::WHITE, 0.0), t::alpha(iced::Color::WHITE, 0.10), 0.0)), ..Default::default() });

    let largo = app.win.width >= 760.0;
    let mut r = row![].width(Length::Fill).height(Length::Fill);
    if largo {
        r = r.push(esq).push(divisor);
    }
    r.push(painel).into()
}
