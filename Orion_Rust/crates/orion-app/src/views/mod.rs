//! Telas. `root` monta a janela: entrada → casca (barra lateral + barra superior + palco) + camadas por cima.

pub mod chat;
pub mod composer;
pub mod conflito;
pub mod ctx;
pub mod editor;
pub mod home;
pub mod login;
pub mod onboarding;
pub mod memory;
pub mod menus;
pub mod mini;
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

/// Atualiza os progressos das animações a partir das marcas de tempo (lidos pelo `view`).
pub fn sync_anim(app: &App) {
    use crate::anim::{self as an, progress};
    let (n, r) = (app.now, app.p.reduced);
    let a = &app.at;
    an::PAGE.set(progress(a.page, n, 220, r));
    an::POP.set(progress(a.pop, n, 160, r));
    an::MODAL.set(progress(a.modal, n, 240, r));
    an::SCRIM.set(progress(a.scrim, n, 180, r));
    an::OVER.set(progress(a.over, n, 200, r));
    an::DRAWER.set(progress(a.drawer, n, 250, r));
    an::TOAST.set(progress(a.toast, n, 200, r));
    an::MORE.set(progress(a.more, n, 200, r));
    an::TOOLS.set(progress(a.tools, n, 200, r));
    an::SWITCH.set(progress(a.switch.as_ref().map(|s| s.1), n, 200, r));
    if let Ok(mut k) = an::SWITCH_KEY.lock() {
        *k = a.switch.as_ref().map(|s| s.0.clone()).unwrap_or_default();
    }
}

pub fn root(app: &App) -> El<'_> {
    let base: El = if let Some(message) = &app.loading {
        container(column![belt_fg(false), txf("Orion", 34.0, t::FG, t::SERIF), tx(crate::i18n::status(message), 14.0, t::FG2)].spacing(18).align_x(iced::alignment::Horizontal::Center)).width(Length::Fill).height(Length::Fill).center_x(Length::Fill).center_y(Length::Fill).into()
    } else if app.onboarding.is_some() { onboarding::view(app) } else if !app.logged { login::view(app) } else if app.mini { mini::view(app) } else { shell(app) };
    let base = if !app.logged || app.onboarding.is_some() { column![topbar::gate(), base].height(Length::Fill).into() } else { base };
    let mut layers: Vec<El> = vec![base];
    if app.logged {
        if app.drawer && app.is_drawer_mode() {
            let dp = crate::anim::DRAWER.get();
            layers.push(opaque(mouse_area(container(sp_w(0.0)).width(Length::Fill).height(Length::Fill).style(move |_| container::Style { background: Some(t::alpha(t::SCRIM, dp).into()), ..Default::default() })).on_press(Msg::Drawer(false))));
            let w = 300.0f32.min(app.win.width * 0.86);
            layers.push(iced::widget::float(row![container(sidebar::view(app)).width(Length::Fixed(w)).height(Length::Fill), vline_full()]).translate(move |_, _| iced::Vector::new(-(1.0 - dp) * w, 0.0)).into());
        }
        if app.set_open {
            layers.push(settings::modal(app));
        }
        if app.editor.is_some() {
            layers.push(editor::modal(app));
        }
        if app.conflito.is_some() {
            layers.push(conflito::modal(app));
        }
        if app.pal.open {
            layers.push(overlays::palette(app));
        }
        if app.qcap.is_some() {
            layers.push(overlays::quick_capture(app));
        }
        if app.auto_senha.is_some() {
            layers.push(overlays::auto_senha(app));
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
    let p = crate::anim::PAGE.get();
    let inner: El = if matches!(app.view, View::Chat) { inner } else { rise(fade(inner, p, t::BG), p, 3.0) };
    container(inner).width(Length::Fill).height(Length::Fill).into()
}
