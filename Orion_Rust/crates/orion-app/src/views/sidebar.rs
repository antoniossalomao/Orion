//! Barra lateral (§9): marca, busca, navegação, lista de conversas por grupo e rodapé com a conta.

use crate::app::{App, Msg, Pop, View};
use crate::data::{ConvState, Origin};
use crate::icons::icon;
use crate::popover::Popover;
use crate::sky::spinner;
use crate::theme as t;
use crate::ui::*;
use crate::views::login::field_style;
use crate::views::menus;
use iced::alignment::Vertical;
use iced::widget::{button, column, container, mouse_area, row, scrollable, stack, text_input};
use iced::{Background, Border, Length};

const NAV: [(View, &str, &str); 5] = [(View::Home, "plus", "Nova conversa"), (View::Projetos, "folder", "Projetos"), (View::Memoria, "graph", "Memória"), (View::Aprovacoes, "shield", "Aprovações"), (View::Rotinas, "clock", "Rotinas")];
const MORE: [(View, &str, &str); 7] = [(View::Resultados, "result", "Resultados"), (View::Nuvem, "cloud", "Nuvem"), (View::Fontes, "db", "Fontes"), (View::Conhecimento, "book", "Conhecimento"), (View::Painel, "gauge", "Painel"), (View::Vida, "heart", "Vida"), (View::Arquivadas, "archive", "Arquivadas")];

fn nav_style(cur: bool) -> impl Fn(&iced::Theme, button::Status) -> button::Style {
    move |_, st| {
        let hov = matches!(st, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: if cur { Some(Background::Color(t::SEL)) } else if hov { Some(Background::Color(t::HOVER)) } else { None },
            text_color: if cur || hov { t::FG } else { t::FG2 },
            border: Border { radius: 8.0.into(), ..Default::default() },
            shadow: Default::default(),
            snap: true,
        }
    }
}

fn nav_item<'a>(app: &'a App, v: View, ic: &str, label: &str, idx: usize) -> El<'a> {
    let cur = app.view == v && v != View::Home;
    let fg = if cur || app.hover == Some((1, idx)) { t::FG } else { t::FG2 };
    let mut r = row![icon(ic, 16.0, fg), tx(label.to_string(), 13.5, fg)].spacing(11).align_y(Vertical::Center);
    r = r.push(grow());
    match v {
        View::Aprovacoes if app.pend_count() > 0 => {
            r = r.push(container(tx(app.pend_count().to_string(), 11.0, t::SIGNAL)).width(Length::Fixed(18.0)).height(Length::Fixed(18.0)).center_x(Length::Fixed(18.0)).center_y(Length::Fixed(18.0)).style(surface(Some(t::SIGNAL_SOFT), None, 9.0)));
        }
        View::Rotinas if app.rots.iter().any(|x| x.news) => r = r.push(dot_el(t::ACCENT, 6.0)),
        View::Nuvem if app.cloud.iter().any(|c| c.running) => r = r.push(spinner(app.spin, 12.0)),
        View::Home if app.hover == Some((1, 0)) => r = r.push(kbd("Ctrl ⇧ O")),
        _ => {}
    }
    let b = button(container(r).center_y(Length::Fill)).width(Length::Fill).height(Length::Fixed(if app.p.compact { 26.0 } else { 32.0 })).padding(pad(0.0, 10.0, 0.0, 10.0)).on_press(if v == View::Home { Msg::NewChat } else { Msg::Go(v) }).style(nav_style(cur));
    mouse_area(b).on_enter(Msg::Hover(Some((1, idx)))).on_exit(Msg::Hover(None)).into()
}

fn more_item<'a>(app: &'a App) -> El<'a> {
    let r = row![container(icon("chevR", 16.0, t::FG2)).rotate_placeholder(), tx("Mais", 13.5, t::FG2)].spacing(11).align_y(Vertical::Center);
    let _ = app;
    r.into()
}

trait Rot {
    fn rotate_placeholder(self) -> Self;
}
impl<T> Rot for T {
    fn rotate_placeholder(self) -> Self {
        self
    }
}

pub fn view(app: &App) -> El<'_> {
    let drawer = app.is_drawer_mode();
    let top = row![
        if drawer { tip(ib("close", Some(Msg::Drawer(false)), false), "Fechar menu") } else { tip(ib("panelL", Some(Msg::Collapse), false), "Recolher (Ctrl B)") },
        container(row![belt_fg(false), txf("Orion", 15.0, t::FG, t::UI_MED)].spacing(10).align_y(Vertical::Center)).padding(pad(0.0, 8.0, 0.0, 8.0)),
        grow(),
        if drawer { sp_w(0.0) } else { tip(ib("back", if app.hist.is_empty() { None } else { Some(Msg::Back) }, false), "Voltar") },
        if drawer { sp_w(0.0) } else { tip(ib("fwd", if app.fwd.is_empty() { None } else { Some(Msg::Fwd) }, false), "Avançar") },
    ]
    .spacing(2)
    .align_y(Vertical::Center)
    .height(Length::Fixed(46.0));

    let search = stack![
        text_input("Procurar nas conversas", &app.side_q)
            .on_input(Msg::SideQ)
            .size(13)
            .font(t::UI)
            .padding(pad(7.0, 10.0, 7.0, 32.0))
            .style(|th, st| {
                let mut s = field_style(t::HOVER, iced::Color::TRANSPARENT, t::LINE2, 8.0)(th, st);
                if matches!(st, text_input::Status::Focused { .. }) {
                    s.background = t::BG.into();
                }
                s
            }),
        container(icon("search", 14.0, t::FG3)).padding(pad(9.0, 0.0, 0.0, 10.0)),
    ];

    let mut nav = column![].spacing(1);
    for (i, (v, ic, l)) in NAV.iter().enumerate() {
        nav = nav.push(nav_item(app, *v, ic, l, i));
    }
    // "Mais" com a seta girando: no iced a seta troca entre chevR e chevD (90°)
    let ang = { let p = crate::anim::MORE.get(); 90.0 * if app.more { p } else { 1.0 - p } };
    let mh = app.hover == Some((1, 99));
    let more_fg = if mh { t::FG } else { t::FG2 };
    let more_btn = button(container(row![crate::icons::icon_rot("chevR", 16.0, more_fg, ang), tx("Mais", 13.5, more_fg)].spacing(11).align_y(Vertical::Center)).center_y(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fixed(if app.p.compact { 26.0 } else { 32.0 }))
        .padding(pad(0.0, 10.0, 0.0, 10.0))
        .on_press(Msg::ToggleMore)
        .style(nav_style(false));
    nav = nav.push(mouse_area(more_btn).on_enter(Msg::Hover(Some((1, 99)))).on_exit(Msg::Hover(None)));
    if app.more {
        for (i, (v, ic, l)) in MORE.iter().enumerate() {
            nav = nav.push(nav_item(app, *v, ic, l, 10 + i));
        }
    }
    let _ = more_item;

    let q = app.side_q.trim().to_lowercase();
    let groups: [(&str, fn(&crate::data::Conv) -> bool); 5] = [
        ("Fixadas", |c| c.pinned),
        ("Hoje", |c| !c.pinned && c.day == "Hoje"),
        ("Ontem", |c| !c.pinned && c.day == "Ontem"),
        ("7 dias", |c| !c.pinned && c.day == "7 dias"),
        ("Mais antigos", |c| !c.pinned && c.day == "Mais antigos"),
    ];
    let mut list = column![].spacing(0);
    let mut any = false;
    for (g, f) in groups {
        let items: Vec<usize> = app.convs.iter().enumerate().filter(|(_, c)| f(c) && (q.is_empty() || format!("{} {} {}", c.title, c.proj, c.msgs.iter().map(|m| match m { crate::data::ChatMsg::User(s) => s.to_string(), crate::data::ChatMsg::Orion(o) => o.body.to_string() }).collect::<Vec<_>>().join(" ")).to_lowercase().contains(&q))).map(|(i, _)| i).collect();
        if items.is_empty() {
            continue;
        }
        any = true;
        list = list.push(container(tx(g, 11.5, t::FG3)).padding(pad(18.0, 10.0, 4.0, 10.0)));
        for i in items {
            list = list.push(conv_item(app, i));
        }
    }
    if !any {
        list = list.push(container(tx(format!("Nenhuma conversa com “{}”.", app.side_q), 13.0, t::FG3)).padding(10));
    }
    let scroll = scrollable(list.padding(pad(0.0, 8.0, 8.0, 8.0))).direction(scrollable::Direction::Vertical(thin_bar())).style(thin_scroll).height(Length::Fill);
    let scroll = container(scroll).padding(pad(0.0, 0.0, 0.0, 0.0)).width(Length::Fill).height(Length::Fill);

    let me_btn = button(
        row![
            container(txf("AS", 10.5, t::FG, t::UI_SEMI)).width(Length::Fixed(26.0)).height(Length::Fixed(26.0)).center_x(Length::Fixed(26.0)).center_y(Length::Fixed(26.0)).style(surface(Some(t::RAISE2), Some(t::LINE2), 13.0)),
            row![tx(app.p.call.clone(), 13.5, t::FG), tx(" · PC principal", 12.0, t::FG3)],
            grow(),
            icon("chevD", 14.0, t::FG3)
        ]
        .spacing(10)
        .align_y(Vertical::Center),
    )
    .width(Length::Fill)
    .height(Length::Fixed(40.0))
    .padding(pad(0.0, 8.0, 0.0, 8.0))
    .on_press(Msg::Pop(Pop::Account))
    .style(|_, st| button::Style { background: if matches!(st, button::Status::Hovered) { Some(Background::Color(t::HOVER)) } else { None }, text_color: t::FG, border: Border { radius: 8.0.into(), ..Default::default() }, shadow: Default::default(), snap: true });
    let me: El = Popover::new(me_btn, menus::account_menu(app), app.pop == Some(Pop::Account)).above(true).on_dismiss(Msg::ClosePop).into();
    let foot = column![hline(), sp_h(8.0), row![container(me).width(Length::Fill), tip(ib("gear", Some(Msg::OpenSettings(None)), false), "Configurações (Ctrl ,)")].spacing(4).align_y(Vertical::Center)];

    container(column![top, search_wrap(search), container(nav).padding(pad(0.0, 8.0, 0.0, 8.0)), scroll, container(foot).padding(pad(0.0, 8.0, 8.0, 8.0))].spacing(0))
        .width(Length::Fixed(if app.is_drawer_mode() { 300.0f32.min(app.win.width * 0.86) } else { t::SIDE_W }))
        .height(Length::Fill)
        .padding(pad(0.0, 0.0, 0.0, 0.0))
        .style(|_| container::Style { background: Some(t::SIDE.into()), ..Default::default() })
        .into()
}

fn search_wrap<'a>(e: impl Into<El<'a>>) -> El<'a> {
    container(e).padding(pad(4.0, 8.0, 10.0, 8.0)).into()
}

fn conv_item(app: &App, i: usize) -> El<'_> {
    let c = &app.convs[i];
    let cur = app.view == View::Chat && app.conv == Some(i);
    let mut title = row![].spacing(6).align_y(Vertical::Center);
    match c.origin {
        Some(Origin::Rotina) => title = title.push(icon("clock", 12.0, t::FG3)),
        Some(Origin::Telegram) => title = title.push(icon("send2", 12.0, t::FG3)),
        None => {}
    }
    title = title.push(tx(ell(&c.title, 190.0, 13.5), 13.5, t::FG));
    let suffix = match c.origin {
        Some(Origin::Rotina) => " · Rotina",
        Some(Origin::Telegram) => " · via Telegram",
        None => "",
    };
    let sub = tx(format!("{}{}", c.proj, suffix), 11.5, t::FG3);
    let ind: El = match c.state {
        ConvState::Pending => dot_halo(t::SIGNAL, t::SIGNAL_SOFT),
        ConvState::Running => spinner(app.spin, 12.0),
        ConvState::Unread => dot_el(t::ACCENT, 6.0),
        ConvState::Normal => sp_w(0.0),
    };
    let pad_v = if app.p.compact { 3.0 } else { 6.0 };
    let body = row![column![title, sub].width(Length::Fill), container(ind).center_y(Length::Shrink)].spacing(8).align_y(Vertical::Center);
    button(body)
        .width(Length::Fill)
        .padding(pad(pad_v, 10.0, pad_v, 10.0))
        .on_press(Msg::OpenConv(i))
        .style(move |_, st| button::Style {
            background: if cur { Some(Background::Color(t::SEL)) } else if matches!(st, button::Status::Hovered) { Some(Background::Color(t::HOVER)) } else { None },
            text_color: t::FG,
            border: Border { radius: 8.0.into(), ..Default::default() },
            shadow: Default::default(),
            snap: true,
        })
        .into()
}
