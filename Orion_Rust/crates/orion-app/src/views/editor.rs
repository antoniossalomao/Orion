//! Editor de nota (§17): modal 1100×760 com texto à esquerda e prévia ao vivo à direita.

use crate::app::{App, Msg};
use crate::icons::icon;
use crate::md;
use crate::theme as t;
use crate::ui::*;
use iced::alignment::Vertical;
use iced::widget::{column, container, mouse_area, opaque, row, scrollable, text, text_editor};
use iced::{Background, Border, Length};

pub fn modal(app: &App) -> El<'_> {
    let e = app.editor.as_ref().unwrap();
    let head = container(
        row![
            icon("file", 16.0, t::FG2),
            txf(e.name.clone(), 18.0, t::FG, t::SERIF),
            container(txf(ell(&e.path, 420.0, 12.0), 12.0, t::FG3, t::MONO)).width(Length::Fill),
            btn("Obsidian", Kind::Ghost, true, None, Some(Msg::Toast("Abriria no Obsidian".into()))),
            btn("Salvar e commit", Kind::Primary, true, None, Some(Msg::EditorSave)),
            ib("close", Some(Msg::EditorClose), false)
        ]
        .spacing(8)
        .align_y(Vertical::Center),
    )
    .padding(pad(10.0, 12.0, 10.0, 18.0));
    let ed = text_editor(&e.content)
        .on_action(Msg::EditorEdit)
        .font(t::MONO)
        .size(13)
        .line_height(text::LineHeight::Relative(1.7))
        .padding(18)
        .height(Length::Fill)
        .style(|_, _| text_editor::Style { background: Background::Color(t::SIDE), border: Border::default(), placeholder: t::FG3, value: t::FG, selection: t::ACCENT_SOFT });
    let blocks = md::parse(&e.content.text());
    let prev = scrollable(container(md::render(&blocks, md::Style::preview())).padding(pad(18.0, 24.0, 18.0, 24.0)).width(Length::Fill)).direction(scrollable::Direction::Vertical(thin_bar())).style(thin_scroll).height(Length::Fill);
    let mobile = app.is_drawer_mode();
    let body: El = if mobile { column![container(ed).height(Length::FillPortion(1)), hline(), container(prev).height(Length::FillPortion(1))].into() } else { row![container(ed).width(Length::FillPortion(1)).height(Length::Fill), vline_f(), container(prev).width(Length::FillPortion(1)).height(Length::Fill)].into() };
    let (w, h) = (1100.0f32.min(app.win.width - 48.0), 760.0f32.min(app.win.height - 48.0));
    let dialog = container(column![head, hline(), body])
        .width(if mobile { Length::Fill } else { Length::Fixed(w) })
        .height(if mobile { Length::Fill } else { Length::Fixed(h) })
        .clip(true)
        .style(move |_| container::Style { background: Some(t::BG.into()), border: Border { color: if mobile { iced::Color::TRANSPARENT } else { t::LINE2 }, width: if mobile { 0.0 } else { 1.0 }, radius: if mobile { 0.0 } else { 18.0 }.into() }, shadow: shadow(), ..Default::default() });
    opaque(mouse_area(container(opaque(mouse_area(dialog).on_press(Msg::Noop))).width(Length::Fill).height(Length::Fill).center_x(Length::Fill).center_y(Length::Fill).style(|_| container::Style { background: Some(t::SCRIM.into()), ..Default::default() })).on_press(Msg::EditorClose))
}

fn vline_f<'a>() -> El<'a> {
    container(sp_w(0.0)).width(Length::Fixed(1.0)).height(Length::Fill).style(|_| container::Style { background: Some(t::LINE.into()), ..Default::default() }).into()
}
