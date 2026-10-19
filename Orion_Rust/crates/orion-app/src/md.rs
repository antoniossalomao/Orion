//! Marcação mínima das respostas e da prévia do editor: `**negrito**`, `` `código` ``, `[[wikilink]]`, títulos, citação,
//! listas, bloco de código e frontmatter (§13.3, §17).

use crate::theme as t;
use iced::widget::{column, container, rich_text, row, scrollable, span, text, Column};
use iced::{Border, Color, Element, Font, Length, Padding};

#[derive(Clone, Debug, PartialEq)]
pub enum Inline {
    Text(String),
    Bold(String),
    Code(String),
    Wiki(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    P(Vec<Inline>),
    H(u8, Vec<Inline>),
    Quote(Vec<Inline>),
    Ul(Vec<Vec<Inline>>),
    Ol(Vec<Vec<Inline>>),
    Code(Vec<String>),
    Front(Vec<String>),
}

pub fn inline(src: &str) -> Vec<Inline> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    let flush = |buf: &mut String, out: &mut Vec<Inline>| {
        if !buf.is_empty() {
            out.push(Inline::Text(std::mem::take(buf)));
        }
    };
    while i < chars.len() {
        let rest: String = chars[i..].iter().collect();
        if rest.starts_with("**") {
            if let Some(end) = rest[2..].find("**") {
                flush(&mut buf, &mut out);
                out.push(Inline::Bold(rest[2..2 + end].to_string()));
                i += rest[..2 + end + 2].chars().count();
                continue;
            }
        } else if rest.starts_with('`') {
            if let Some(end) = rest[1..].find('`') {
                flush(&mut buf, &mut out);
                out.push(Inline::Code(rest[1..1 + end].to_string()));
                i += rest[..1 + end + 1].chars().count();
                continue;
            }
        } else if rest.starts_with("[[") {
            if let Some(end) = rest[2..].find("]]") {
                flush(&mut buf, &mut out);
                let inner = &rest[2..2 + end];
                let label = inner.split('|').next_back().unwrap_or(inner);
                out.push(Inline::Wiki(label.to_string()));
                i += rest[..2 + end + 2].chars().count();
                continue;
            }
        }
        buf.push(chars[i]);
        i += 1;
    }
    flush(&mut buf, &mut out);
    out
}

pub fn parse(src: &str) -> Vec<Block> {
    let lines: Vec<&str> = src.lines().collect();
    let mut blocks = Vec::new();
    let mut i = 0;
    if lines.first().map(|l| l.trim() == "---").unwrap_or(false) {
        if let Some(end) = lines.iter().skip(1).position(|l| l.trim() == "---") {
            blocks.push(Block::Front(lines[1..1 + end].iter().map(|s| s.to_string()).collect()));
            i = end + 2;
        }
    }
    while i < lines.len() {
        let l = lines[i];
        if l.trim().is_empty() {
            i += 1;
        } else if l.starts_with("```") {
            let mut code = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].starts_with("```") {
                code.push(lines[i].to_string());
                i += 1;
            }
            i += 1;
            blocks.push(Block::Code(code));
        } else if let Some(h) = l.strip_prefix("### ") {
            blocks.push(Block::H(3, inline(h)));
            i += 1;
        } else if let Some(h) = l.strip_prefix("## ") {
            blocks.push(Block::H(2, inline(h)));
            i += 1;
        } else if let Some(h) = l.strip_prefix("# ") {
            blocks.push(Block::H(1, inline(h)));
            i += 1;
        } else if let Some(q) = l.strip_prefix("> ") {
            blocks.push(Block::Quote(inline(q)));
            i += 1;
        } else if l.starts_with("- ") {
            let mut items = Vec::new();
            while i < lines.len() && lines[i].starts_with("- ") {
                items.push(inline(&lines[i][2..]));
                i += 1;
            }
            blocks.push(Block::Ul(items));
        } else if is_ol(l) {
            let mut items = Vec::new();
            while i < lines.len() && is_ol(lines[i]) {
                let p = lines[i].find(". ").unwrap_or(0) + 2;
                items.push(inline(&lines[i][p..]));
                i += 1;
            }
            blocks.push(Block::Ol(items));
        } else {
            let mut p = String::from(l);
            i += 1;
            while i < lines.len() && !lines[i].trim().is_empty() && !is_block_start(lines[i]) {
                p.push(' ');
                p.push_str(lines[i]);
                i += 1;
            }
            blocks.push(Block::P(inline(&p)));
        }
    }
    blocks
}

fn is_ol(l: &str) -> bool {
    let d = l.chars().take_while(|c| c.is_ascii_digit()).count();
    d > 0 && l[d..].starts_with(". ")
}
fn is_block_start(l: &str) -> bool {
    l.starts_with("```") || l.starts_with('#') || l.starts_with("> ") || l.starts_with("- ") || is_ol(l)
}

/// Parâmetros de aparência (chat usa Newsreader; a prévia do editor também).
#[derive(Clone, Copy)]
pub struct Style {
    pub font: Font,
    pub bold: Font,
    pub size: f32,
    pub line: f32,
    pub h_font: Font,
    pub h1: f32,
    pub h2: f32,
    pub h3: f32,
    pub gap: f32,
}

impl Style {
    pub fn chat(font: Font, bold: Font, size: f32) -> Self {
        Style { font, bold, size, line: 1.65, h_font: t::SERIF_MED, h1: 28.0, h2: 21.0, h3: size, gap: size * 0.8 }
    }
    pub fn preview() -> Self {
        Style { font: t::SERIF, bold: t::SERIF_SEMI, size: 16.0, line: 1.65, h_font: t::SERIF_MED, h1: 28.0, h2: 21.0, h3: 18.0, gap: 12.8 }
    }
}

fn spans<'a>(inl: &[Inline], st: &Style, base: Color) -> Vec<iced::widget::text::Span<'a, (), Font>> {
    inl.iter()
        .map(|i| match i {
            Inline::Text(s) => span(s.clone()).font(st.font).size(st.size).color(base),
            Inline::Bold(s) => span(s.clone()).font(st.bold).size(st.size).color(base),
            Inline::Code(s) => span(s.clone())
                .font(t::MONO)
                .size(st.size * 0.78)
                .color(base)
                .background(iced::Background::Color(t::RAISE2))
                .border(Border { radius: 5.0.into(), ..Default::default() })
                .padding(Padding { top: 1.0, bottom: 1.0, left: 4.0, right: 4.0 }),
            Inline::Wiki(s) => span(s.clone()).font(st.font).size(st.size).color(t::ACCENT).underline(true),
        })
        .collect()
}

fn code_line<'a, M: 'a>(l: &str) -> Element<'a, M> {
    const KW: &[&str] = &["let", "unsafe", "fn", "pub", "use", "mut", "impl", "struct", "enum", "match", "if", "else", "for", "in", "return", "const", "static", "async", "await"];
    if l.trim_start().starts_with("//") {
        return text(l.to_string()).font(t::MONO).size(12.5).color(t::FG3).into();
    }
    let mut parts: Vec<iced::widget::text::Span<'a, (), Font>> = Vec::new();
    let mut word = String::new();
    let flush = |w: &mut String, parts: &mut Vec<iced::widget::text::Span<'a, (), Font>>| {
        if w.is_empty() {
            return;
        }
        let col = if KW.contains(&w.as_str()) {
            t::ACCENT
        } else if w.chars().all(|c| c.is_ascii_digit()) {
            t::SIGNAL
        } else {
            t::FG
        };
        parts.push(span(std::mem::take(w)).font(t::MONO).size(12.5).color(col));
    };
    for c in l.chars() {
        if c.is_alphanumeric() || c == '_' {
            word.push(c);
        } else {
            flush(&mut word, &mut parts);
            parts.push(span(c.to_string()).font(t::MONO).size(12.5).color(t::FG));
        }
    }
    flush(&mut word, &mut parts);
    if parts.is_empty() {
        parts.push(span(" ").font(t::MONO).size(12.5));
    }
    rich_text(parts).into()
}

pub fn render<'a, M: Clone + 'a>(blocks: &[Block], st: Style) -> Element<'a, M> {
    let mut col: Column<'a, M> = column![].spacing(0);
    let n = blocks.len();
    for (idx, b) in blocks.iter().enumerate() {
        let last = idx + 1 == n;
        let below = if last { 0.0 } else { st.gap };
        let el: Element<'a, M> = match b {
            Block::P(inl) => rich_text(spans(inl, &st, t::FG)).line_height(iced::widget::text::LineHeight::Relative(st.line)).into(),
            Block::H(lv, inl) => {
                let (sz, top) = match lv {
                    1 => (st.h1, 0.0),
                    2 => (st.h2, 8.0),
                    _ => (st.h3, 6.0),
                };
                let sp: Vec<iced::widget::text::Span<'a, (), Font>> = inl.iter().map(|i| match i {
                    Inline::Text(s) | Inline::Bold(s) | Inline::Code(s) | Inline::Wiki(s) => span(s.clone()).font(st.h_font).size(sz).color(t::FG),
                }).collect();
                container(rich_text(sp)).padding(Padding { top, bottom: 2.0, ..Default::default() }).into()
            }
            Block::Quote(inl) => container(rich_text(spans(inl, &st, t::FG2)).line_height(iced::widget::text::LineHeight::Relative(st.line)))
                .padding(Padding { top: 6.0, bottom: 6.0, left: 12.0, right: 0.0 })
                .style(|_| container::Style { border: Border { color: t::LINE3, width: 0.0, radius: 0.0.into() }, ..Default::default() })
                .into(),
            Block::Ul(items) | Block::Ol(items) => {
                let ordered = matches!(b, Block::Ol(_));
                let mut c = column![].spacing(st.size * 0.3);
                for (k, it) in items.iter().enumerate() {
                    let mark = if ordered { format!("{}.", k + 1) } else { "•".into() };
                    c = c.push(row![
                        container(text(mark).font(st.font).size(st.size).color(t::FG3)).width(Length::Fixed(st.size * 1.3)),
                        rich_text(spans(it, &st, t::FG)).line_height(iced::widget::text::LineHeight::Relative(st.line)).width(Length::Fill)
                    ]);
                }
                c.into()
            }
            Block::Code(lines) => {
                let mut c = column![].spacing(0);
                for l in lines {
                    c = c.push(code_line(l));
                }
                container(scrollable(c).direction(scrollable::Direction::Horizontal(scrollable::Scrollbar::new().width(0).scroller_width(0))))
                    .padding(Padding { top: 14.0, bottom: 14.0, left: 16.0, right: 16.0 })
                    .width(Length::Fill)
                    .style(|_| container::Style {
                        background: Some(t::SIDE.into()),
                        border: Border { color: t::LINE, width: 1.0, radius: 12.0.into() },
                        ..Default::default()
                    })
                    .into()
            }
            Block::Front(lines) => {
                let mut c = column![].spacing(0);
                for l in lines {
                    c = c.push(text(l.clone()).font(t::MONO).size(12.0).color(t::FG3));
                }
                container(c)
                    .padding(Padding { top: 8.0, bottom: 8.0, left: 12.0, right: 12.0 })
                    .width(Length::Fill)
                    .style(|_| container::Style { border: Border { color: t::LINE, width: 1.0, radius: 10.0.into() }, ..Default::default() })
                    .into()
            }
        };
        col = col.push(container(el).padding(Padding { bottom: below, ..Default::default() }));
    }
    col.into()
}
