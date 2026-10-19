//! Popovers: itens, grupos e os menus do modelo, projeto, conta e conversa (§6, §9.6, §13.5, §24).

use crate::app::{App, Msg, Sec, View, EFFORTS, PROV_MODELS, PROV_NAMES};
use crate::data::PROJ;
use crate::icons::icon;
use crate::theme as t;
use crate::ui::*;
use iced::alignment::Vertical;
use iced::widget::{button, column, container, row};
use iced::{Background, Border, Color, Length};

pub fn menu_box<'a>(items: Vec<El<'a>>, width: f32) -> El<'a> {
    container(column(items).spacing(0)).width(Length::Fixed(width)).padding(4).style(|_| container::Style { background: Some(t::RAISE.into()), border: Border { color: t::LINE2, width: 1.0, radius: 12.0.into() }, shadow: shadow(), ..Default::default() }).into()
}

pub fn item<'a>(ic: Option<&str>, label: &str, desc: Option<&str>, checked: bool, danger: bool, msg: Msg) -> El<'a> {
    let fg = if danger { t::DANGER } else { t::FG };
    let mut r = row![].spacing(10).align_y(Vertical::Center);
    if let Some(i) = ic {
        r = r.push(icon(i, 14.0, fg));
    }
    let mut txt = column![tx(label.to_string(), 13.0, fg)].spacing(0);
    if let Some(d) = desc {
        txt = txt.push(tx(d.to_string(), 11.5, t::FG3));
    }
    r = r.push(container(txt).width(Length::Fill));
    if checked {
        r = r.push(icon("check", 14.0, t::FG));
    }
    button(r).width(Length::Fill).padding(crate::ui::pad(7.0, 10.0, 7.0, 10.0)).on_press(msg).style(|_, st| button::Style {
        background: if matches!(st, button::Status::Hovered | button::Status::Pressed) { Some(Background::Color(t::HOVER)) } else { None },
        text_color: t::FG,
        border: Border { radius: 8.0.into(), ..Default::default() },
        shadow: Default::default(),
        snap: true,
    }).into()
}

pub fn group<'a>(label: &str) -> El<'a> {
    container(tx(label.to_string(), 11.0, t::FG3)).padding(crate::ui::pad(8.0, 10.0, 3.0, 10.0)).into()
}
pub fn hr<'a>() -> El<'a> {
    container(hline()).padding(crate::ui::pad(4.0, 6.0, 4.0, 6.0)).into()
}

pub fn model_menu(app: &App) -> El<'_> {
    let mut v: Vec<El> = vec![item(None, "Auto", Some("Escolhe pelo tipo de tarefa"), app.p.prov == 0, false, Msg::SetProv(0))];
    for pi in 1..=3 {
        v.push(group(&format!("{} · Assinatura, pela CLI oficial", PROV_NAMES[pi])));
        for (mi, m) in PROV_MODELS[pi].iter().enumerate() {
            let on = app.p.prov == pi && app.p.model == mi;
            v.push(item(None, &format!("{} {}", PROV_NAMES[pi], m), None, on, false, Msg::SetModelFull(pi, mi)));
        }
    }
    v.push(group("Orion"));
    v.push(item(None, "Orion", Some("APIs gratuitas; ordem e cotas nas configurações"), app.p.prov == 4, false, Msg::SetProv(4)));
    v.push(hr());
    v.push(group("Esforço (Claude, Codex, Gemini)"));
    for (i, e) in EFFORTS.iter().enumerate() {
        v.push(item(None, e, None, app.p.effort == i, false, Msg::SetEffort(i)));
    }
    menu_box(v, 300.0)
}

pub fn project_menu(app: &App) -> El<'_> {
    let mut v: Vec<El> = PROJ.iter().enumerate().map(|(i, p)| item(Some("folder"), p.n, None, app.p.proj == i, false, Msg::SetProj(i))).collect();
    v.push(item(Some("folder"), "Sem projeto", None, app.p.proj == 99, false, Msg::SetProj(99)));
    menu_box(v, 240.0)
}

pub fn account_menu(app: &App) -> El<'_> {
    let head = container(column![tx(app.p.name.clone(), 13.0, t::FG), tx("PC principal · notebook sincronizado", 11.5, t::FG3)]).padding(crate::ui::pad(8.0, 10.0, 6.0, 10.0));
    menu_box(
        vec![
            head.into(),
            hr(),
            item(Some("gear"), "Configurações", None, false, false, Msg::OpenSettings(None)),
            item(Some("shield"), "Privacidade e autonomia", None, false, false, Msg::OpenSettings(Some(Sec::Privacidade))),
            item(Some("keys"), "Atalhos", None, false, false, Msg::OpenSettings(Some(Sec::Atalhos))),
            hr(),
            item(Some("logout"), "Sair", None, false, false, Msg::Logout),
        ],
        244.0,
    )
}

pub fn conv_menu(app: &App) -> El<'_> {
    let pinned = app.conv.map(|i| app.convs[i].pinned).unwrap_or(false);
    let has_note = app.conv.map(|i| app.convs[i].note.is_some()).unwrap_or(false);
    let path = app.conv.and_then(|i| app.convs[i].note.as_ref().map(|n| n.path.to_string())).unwrap_or_default();
    menu_box(
        vec![
            item(Some("pin"), if pinned { "Desafixar conversa" } else { "Fixar conversa" }, None, false, false, Msg::PinToggle),
            item(Some("edit"), "Renomear", Some("Título gerado sozinho, editável"), false, false, Msg::Toast("Renomear: título editável (simulado)".into())),
            item(Some("file"), "Abrir nota no vault", None, false, false, if has_note { Msg::EditorOpen(path) } else { Msg::Toast("A nota é gravada no fim da primeira rodada".into()) }),
            item(Some("archive"), "Arquivar", None, false, false, Msg::ArchiveConv),
            hr(),
            item(Some("trash"), "Apagar conversa", None, false, true, Msg::DeleteConv),
        ],
        260.0,
    )
}

pub fn simple_menu<'a>(opts: &[&str], cur: usize, key: &'static str) -> El<'a> {
    menu_box(opts.iter().enumerate().map(|(i, o)| item(None, o, None, i == cur, false, Msg::Pick(key, i))).collect(), 260.0)
}

pub fn view_title(v: View) -> &'static str {
    v.title()
}

pub fn _color() -> Color {
    Color::TRANSPARENT
}
