//! Telas. `root` monta a janela: entrada → casca (barra lateral + barra superior + palco) + camadas por cima.

pub mod chat;
pub mod composer;
pub mod ctx;
pub mod editor;
pub mod home;
pub mod login;
pub mod memory;
pub mod menus;
pub mod overlays;
pub mod pages;
pub mod settings;
pub mod sidebar;
pub mod topbar;

use crate::app::{App, Msg, View};
use crate::theme as t;
use crate::ui::*;
use iced::widget::{column, container, mouse_area, opaque, row, stack};
use iced::Length;

pub fn root(app: &App) -> El<'_> {
    let base: El = if !app.logged { login::view(app) } else { shell(app) };
    let mut layers: Vec<El> = vec![base];
    if app.logged {
        if app.drawer && app.is_drawer_mode() {
            layers.push(opaque(mouse_area(container(sp_w(0.0)).width(Length::Fill).height(Length::Fill).style(|_| container::Style { background: Some(t::SCRIM.into()), ..Default::default() })).on_press(Msg::Drawer(false))));
            layers.push(container(sidebar::view(app)).width(Length::Fixed(300.0f32.min(app.win.width * 0.86))).height(Length::Fill).style(|_| container::Style { shadow: shadow(), ..Default::default() }).into());
        }
        if app.set_open {
            layers.push(settings::modal(app));
        }
        if app.editor.is_some() {
            layers.push(editor::modal(app));
        }
        if app.pal.open {
            layers.push(overlays::palette(app));
        }
        if app.qcap.is_some() {
            layers.push(overlays::quick_capture(app));
        }
        if app.isso.is_some() {
            layers.push(overlays::isso(app));
        }
        if app.voice {
            layers.push(overlays::island(app));
        }
        if app.p.cam[4] {
            layers.push(overlays::caption());
        }
    }
    if let Some((msg, _)) = &app.toast {
        layers.push(overlays::toast(msg.clone()));
    }
    let st: El = stack(layers).width(Length::Fill).height(Length::Fill).into();
    mouse_area(st).on_press(Msg::Bg).into()
}

fn shell(app: &App) -> El<'_> {
    let drawer = app.is_drawer_mode();
    let side: Option<El> = if app.collapsed || drawer { None } else { Some(row![sidebar::view(app), vline_full()].into()) };
    let body = row![stage(app), if app.show_ctx() { ctx::view(app) } else { sp_w(0.0) }].height(Length::Fill);
    let main = column![topbar::view(app), body].width(Length::Fill).height(Length::Fill);
    let mut r = row![].width(Length::Fill).height(Length::Fill);
    if let Some(s) = side {
        r = r.push(s);
    }
    container(r.push(main)).width(Length::Fill).height(Length::Fill).style(|_| container::Style { background: Some(t::BG.into()), text_color: Some(t::FG), ..Default::default() }).into()
}

fn vline_full<'a>() -> El<'a> {
    container(sp_w(0.0)).width(Length::Fixed(1.0)).height(Length::Fill).style(|_| container::Style { background: Some(t::LINE.into()), ..Default::default() }).into()
}

fn stage(app: &App) -> El<'_> {
    let inner: El = match app.view {
        View::Home => home::view(app),
        View::Chat => chat::view(app),
        View::Memoria => memory::view(app),
        v => pages::view(app, v),
    };
    container(inner).width(Length::Fill).height(Length::Fill).into()
}
