//! Início (§11): constelação ao fundo, saudação, composer, linha de estado e sugestões.

use crate::app::{App, Msg, View};
use crate::icons::icon;
use crate::sky::Sky;
use crate::theme as t;
use crate::ui::*;
use iced::alignment::{Horizontal, Vertical};
use iced::widget::{button, canvas, column, container, rich_text, row, span, stack};
use iced::{Background, Border, Length};

pub fn local_hour() -> u32 {
    use chrono::Timelike;
    chrono::Local::now().hour()
}

pub fn greet(h: u32) -> &'static str {
    if h < 5 {
        crate::i18n::text("Boa noite")
    } else if h < 12 {
        crate::i18n::text("Bom dia")
    } else if h < 18 {
        crate::i18n::text("Boa tarde")
    } else {
        crate::i18n::text("Boa noite")
    }
}

fn sugg<'a>(ic: Option<&str>, dot: bool, label: String, warn: bool, msg: Msg) -> El<'a> {
    let mut r = row![].spacing(8).align_y(Vertical::Center);
    if dot {
        r = r.push(dot_el(t::SIGNAL, 6.0));
    }
    if let Some(i) = ic {
        r = r.push(icon(i, 16.0, t::FG2));
    }
    r = r.push(tx(label, 13.0, if warn { t::FG } else { t::FG2 }).wrapping(iced::widget::text::Wrapping::None));
    button(container(r).center_y(Length::Fill))
        .height(Length::Fixed(34.0))
        .padding(pad(0.0, 14.0, 0.0, 14.0))
        .on_press(msg)
        .style(move |_, st| button::Style {
            background: Some(Background::Color(t::mix(t::BG, 0.7))),
            text_color: t::FG2,
            border: Border { color: if warn { t::mix(t::SIGNAL, 0.4) } else if matches!(st, button::Status::Hovered) { t::LINE3 } else { t::LINE2 }, width: 1.0, radius: 999.0.into() },
            shadow: Default::default(),
            snap: true,
        })
        .into()
}

pub fn view(app: &App) -> El<'_> {
    let sky = canvas(Sky { alpha: 0.55, constellation: true, intro: 1.0, belt: if app.voice { 1.0 } else { 0.0 } }).width(Length::Fill).height(Length::Fill);
    let hour = app.hour.unwrap_or_else(local_hour);
    let size = (app.win.width * 0.05).clamp(34.0, 46.0);
    let hello_spans: Vec<iced::widget::text::Span<'_, (), iced::Font>> = vec![
        span(format!("{}, ", greet(hour))).font(t::SERIF).size(size).color(t::FG),
        span(app.p.call.clone()).font(t::SERIF_IT).size(size).color(t::FG2),
        span(".").font(t::SERIF).size(size).color(t::FG),
    ];
    let hello = rich_text(hello_spans)
    .width(Length::Fill)
    .align_x(Horizontal::Center)
    .line_height(iced::widget::text::LineHeight::Relative(1.08));
    let eyebrow = row![grow(), belt_fg(false), txf(spaced(crate::i18n::text("SEU ESPAÇO COM ORION"), 1), 11.0, t::FG3, t::UI_MED), grow()].spacing(10).align_y(Vertical::Center);

    let mut status = row![
        grow(),
        row![dot_el(t::OK, 6.0), tx(crate::i18n::text("Conectado"), 12.0, t::FG3)].spacing(6).align_y(Vertical::Center),
        row![icon("cpu", 14.0, t::FG3), tx(app.p.model_label(), 12.0, t::FG3)].spacing(6).align_y(Vertical::Center),
        row![icon("graph", 14.0, t::FG3), tx(match app.mem.nodes.len() { 1 => crate::i18n::text("1 nota no vault").to_string(), n => crate::i18n::formatted("{n} notas no vault", format!("{n} notas no vault")) }, 12.0, t::FG3)].spacing(6).align_y(Vertical::Center),
    ];
    // O horário do backup só aparece nos dados de exemplo: ainda não vem do núcleo.
    if !app.is_live() {
        status = status.push(row![icon("clock", 14.0, t::FG3), tx(crate::i18n::text("Backup do vault às 14:00"), 12.0, t::FG3)].spacing(6).align_y(Vertical::Center));
    }
    let status = status.push(grow()).spacing(16).align_y(Vertical::Center);

    let est = |label: &str, lead: f32| 28.0 + lead + label.chars().count() as f32 * 13.0 * 0.5 + 2.0;
    let mut items: Vec<(f32, El)> = vec![];
    if let Some((ci, _)) = app.pend_list().first() {
        let l = crate::i18n::formatted("{} espera você", format!("{} espera você", app.convs[*ci].title));
        items.push((est(&l, 14.0), sugg(None, true, l, true, Msg::OpenConv(*ci))));
    }
    if let Some(bi) = app.convs.iter().position(|c| c.id == "brief") {
        items.push((est("Ler o briefing da manhã", 24.0), sugg(Some("bell"), false, crate::i18n::text("Ler o briefing da manhã").into(), false, Msg::OpenConv(bi))));
    }
    items.push((est("Resumir o vault da semana", 24.0), sugg(Some("book"), false, crate::i18n::text("Resumir o vault da semana").into(), false, Msg::Fill("Resuma o que mudou no vault esta semana"))));
    items.push((est("Explorar a memória", 24.0), sugg(Some("graph"), false, crate::i18n::text("Explorar a memória").into(), false, Msg::Go(View::Memoria))));
    let mut lines: Vec<Vec<El>> = vec![vec![]];
    let mut used = 0.0;
    for (w, el) in items {
        if used > 0.0 && used + 8.0 + w > 660.0 {
            lines.push(vec![]);
            used = 0.0;
        }
        used += if used > 0.0 { 8.0 + w } else { w };
        lines.last_mut().unwrap().push(el);
    }
    let mut sg = column![].spacing(8);
    for l in lines {
        let mut r = row![grow()].spacing(8).align_y(Vertical::Center);
        for e in l {
            r = r.push(e);
        }
        sg = sg.push(r.push(grow()));
    }

    // Projetos recentes (F01.24): até três, na ordem da lista; o clique abre a conversa do projeto ou um chat novo nele.
    let mut recent = row![grow(), tx(crate::i18n::text("Projetos recentes"), 12.0, t::FG3)].spacing(10).align_y(Vertical::Center);
    for (i, p) in app.projects.iter().enumerate().take(3) {
        recent = recent.push(sugg(Some("folder"), false, p.n.clone(), false, Msg::OpenProject(i)));
    }
    let recent = recent.push(grow());

    let mut content = column![eyebrow, container(hello).padding(pad(4.0, 0.0, 22.0, 0.0)), crate::views::composer::view(app, true), container(status).padding(pad(6.0, 0.0, 0.0, 0.0)), container(sg).padding(pad(18.0, 0.0, 0.0, 0.0))]
        .spacing(12)
        .width(Length::Fixed(660.0));
    if !app.projects.is_empty() {
        content = content.push(container(recent).padding(pad(8.0, 0.0, 0.0, 0.0)));
    }
    let c = container(content).width(Length::Fill).height(Length::Fill).center_x(Length::Fill).center_y(Length::Fill).padding(pad(40.0, 16.0, 56.0, 16.0));
    stack![sky, c].width(Length::Fill).height(Length::Fill).into()
}
