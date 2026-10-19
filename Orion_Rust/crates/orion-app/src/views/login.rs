use crate::app::{App, Msg};
use crate::sky::Sky;
use crate::theme as t;
use crate::ui::*;
use iced::alignment::Horizontal;
use iced::widget::{canvas, column, container, stack, text_input};
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

pub fn view(app: &App) -> El<'_> {
    let sky = canvas(Sky { alpha: 1.0, constellation: true, belt: 0.0 }).width(Length::Fill).height(Length::Fill);
    let code = app.challenge.is_some();
    let field = text_input(if code { "Código do Telegram" } else { "Senha" }, &app.pw)
        .on_input(Msg::PwChanged)
        .on_submit(Msg::Login)
        .secure(!code)
        .size(14)
        .font(t::UI)
        .padding(iced::Padding { top: 11.0, bottom: 11.0, left: 14.0, right: 14.0 })
        .style(field_style(t::mix(t::RAISE, 0.85), t::LINE2, t::LINE3, 10.0));
    let btn = btn_wide("Entrar", Msg::Login);
    let card = column![
        column![belt_fg(true), txf("Orion", 40.0, t::FG, t::SERIF).line_height(iced::widget::text::LineHeight::Relative(1.0))].spacing(4),
        tx("Entre com a senha deste computador. Nada sai daqui sem a sua aprovação.", 13.5, t::FG2),
        field,
        container(tx(app.gate_err.clone(), 12.0, if app.gate_err.starts_with("Conectando") || app.gate_err.starts_with("Entrando") { t::FG3 } else { t::DANGER })).height(Length::Fixed(18.0)),
        btn,
        tx(match &app.api { Some(a) => format!("Servidor: {}. A senha volta a ser pedida quando o Windows é bloqueado.", a.base), None => "Protótipo com dados de exemplo. Qualquer senha entra. A senha volta a ser pedida quando o Windows é bloqueado.".into() }, 12.0, t::FG3),
    ]
    .spacing(14)
    .width(Length::Fixed(340.0));
    let c = container(card).width(Length::Fill).height(Length::Fill).center_x(Length::Fill).center_y(Length::Fill);
    container(stack![sky, c]).width(Length::Fill).height(Length::Fill).style(|_| container::Style { background: Some(t::BG.into()), ..Default::default() }).into()
}

fn btn_wide<'a>(label: &str, msg: Msg) -> El<'a> {
    let _ = Horizontal::Left;
    iced::widget::button(container(txf(label.to_string(), 14.0, t::PRIMARY_INK, t::UI_MED)).width(Length::Fill).center_x(Length::Fill).center_y(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fixed(42.0))
        .padding(0)
        .on_press(msg)
        .style(|_, st| iced::widget::button::Style {
            background: Some(if matches!(st, iced::widget::button::Status::Hovered) { iced::Color { r: t::PRIMARY.r * 1.06, g: t::PRIMARY.g * 1.06, b: t::PRIMARY.b * 1.06, a: 1.0 } } else { t::PRIMARY }.into()),
            text_color: t::PRIMARY_INK,
            border: Border { radius: 8.0.into(), ..Default::default() },
            shadow: Default::default(),
            snap: true,
        })
        .into()
}
