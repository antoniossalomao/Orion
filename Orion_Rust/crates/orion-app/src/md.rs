//! Marcação das respostas e da prévia do editor: `**negrito**`, `*itálico*`, `` `código` ``, `[[wikilink]]`, `[texto](url)`,
//! links soltos, títulos, citação, listas, tabelas, régua, bloco de código e frontmatter (§13.3, §17).

use crate::theme as t;
use iced::widget::{column, container, rich_text, row, scrollable, span, text, Column};
use iced::{Border, Color, Element, Font, Length, Padding};

#[derive(Clone, Debug, PartialEq)]
pub enum Inline {
    Text(String),
    Bold(String),
    Italic(String),
    /// Fórmula `$...$` já convertida em texto Unicode.
    Math(String),
    Code(String),
    Wiki(String),
    Link(String, String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    P(Vec<Inline>),
    H(u8, Vec<Inline>),
    Quote(Vec<Inline>),
    Ul(Vec<Vec<Inline>>),
    Ol(Vec<Vec<Inline>>),
    /// (linguagem do bloco, linhas)
    Code(String, Vec<String>),
    Front(Vec<String>),
    Table(Vec<Vec<Inline>>, Vec<Vec<Vec<Inline>>>),
    Rule,
    /// Fórmula em bloco (`$$...$$`): (LaTeX cru, versão em texto Unicode). O cru é diagramado; o texto é o plano B.
    Math(String, String),
    /// Imagem numa linha só: `![texto alternativo](caminho)`. Só arquivo local de imagem que exista; o resto vira o texto alternativo.
    Image(String, String),
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
        } else if rest.starts_with('*') && !rest.starts_with("**") {
            if let Some(end) = rest[1..].find('*') {
                let inner = &rest[1..1 + end];
                if !inner.is_empty() && !inner.starts_with(' ') && !inner.ends_with(' ') {
                    flush(&mut buf, &mut out);
                    out.push(Inline::Italic(inner.to_string()));
                    i += rest[..1 + end + 1].chars().count();
                    continue;
                }
            }
        } else if rest.starts_with('_') && (i == 0 || !chars[i - 1].is_alphanumeric()) {
            if let Some(end) = rest[1..].find('_') {
                let inner = &rest[1..1 + end];
                let after = rest[1 + end + 1..].chars().next();
                if !inner.is_empty() && !inner.starts_with(' ') && !inner.ends_with(' ') && !after.is_some_and(|c| c.is_alphanumeric()) {
                    flush(&mut buf, &mut out);
                    out.push(Inline::Italic(inner.to_string()));
                    i += rest[..1 + end + 1].chars().count();
                    continue;
                }
            }
        } else if rest.starts_with('$') && !rest.starts_with("$$") {
            if let Some(end) = rest[1..].find('$') {
                let inner = &rest[1..1 + end];
                // dinheiro ("$5 e $6") não é fórmula: exige comando, expoente ou índice e nada colado nas bordas
                if !inner.is_empty() && !inner.starts_with(' ') && !inner.ends_with(' ') && (inner.contains('\\') || inner.contains('^') || inner.contains('_')) {
                    flush(&mut buf, &mut out);
                    out.push(Inline::Math(latex_to_text(inner)));
                    i += rest[..1 + end + 1].chars().count();
                    continue;
                }
            }
        } else if rest.starts_with('[') && !rest.starts_with("[[") {
            if let Some(mid) = rest.find("](") {
                if let Some(close) = rest[mid + 2..].find(')') {
                    let label = &rest[1..mid];
                    let url = rest[mid + 2..mid + 2 + close].trim();
                    if !label.contains('\n') && is_web(url) {
                        flush(&mut buf, &mut out);
                        out.push(Inline::Link(label.to_string(), url.to_string()));
                        i += rest[..mid + 2 + close + 1].chars().count();
                        continue;
                    }
                }
            }
        } else if (rest.starts_with("http://") || rest.starts_with("https://")) && (i == 0 || !chars[i - 1].is_alphanumeric()) {
            let raw: String = rest.chars().take_while(|c| !c.is_whitespace() && !matches!(c, '<' | '>' | '"' | '`')).collect();
            let url = raw.trim_end_matches(['.', ',', ';', ':', '!', '?', ')', ']']).to_string();
            if url.len() > 8 {
                flush(&mut buf, &mut out);
                i += url.chars().count();
                out.push(Inline::Link(url.clone(), url));
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

/// LaTeX simples em texto Unicode: letras gregas, operadores, frações, raízes, expoentes e índices.
/// Não é um typesetter: o que não reconhece fica como veio (a fórmula nunca some).
pub fn latex_to_text(src: &str) -> String {
    const SIMB: &[(&str, &str)] = &[
        ("\\alpha", "α"), ("\\beta", "β"), ("\\gamma", "γ"), ("\\delta", "δ"), ("\\epsilon", "ε"), ("\\theta", "θ"),
        ("\\lambda", "λ"), ("\\mu", "μ"), ("\\pi", "π"), ("\\rho", "ρ"), ("\\sigma", "σ"), ("\\tau", "τ"), ("\\phi", "φ"),
        ("\\omega", "ω"), ("\\Delta", "Δ"), ("\\Sigma", "Σ"), ("\\Omega", "Ω"), ("\\Pi", "Π"),
        ("\\times", "×"), ("\\cdot", "·"), ("\\pm", "±"), ("\\leq", "≤"), ("\\le", "≤"), ("\\geq", "≥"), ("\\ge", "≥"),
        ("\\neq", "≠"), ("\\ne", "≠"), ("\\approx", "≈"), ("\\infty", "∞"), ("\\sum", "∑"), ("\\prod", "∏"), ("\\int", "∫"),
        ("\\rightarrow", "→"), ("\\to", "→"), ("\\leftarrow", "←"), ("\\Rightarrow", "⇒"), ("\\in", "∈"), ("\\partial", "∂"),
        ("\\ldots", "…"), ("\\cdots", "⋯"), ("\\quad", "  "), ("\\,", " "), ("\\left", ""), ("\\right", ""),
    ];
    fn grupo(s: &str, de: usize) -> Option<(String, usize)> {
        let rest = &s[de..];
        if !rest.starts_with('{') {
            return None;
        }
        let mut prof = 0;
        for (i, c) in rest.char_indices() {
            match c {
                '{' => prof += 1,
                '}' => {
                    prof -= 1;
                    if prof == 0 {
                        return Some((rest[1..i].to_string(), de + i + 1));
                    }
                }
                _ => {}
            }
        }
        None
    }
    fn sup(t: &str) -> Option<String> {
        t.chars().map(|c| match c { '0' => Some('⁰'), '1' => Some('¹'), '2' => Some('²'), '3' => Some('³'), '4' => Some('⁴'), '5' => Some('⁵'), '6' => Some('⁶'), '7' => Some('⁷'), '8' => Some('⁸'), '9' => Some('⁹'), '+' => Some('⁺'), '-' => Some('⁻'), 'n' => Some('ⁿ'), _ => None }).collect()
    }
    fn sub(t: &str) -> Option<String> {
        t.chars().map(|c| match c { '0' => Some('₀'), '1' => Some('₁'), '2' => Some('₂'), '3' => Some('₃'), '4' => Some('₄'), '5' => Some('₅'), '6' => Some('₆'), '7' => Some('₇'), '8' => Some('₈'), '9' => Some('₉'), '+' => Some('₊'), '-' => Some('₋'), _ => None }).collect()
    }
    let mut out = String::new();
    let mut i = 0;
    while i < src.len() {
        let rest = &src[i..];
        if let Some(r) = rest.strip_prefix("\\frac") {
            let base = i + 5;
            if let Some((a, f1)) = grupo(src, base) {
                if let Some((b, f2)) = grupo(src, f1) {
                    out.push_str(&format!("({})/({})", latex_to_text(&a), latex_to_text(&b)));
                    i = f2;
                    continue;
                }
            }
            let _ = r;
        }
        if rest.starts_with("\\sqrt") {
            if let Some((a, f)) = grupo(src, i + 5) {
                out.push_str(&format!("√({})", latex_to_text(&a)));
                i = f;
                continue;
            }
        }
        if rest.starts_with('^') || rest.starts_with('_') {
            let eh_sup = rest.starts_with('^');
            let (conteudo, fim) = match grupo(src, i + 1) {
                Some((g, f)) => (g, f),
                None => match src[i + 1..].chars().next() {
                    Some(c) => (c.to_string(), i + 1 + c.len_utf8()),
                    None => (String::new(), i + 1),
                },
            };
            let conv = if eh_sup { sup(&conteudo) } else { sub(&conteudo) };
            match conv {
                Some(c) if !conteudo.is_empty() => out.push_str(&c),
                _ => out.push_str(&format!("{}{}", if eh_sup { "^" } else { "_" }, if conteudo.chars().count() > 1 { format!("({})", latex_to_text(&conteudo)) } else { conteudo })),
            }
            i = fim;
            continue;
        }
        if let Some((nome, simb)) = SIMB.iter().filter(|(n, _)| rest.starts_with(n)).max_by_key(|(n, _)| n.len()) {
            let depois = rest[nome.len()..].chars().next();
            if !nome.ends_with(|c: char| c.is_ascii_alphabetic()) || !depois.is_some_and(|c| c.is_ascii_alphabetic()) {
                out.push_str(simb);
                i += nome.len();
                continue;
            }
        }
        let c = rest.chars().next().unwrap();
        if c != '{' && c != '}' {
            out.push(c);
        }
        i += c.len_utf8();
    }
    out
}

/// `![alt](caminho)` sozinho numa linha.
fn image_line(l: &str) -> Option<(String, String)> {
    let t = l.trim();
    let r = t.strip_prefix("![")?;
    let mid = r.find("](")?;
    let src = r[mid + 2..].strip_suffix(')')?.trim();
    (!src.is_empty() && !src.contains(char::is_whitespace)).then(|| (r[..mid].to_string(), src.to_string()))
}

/// Só web: um link da resposta nunca abre arquivo, programa ou esquema do sistema (REGRAS.md: conteúdo externo é dado).
pub fn is_web(url: &str) -> bool {
    let u = url.to_ascii_lowercase();
    (u.starts_with("https://") || u.starts_with("http://")) && !u.contains(char::is_whitespace)
}

/// `src/main.rs`, `C:\\Projetos\\a.rs:12` ou `/home/x/a.py`: um caminho de arquivo (com extensão) e a linha opcional.
/// O texto do modelo é só dado: aqui só se reconhece a forma; quem abre confere se o arquivo existe de verdade.
pub fn file_ref(s: &str) -> Option<(String, Option<u32>)> {
    let t = s.trim();
    if t.is_empty() || t.len() > 260 || t.starts_with('-') || t.contains("://") || t.contains(char::is_whitespace) {
        return None;
    }
    let (path, line) = match t.rsplit_once(':') {
        Some((p, l)) if !l.is_empty() && l.chars().all(|c| c.is_ascii_digit()) && !(p.len() == 1) => (p, l.parse().ok()),
        _ => (t, None),
    };
    let ok_chars = path.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | '\\' | ':' | '~'));
    let name = path.rsplit(['/', '\\']).next().unwrap_or("");
    let ext_ok = name.rsplit_once('.').is_some_and(|(stem, ext)| !stem.is_empty() && (1..=8).contains(&ext.len()) && ext.chars().all(|c| c.is_ascii_alphanumeric()));
    (ok_chars && ext_ok && !path.contains("..")).then(|| (path.to_string(), line))
}

fn split_row(l: &str) -> Vec<String> {
    l.trim().trim_matches('|').split('|').map(|c| c.trim().to_string()).collect()
}

fn is_sep_row(l: &str) -> bool {
    let c = split_row(l);
    !c.is_empty() && l.contains('-') && c.iter().all(|x| !x.is_empty() && x.chars().all(|ch| matches!(ch, '-' | ':' | ' ')) && x.contains('-'))
}

fn is_rule(l: &str) -> bool {
    let t = l.trim();
    t.len() >= 3 && (t.chars().all(|c| c == '-') || t.chars().all(|c| c == '*') || t.chars().all(|c| c == '_'))
}

thread_local! {
    static PARSED: std::cell::RefCell<std::collections::VecDeque<(String, std::sync::Arc<Vec<Block>>)>> = const { std::cell::RefCell::new(std::collections::VecDeque::new()) };
}

pub fn cached_parse(src: &str) -> std::sync::Arc<Vec<Block>> {
    PARSED.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(i) = cache.iter().position(|(text, _)| text == src) {
            let item = cache.remove(i).unwrap();
            let blocks = item.1.clone();
            cache.push_back(item);
            return blocks;
        }
        let blocks = std::sync::Arc::new(parse(src));
        if src.len() <= 262144 {
            while cache.len() >= 128 || cache.iter().map(|(text, _)| text.len()).sum::<usize>() + src.len() > 2 * 1024 * 1024 { cache.pop_front(); }
            cache.push_back((src.to_owned(), blocks.clone()));
        }
        blocks
    })
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
        } else if l.trim().len() > 4 && l.trim().starts_with("$$") && l.trim().ends_with("$$") {
            // fórmula em bloco numa linha só: `$$ x^2 $$`
            let raw = l.trim()[2..l.trim().len() - 2].trim().to_string();
            blocks.push(Block::Math(raw.clone(), latex_to_text(&raw)));
            i += 1;
        } else if let Some((alt, src)) = image_line(l) {
            blocks.push(Block::Image(alt, src));
            i += 1;
        } else if l.trim() == "$$" || (l.trim().starts_with("$$") && l.trim().len() > 2 && !l.trim()[2..].contains("$$")) {
            // fórmula em bloco com `$$` em linha própria (a de uma linha só é tratada acima)
            let mut code = Vec::new();
            let primeira = l.trim()[2..].trim();
            if !primeira.is_empty() {
                code.push(primeira.to_string());
            }
            i += 1;
            // o fecho `$$` pode vir sozinho numa linha ou colado no fim da última linha da fórmula
            while i < lines.len() {
                let t = lines[i].trim();
                if t == "$$" {
                    i += 1;
                    break;
                }
                if let Some(antes) = t.strip_suffix("$$") {
                    code.push(antes.to_string());
                    i += 1;
                    break;
                }
                code.push(lines[i].to_string());
                i += 1;
            }
            let raw = code.join(" ");
            blocks.push(Block::Math(raw.clone(), latex_to_text(&raw)));
        } else if l.starts_with("```") {
            let lang = l.trim_start_matches('`').trim().to_lowercase();
            let mut code = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].starts_with("```") {
                code.push(lines[i].to_string());
                i += 1;
            }
            i += 1;
            blocks.push(Block::Code(lang, code));
        } else if let Some(h) = l.strip_prefix("### ") {
            blocks.push(Block::H(3, inline(h)));
            i += 1;
        } else if let Some(h) = l.strip_prefix("## ") {
            blocks.push(Block::H(2, inline(h)));
            i += 1;
        } else if let Some(h) = l.strip_prefix("# ") {
            blocks.push(Block::H(1, inline(h)));
            i += 1;
        } else if is_rule(l) && !l.starts_with("- ") {
            blocks.push(Block::Rule);
            i += 1;
        } else if l.trim_start().starts_with('|') && i + 1 < lines.len() && is_sep_row(lines[i + 1]) {
            let head: Vec<Vec<Inline>> = split_row(l).iter().map(|c| inline(c)).collect();
            i += 2;
            let mut rows = Vec::new();
            while i < lines.len() && lines[i].trim_start().starts_with('|') {
                rows.push(split_row(lines[i]).iter().map(|c| inline(c)).collect());
                i += 1;
            }
            blocks.push(Block::Table(head, rows));
        } else if l.starts_with('>') {
            let mut q = String::new();
            while i < lines.len() && lines[i].starts_with('>') {
                let t = lines[i].trim_start_matches('>').trim_start();
                if !q.is_empty() {
                    q.push(' ');
                }
                q.push_str(t);
                i += 1;
            }
            blocks.push(Block::Quote(inline(&q)));
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
    l.starts_with("```") || l.starts_with('#') || l.starts_with('>') || l.starts_with("- ") || l.trim_start().starts_with('|') || is_rule(l) || is_ol(l) || l.trim_start().starts_with("$$") || image_line(l).is_some()
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

fn spans<'a>(inl: &[Inline], st: &Style, base: Color) -> Vec<iced::widget::text::Span<'a, String, Font>> {
    inl.iter()
        .map(|i| match i {
            Inline::Text(s) => span(s.clone()).font(st.font).size(st.size).color(base),
            Inline::Bold(s) => span(s.clone()).font(st.bold).size(st.size).color(base),
            Inline::Italic(s) => span(s.clone()).font(Font { style: iced::font::Style::Italic, ..st.font }).size(st.size).color(base),
            Inline::Math(s) => span(s.clone()).font(Font { style: iced::font::Style::Italic, ..st.font }).size(st.size).color(base),
            Inline::Link(label, url) => span(label.clone()).font(st.font).size(st.size).color(t::ACCENT).underline(true).link(url.clone()),
            Inline::Code(s) => {
                // só caminho absoluto vira link: o relativo não tem como ser resolvido e ficaria um link morto
                let arquivo = file_ref(s).is_some_and(|(p, _)| std::path::Path::new(&p).is_absolute());
                let sp = span(s.clone())
                    .font(t::MONO)
                    .size(st.size * 0.78)
                    .color(if arquivo { t::ACCENT } else { base })
                    .underline(arquivo)
                    .background(iced::Background::Color(t::RAISE2))
                    .border(Border { radius: 5.0.into(), ..Default::default() })
                    .padding(Padding { top: 1.0, bottom: 1.0, left: 4.0, right: 4.0 });
                if arquivo { sp.link(format!("orion-file:{s}")) } else { sp }
            }
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
    render_with(blocks, st, None, None)
}

/// Como `render`, mas os links (`[texto](url)` e URLs soltas) mandam a mensagem de `on_link` ao serem clicados.
pub fn render_with<'a, M: Clone + 'a>(blocks: &[Block], st: Style, on_link: Option<fn(String) -> M>, on_copy: Option<fn(String) -> M>) -> Element<'a, M> {
    let para = move |inl: &[Inline], color: Color, st: &Style| -> Element<'a, M> {
        let r = rich_text(spans(inl, st, color)).line_height(iced::widget::text::LineHeight::Relative(st.line));
        match on_link {
            Some(f) => r.on_link_click(f).into(),
            None => r.into(),
        }
    };
    let mut col: Column<'a, M> = column![].spacing(0);
    let n = blocks.len();
    for (idx, b) in blocks.iter().enumerate() {
        let last = idx + 1 == n;
        let below = if last { 0.0 } else { st.gap };
        let el: Element<'a, M> = match b {
            Block::P(inl) => para(inl, t::FG, &st),
            Block::H(lv, inl) => {
                let (sz, top) = match lv {
                    1 => (st.h1, 0.0),
                    2 => (st.h2, 8.0),
                    _ => (st.h3, 6.0),
                };
                let sp: Vec<iced::widget::text::Span<'a, (), Font>> = inl.iter().map(|i| match i {
                    Inline::Text(s) | Inline::Bold(s) | Inline::Italic(s) | Inline::Math(s) | Inline::Code(s) | Inline::Wiki(s) | Inline::Link(s, _) => span(s.clone()).font(st.h_font).size(sz).color(t::FG),
                }).collect();
                container(rich_text(sp)).padding(Padding { top, bottom: 2.0, ..Default::default() }).into()
            }
            Block::Quote(inl) => {
                let bar = container(iced::widget::Space::new().width(Length::Fixed(2.0)).height(Length::Fill))
                    .width(Length::Fixed(2.0))
                    .style(|_| container::Style { background: Some(t::LINE3.into()), border: Border { radius: 1.0.into(), ..Default::default() }, ..Default::default() });
                row![bar, container(para(inl, t::FG2, &st)).padding(Padding { top: 2.0, bottom: 2.0, left: 12.0, right: 0.0 }).width(Length::Fill)]
                    .height(Length::Shrink)
                    .into()
            }
            Block::Math(raw, plain) => match crate::math::typeset(raw, st.size * 1.15) {
                Some(f) => {
                    let h = f.height();
                    container(iced::widget::canvas(f).width(Length::Fill).height(Length::Fixed(h))).width(Length::Fill).padding(Padding { top: 4.0, bottom: 4.0, ..Default::default() }).into()
                }
                None => container(text(plain.clone()).font(Font { style: iced::font::Style::Italic, ..st.font }).size(st.size * 1.1).color(t::FG))
                    .width(Length::Fill)
                    .center_x(Length::Fill)
                    .padding(Padding { top: 6.0, bottom: 6.0, ..Default::default() })
                    .into(),
            },
            Block::Image(alt, src) => {
                let p = std::path::Path::new(src);
                // `/imagens/<nome>` é imagem gerada pelo núcleo (baixada para o cache pelo app); o resto, arquivo absoluto do disco
                let achada = if crate::imgcache::url_do_nucleo(src) { crate::imgcache::da_url(src) } else { p.is_absolute().then(|| crate::imgcache::preparar(p)).flatten() };
                match achada {
                    Some(cache) => container(iced::widget::image(iced::widget::image::Handle::from_path(cache)).content_fit(iced::ContentFit::ScaleDown))
                        .max_width(520.0)
                        .padding(Padding { top: 4.0, bottom: 4.0, ..Default::default() })
                        .into(),
                    None => text(if alt.is_empty() { crate::i18n::text("[imagem]").to_string() } else { crate::i18n::formatted("[imagem: {alt}]", format!("[imagem: {alt}]")) }).font(st.font).size(st.size).color(t::FG3).into(),
                }
            }
            Block::Rule => container(iced::widget::Space::new().height(Length::Fixed(1.0)))
                .width(Length::Fill)
                .height(Length::Fixed(1.0))
                .style(|_| container::Style { background: Some(t::LINE.into()), ..Default::default() })
                .into(),
            Block::Table(head, rows) => {
                let cell = |inl: &[Inline], bold: bool| -> Element<'a, M> {
                    let mut stc = st;
                    if bold {
                        stc.font = st.bold;
                    }
                    container(para(inl, t::FG, &stc)).padding(Padding { top: 6.0, bottom: 6.0, left: 10.0, right: 10.0 }).width(Length::Fill).into()
                };
                let line = || container(iced::widget::Space::new().height(Length::Fixed(1.0))).width(Length::Fill).height(Length::Fixed(1.0)).style(|_| container::Style { background: Some(t::LINE.into()), ..Default::default() });
                let mut c = column![].spacing(0);
                let mut hr = row![].spacing(0);
                for h in head {
                    hr = hr.push(cell(h, true));
                }
                c = c.push(hr).push(line());
                for r in rows {
                    let mut rr = row![].spacing(0);
                    for k in 0..head.len() {
                        rr = rr.push(match r.get(k) {
                            Some(x) => cell(x, false),
                            None => cell(&[], false),
                        });
                    }
                    c = c.push(rr).push(line());
                }
                container(c)
                    .width(Length::Fill)
                    .style(|_| container::Style { border: Border { color: t::LINE, width: 1.0, radius: 10.0.into() }, ..Default::default() })
                    .into()
            }
            Block::Ul(items) | Block::Ol(items) => {
                let ordered = matches!(b, Block::Ol(_));
                let mut c = column![].spacing(st.size * 0.3);
                for (k, it) in items.iter().enumerate() {
                    let mark = if ordered { format!("{}.", k + 1) } else { "•".into() };
                    c = c.push(row![
                        container(text(mark).font(st.font).size(st.size).color(t::FG3)).width(Length::Fixed(st.size * 1.3)),
                        container(para(it, t::FG, &st)).width(Length::Fill)
                    ]);
                }
                c.into()
            }
            Block::Code(lang, lines) => {
                let diagrama = if lang == "mermaid" { crate::mermaid::parse(lines) } else { None };
                let formula_ok = matches!(lang.as_str(), "latex" | "tex" | "math") && crate::math::typeset(&lines.join(" "), st.size).is_some();
                let mut corpo = column![].spacing(0);
                for l in lines {
                    corpo = corpo.push(code_line(l));
                }
                let mut c = column![].spacing(0);
                if !lang.is_empty() || on_copy.is_some() {
                    // sem WebView2 (ADR 0004) não há prévia de diagrama nem de fórmula: o código fica visível e o aviso diz isso
                    let aviso = match lang.as_str() {
                        "mermaid" if diagrama.is_some() => "mermaid · diagrama".to_string(),
                        "mermaid" => crate::i18n::formatted("{lang} · diagrama (este tipo ainda não tem prévia; o código está abaixo)", format!("{lang} · diagrama (este tipo ainda não tem prévia; o código está abaixo)")),
                        "latex" | "tex" | "math" if formula_ok => crate::i18n::formatted("{lang} · fórmula", format!("{lang} · fórmula")),
                        "latex" | "tex" | "math" => crate::i18n::formatted("{lang} · fórmula (não consegui diagramar; o código está abaixo)", format!("{lang} · fórmula (não consegui diagramar; o código está abaixo)")),
                        _ => lang.clone(),
                    };
                    let mut topo = row![container(text(aviso).font(t::MONO).size(11.0).color(t::FG3)).width(Length::Fill)].align_y(iced::alignment::Vertical::Center);
                    if let Some(f) = on_copy {
                        topo = topo.push(
                            iced::widget::button(text(crate::i18n::text("Copiar")).font(t::UI).size(11.5).color(t::FG3))
                                .padding(Padding { top: 1.0, bottom: 1.0, left: 6.0, right: 6.0 })
                                .on_press(f(lines.join("\n")))
                                .style(|_, st| iced::widget::button::Style { background: if matches!(st, iced::widget::button::Status::Hovered) { Some(iced::Background::Color(t::HOVER)) } else { None }, text_color: t::FG3, border: Border { radius: 6.0.into(), ..Default::default() }, ..Default::default() }),
                        );
                    }
                    c = c.push(container(topo).width(Length::Fill).padding(Padding { bottom: 8.0, ..Default::default() }));
                }
                let formula = if matches!(lang.as_str(), "latex" | "tex" | "math") { crate::math::typeset(&lines.join(" "), st.size * 1.15) } else { None };
                if let Some(f) = formula {
                    let h = f.height();
                    c = c.push(iced::widget::canvas(f).width(Length::Fill).height(Length::Fixed(h)));
                } else if let Some(d) = diagrama {
                    let h = d.height;
                    c = c.push(iced::widget::canvas(d).width(Length::Fill).height(Length::Fixed(h)));
                } else {
                    c = c.push(scrollable(corpo).direction(scrollable::Direction::Horizontal(scrollable::Scrollbar::new().width(0).scroller_width(0))));
                }
                container(c)
                    .padding(Padding { top: 12.0, bottom: 14.0, left: 16.0, right: 12.0 })
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
