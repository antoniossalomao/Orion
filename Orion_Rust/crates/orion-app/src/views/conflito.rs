//! Conflito do vault (F06.08): a nota que está no vault ao lado da versão nova do Orion, mais um
//! campo com o texto final. Nada é gravado sem o seu clique (REGRAS.md: você é o administrador).

use crate::app::{App, Msg};
use crate::icons::icon;
use crate::theme as t;
use crate::ui::*;
use iced::alignment::Vertical;
use iced::widget::{column, container, mouse_area, opaque, row, scrollable, text, text_editor};
use iced::{Background, Border, Length};

fn painel<'a>(titulo: &'a str, texto: &'a str) -> El<'a> {
    let corpo = scrollable(container(txf(texto.to_string(), 12.5, t::FG, t::MONO)).padding(14).width(Length::Fill))
        .direction(scrollable::Direction::Vertical(thin_bar()))
        .style(thin_scroll)
        .height(Length::Fill);
    column![container(tx(titulo, 12.0, t::FG3)).padding(pad(8.0, 14.0, 6.0, 14.0)), hline(), corpo].width(Length::FillPortion(1)).height(Length::Fill).into()
}

pub fn modal(app: &App) -> El<'_> {
    let c = app.conflito.as_ref().unwrap();
    let nome = c.original_path.rsplit('/').next().unwrap_or(&c.original_path).to_string();
    let head = container(
        row![
            icon("file", 16.0, t::FG2),
            txf(nome, 18.0, t::FG, t::SERIF),
            container(tx(crate::i18n::text("Sua nota no vault e a versão nova do Orion diferem."), 12.0, t::FG3)).width(Length::Fill),
            ib("close", Some(Msg::ConflitoFechar), false)
        ]
        .spacing(8)
        .align_y(Vertical::Center),
    )
    .padding(pad(10.0, 12.0, 10.0, 18.0));
    let lados: El = if app.is_drawer_mode() {
        column![painel(crate::i18n::text("No vault (sua versão)"), &c.original), hline(), painel(crate::i18n::text("Versão nova do Orion"), &c.proposal)].into()
    } else {
        row![painel(crate::i18n::text("No vault (sua versão)"), &c.original), vline_c(), painel(crate::i18n::text("Versão nova do Orion"), &c.proposal)].into()
    };
    let final_ed = text_editor(&c.content)
        .on_action(Msg::ConflitoEdit)
        .font(t::MONO)
        .size(12.5)
        .line_height(text::LineHeight::Relative(1.6))
        .padding(14)
        .height(Length::Fill)
        .style(|_, _| text_editor::Style { background: Background::Color(t::SIDE), border: Border::default(), placeholder: t::FG3, value: t::FG, selection: t::ACCENT_SOFT });
    let livre = !c.busy;
    let acoes = container(
        row![
            tx(crate::i18n::text("Texto final"), 12.0, t::FG3),
            container(sp_w(0.0)).width(Length::Fill),
            btn(crate::i18n::text("Usar a do vault"), Kind::Ghost, livre, None, Some(Msg::ConflitoUsar(false))),
            btn(crate::i18n::text("Usar a do Orion"), Kind::Ghost, livre, None, Some(Msg::ConflitoUsar(true))),
            btn(crate::i18n::text("Descartar proposta"), Kind::Normal, livre, None, Some(Msg::ConflitoDescartar)),
            btn(crate::i18n::text("Salvar texto final"), Kind::Primary, livre, None, Some(Msg::ConflitoSalvar)),
        ]
        .spacing(8)
        .align_y(Vertical::Center),
    )
    .padding(pad(8.0, 14.0, 8.0, 14.0));
    let (w, h) = (1100.0f32.min(app.win.width - 48.0), 760.0f32.min(app.win.height - 48.0));
    let mobile = app.is_drawer_mode();
    let dialog = container(column![head, hline(), container(lados).height(Length::FillPortion(3)), hline(), acoes, container(final_ed).height(Length::FillPortion(2))])
        .width(if mobile { Length::Fill } else { Length::Fixed(w) })
        .height(if mobile { Length::Fill } else { Length::Fixed(h) })
        .clip(true)
        .style(move |_| container::Style { background: Some(t::BG.into()), border: Border { color: if mobile { iced::Color::TRANSPARENT } else { t::LINE2 }, width: if mobile { 0.0 } else { 1.0 }, radius: if mobile { 0.0 } else { 18.0 }.into() }, shadow: shadow(), ..Default::default() });
    let (mp, sp) = (crate::anim::MODAL.get(), crate::anim::SCRIM.get());
    let dialog = rise(fade(dialog, mp, t::BG), mp, 6.0);
    opaque(mouse_area(container(opaque(mouse_area(dialog).on_press(Msg::Noop))).width(Length::Fill).height(Length::Fill).center_x(Length::Fill).center_y(Length::Fill).style(move |_| container::Style { background: Some(t::alpha(t::SCRIM, sp).into()), ..Default::default() })).on_press(Msg::ConflitoFechar))
}

fn vline_c<'a>() -> El<'a> {
    container(sp_w(0.0)).width(Length::Fixed(1.0)).height(Length::Fill).style(|_| container::Style { background: Some(t::LINE.into()), ..Default::default() }).into()
}
