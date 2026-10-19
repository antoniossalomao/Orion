//! Painel da direita (§14): Contexto, Arquivos, Diff e Terminal.

use crate::app::{App, CtxTab, Msg};
use crate::data::{ApprState, ChatMsg};
use crate::icons::icon;
use crate::theme as t;
use crate::ui::*;
use crate::views::login::field_style;
use iced::alignment::Vertical;
use iced::widget::{button, column, container, row, scrollable, text_input};
use iced::{Border, Length};

const TABS: [(CtxTab, &str); 4] = [(CtxTab::Contexto, "Contexto"), (CtxTab::Arquivos, "Arquivos"), (CtxTab::Diff, "Diff"), (CtxTab::Terminal, "Terminal")];

fn kv<'a>(ic: &str, s: El<'a>) -> El<'a> {
    row![icon(ic, 14.0, t::FG3), s].spacing(8).align_y(Vertical::Center).into()
}

pub fn view(app: &App) -> El<'_> {
    let Some(ci) = app.conv else { return sp_w(0.0) };
    let c = &app.convs[ci];
    let mut tabs = row![].spacing(2);
    for (tb, l) in TABS {
        let sel = app.ctx_tab == tb;
        let b = button(container(tx(l, 12.5, if sel { t::FG } else { t::FG3 })).center_y(Length::Fill)).height(Length::Fixed(32.0)).padding(pad(0.0, 10.0, 0.0, 10.0)).on_press(Msg::CtxTab(tb)).style(|_, _| button::Style::default());
        tabs = tabs.push(column![b, container(sp_w(0.0)).width(Length::Fill).height(Length::Fixed(1.5)).style(move |_| container::Style { background: if sel { Some(t::FG.into()) } else { None }, ..Default::default() })].width(Length::Shrink));
    }
    let tabs = column![container(tabs).padding(pad(8.0, 8.0, 0.0, 8.0)), hline()];

    let body: El = match app.ctx_tab {
        CtxTab::Contexto => {
            let pct = if c.ctx == 0 { 8 } else { c.ctx };
            let mut v = column![].spacing(8);
            let mut amb = column![kv("folder", tx(c.proj.clone(), 12.5, t::FG2).into())].spacing(7);
            if let Some(b) = c.branch.clone() {
                amb = amb.push(kv("branch", txf(b, 12.0, t::FG2, t::MONO).into()));
            }
            amb = amb.push(kv("cpu", tx(app.p.model_label(), 12.5, t::FG2).into()));
            v = v.push(card("Ambiente", sp_w(0.0), amb.into()));
            match &c.note {
                Some(n) => {
                    let name = n.path.rsplit('/').next().unwrap_or(&n.path);
                    let inner = column![
                        kv("file", tx(ell(name, 220.0, 12.5), 12.5, t::FG2).into()),
                        row![icon("branch", 14.0, t::FG3), txf(format!("main · {}", n.commit), 12.0, t::FG2, t::MONO), tx(format!("· {}", n.at), 12.5, t::FG3)].spacing(8).align_y(Vertical::Center),
                        row![btn("Abrir nota", Kind::Normal, true, Some("edit"), Some(Msg::EditorOpen(n.path.to_string()))), btn("Obsidian", Kind::Ghost, true, None, Some(Msg::Toast("Abriria no Obsidian".into())))].spacing(6),
                    ]
                    .spacing(7);
                    v = v.push(card("Vault", tx(n.at.clone(), 12.5, t::FG3).into(), inner.into()));
                }
                None => v = v.push(card("Vault", sp_w(0.0), tx("A nota desta conversa é gravada no fim de cada rodada.", 12.5, t::FG3).into())),
            }
            v = v.push(card("Contexto", tx(format!("{pct}%"), 12.5, t::FG3).into(), column![meter(pct as f32, 3.0, t::FG2), tx(format!("{} mil de 200 mil tokens", pct * 2), 11.5, t::FG3)].spacing(7).into()));
            if c.id == "ram" {
                let files = ["src/memory/index.rs", "src/voice/wake.rs", "src/embed/cache.rs"];
                let mut inner = column![].spacing(7);
                for f in files {
                    inner = inner.push(kv("file", txf(f, 12.0, t::FG2, t::MONO).into()));
                }
                inner = inner.push(btn("Ver o diff", Kind::Normal, true, None, Some(Msg::CtxTab(CtxTab::Diff))));
                let right = row![txf("+176", 11.5, t::OK, t::MONO), txf("−0", 11.5, t::DANGER, t::MONO)].spacing(6);
                v = v.push(card("Alterações", right.into(), inner.into()));
            }
            v.into()
        }
        CtxTab::Arquivos => {
            let line = |s: &str, col: iced::Color| -> El<'static> { txf(s.to_string(), 12.5, col, t::MONO).line_height(iced::widget::text::LineHeight::Relative(1.9)).into() };
            let tree = column![
                line("▾ orion/", t::FG),
                line("  ▾ crates/", t::FG),
                line("    ▾ orion-memory/src/", t::FG),
                line("      index.rs  M", t::SIGNAL),
                line("      lib.rs", t::FG2),
                line("    ▾ orion-voice/src/", t::FG),
                line("      wake.rs  M", t::SIGNAL),
                line("    ▾ orion-embed/src/", t::FG),
                line("      cache.rs  M", t::SIGNAL),
                line("    ▸ orion-ai/", t::FG),
                line("    ▸ orion-ui/", t::FG),
                line("  Cargo.toml", t::FG2),
                line("  README.md", t::FG2),
            ];
            let gh = card(
                "GitHub",
                tx("antoniossalomao/Orion", 12.5, t::FG3).into(),
                column![
                    row![badge("CI ok", Tone::Ok), tx("main · 1bda05d", 12.5, t::FG2)].spacing(8).align_y(Vertical::Center),
                    row![badge("PR aberto", Tone::Plain), tx("#29 quirky-dijkstra", 12.5, t::FG2)].spacing(8).align_y(Vertical::Center),
                    row![btn("Abrir PR", Kind::Normal, true, None, Some(Msg::Toast("Abrir PR pede aprovação".into()))), btn("Merge", Kind::Normal, true, None, Some(Msg::Toast("Merge pede aprovação".into())))].spacing(6)
                ]
                .spacing(7)
                .into(),
            );
            column![tree, gh].spacing(8).into()
        }
        CtxTab::Diff => {
            let hd = |s: &str| -> El<'static> { container(txf(s.to_string(), 12.0, t::FG3, t::MONO).wrapping(iced::widget::text::Wrapping::None)).width(Length::Fill).padding(pad(4.0, 10.0, 4.0, 10.0)).style(surface(Some(t::RAISE), None, 0.0)).into() };
            let ln = |s: &str, fg: iced::Color, bg: iced::Color| -> El<'static> { container(txf(s.to_string(), 12.0, fg, t::MONO).wrapping(iced::widget::text::Wrapping::None)).width(Length::Fill).padding(pad(0.0, 10.0, 0.0, 10.0)).style(surface(Some(bg), None, 0.0)).into() };
            let del = t::mix(t::DANGER, 0.10);
            let add = t::mix(t::OK, 0.10);
            let diff = container(column![
                hd("src/memory/index.rs"),
                ln("- let bytes = std::fs::read(&path)?;", t::DANGER, del),
                ln("- let index = VectorIndex::from_bytes(&bytes, 1024)?;", t::DANGER, del),
                ln("+ let file = File::open(&path)?;", t::OK, add),
                ln("+ let map = unsafe { Mmap::map(&file)? };", t::OK, add),
                ln("+ let index = VectorIndex::from_bytes(&map, 1024)?;", t::OK, add),
                hd("src/embed/cache.rs"),
                ln("- cache: HashMap<u64, Vec<f32>>,", t::DANGER, del),
                ln("+ cache: LruCache<u64, Vec<f32>>, // 64 MB", t::OK, add),
            ])
            .width(Length::Fill)
            .clip(true)
            .style(surface(Some(t::BG), Some(t::LINE), 10.0));
            let pending = c.msgs.iter().enumerate().find(|(_, m)| matches!(m, ChatMsg::Orion(o) if matches!(&o.approval, Some(a) if a.state == ApprState::Open)));
            let mut v = column![diff].spacing(8);
            if let Some((mi, _)) = pending {
                v = v.push(row![btn("Aprovar", Kind::Primary, true, None, Some(Msg::Approve(ci, mi))), btn("Negar", Kind::Normal, true, None, Some(Msg::Deny(ci, mi)))].spacing(6));
            }
            v.into()
        }
        CtxTab::Terminal => {
            let mut out = column![].spacing(0);
            for l in &app.term_out {
                out = out.push(txf(if l.is_empty() { " ".to_string() } else { l.clone() }, 12.0, t::TERM_FG, t::MONO).line_height(iced::widget::text::LineHeight::Relative(1.6)));
            }
            let inp = text_input("PowerShell", &app.term_in).on_input(Msg::TermIn).on_submit(Msg::TermRun).size(12).font(t::MONO).padding(0).style(|th, st| {
                let mut s = field_style(iced::Color::TRANSPARENT, iced::Color::TRANSPARENT, iced::Color::TRANSPARENT, 0.0)(th, st);
                s.border.width = 0.0;
                s.value = t::TERM_FG;
                s
            });
            container(column![
                container(scrollable(container(out).padding(10)).direction(scrollable::Direction::Vertical(thin_bar())).style(thin_scroll).anchor_bottom().width(Length::Fill)).height(Length::Fill),
                hline(),
                container(row![txf("PS>", 12.0, t::ACCENT, t::MONO), inp].spacing(6).align_y(Vertical::Center)).padding(pad(6.0, 10.0, 6.0, 10.0)),
            ])
            .width(Length::Fill)
            .height(Length::Fixed(720.0))
            .style(|_| container::Style { background: Some(t::TERM_BG.into()), border: Border { color: t::LINE, width: 1.0, radius: 10.0.into() }, ..Default::default() })
            .into()
        }
    };
    let body = scrollable(container(body).padding(10)).direction(scrollable::Direction::Vertical(thin_bar())).style(thin_scroll).height(Length::Fill);
    row![vline_full(), container(column![tabs, body]).width(Length::Fill).height(Length::Fill).style(|_| container::Style { background: Some(t::SIDE.into()), ..Default::default() })]
        .width(Length::Fixed(t::CTX_W))
        .height(Length::Fill)
        .into()
}

fn vline_full<'a>() -> El<'a> {
    container(sp_w(0.0)).width(Length::Fixed(1.0)).height(Length::Fill).style(|_| container::Style { background: Some(t::LINE.into()), ..Default::default() }).into()
}
