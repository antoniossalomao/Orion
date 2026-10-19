//! Barra superior (§10): título, pílulas de estado, captura rápida, "Isso?", busca, painel e controles da janela.

use crate::app::{App, Msg, Pop, View};
use crate::icons::icon;
use crate::popover::{Popover, Side};
use crate::theme as t;
use crate::ui::*;
use crate::views::menus;
use iced::alignment::Vertical;
use iced::widget::{button, container, mouse_area, row};
use iced::{Background, Border, Length};

pub fn view(app: &App) -> El<'_> {
    let drawer = app.is_drawer_mode();
    let mut r = row![].spacing(4).align_y(Vertical::Center).height(Length::Fixed(46.0)).padding(pad(0.0, 6.0, 0.0, 12.0));
    if drawer {
        r = r.push(tip(ib("menu", Some(Msg::Drawer(true)), false), "Abrir menu"));
    }
    if app.collapsed && !drawer {
        r = r.push(tip(ib("panelL", Some(Msg::Collapse), false), "Abrir barra lateral"));
        r = r.push(tip(ib("plus", Some(Msg::NewChat), false), "Nova conversa"));
    }
    // título
    if app.view == View::Chat {
        if let Some(i) = app.conv {
            let base = button(container(row![tx(ell(&app.convs[i].title, 420.0, 13.5), 13.5, t::FG), icon("chevD", 14.0, t::FG3)].spacing(6).align_y(Vertical::Center)).center_y(Length::Fill))
                .height(Length::Fixed(30.0))
                .padding(pad(0.0, 8.0, 0.0, 8.0))
                .on_press(Msg::Pop(Pop::ConvMenu))
                .style(|_, st| button::Style { background: if matches!(st, button::Status::Hovered) { Some(Background::Color(t::HOVER)) } else { None }, text_color: t::FG, border: Border { radius: 8.0.into(), ..Default::default() }, shadow: Default::default(), snap: true });
            let pop: El = Popover::new(base, menus::conv_menu(app), app.pop == Some(Pop::ConvMenu)).on_dismiss(Msg::ClosePop).into();
            r = r.push(pop);
        }
    } else if !app.view.title().is_empty() {
        r = r.push(container(txf(app.view.title(), 13.5, t::FG, t::UI_MED)).padding(pad(0.0, 8.0, 0.0, 8.0)));
    }
    r = r.push(mouse_area(container(sp_w(0.0)).width(Length::Fill).height(Length::Fill)).on_press(Msg::DragWindow).on_double_click(Msg::Fullscreen));
    // pílulas
    if app.p.cam[1] {
        r = r.push(pill(t::CAM_DOT, "Câmera", t::mix(t::DANGER, 0.45), true, Some(Msg::OpenSettings(Some(crate::app::Sec::Camera)))));
    }
    if app.p.auto {
        r = r.push(pill(t::ACCENT, "Autônomo", t::mix(t::ACCENT, 0.45), true, Some(Msg::Toggle("auto"))));
    }
    let pend = app.pend_count();
    if app.streaming.is_some() {
        let a = 0.3 + 0.7 * (0.5 + 0.5 * (app.spin * 1.5).cos());
        r = r.push(pill(t::alpha(t::SIGNAL, a), "Pensando", t::LINE2, false, None));
    } else if app.voice {
        r = r.push(pill(t::OK, "Ouvindo", t::LINE2, false, None));
    } else if pend > 0 {
        r = r.push(pill(t::SIGNAL, &format!("Aguardando você · {pend}"), t::mix(t::SIGNAL, 0.4), true, Some(Msg::Go(View::Aprovacoes))));
    }
    if !drawer {
        r = r.push(tip(ib("inbox", Some(Msg::QcapOpen), false), "Captura rápida (Win + Shift + O)"));
        r = r.push(tip(ib("scan", Some(Msg::IssoOpen), false), "Isso? Captura a tela e pergunta (Win + Shift + I)"));
        let cmd = button(container(row![icon("search", 14.0, t::FG3), tx("Buscar", 12.5, t::FG3), kbd("Ctrl K")].spacing(8).align_y(Vertical::Center)).center_y(Length::Fill))
            .height(Length::Fixed(30.0))
            .padding(pad(0.0, 8.0, 0.0, 10.0))
            .on_press(Msg::PalOpen)
            .style(|_, st| button::Style { background: if matches!(st, button::Status::Hovered) { Some(Background::Color(t::HOVER)) } else { None }, text_color: t::FG3, border: Border { radius: 8.0.into(), ..Default::default() }, shadow: Default::default(), snap: true });
        r = r.push(cmd);
    }
    if app.view == View::Chat && app.win.width > t::CTX_MIN_W {
        r = r.push(tip(ib("panelR", Some(Msg::CtxToggle), app.show_ctx()), "Painel da direita"));
    }
    if !drawer {
        r = r.push(container(vline(18.0)).padding(pad(0.0, 6.0, 0.0, 6.0)));
        r = r.push(tip(ib_wh("min", 34.0, 30.0, 16.0, Some(Msg::Minimize), t::HOVER, t::FG), "Minimizar"));
        r = r.push(tip(ib_wh("max", 34.0, 30.0, 14.0, Some(Msg::Fullscreen), t::HOVER, t::FG), "Tela cheia"));
        r = r.push(tip(ib_wh("close", 34.0, 30.0, 16.0, Some(Msg::CloseWindow), t::DANGER, iced::Color::WHITE), "Fechar"));
    }
    let _ = Side::Left;
    r.into()
}
