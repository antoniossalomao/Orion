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

const TABS: [(CtxTab, &str); 5] = [(CtxTab::Contexto, "Contexto"), (CtxTab::Arquivos, "Arquivos"), (CtxTab::Diff, "Diff"), (CtxTab::Terminal, "Terminal"), (CtxTab::Artefatos, "Artefatos")];

fn kv<'a>(ic: &str, s: El<'a>) -> El<'a> {
    row![icon(ic, 14.0, t::FG3), s].spacing(8).align_y(Vertical::Center).into()
}

pub fn view(app: &App) -> El<'_> {
    let Some(ci) = app.conv else { return sp_w(0.0) };
    let c = &app.convs[ci];
    let mut tabs = row![].spacing(2);
    for (tb, l) in TABS {
        if tb == CtxTab::Terminal && app.is_live() {
            continue;
        }
        let sel = app.ctx_tab == tb;
        let b = button(container(tx(crate::i18n::text(l), 12.5, if sel { t::FG } else { t::FG3 })).center_y(Length::Fill)).height(Length::Fixed(32.0)).padding(pad(0.0, 6.0, 0.0, 6.0)).on_press(Msg::CtxTab(tb)).style(|_, _| button::Style::default());
        tabs = tabs.push(column![b, container(sp_w(0.0)).width(Length::Fill).height(Length::Fixed(1.5)).style(move |_| container::Style { background: if sel { Some(t::FG.into()) } else { None }, ..Default::default() })].width(Length::Shrink));
    }
    let tabs = column![container(tabs).padding(pad(8.0, 8.0, 0.0, 8.0)), hline()];

    let body: El = match app.ctx_tab {
        CtxTab::Contexto => {
            let pct = if c.ctx == 0 { 8 } else { c.ctx };
            let mut v = column![].spacing(8);
            let mut amb = column![kv("folder", tx(c.proj.clone(), 12.5, t::FG2).into())].spacing(6);
            if let Some(b) = c.branch.clone() {
                amb = amb.push(kv("branch", txf(b, 12.0, t::FG2, t::MONO).into()));
            }
            amb = amb.push(kv("cpu", tx(app.p.model_label(), 12.5, t::FG2).into()));
            v = v.push(card(crate::i18n::text("Ambiente"), sp_w(0.0), amb.into()));
            match &c.note {
                Some(n) => {
                    let name = n.path.rsplit('/').next().unwrap_or(&n.path);
                    let inner = column![
                        kv("file", tx(ell(name, 220.0, 12.5), 12.5, t::FG2).into()),
                        row![icon("branch", 14.0, t::FG3), txf(format!("main · {}", n.commit), 12.0, t::FG2, t::MONO), tx(format!("· {}", n.at), 12.5, t::FG3)].spacing(8).align_y(Vertical::Center),
                        row![btn(crate::i18n::text("Abrir nota"), Kind::Normal, true, Some("edit"), Some(Msg::EditorOpen(n.path.to_string()))), btn("Obsidian", Kind::Ghost, true, None, Some(Msg::Toast(crate::i18n::text("Abriria no Obsidian").into())))].spacing(6),
                    ]
                    .spacing(6);
                    v = v.push(card("Vault", tx(n.at.clone(), 12.5, t::FG3).into(), inner.into()));
                }
                None => v = v.push(card("Vault", sp_w(0.0), tx(crate::i18n::text("A nota desta conversa é gravada no fim de cada rodada."), 12.5, t::FG3).into())),
            }
            v = v.push(card(crate::i18n::text("Contexto"), tx(format!("{pct}%"), 12.5, t::FG3).into(), column![meter(pct as f32, 3.0, t::FG2), tx(crate::i18n::formatted("{} mil de 200 mil tokens", format!("{} mil de 200 mil tokens", pct * 2)), 11.5, t::FG3)].spacing(6).into()));
            if c.id == "ram" {
                let files = ["src/memory/index.rs", "src/voice/wake.rs", "src/embed/cache.rs"];
                let mut inner = column![].spacing(6);
                for f in files {
                    inner = inner.push(kv("file", txf(f, 12.0, t::FG2, t::MONO).into()));
                }
                inner = inner.push(btn(crate::i18n::text("Ver o diff"), Kind::Normal, true, None, Some(Msg::CtxTab(CtxTab::Diff))));
                let right = row![txf("+176", 11.5, t::OK, t::MONO), txf("−0", 11.5, t::DANGER, t::MONO)].spacing(6);
                v = v.push(card(crate::i18n::text("Alterações"), right.into(), inner.into()));
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
                    row![badge(crate::i18n::text("PR aberto"), Tone::Plain), tx("#29 quirky-dijkstra", 12.5, t::FG2)].spacing(8).align_y(Vertical::Center),
                    row![btn(crate::i18n::text("Abrir PR"), Kind::Normal, true, None, Some(Msg::Toast(crate::i18n::text("Abrir PR pede aprovação").into()))), btn("Merge", Kind::Normal, true, None, Some(Msg::Toast(crate::i18n::text("Merge pede aprovação").into())))].spacing(6)
                ]
                .spacing(6)
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
                v = v.push(row![btn(crate::i18n::text("Aprovar"), Kind::Primary, true, None, Some(Msg::Approve(ci, mi))), btn(crate::i18n::text("Negar"), Kind::Normal, true, None, Some(Msg::Deny(ci, mi)))].spacing(6));
            }
            v.into()
        }
        CtxTab::Artefatos => {
            let lista = artifacts(c);
            let mut v = column![].spacing(8);
            if lista.is_empty() {
                v = v.push(container(tx(crate::i18n::text("Nenhum artefato ainda. Blocos de código longos e documentos que o Orion escrever aparecem aqui, com copiar e salvar."), 12.5, t::FG3)).padding(pad(4.0, 4.0, 4.0, 4.0)));
            }
            for (n, (lang, body)) in lista.iter().enumerate() {
                let linhas = body.lines().count();
                let titulo = if lang.is_empty() { "texto".to_string() } else { lang.clone() };
                let previa = body.lines().find(|l| !l.trim().is_empty()).unwrap_or("").to_string();
                let mut botoes = row![btn(crate::i18n::text("Guardar versão"), Kind::Normal, true, None, (!app.artifact_busy).then_some(Msg::ArtifactRecord(n))), btn(crate::i18n::text("Copiar"), Kind::Normal, true, None, Some(Msg::CopyText(body.clone()))), btn(crate::i18n::text("Salvar"), Kind::Ghost, true, None, Some(Msg::ArtifactSave(n)))].spacing(6);
                // F15.10: documento (Markdown ou texto) pode ser lido em voz alta; código, não
                if matches!(lang.as_str(), "" | "md" | "markdown" | "text" | "txt") {
                    botoes = botoes.push(btn(crate::i18n::text("Ouvir"), Kind::Ghost, true, None, Some(Msg::Listen(body.clone()))));
                }
                for record in app.artifact_records.iter().filter(|a| a["session_id"] == c.id) {
                    if let Some(id) = record["id"].as_str() {
                        botoes = botoes.push(btn(&crate::i18n::formatted("Atualizar {}", format!("Atualizar {}", record["title"].as_str().unwrap_or("artefato"))), Kind::Ghost, true, None, (!app.artifact_busy).then(|| Msg::ArtifactUpdate(n, id.into()))));
                    }
                }
                let aberta = app.art_preview == Some(n);
                if lang == "html" {
                    botoes = botoes.push(btn(if aberta { crate::i18n::text("Fechar prévia") } else { crate::i18n::text("Prévia") }, Kind::Ghost, true, None, Some(Msg::ArtifactPreview(n))));
                }
                let mut inner = column![txf(ell(&previa, 250.0, 12.0), 12.0, t::FG2, t::MONO), botoes].spacing(8);
                if aberta && lang == "html" {
                    // prévia estática: sem CSS, sem JavaScript e sem rede; o HTML nunca é executado nem aberto sozinho
                    let blocos = crate::html::to_blocks(body);
                    let est = crate::md::Style { font: t::UI, bold: t::UI_SEMI, size: 13.0, line: 1.5, h_font: t::UI_MED, h1: 17.0, h2: 15.0, h3: 14.0, gap: 6.0 };
                    inner = inner.push(tx(crate::i18n::text("Prévia estática: sem CSS, JavaScript nem imagens. O arquivo salvo mostra o original."), 11.5, t::FG3));
                    inner = inner.push(container(crate::md::render_with(&blocos, est, Some(Msg::OpenUrl), None)).padding(pad(10.0, 10.0, 10.0, 10.0)).width(Length::Fill).style(surface(Some(t::BG), Some(t::LINE), 8.0)));
                }
                v = v.push(card(&crate::i18n::formatted("{titulo} · {linhas} linhas", format!("{titulo} · {linhas} linhas")), tx(format!("#{}", n + 1), 12.0, t::FG3).into(), inner.into()));
            }
            for record in app.artifact_records.iter().filter(|a| a["session_id"] == c.id) {
                let latest = record["version"].as_i64().unwrap_or(1);
                let id = record["id"].as_str().unwrap_or("");
                let mut versions = row![].spacing(4);
                for version in (latest.saturating_sub(7).max(1)..=latest).rev() { versions = versions.push(btn(&format!("v{version}"), Kind::Ghost, true, None, Some(Msg::ArtifactRead(id.into(), version)))); }
                v = v.push(card(record["title"].as_str().unwrap_or("Artefato"), sp_w(0.0), versions.into()));
            }
            if let Some(detail) = app.artifact_detail.as_ref().filter(|a| a["session_id"] == c.id) {
                let body = detail["content"].as_str().unwrap_or("");
                let blocks = crate::md::cached_parse(body);
                v = v.push(tx(format!("{} · v{}", detail["title"].as_str().unwrap_or(crate::i18n::text("Artefato")), detail["version"]), 13.0, t::FG));
                let version = detail["version"].as_i64().unwrap_or(1);
                let id = detail["id"].as_str().unwrap_or("");
                let latest = app.artifact_records.iter().find(|a| a["id"] == id).and_then(|a| a["version"].as_i64()).unwrap_or(version);
                v = v.push(row![btn(crate::i18n::text("Versão anterior"), Kind::Ghost, true, None, (version > 1).then(|| Msg::ArtifactRead(id.into(), version - 1))), btn(crate::i18n::text("Próxima versão"), Kind::Ghost, true, None, (version < latest).then(|| Msg::ArtifactRead(id.into(), version + 1)))].spacing(4));
                v = v.push(btn(crate::i18n::text("Copiar esta versão"), Kind::Normal, true, None, Some(Msg::CopyText(body.into()))));
                v = v.push(crate::md::render(&blocks, crate::md::Style::preview()));
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
            .height(Length::Fill)
            .style(|_| container::Style { background: Some(t::TERM_BG.into()), border: Border { color: t::LINE, width: 1.0, radius: t::R_CARD.into() }, ..Default::default() })
            .into()
        }
    };
    let body: El = if app.ctx_tab == CtxTab::Terminal {
        container(body).padding(10).width(Length::Fill).height(Length::Fill).into()
    } else {
        scrollable(container(body).padding(10)).direction(scrollable::Direction::Vertical(thin_bar())).style(thin_scroll).height(Length::Fill).into()
    };
    row![vline_full(), container(column![tabs, body]).width(Length::Fill).height(Length::Fill).style(|_| container::Style { background: Some(t::SIDE.into()), ..Default::default() })]
        .width(Length::Fixed(t::CTX_W))
        .height(Length::Fill)
        .into()
}

/// Blocos de código e fórmulas das respostas da conversa (de 5 linhas ou mais, ou de linguagens de documento): os artefatos.
pub fn artifacts(c: &crate::data::Conv) -> Vec<(String, String)> {
    let mut out = vec![];
    for m in &c.msgs {
        if let ChatMsg::Orion(o) = m {
            if o.body.chars().count() >= 2000 && !o.body.trim_start().starts_with("```") { out.push(("markdown".into(), o.body.clone())); }
            for b in crate::md::cached_parse(&o.body).iter() {
                if let crate::md::Block::Code(lang, lines) = b {
                    if lines.len() >= 5 || matches!(lang.as_str(), "html" | "md" | "markdown" | "json" | "mermaid" | "latex" | "tex") {
                        out.push((lang.clone(), lines.join("\n")));
                    }
                }
            }
        }
    }
    out
}

/// Extensão do arquivo salvo, pela linguagem do bloco.
pub fn ext_of(lang: &str) -> &'static str {
    match lang {
        "rust" | "rs" => "rs",
        "python" | "py" => "py",
        "javascript" | "js" => "js",
        "typescript" | "ts" => "ts",
        "html" => "html",
        "json" => "json",
        "md" | "markdown" => "md",
        "mermaid" => "mmd",
        "latex" | "tex" => "tex",
        "sh" | "bash" | "shell" => "sh",
        "powershell" | "ps1" => "ps1",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "sql" => "sql",
        "css" => "css",
        _ => "txt",
    }
}

fn vline_full<'a>() -> El<'a> {
    container(sp_w(0.0)).width(Length::Fixed(1.0)).height(Length::Fill).style(|_| container::Style { background: Some(t::LINE.into()), ..Default::default() }).into()
}
