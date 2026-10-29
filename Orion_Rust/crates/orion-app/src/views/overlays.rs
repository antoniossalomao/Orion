//! Camadas por cima da janela: paleta Ctrl K (§19), captura rápida e "Isso?" (§20), mini barra de voz, legenda e toast (§21).

use crate::app::{App, CapKind, Msg, View};
use crate::icons::icon;
use crate::theme as t;
use crate::ui::*;
use crate::views::login::field_style;
use iced::alignment::{Horizontal, Vertical};
use iced::mouse;
use iced::widget::canvas::{self, Frame, Geometry, Path};
use iced::widget::{button, canvas as cv, column, container, mouse_area, opaque, row, scrollable, stack, text_input};
use iced::{Background, Border, Color, Length, Point, Rectangle, Renderer, Task, Theme};

pub struct PalItem {
    pub icon: &'static str,
    pub title: String,
    pub group: String,
    pub msg: Msg,
}

pub fn pal_items(app: &App) -> Vec<PalItem> {
    // `>` no começo restringe a paleta às ações (F01.21).
    let raw = app.pal.q.trim().to_lowercase();
    let only_actions = raw.starts_with('>');
    let q = raw.trim_start_matches('>').trim().to_string();
    let mut out: Vec<PalItem> = vec![];
    if !only_actions {
        for (i, c) in app.convs.iter().enumerate() {
            let hay = format!("{} {} {}", c.title, c.proj, c.msgs.iter().map(|m| match m { crate::data::ChatMsg::User(s) => s.to_string(), crate::data::ChatMsg::Orion(o) => o.body.to_string() }).collect::<Vec<_>>().join(" ")).to_lowercase();
            if q.is_empty() || hay.contains(&q) {
                out.push(PalItem { icon: "chat", title: c.title.to_string(), group: format!("Conversa · {}", c.proj), msg: Msg::OpenConv(i) });
            }
            if out.len() >= 6 {
                break;
            }
        }
        if !q.is_empty() {
            for n in app.mem.nodes.iter().filter(|n| n.label.to_lowercase().contains(&q)).take(6) {
                out.push(PalItem { icon: "file", title: n.label.clone(), group: format!("Nota · {}", n.folder.split('/').next().unwrap_or("")), msg: Msg::EditorOpen(format!("{}/{}.md", n.folder, n.label)) });
            }
        }
    }
    let mut acts: Vec<PalItem> = vec![
        PalItem { icon: "plus", title: crate::i18n::text("Nova conversa").into(), group: crate::i18n::text("Ação").into(), msg: Msg::NewChat },
        PalItem { icon: "gear", title: crate::i18n::text("Configurações").into(), group: crate::i18n::text("Ação").into(), msg: Msg::OpenSettings(None) },
        PalItem { icon: "inbox", title: crate::i18n::text("Captura rápida").into(), group: crate::i18n::text("Ação · Ctrl + Alt + O").into(), msg: Msg::QcapOpen },
        PalItem { icon: "scan", title: crate::i18n::text("Isso? (capturar a tela)").into(), group: crate::i18n::text("Ação · Ctrl + Alt + I").into(), msg: Msg::IssoOpen },
        PalItem { icon: "bolt", title: if app.p.auto { crate::i18n::text("Desligar modo autônomo").into() } else { crate::i18n::text("Ligar modo autônomo").into() }, group: crate::i18n::text("Ação").into(), msg: Msg::Toggle("auto") },
    ];
    let nav: [(View, &str, &str); 8] = [
        (View::Projetos, "folder", crate::i18n::text("Projetos")),
        (View::Memoria, "graph", crate::i18n::text("Memória")),
        (View::Aprovacoes, "shield", crate::i18n::text("Aprovações")),
        (View::Rotinas, "clock", crate::i18n::text("Rotinas")),
        (View::Nuvem, "cloud", crate::i18n::text("Segundo plano")),
        (View::Conhecimento, "book", crate::i18n::text("Conhecimento")),
        (View::Arquivadas, "archive", crate::i18n::text("Arquivadas")),
        (View::Home, "plus", crate::i18n::text("Início")),
    ];
    for (v, i, l) in nav.iter().take(7) {
        acts.push(PalItem { icon: i, title: format!("Ir para {l}"), group: "Ir para".into(), msg: Msg::Go(*v) });
    }
    out.extend(acts.into_iter().filter(|c| q.is_empty() || format!("{} {}", c.title, c.group).to_lowercase().contains(&q)));
    out
}

pub fn pal_run(app: &mut App, i: usize) -> Task<Msg> {
    let items = pal_items(app);
    app.pal.open = false;
    match items.into_iter().nth(i) {
        Some(it) => app.update(it.msg),
        None => Task::none(),
    }
}

fn scrim_box<'a>(inner: El<'a>, top: f32, bg: Color, close: Msg) -> El<'a> {
    let (op, sp) = (crate::anim::OVER.get(), crate::anim::SCRIM.get());
    let inner = rise(fade(inner, op, t::RAISE), op, 6.0);
    let col = column![sp_h(top), container(opaque(mouse_area(inner).on_press(Msg::Noop))).width(Length::Fill).center_x(Length::Fill)];
    opaque(mouse_area(container(col).width(Length::Fill).height(Length::Fill).style(move |_| container::Style { background: Some(t::alpha(bg, sp).into()), ..Default::default() })).on_press(close))
}

pub fn palette(app: &App) -> El<'_> {
    let items = pal_items(app);
    let sel = app.pal.sel.min(items.len().saturating_sub(1));
    let input = text_input(crate::i18n::text("Buscar conversas e notas do vault"), &app.pal.q)
        .on_input(Msg::PalQ)
        .on_submit(Msg::PalRun(sel))
        .size(t::snap_size(15 as f32))
        .font(t::UI)
        .padding(pad(15.0, 0.0, 15.0, 0.0))
        .style(|th, st| {
            let mut s = field_style(Color::TRANSPARENT, Color::TRANSPARENT, Color::TRANSPARENT, 0.0)(th, st);
            s.border.width = 0.0;
            s
        });
    let head = container(row![icon("search", 16.0, t::FG3), input, kbd("Esc")].spacing(10).align_y(Vertical::Center)).padding(pad(0.0, 16.0, 0.0, 16.0));
    let mut list = column![].spacing(0);
    if items.is_empty() {
        list = list.push(container(tx(crate::i18n::text("Nada encontrado"), 13.5, t::FG2)).padding(pad(8.0, 10.0, 8.0, 10.0)));
    }
    for (i, it) in items.iter().enumerate() {
        let on = i == sel;
        list = list.push(
            button(row![icon(it.icon, 14.0, if on { t::FG } else { t::FG2 }), container(tx(ell(&it.title, 380.0, 13.5), 13.5, if on { t::FG } else { t::FG2 })).width(Length::Fill), tx(it.group.clone(), 11.5, t::FG3)].spacing(10).align_y(Vertical::Center))
                .width(Length::Fill)
                .padding(pad(8.0, 10.0, 8.0, 10.0))
                .on_press(Msg::PalRun(i))
                .style(move |_, st| button::Style { background: if on { Some(Background::Color(t::ACCENT_SOFT)) } else if matches!(st, button::Status::Hovered) { Some(Background::Color(t::HOVER)) } else { None }, text_color: t::FG, border: Border { radius: t::R_CTRL.into(), ..Default::default() }, shadow: Default::default(), snap: true }),
        );
    }
    let body = scrollable(container(list).padding(6)).direction(scrollable::Direction::Vertical(thin_bar())).style(thin_scroll).height(Length::Shrink);
    let boxed = container(column![head, hline(), container(body).max_height(372.0)]).width(Length::Fixed(600.0f32.min(app.win.width - 32.0))).clip(true).style(|_| container::Style { background: Some(t::RAISE.into()), border: Border { color: t::LINE2, width: 1.0, radius: t::R_PANEL.into() }, shadow: shadow(), ..Default::default() });
    scrim_box(boxed.into(), app.win.height * 0.14, t::SCRIM, Msg::PalClose)
}

pub fn quick_capture(app: &App) -> El<'_> {
    let q = app.qcap.as_ref().unwrap();
    let input = text_input(crate::i18n::text("Uma linha: tarefa, lembrete, gasto ou nota"), &q.text)
        .on_input(Msg::QcapText)
        .on_submit(Msg::QcapSave)
        .size(16)
        .font(t::UI)
        .padding(pad(15.0, 0.0, 15.0, 0.0))
        .style(|th, st| {
            let mut s = field_style(Color::TRANSPARENT, Color::TRANSPARENT, Color::TRANSPARENT, 0.0)(th, st);
            s.border.width = 0.0;
            s
        });
    let head = container(row![icon("inbox", 16.0, t::FG3), input, kbd("Enter")].spacing(10).align_y(Vertical::Center)).padding(pad(0.0, 12.0, 0.0, 12.0));
    let kinds = [crate::i18n::text("Tarefa"), crate::i18n::text("Lembrete"), crate::i18n::text("Gasto"), crate::i18n::text("Nota")];
    let cur = match q.kind {
        CapKind::Tarefa => 0,
        CapKind::Lembrete => 1,
        CapKind::Gasto => 2,
        CapKind::Nota => 3,
    };
    let foot = container(row![seg(&kinds, cur, Msg::QcapKind), grow(), tx(crate::i18n::text("Vai para "), 12.0, t::FG3), txf("00 Inbox", 12.0, t::FG3, t::MONO), tx(" · Ctrl + Alt + O", 12.0, t::FG3)].spacing(6).align_y(Vertical::Center)).padding(pad(8.0, 12.0, 8.0, 12.0));
    let boxed = container(column![head, hline(), foot]).width(Length::Fixed(620.0f32.min(app.win.width - 32.0))).clip(true).style(|_| container::Style { background: Some(t::RAISE.into()), border: Border { color: t::LINE2, width: 1.0, radius: t::R_PANEL.into() }, shadow: shadow(), ..Default::default() });
    scrim_box(boxed.into(), app.win.height * 0.22, Color::from_rgba(0.0, 0.0, 0.0, 0.25), Msg::QcapClose)
}

pub fn auto_senha(app: &App) -> El<'_> {
    let v = app.auto_senha.as_deref().unwrap_or("");
    let input = text_input(crate::i18n::text("Sua senha"), v)
        .on_input(Msg::AutoSenhaText)
        .on_submit(Msg::AutoSenhaOk)
        .secure(true)
        .size(t::snap_size(15 as f32))
        .font(t::UI)
        .padding(pad(12.0, 12.0, 12.0, 12.0));
    let corpo = column![
        tx(crate::i18n::text("Ligar o modo autônomo"), 15.0, t::FG),
        tx(crate::i18n::text("Digite a senha de novo. Com o modo ligado o Orion segue sozinho em tarefas longas; ações que podem quebrar o PC ou o projeto continuam pedindo confirmação."), 12.0, t::FG3),
        input,
        row![grow(), btn(crate::i18n::text("Cancelar"), Kind::Normal, true, None, Some(Msg::AutoSenhaClose)), btn(crate::i18n::text("Ligar"), Kind::Primary, !v.is_empty(), None, Some(Msg::AutoSenhaOk))].spacing(8),
    ]
    .spacing(12);
    let boxed = container(corpo).padding(pad(18.0, 18.0, 18.0, 18.0)).width(Length::Fixed(440.0f32.min(app.win.width - 32.0))).style(|_| container::Style { background: Some(t::RAISE.into()), border: Border { color: t::LINE2, width: 1.0, radius: t::R_PANEL.into() }, shadow: shadow(), ..Default::default() });
    scrim_box(boxed.into(), app.win.height * 0.22, Color::from_rgba(0.0, 0.0, 0.0, 0.35), Msg::AutoSenhaClose)
}

pub fn isso(app: &App) -> El<'_> {
    let i = app.isso.as_ref().unwrap();
    let (w, h) = (app.win.width, app.win.height);
    let (x, y, sw, sh) = ((w * 0.30).round(), (h * 0.28).round(), (w * 0.34).round(), (h * 0.26).round());
    let dim = |ww: f32, hh: f32| container(sp_w(0.0)).width(Length::Fixed(ww.max(0.0))).height(Length::Fixed(hh.max(0.0))).style(|_| container::Style { background: Some(Color::from_rgba(0.0, 0.0, 0.0, 0.25).into()), ..Default::default() });
    // fora da seleção escurece mais (box-shadow 0 0 0 9999px)
    let around: El = column![dim(w, y), row![dim(x, sh), sp_w(sw), dim(w - x - sw, sh)], dim(w, h - y - sh)].into();
    let sel = container(sp_w(0.0)).width(Length::Fixed(sw)).height(Length::Fixed(sh)).style(|_| container::Style { background: Some(t::alpha(t::PRIMARY, 0.08).into()), border: Border { color: t::PRIMARY, width: 1.5, radius: 4.0.into() }, ..Default::default() });
    let sel_pos = container(sel).padding(pad(y, 0.0, 0.0, x));
    let hint = container(container(tx(crate::i18n::text("Arraste para escolher a área · Esc cancela · Ctrl + Alt + I"), 12.5, t::FG2)).padding(pad(6.0, 12.0, 6.0, 12.0)).style(surface(Some(t::RAISE), Some(t::LINE2), 999.0))).width(Length::Fill).center_x(Length::Fill).padding(pad(18.0, 0.0, 0.0, 0.0));
    let input = text_input(crate::i18n::text("O que é isso? (Enter pergunta)"), &i.q).on_input(Msg::IssoQ).on_submit(Msg::IssoAsk).size(t::snap_size(13.5 as f32)).font(t::UI).padding(pad(8.0, 10.0, 8.0, 10.0)).style(field_style(t::BG, t::FIELD_LINE, t::ACCENT, 9.0));
    let mut ask = column![input].spacing(8);
    if i.answered {
        ask = ask.push(txf(crate::i18n::text("É o painel de RAM do Orion: 296 MB em uso, com o índice vetorial em mmap ocupando a maior parte. Nada fora do normal."), 15.0, t::FG, t::SERIF).line_height(iced::widget::text::LineHeight::Relative(1.55)));
    }
    ask = ask.push(row![grow(), btn(crate::i18n::text("Fechar"), Kind::Ghost, true, None, Some(Msg::IssoClose)), btn(crate::i18n::text("Levar para o chat"), Kind::Normal, true, None, Some(Msg::Toast(crate::i18n::text("Captura anexada a uma conversa nova (simulado)").into())))].spacing(6));
    let ask_box = container(ask).width(Length::Fixed(420.0f32.min(w - 32.0))).padding(10).style(|_| container::Style { background: Some(t::RAISE.into()), border: Border { color: t::LINE2, width: 1.0, radius: t::R_PANEL.into() }, shadow: shadow(), ..Default::default() });
    let ask_pos = container(ask_box).padding(pad((y + sh + 10.0).min(h - 170.0), 0.0, 0.0, x.min(w - 436.0)));
    let base = container(sp_w(0.0)).width(Length::Fill).height(Length::Fill).style(|_| container::Style { background: Some(Color::from_rgba(0.0, 0.0, 0.0, 0.45).into()), ..Default::default() });
    opaque(stack![base, around, sel_pos, hint, ask_pos])
}

/// Sete pontos de céu da mini barra (72×32): (10,8) signal · (62,9) star · cinturão (28,17) (36,16) (44,15) ok · (14,27) star · (60,26) conceito.
struct IslandSky;
impl canvas::Program<Msg> for IslandSky {
    type State = ();
    fn draw(&self, _s: &(), r: &Renderer, _t: &Theme, b: Rectangle, _c: mouse::Cursor) -> Vec<Geometry> {
        let mut f = Frame::new(r, b.size());
        for (x, y, rad, c) in [(10.0, 8.0, 2.4, t::SIGNAL), (62.0, 9.0, 1.4, t::STAR), (28.0, 17.0, 2.0, t::OK), (36.0, 16.0, 2.0, t::OK), (44.0, 15.0, 2.0, t::OK), (14.0, 27.0, 1.4, t::STAR), (60.0, 26.0, 2.4, t::N_CONCEITO)] {
            f.fill(&Path::circle(Point::new(x, y), rad * 3.0), t::alpha(c, 0.18));
            f.fill(&Path::circle(Point::new(x, y), rad), c);
        }
        vec![f.into_geometry()]
    }
}

pub fn island(app: &App) -> El<'_> {
    let status=app.resources.audio.as_ref().map(|v|format!("{} · {}",crate::i18n::text(match v["status"].as_str().unwrap_or(""){"recording"=>"Gravando…","transcribing"=>"Transcrevendo…","thinking"=>"Pensando…","speaking"=>"Falando…",_=>"Voz"}),v["reply"].as_str().or_else(||v["transcript"].as_str()).unwrap_or(""))).unwrap_or_default();
    let pill = container(row![cv(IslandSky).width(72).height(32), tx(status, 13.0, t::FG2), tip(ib("close", Some(Msg::Live), false), crate::i18n::text("Parar de ouvir"))].spacing(12).align_y(Vertical::Center))
        .height(Length::Fixed(44.0))
        .padding(pad(0.0, 8.0, 0.0, 10.0))
        .style(|_| container::Style { background: Some(t::ISLAND_BG.into()), border: Border { color: t::LINE2, width: 1.0, radius: t::R_PILL.into() }, shadow: shadow(), ..Default::default() });
    container(pill).width(Length::Fill).center_x(Length::Fill).padding(pad(10.0, 0.0, 0.0, 0.0)).into()
}

pub fn caption<'a>(lines:&[String]) -> El<'a> {
    let c = container(tx(lines.iter().rev().take(2).rev().cloned().collect::<Vec<_>>().join("\n"),16.0,Color::WHITE)).padding(pad(8.0, 16.0, 8.0, 16.0)).style(|_| container::Style { background: Some(Color::from_rgba(0.0, 0.0, 0.0, 0.78).into()), border: Border { radius: t::R_CARD.into(), ..Default::default() }, ..Default::default() });
    container(c).width(Length::Fill).height(Length::Fill).center_x(Length::Fill).align_y(Vertical::Bottom).padding(pad(0.0, 0.0, 28.0, 0.0)).into()
}

/// F07.06: a seção de Configurações que resolve o aviso (cota, modelo/chave, ferramentas, vault).
pub fn config_target(msg: &str) -> Option<crate::app::Sec> {
    use crate::app::Sec;
    let m = msg.to_lowercase();
    if m.contains("cota") || m.contains("orçamento") || m.contains("limite") {
        Some(Sec::Uso)
    } else if m.contains("chave de api") || (m.contains("modelo") && m.contains("configur")) {
        Some(Sec::Modelos)
    } else if m.contains("ferramenta") {
        Some(Sec::Tools)
    } else if m.contains("vault") {
        Some(Sec::Sistema)
    } else {
        None
    }
}

pub fn toast<'a>(msg: String) -> El<'a> {
    let target = config_target(&msg);
    let msg = crate::i18n::status(&msg);
    let p = crate::anim::TOAST.get();
    let mut linha = row![tx(msg, 13.0, t::alpha(t::PRIMARY_INK, p))].spacing(10).align_y(Vertical::Center);
    if let Some(sec) = target {
        linha = linha.push(btn(crate::i18n::text("Abrir configurações"), Kind::Ghost, true, None, Some(Msg::OpenSettings(Some(sec)))));
    }
    let c = container(linha).padding(pad(8.0, 12.0, 8.0, 12.0)).style(move |_| container::Style { background: Some(t::alpha(t::PRIMARY, p).into()), border: Border { radius: t::R_CARD.into(), ..Default::default() }, shadow: shadow(), ..Default::default() });
    container(c).width(Length::Fill).height(Length::Fill).center_x(Length::Fill).align_y(Vertical::Bottom).padding(pad(0.0, 0.0, 20.0 - 6.0 * (1.0 - p), 0.0)).into()
}

pub fn _h() -> Horizontal {
    Horizontal::Left
}
