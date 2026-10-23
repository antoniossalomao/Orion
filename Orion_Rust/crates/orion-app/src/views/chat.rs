//! Conversa (§13): fio de mensagens, ferramentas recolhíveis, aprovação, ações e composer.

use crate::app::{App, Msg, Prefs};
use crate::data::{ApprState, ChatMsg, OrionMsg};
use crate::icons::icon;
use crate::md;
use crate::theme as t;
use crate::ui::*;
use iced::alignment::{Horizontal, Vertical};
use iced::widget::{button, column, container, mouse_area, row, scrollable, text_input};
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

pub const JANELA: usize = 60;

/// Minimapa: uma faixa fina com um bloco por mensagem (altura ~ tamanho do texto); o trecho visível fica claro.
fn minimapa<'a>(app: &'a App, c: &'a crate::data::Conv, primeira: usize) -> El<'a> {
    let peso = |m: &ChatMsg| -> u16 { let n = match m { ChatMsg::User(s) => s.len(), ChatMsg::Orion(o) => o.body.len() }; ((n / 80) + 2).min(400) as u16 };
    // só as mensagens montadas: o deslocamento da rolagem é relativo a elas, não à conversa inteira
    let total: f32 = c.msgs.iter().skip(primeira).map(|m| peso(m) as f32).sum::<f32>().max(1.0);
    let (ini, tam) = app.chat_vp;
    let mut col = column![].spacing(1).width(Length::Fixed(6.0)).height(Length::Fill);
    let mut acc = 0.0f32;
    for m in c.msgs.iter().skip(primeira) {
        let w = peso(m);
        let (a, b) = (acc / total, (acc + w as f32) / total);
        acc += w as f32;
        let visivel = b > ini && a < ini + tam;
        let user = matches!(m, ChatMsg::User(_));
        let cor = if visivel { t::FG2 } else if user { t::LINE2 } else { t::LINE };
        let alvo = (a + b) / 2.0;
        col = col.push(
            button(sp_w(0.0)).padding(0).width(Length::Fill).height(Length::FillPortion(w)).on_press(Msg::ChatJump(alvo)).style(move |_, _| button::Style { background: Some(cor.into()), border: Border { radius: 2.0.into(), ..Default::default() }, ..Default::default() }),
        );
    }
    container(col).height(Length::Fill).padding(pad(10.0, 4.0, 14.0, 4.0)).into()
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
        if let Some(p) = c.pin.clone() {
            col = col.push(dashed(container(row![icon("pin", 16.0, t::FG3), tx(format!("Fixado: {p}"), 12.5, t::FG2)].spacing(8).align_y(Vertical::Center)).width(Length::Fill).padding(pad(8.0, 12.0, 8.0, 12.0)), 10.0, t::LINE2));
        }
        let last_orion = c.msgs.iter().rposition(|m| matches!(m, ChatMsg::Orion(_)));
        // Janela virtual: só as mensagens perto da tela são montadas (ver ChatScrolled); as mais antigas voltam ao rolar para cima.
        let mostradas = app.chat_win.get(&ci).copied().unwrap_or(JANELA).max(JANELA);
        let primeira = c.msgs.len().saturating_sub(mostradas);
        if primeira > 0 {
            col = col.push(
                container(btn(&format!("Carregar mais ({primeira} anteriores)"), Kind::Ghost, true, None, Some(Msg::ShowAll(ci)))).width(Length::Fill).center_x(Length::Fill),
            );
        }
        for (mi, m) in c.msgs.iter().enumerate().skip(primeira) {
            match m {
                ChatMsg::User(s) => col = col.push(user_msg(app, mi, s.clone(), content_w)),
                ChatMsg::Orion(o) => col = col.push(orion_msg(app, ci, mi, o, Some(mi) == last_orion, content_w)),
            }
        }
        let body = container(col).width(Length::Fill).center_x(Length::Fill).padding(pad(28.0, 0.0, 36.0, 0.0));
        let lista = scrollable(body).id("chat-scroll").on_scroll(Msg::ChatScrolled).direction(scrollable::Direction::Vertical(thin_bar())).style(thin_scroll).anchor_bottom().height(Length::Fill);
        // O Scrollable fica sempre na mesma posição da árvore (dentro do stack): trocar de tipo recriaria o widget e
        // perderia a posição de rolagem. Só a camada do botão aparece e some.
        let fim: El = if app.chat_end {
            sp_w(0.0)
        } else {
            container(btn("Ir ao fim", Kind::Normal, true, Some("chevD"), Some(Msg::ChatEnd))).width(Length::Fill).height(Length::Fill).center_x(Length::Fill).align_y(Vertical::Bottom).padding(pad(0.0, 0.0, 14.0, 0.0)).into()
        };
        let mapa: El = if c.msgs.len() - primeira >= 8 { container(minimapa(app, c, primeira)).width(Length::Fill).height(Length::Fill).align_x(Horizontal::Right).into() } else { sp_w(0.0) };
        iced::widget::stack![lista, fim, mapa].into()
    };

    let mut tail = column![].spacing(6).width(Length::Fixed((width - 32.0).min(app.win.width - 64.0))).align_x(Horizontal::Center);
    if let Some(form) = app.rename_form.as_ref().filter(|f| f.session_id == c.id) {
        let mut input = text_input("Título da conversa", &form.title).id("conversation-title").padding(10).size(14);
        if !form.busy { input = input.on_input(Msg::RenameText).on_submit(Msg::RenameSave); }
        let mut rename = column![
            tx("Renomear conversa", 13.0, t::FG2),
            row![input, btn(if form.busy { "Salvando…" } else { "Salvar" }, Kind::Primary, true, None, (!form.busy).then_some(Msg::RenameSave)), btn("Cancelar", Kind::Ghost, true, None, (!form.busy).then_some(Msg::RenameClose))].spacing(8),
        ].spacing(6);
        if !form.error.is_empty() { rename = rename.push(tx(form.error.clone(), 12.0, t::DANGER)); }
        tail = tail.push(rename);
    }
    if let Some(b) = c.banner.clone() {
        tail = tail.push(
            container(row![dot_el(t::SIGNAL, 6.0), tx(b.clone(), 12.5, t::FG2), grow(), icon("close", 14.0, t::FG3)].spacing(8).align_y(Vertical::Center))
                .width(Length::Fill)
                .padding(pad(6.0, 12.0, 6.0, 12.0))
                .style(surface(Some(t::RAISE), Some(t::LINE), 9.0)),
        );
    }
    tail = tail.push(crate::views::composer::view(app, false));
    let tail = container(tail).width(Length::Fill).center_x(Length::Fill).padding(pad(0.0, 16.0, 14.0, 16.0));
    column![thread, tail].height(Length::Fill).into()
}

fn user_msg<'a>(app: &'a App, mi: usize, s: String, content_w: f32) -> El<'a> {
    let bubble = container(txf(s.clone(), 15.0, t::FG, t::UI)).padding(pad(10.0, 15.0, 10.0, 15.0)).max_width(content_w * 0.8).style(surface(Some(t::RAISE2), None, t::RADIUS_MAX));
    let show = app.hover == Some((2, mi));
    let acts: El = if show {
        row![tip(ib_sz("edit", 26.0, 16.0, (!app.is_live()).then(|| Msg::Toast("Editar e reenviar cria um ramo da conversa".into())), false), "Editar e reenviar (em breve)"), tip(ib_sz("copy", 26.0, 16.0, Some(Msg::CopyText(s)), false), "Copiar"), tip(ib_sz("pin", 26.0, 16.0, (!app.is_live()).then(|| Msg::Toast("Fixada como contexto permanente".into())), false), "Fixar como contexto (em breve)")].into()
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
            button(row![tx(o.tools_label.clone(), 13.0, t::FG3), crate::icons::icon_rot("chevR", 16.0, t::FG3, { let p = if app.at.tools.is_some() { crate::anim::TOOLS.get() } else { 1.0 }; 90.0 * if open { p } else { 1.0 - p } })].spacing(6).align_y(Vertical::Center))
                .padding(0)
                .on_press(Msg::ToggleTools(ci, mi))
                .style(|_, _| button::Style { text_color: t::FG3, ..Default::default() }),
        );
        if open {
            let mut list = column![].spacing(0);
            for tl in &o.tools {
                list = list.push(
                    row![
                        container(tx(tl.kind.clone(), 12.5, t::FG3)).width(Length::Fixed(130.0)),
                        container(txf(ell(&tl.value, content_w - 250.0, 12.0), 12.0, t::FG2, t::MONO)).width(Length::Fill),
                        tx(tl.time.clone(), 11.5, t::FG3)
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
        if s.total != usize::MAX {
            let words: Vec<&str> = o.body.split_whitespace().take(s.words).collect();
            text = words.join(" ");
        }
        text.push_str(if (app.now.saturating_duration_since(app.t0).as_millis() / 500) % 2 == 0 { " █" } else { "  " });
    }
    let blocks = md::parse(&text);
    col = col.push(md::render_with(&blocks, md::Style::chat(f, fb, app.p.chat_fs()), Some(Msg::OpenUrl), Some(Msg::CopyText)));

    if let Some(a) = &o.approval {
        col = col.push(approval(a, ci, mi));
    }
    if streaming.is_none() && o.time != "pendente" {
        let mut acts = row![tip(ib_sz("copy", 28.0, 16.0, Some(Msg::CopyText(o.body.clone())), false), "Copiar"), tip(ib_sz("speaker", 28.0, 16.0, (!app.is_live()).then(|| Msg::Toast("Lendo em voz alta (simulado)".into())), false), "Ouvir (em breve)")].spacing(0).align_y(Vertical::Center);
        if is_last {
            acts = acts.push(tip(ib_sz("retry", 28.0, 16.0, (!app.is_live()).then(|| Msg::Toast("Gerar de novo…".into())), false), "Gerar de novo (em breve)"));
        }
        acts = acts.push(container(tx(o.time.clone(), 11.5, t::FG3)).padding(pad(0.0, 0.0, 0.0, 6.0)));
        let via_txt = if o.via.is_empty() { None } else { Some(o.via.clone()) };
        if let Some(v) = via_txt {
            let mut vr = row![tx(v.clone(), 11.5, t::FG4)].spacing(4).align_y(Vertical::Center);
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
    let txt = md::render(&md::parse(&a.text), md::Style { font: t::UI, bold: t::UI_SEMI, size: 13.5, line: 1.5, h_font: t::UI_MED, h1: 16.0, h2: 15.0, h3: 14.0, gap: 4.0 });
    match &a.state {
        ApprState::Open => {
            let mut btns = iced::widget::Row::with_children(approval_actions(a, Msg::Approve(ci, mi), Msg::Deny(ci, mi))).spacing(8);
            if a.diff_button {
                btns = btns.push(btn("Ver o diff", Kind::Ghost, true, None, Some(Msg::ShowDiff)));
            }
            container(column![row![dot_halo(t::SIGNAL, t::SIGNAL_SOFT), txf("Aguardando sua aprovação", 13.0, t::SIGNAL, t::UI_MED)].spacing(8).align_y(Vertical::Center), container(txt).padding(pad(0.0, 0.0, 0.0, 0.0)), approval_detail(a), container(btns).padding(pad(2.0, 0.0, 0.0, 0.0))].spacing(8))
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
