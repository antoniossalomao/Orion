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

/// Entrada no estilo do instalador: céu à esquerda, painel com campos à direita.
pub fn view(app: &App) -> El<'_> {
    let code = app.challenge.is_some();
    let estilo = field_style(t::RAISE, t::LINE, t::ACCENT, 8.0);
    let usuario = text_input("usuário", &app.user).on_input(Msg::UserChanged).on_submit(Msg::Login).size(13).font(t::MONO).padding(pad(9.0, 11.0, 9.0, 11.0)).style(estilo);
    let senha = text_input(if code { "código do Telegram" } else { "senha" }, &app.pw).on_input(Msg::PwChanged).on_submit(Msg::Login).secure(!code).size(13).font(t::MONO).padding(pad(9.0, 11.0, 9.0, 11.0)).style(field_style(t::RAISE, t::LINE, t::ACCENT, 8.0));
    let neutro = app.gate_err.is_empty() || app.gate_err.starts_with("Conectando") || app.gate_err.starts_with("Entrando") || app.gate_err.starts_with("Abrindo") || app.gate_err.starts_with("Digite a senha");
    let aviso = container(tx(if app.gate_err.starts_with("Digite a senha") { String::new() } else { app.gate_err.clone() }, 12.0, if neutro { t::FG3 } else { t::DANGER })).height(Length::Fixed(16.0));
    let rotulo = |s: &'static str| txf(s, 12.0, t::FG2, t::UI);
    let mut campos = column![].spacing(14);
    if !code {
        campos = campos.push(column![rotulo("Usuário"), usuario].spacing(6));
    }
    campos = campos.push(column![rotulo(if code { "Código" } else { "Senha" }), senha].spacing(6));
    let corpo = column![
        txf(spaced("entrar", 1).to_uppercase(), 11.0, t::FG3, t::MONO),
        txf(if code { "Confirme sua identidade" } else { "Bem-vindo de volta" }, 24.0, t::FG, t::UI_MED),
        tx(if code { "Digite o código de uso único que o Orion mandou ao seu Telegram." } else { "Seu assistente de desktop. Tudo fica neste computador." }, 14.0, t::FG2),
        sp_h(4.0),
        campos,
        aviso,
    ]
    .spacing(10)
    .width(Length::Fill);
    let rodape = column![
        hline(),
        row![txf(if app.db_locked { "A mesma senha abre o banco cifrado" } else { "A senha volta quando o Windows bloqueia" }, 11.0, t::FG3, t::MONO), grow(), btn("Entrar", Kind::Primary, false, None, Some(Msg::Login))].spacing(8).align_y(iced::alignment::Vertical::Center),
    ]
    .spacing(14);
    let painel = container(column![container(container(corpo).max_width(380.0)).height(Length::Fill).center_y(Length::Fill), rodape].width(Length::Fill)).padding(pad(28.0, 28.0, 20.0, 28.0)).width(Length::FillPortion(105)).height(Length::Fill).style(|_| container::Style { background: Some(t::SIDE.into()), ..Default::default() });
    let sky = canvas(Sky { alpha: 1.0, constellation: true, belt: 0.0 }).width(Length::Fill).height(Length::Fill);
    let legenda = container(row![txf("Orion", 22.0, t::FG, t::UI_MED), grow(), column![txf("Betelgeuse", 11.0, t::SIGNAL, t::MONO), txf(concat!("v", env!("CARGO_PKG_VERSION")), 11.0, t::FG3, t::MONO)].align_x(Horizontal::Right)].align_y(iced::alignment::Vertical::Bottom)).width(Length::Fill).height(Length::Fill).align_bottom(Length::Fill).padding(pad(0.0, 22.0, 20.0, 22.0));
    let esq = container(stack![sky, legenda]).width(Length::FillPortion(100)).height(Length::Fill).style(|_| container::Style { background: Some(t::BG.into()), ..Default::default() });
    let largo = app.win.width >= 760.0;
    let mut r = row![].width(Length::Fill).height(Length::Fill);
    if largo {
        r = r.push(esq).push(container(sp_w(1.0)).width(Length::Fixed(1.0)).height(Length::Fill).style(|_| container::Style { background: Some(t::LINE.into()), ..Default::default() }));
    }
    r.push(painel).into()
}
