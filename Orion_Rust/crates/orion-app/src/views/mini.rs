//! Mini barra: a janela encolhe para uma barra no topo da tela (sempre à frente) com o campo de
//! pergunta, a última resposta e o uso do dia. Vem da bandeja, do atalho Ctrl Alt Espaço ou da voz.

use crate::app::{App, Msg};
use crate::data::ChatMsg;
use crate::theme as t;
use crate::ui::*;
use crate::views::composer;
use iced::widget::{column, container, row, Space};
use iced::{Color, Length};

pub fn view(app: &App) -> El<'_> {
    let topo = row![
        belt_fg(false),
        txf("Orion".to_string(), 15.0, t::FG, t::SERIF),
        Space::new().width(Length::Fill),
        btn(crate::i18n::text("Abrir o Orion"), Kind::Normal, true, None, Some(Msg::Mini(false))),
        ib_wh("close", 34.0, 30.0, 16.0, Some(Msg::CloseWindow), t::DANGER, Color::WHITE),
    ]
    .spacing(10)
    .align_y(iced::alignment::Vertical::Center);

    let resposta: String = app
        .conv
        .and_then(|i| app.convs.get(i))
        .and_then(|c| c.msgs.iter().rev().find_map(|m| if let ChatMsg::Orion(o) = m { Some(o.body.clone()) } else { None }))
        .map(|b| b.chars().take(420).collect())
        .unwrap_or_default();

    let uso: El = match app.budget.as_ref().and_then(|b| b.get("quotas")).and_then(|q| q.as_array()).and_then(|q| q.first()) {
        Some(q) => {
            let pct = q.get("percent").and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
            let (u, cap) = (q.get("used").and_then(|x| x.as_i64()).unwrap_or(0), q.get("weekly_calls").and_then(|x| x.as_i64()).unwrap_or(0));
            row![tx(crate::i18n::formatted("{} · {u} de {cap}", format!("{} · {u} de {cap}", q.get("provider").and_then(|x| x.as_str()).unwrap_or("uso"))), 11.5, t::FG3), container(meter(pct, 3.0, if pct >= 100.0 { t::DANGER } else { t::FG2 })).width(Length::Fixed(120.0))].spacing(10).align_y(iced::alignment::Vertical::Center).into()
        }
        None => tx(crate::i18n::text("Uso: sem cotas configuradas"), 11.5, t::FG3).into(),
    };

    let mut col = column![topo, composer::view(app, false)].spacing(10);
    if !resposta.is_empty() {
        col = col.push(tx(resposta, 13.0, t::FG2));
    }
    col = col.push(uso);
    container(col).padding(14).width(Length::Fill).height(Length::Fill).style(surface(Some(t::BG), None, 0.0)).into()
}
