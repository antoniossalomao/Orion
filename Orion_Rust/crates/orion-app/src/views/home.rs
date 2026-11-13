//! Início (§11): constelação ao fundo, saudação, composer, linha de estado e sugestões.

use crate::app::{App, Msg, View};
use crate::icons::icon;
use crate::theme as t;
use crate::ui::*;
use iced::alignment::{Horizontal, Vertical};
use iced::widget::{button, column, container, rich_text, row, span};
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
    let mut r = row![].spacing(t::SP_2).align_y(Vertical::Center);
    if dot {
        r = r.push(dot_el(t::SIGNAL, 6.0));
    }
    if let Some(i) = ic {
        r = r.push(icon(i, 16.0, t::FG2));
    }
    r = r.push(tx(label, 13.0, if warn { t::FG } else { t::FG2 }).wrapping(iced::widget::text::Wrapping::None));
    button(container(r).center_y(Length::Fill))
        .height(Length::Fixed(34.0))
        .padding(pad(0.0, t::SP_3, 0.0, t::SP_3))
        .on_press(msg)
        .style(move |_, st| button::Style {
            background: Some(Background::Color(t::mix(t::BG, 0.7))),
            text_color: t::FG2,
            border: Border { color: if warn { t::mix(t::SIGNAL, 0.4) } else if matches!(st, button::Status::Hovered) { t::LINE3 } else { t::LINE2 }, width: 1.0, radius: t::R_PILL.into() },
            shadow: Default::default(),
            snap: true,
        })
        .into()
}

/// Até três projetos, do que tem conversa mais recente (as conversas vêm da mais nova para a mais antiga) ao resto da lista.
fn recent_projects(app: &App) -> Vec<usize> {
    let mut out: Vec<usize> = vec![];
    for c in &app.convs {
        if let Some(i) = app.projects.iter().position(|p| p.n == c.proj) {
            if !out.contains(&i) {
                out.push(i);
            }
        }
    }
    for i in 0..app.projects.len() {
        if !out.contains(&i) {
            out.push(i);
        }
    }
    out.truncate(3);
    out
}

pub fn view(app: &App) -> El<'_> {

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
    let eyebrow = row![grow(), belt_fg(false), txf(spaced(crate::i18n::text("SEU ESPAÇO COM ORION"), 1), 11.0, t::FG3, t::UI_MED), grow()].spacing(t::SP_25).align_y(Vertical::Center);

    // Projetos recentes (F01.24): até três, na ordem da lista; o clique abre a conversa do projeto ou um chat novo nele.
    let mut recent = row![grow(), tx(crate::i18n::text("Projetos recentes"), 12.0, t::FG3)].spacing(t::SP_25).align_y(Vertical::Center);
    for i in recent_projects(app) {
        recent = recent.push(sugg(Some("folder"), false, ell(&app.projects[i].n, 100.0, 13.0), false, Msg::OpenProject(i)));
    }
    let recent = recent.push(grow());

    // presença do Orion acima da saudação (esfera de poeira que respira e reage à voz)
    // centralizada, logo acima da barra de chat (pedido do Antônio)
    let esfera = container(iced::widget::canvas(crate::presenca::Esfera { motor: &app.presenca }).width(Length::Fixed(200.0)).height(Length::Fixed(180.0))).width(Length::Fill).center_x(Length::Fill);
    // tarefas rápidas: só texto apagado, sem moldura (discreto, pedido do Antônio)
    let rapida = |label: &'static str, msg: Msg| -> El<'_> {
        button(tx(crate::i18n::text(label), 12.0, t::FG3))
            .padding(pad(t::SP_05, t::SP_15, t::SP_05, t::SP_15))
            .on_press(msg)
            .style(|_, st| button::Style { background: None, text_color: t::FG3, border: Border { color: if matches!(st, button::Status::Hovered) { t::LINE2 } else { iced::Color::TRANSPARENT }, width: 1.0, radius: t::R_PILL.into() }, shadow: Default::default(), snap: true })
            .into()
    };
    let mut rapidas = row![grow()].spacing(t::SP_1).align_y(Vertical::Center);
    if let Some(bi) = app.convs.iter().position(|c| c.id == "brief") {
        rapidas = rapidas.push(rapida("Ler o briefing da manhã", Msg::OpenConv(bi)));
    }
    rapidas = rapidas.push(rapida("Resumir o vault da semana", Msg::Fill("Resuma o que mudou no vault esta semana")));
    rapidas = rapidas.push(rapida("Explorar a memória", Msg::Go(View::Memoria)));
    let rapidas = rapidas.push(grow());

    let mut content = column![eyebrow, container(hello).padding(pad(t::SP_1, 0.0, t::SP_2, 0.0)), esfera, crate::views::composer::view(app, true)]
        .spacing(t::SP_3)
        .width(Length::Fixed(660.0));
    if !app.projects.is_empty() {
        content = content.push(container(recent).padding(pad(t::SP_2, 0.0, 0.0, 0.0)));
    }
    content = content.push(container(rapidas).padding(pad(t::SP_1, 0.0, 0.0, 0.0)));
    let c = container(content).width(Length::Fill).height(Length::Fill).center_x(Length::Fill).center_y(Length::Fill).padding(pad(t::SP_10, t::SP_4, t::SP_14, t::SP_4));
    // o fundo de espaço vem do `shell`, atrás da barra do topo também
    c.into()
}
