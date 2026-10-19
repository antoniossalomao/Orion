//! Conversa (§13): fio de mensagens, ferramentas recolhíveis, aprovação, ações e composer.

use crate::app::{App, Msg, Prefs};
use crate::data::{ApprState, ChatMsg, OrionMsg};
use crate::icons::icon;
use crate::md;
use crate::theme as t;
use crate::ui::*;
use iced::alignment::{Horizontal, Vertical};
use iced::widget::{button, column, container, mouse_area, row, scrollable};
use iced::{Border, Font, Length};

pub fn via_label(p: &Prefs) -> String {
    match p.prov {
        0 => "Orion · Cerebras".into(),
        4 => "Orion · Gemini".into(),
        _ => p.model_label(),
    }
}

pub fn chat_fonts(p: &Prefs) -> (Font, Font) {
    match p.font {
        0 => (t::SERIF, t::SERIF_SEMI),
        1 => (t::UI, t::UI_SEMI),
        _ => (Font::with_name("Segoe UI"), Font::with_name("Segoe UI")),
    }
}

fn avail_width(app: &App) -> f32 {
    let side = if app.collapsed || app.is_drawer_mode() { 0.0 } else { t::SIDE_W + 1.0 };
    let ctx = if app.show_ctx() { t::CTX_W } else { 0.0 };
    app.win.width - side - ctx
}

pub fn view(app: &App) -> El<'_> {
    let Some(ci) = app.conv else { return sp_w(0.0) };
    let c = &app.convs[ci];
    let width = app.p.thread_w().min(avail_width(app));
    let content_w = width - 40.0;

    let thread: El = if c.msgs.is_empty() {
        container(
            column![belt_fg(true), txf("Conversa antiga", 28.0, t::FG, t::SERIF), tx("Conversa antiga. O resumo está no vault; escreva para continuar.", 14.0, t::FG3)].spacing(14).align_x(Horizontal::Center),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
    } else {
        let mut col = column![].spacing(30).width(Length::Fixed(content_w));
        if let Some(p) = c.pin {
            col = col.push(dashed(container(row![icon("pin", 16.0, t::FG3), tx(format!("Fixado: {p}"), 12.5, t::FG2)].spacing(8).align_y(Vertical::Center)).width(Length::Fill).padding(pad(8.0, 12.0, 8.0, 12.0)), 10.0, t::LINE2));
        }
        let last_orion = c.msgs.iter().rposition(|m| matches!(m, ChatMsg::Orion(_)));
        for (mi, m) in c.msgs.iter().enumerate() {
            match m {
                ChatMsg::User(s) => col = col.push(user_msg(app, mi, s, content_w)),
                ChatMsg::Orion(o) => col = col.push(orion_msg(app, ci, mi, o, Some(mi) == last_orion, content_w)),
            }
        }
        let body = container(col).width(Length::Fill).center_x(Length::Fill).padding(pad(28.0, 0.0, 36.0, 0.0));
        scrollable(body).direction(scrollable::Direction::Vertical(thin_bar())).style(thin_scroll).anchor_bottom().height(Length::Fill).into()
    };

    let mut tail = column![].spacing(6).width(Length::Fixed((width - 32.0).min(app.win.width - 64.0))).align_x(Horizontal::Center);
    if let Some(b) = c.banner {
        tail = tail.push(
            container(row![dot_el(t::SIGNAL, 6.0), tx(b, 12.5, t::FG2), grow(), icon("close", 14.0, t::FG3)].spacing(8).align_y(Vertical::Center))
                .width(Length::Fill)
                .padding(pad(6.0, 12.0, 6.0, 12.0))
                .style(surface(Some(t::RAISE), Some(t::LINE), 9.0)),
        );
    }
    tail = tail.push(crate::views::composer::view(app, false));
    let tail = container(tail).width(Length::Fill).center_x(Length::Fill).padding(pad(0.0, 16.0, 14.0, 16.0));
    column![thread, tail].height(Length::Fill).into()
}

fn user_msg<'a>(app: &'a App, mi: usize, s: &'static str, content_w: f32) -> El<'a> {
    let bubble = container(txf(s, 15.0, t::FG, t::UI)).padding(pad(10.0, 15.0, 10.0, 15.0)).max_width(content_w * 0.8).style(surface(Some(t::RAISE2), None, 16.0));
    let show = app.hover == Some((2, mi));
    let acts: El = if show {
        row![tip(ib_sz("edit", 26.0, 16.0, Some(Msg::Toast("Editar e reenviar cria um ramo da conversa".into())), false), "Editar e reenviar"), tip(ib_sz("copy", 26.0, 16.0, Some(Msg::Toast("Copiado".into())), false), "Copiar"), tip(ib_sz("pin", 26.0, 16.0, Some(Msg::Toast("Fixada como contexto permanente".into())), false), "Fixar como contexto")].into()
    } else {
        container(sp_w(0.0)).height(Length::Fixed(26.0)).into()
    };
    let col = column![bubble, acts].spacing(4).align_x(Horizontal::Right);
    let r = row![grow(), mouse_area(col).on_enter(Msg::Hover(Some((2, mi)))).on_exit(Msg::Hover(None))];
    r.into()
}

fn orion_msg<'a>(app: &'a App, ci: usize, mi: usize, o: &'a OrionMsg, is_last: bool, content_w: f32) -> El<'a> {
    let streaming = app.streaming.as_ref().filter(|s| s.conv == ci && is_last);
    let mut col = column![].spacing(12).width(Length::Fill);
    if !o.tools.is_empty() && streaming.is_none() {
        let open = app.tools_open.contains(&(ci, mi));
        col = col.push(
            button(row![tx(o.tools_label, 13.0, t::FG3), crate::icons::icon_rot("chevR", 16.0, t::FG3, { let p = if app.at.tools.is_some() { crate::anim::TOOLS.get() } else { 1.0 }; 90.0 * if open { p } else { 1.0 - p } })].spacing(6).align_y(Vertical::Center))
                .padding(0)
                .on_press(Msg::ToggleTools(ci, mi))
                .style(|_, _| button::Style { text_color: t::FG3, ..Default::default() }),
        );
        if open {
            let mut list = column![].spacing(0);
            for tl in &o.tools {
                list = list.push(
                    row![
                        container(tx(tl.kind, 12.5, t::FG3)).width(Length::Fixed(70.0)),
                        container(txf(ell(tl.value, content_w - 190.0, 12.0), 12.0, t::FG2, t::MONO)).width(Length::Fill),
                        tx(tl.time, 11.5, t::FG3)
                    ]
                    .spacing(10)
                    .align_y(Vertical::Center)
                    .padding(pad(3.0, 0.0, 3.0, 0.0)),
                );
            }
            col = col.push(row![sp_w(3.0), vline(list_h(o.tools.len())), sp_w(13.0), list.width(Length::Fill)].padding(pad(0.0, 0.0, 0.0, 0.0)));
        }
    }
    // corpo
    let (f, fb) = chat_fonts(&app.p);
    let mut text = o.body.to_string();
    if let Some(s) = streaming {
        let words: Vec<&str> = o.body.split_whitespace().take(s.words).collect();
        text = words.join(" ");
        text.push_str(if (app.now.saturating_duration_since(app.t0).as_millis() / 500) % 2 == 0 { " █" } else { "  " });
    }
    let blocks = md::parse(&text);
    col = col.push(md::render(&blocks, md::Style::chat(f, fb, app.p.chat_fs())));

    if let Some(a) = &o.approval {
        col = col.push(approval(a, ci, mi));
    }
    if streaming.is_none() {
        let mut acts = row![tip(ib_sz("copy", 28.0, 16.0, Some(Msg::Toast("Copiado".into())), false), "Copiar"), tip(ib_sz("speaker", 28.0, 16.0, Some(Msg::Toast("Lendo em voz alta (simulado)".into())), false), "Ouvir")].spacing(0).align_y(Vertical::Center);
        if is_last {
            acts = acts.push(tip(ib_sz("retry", 28.0, 16.0, Some(Msg::Toast("Gerar de novo…".into())), false), "Gerar de novo"));
        }
        acts = acts.push(container(tx(o.time, 11.5, t::FG3)).padding(pad(0.0, 0.0, 0.0, 6.0)));
        let via_txt = if o.via.is_empty() { None } else { Some(o.via) };
        if let Some(v) = via_txt {
            let mut vr = row![tx(v, 11.5, t::FG4)].spacing(4).align_y(Vertical::Center);
            if !o.out.is_empty() {
                vr = vr.push(icon("out", 12.0, t::FG4));
            }
            let msg = if o.out.is_empty() { format!("{v} · nada saiu do computador") } else { format!("{v} · saiu do PC: {}", o.out) };
            acts = acts.push(container(button(vr).padding(pad(2.0, 6.0, 2.0, 6.0)).on_press(Msg::Toast(msg)).style(|_, st| button::Style { background: if matches!(st, button::Status::Hovered) { Some(t::HOVER.into()) } else { None }, text_color: t::FG4, border: Border { radius: 6.0.into(), ..Default::default() }, ..Default::default() })).padding(pad(0.0, 0.0, 0.0, 8.0)));
        }
        if o.saved {
            col = col.push(container(acts).padding(pad(0.0, 0.0, 0.0, 0.0)));
        } else {
            col = col.push(acts);
        }
    }
    col.into()
}

fn list_h(n: usize) -> f32 {
    n as f32 * 25.0
}

fn approval<'a>(a: &'a crate::data::Approval, ci: usize, mi: usize) -> El<'a> {
    let txt = md::render(&md::parse(a.text), md::Style { font: t::UI, bold: t::UI_SEMI, size: 13.5, line: 1.5, h_font: t::UI_MED, h1: 16.0, h2: 15.0, h3: 14.0, gap: 4.0 });
    match &a.state {
        ApprState::Open => {
            let mut btns = row![btn("Aprovar", Kind::Primary, true, None, Some(Msg::Approve(ci, mi))), btn("Negar", Kind::Normal, true, None, Some(Msg::Deny(ci, mi)))].spacing(8);
            if a.diff_button {
                btns = btns.push(btn("Ver o diff", Kind::Ghost, true, None, Some(Msg::ShowDiff)));
            }
            container(column![row![dot_halo(t::SIGNAL, t::SIGNAL_SOFT), txf("Aguardando sua aprovação", 13.0, t::SIGNAL, t::UI_MED)].spacing(8).align_y(Vertical::Center), container(txt).padding(pad(0.0, 0.0, 0.0, 0.0)), container(btns).padding(pad(2.0, 0.0, 0.0, 0.0))].spacing(8))
                .width(Length::Fill)
                .max_width(640.0)
                .padding(pad(14.0, 16.0, 14.0, 16.0))
                .style(|_| container::Style { background: Some(t::SIGNAL_SOFT.into()), border: Border { color: t::mix(t::SIGNAL, 0.34), width: 1.0, radius: 14.0.into() }, ..Default::default() })
                .into()
        }
        ApprState::Approved(m) | ApprState::Denied(m) => {
            let ok = matches!(a.state, ApprState::Approved(_));
            container(row![dot_el(if ok { t::OK } else { t::FG3 }, 6.0), tx(m.clone(), 13.0, t::FG2)].spacing(8).align_y(Vertical::Center))
                .width(Length::Fill)
                .max_width(640.0)
                .padding(pad(10.0, 14.0, 10.0, 14.0))
                .style(surface(None, Some(t::LINE), 14.0))
                .into()
        }
    }
}
