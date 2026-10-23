//! Prévia estática de HTML nos artefatos, sem WebView (ADR 0004): o HTML vira os mesmos blocos do Markdown
//! (títulos, parágrafos, listas, tabelas, citações, código, links). Não há CSS, JavaScript nem recursos de rede:
//! `script`, `style`, `iframe`, `object` e afins são descartados e nada do HTML é executado ou aberto sozinho
//! (REGRAS.md: conteúdo externo é dado). Links só valem se forem http(s) e abrem pelo clique do usuário.

use crate::md::{Block, Inline};

const PULAR: &[&str] = &["script", "style", "head", "title", "svg", "noscript", "iframe", "object", "embed", "template", "canvas", "audio", "video"];
const BLOCOS: &[&str] = &["p", "div", "section", "article", "header", "footer", "main", "nav", "aside", "form", "figure", "figcaption", "details", "summary", "body", "html", "address", "fieldset"];

enum Tok {
    Text(String),
    Open(String, Vec<(String, String)>),
    Close(String),
}

fn decode(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        match rest.find(';').filter(|j| *j <= 10) {
            Some(j) => {
                let ent = &rest[1..j];
                let rep: Option<String> = match ent {
                    "amp" => Some("&".into()),
                    "lt" => Some("<".into()),
                    "gt" => Some(">".into()),
                    "quot" => Some("\"".into()),
                    "apos" | "#39" => Some("'".into()),
                    "nbsp" => Some(" ".into()),
                    "copy" => Some("©".into()),
                    "mdash" => Some("—".into()),
                    "ndash" => Some("–".into()),
                    "hellip" => Some("…".into()),
                    _ if ent.starts_with("#x") || ent.starts_with("#X") => u32::from_str_radix(&ent[2..], 16).ok().and_then(char::from_u32).map(String::from),
                    _ if ent.starts_with('#') => ent[1..].parse::<u32>().ok().and_then(char::from_u32).map(String::from),
                    _ => None,
                };
                match rep {
                    Some(r) => {
                        out.push_str(&r);
                        rest = &rest[j + 1..];
                    }
                    None => {
                        out.push('&');
                        rest = &rest[1..];
                    }
                }
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn tokens(src: &str) -> Vec<Tok> {
    let c: Vec<char> = src.chars().collect();
    let mut out = vec![];
    let mut i = 0;
    let mut text = String::new();
    let starts = |i: usize, s: &str| s.chars().enumerate().all(|(k, ch)| c.get(i + k) == Some(&ch));
    while i < c.len() {
        if c[i] != '<' {
            text.push(c[i]);
            i += 1;
            continue;
        }
        if starts(i, "<!--") {
            i += 4;
            while i < c.len() && !starts(i, "-->") {
                i += 1;
            }
            i += 3;
            continue;
        }
        // só é tag se vier letra, `/` ou `!` depois do `<`
        let nxt = c.get(i + 1).copied().unwrap_or(' ');
        if !(nxt.is_ascii_alphabetic() || nxt == '/' || nxt == '!' || nxt == '?') {
            text.push('<');
            i += 1;
            continue;
        }
        if !text.is_empty() {
            out.push(Tok::Text(std::mem::take(&mut text)));
        }
        let mut j = i + 1;
        let mut q: Option<char> = None;
        let mut raw = String::new();
        while j < c.len() {
            let ch = c[j];
            match q {
                Some(qc) if ch == qc => q = None,
                Some(_) => {}
                None if ch == '"' || ch == '\'' => q = Some(ch),
                None if ch == '>' => break,
                None => {}
            }
            raw.push(ch);
            j += 1;
        }
        i = j + 1;
        if raw.starts_with('!') || raw.starts_with('?') {
            continue;
        }
        let closing = raw.starts_with('/');
        let body = raw.trim_start_matches('/').trim_end_matches('/').trim();
        let name: String = body.chars().take_while(|ch| ch.is_ascii_alphanumeric()).collect::<String>().to_lowercase();
        if name.is_empty() {
            continue;
        }
        if closing {
            out.push(Tok::Close(name));
        } else {
            let mut attrs = vec![];
            let rest: Vec<char> = body[name.len()..].chars().collect();
            let mut k = 0;
            while k < rest.len() {
                while k < rest.len() && rest[k].is_whitespace() {
                    k += 1;
                }
                let ks = k;
                while k < rest.len() && !rest[k].is_whitespace() && rest[k] != '=' {
                    k += 1;
                }
                let key: String = rest[ks..k].iter().collect::<String>().to_lowercase();
                let mut val = String::new();
                if k < rest.len() && rest[k] == '=' {
                    k += 1;
                    if k < rest.len() && (rest[k] == '"' || rest[k] == '\'') {
                        let qc = rest[k];
                        k += 1;
                        while k < rest.len() && rest[k] != qc {
                            val.push(rest[k]);
                            k += 1;
                        }
                        k += 1;
                    } else {
                        while k < rest.len() && !rest[k].is_whitespace() {
                            val.push(rest[k]);
                            k += 1;
                        }
                    }
                }
                if !key.is_empty() {
                    attrs.push((key, decode(&val)));
                }
            }
            out.push(Tok::Open(name, attrs));
        }
    }
    if !text.is_empty() {
        out.push(Tok::Text(text));
    }
    out
}

#[derive(Default)]
struct S {
    blocks: Vec<Block>,
    inl: Vec<Inline>,
    bold: u32,
    ital: u32,
    code: u32,
    link: Option<String>,
    pre: Option<(String, String)>, // (linguagem, texto)
    list: Option<(bool, Vec<Vec<Inline>>)>,
    depth: usize,
    in_li: bool,
    heading: Option<u8>,
    quote: u32,
    table: Option<T>,
    cell: bool,
}

#[derive(Default)]
struct T {
    head: Vec<Vec<Inline>>,
    rows: Vec<Vec<Vec<Inline>>>,
    row: Vec<Vec<Inline>>,
    row_th: bool,
}

impl S {
    fn push_text(&mut self, raw: &str) {
        if let Some((_, buf)) = self.pre.as_mut() {
            buf.push_str(raw);
            return;
        }
        // colapsa espaços como o navegador
        let mut t = String::new();
        let mut esp = matches!(self.inl.last(), None) || matches!(self.inl.last(), Some(Inline::Text(s)) if s.ends_with(' ') || s.ends_with('\n'));
        for ch in raw.chars() {
            if ch.is_whitespace() {
                if !esp {
                    t.push(' ');
                    esp = true;
                }
            } else {
                t.push(ch);
                esp = false;
            }
        }
        if t.is_empty() {
            return;
        }
        self.inl.push(if let Some(u) = self.link.clone() {
            Inline::Link(t, u)
        } else if self.code > 0 {
            Inline::Code(t)
        } else if self.bold > 0 {
            Inline::Bold(t)
        } else if self.ital > 0 {
            Inline::Italic(t)
        } else {
            Inline::Text(t)
        });
    }
    fn flush(&mut self) {
        if self.in_li || self.cell || self.heading.is_some() {
            return;
        }
        let mut v = std::mem::take(&mut self.inl);
        if let Some(Inline::Text(s)) = v.last_mut() {
            *s = s.trim_end().to_string();
        }
        v.retain(|i| !matches!(i, Inline::Text(s) if s.is_empty()));
        if v.is_empty() {
            return;
        }
        self.blocks.push(if self.quote > 0 { Block::Quote(v) } else { Block::P(v) });
    }
    fn close_list(&mut self) {
        if let Some((ord, items)) = self.list.take() {
            if !items.is_empty() {
                self.blocks.push(if ord { Block::Ol(items) } else { Block::Ul(items) });
            }
        }
    }
}

pub fn to_blocks(src: &str) -> Vec<Block> {
    let src: String = src.chars().take(200_000).collect();
    let mut s = S::default();
    let mut skip: Option<String> = None;
    for tok in tokens(&src) {
        if let Some(n) = &skip {
            if matches!(&tok, Tok::Close(c) if c == n) {
                skip = None;
            }
            continue;
        }
        match tok {
            Tok::Text(t) => {
                let d = decode(&t);
                s.push_text(&d);
            }
            Tok::Open(n, attrs) => {
                let n = n.as_str();
                if PULAR.contains(&n) {
                    skip = Some(n.to_string());
                    continue;
                }
                match n {
                    "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                        s.flush();
                        s.heading = Some(n[1..].parse().unwrap_or(3));
                    }
                    "strong" | "b" => s.bold += 1,
                    "em" | "i" | "cite" | "dfn" => s.ital += 1,
                    "code" | "kbd" | "samp" | "tt" => {
                        if let Some((lang, _)) = s.pre.as_mut() {
                            if let Some((_, cls)) = attrs.iter().find(|(k, _)| k == "class") {
                                if let Some(l) = cls.split_whitespace().find_map(|c| c.strip_prefix("language-")) {
                                    *lang = l.to_lowercase();
                                }
                            }
                        } else {
                            s.code += 1;
                        }
                    }
                    "a" => {
                        if let Some((_, h)) = attrs.iter().find(|(k, _)| k == "href") {
                            if crate::md::is_web(h) {
                                s.link = Some(h.clone());
                            }
                        }
                    }
                    "br" => s.inl.push(Inline::Text("\n".into())),
                    "hr" => {
                        s.flush();
                        s.blocks.push(Block::Rule);
                    }
                    "img" => {
                        let alt = attrs.iter().find(|(k, _)| k == "alt").map(|(_, v)| v.clone()).unwrap_or_default();
                        s.inl.push(Inline::Text(if alt.is_empty() { "[imagem]".into() } else { format!("[imagem: {alt}]") }));
                    }
                    "input" | "textarea" | "select" => {
                        let ph = attrs.iter().find(|(k, _)| k == "placeholder" || k == "value").map(|(_, v)| v.clone()).unwrap_or_default();
                        s.inl.push(Inline::Text(format!("[campo{}]", if ph.is_empty() { String::new() } else { format!(": {ph}") })));
                    }
                    "button" => s.inl.push(Inline::Text("[ ".into())),
                    "pre" => {
                        s.flush();
                        s.pre = Some((String::new(), String::new()));
                    }
                    "blockquote" => {
                        s.flush();
                        s.quote += 1;
                    }
                    "ul" | "ol" => {
                        if s.list.is_none() {
                            s.flush();
                            s.list = Some((n == "ol", vec![]));
                        }
                        s.depth += 1;
                    }
                    "li" => {
                        if s.in_li {
                            s.close_item();
                        }
                        s.in_li = true;
                        s.inl.clear();
                    }
                    "table" => {
                        s.flush();
                        s.table = Some(T::default());
                    }
                    "tr" => {
                        if let Some(t) = s.table.as_mut() {
                            t.row.clear();
                            t.row_th = false;
                        }
                    }
                    "td" | "th" => {
                        if let Some(t) = s.table.as_mut() {
                            if n == "th" {
                                t.row_th = true;
                            }
                        }
                        s.cell = true;
                        s.inl.clear();
                    }
                    _ if BLOCOS.contains(&n) => s.flush(),
                    _ => {}
                }
            }
            Tok::Close(n) => {
                let n = n.as_str();
                match n {
                    "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                        if let Some(l) = s.heading.take() {
                            let mut v = std::mem::take(&mut s.inl);
                            if let Some(Inline::Text(t)) = v.last_mut() {
                                *t = t.trim_end().to_string();
                            }
                            if !v.is_empty() {
                                s.blocks.push(Block::H(l, v));
                            }
                        }
                    }
                    "strong" | "b" => s.bold = s.bold.saturating_sub(1),
                    "em" | "i" | "cite" | "dfn" => s.ital = s.ital.saturating_sub(1),
                    "code" | "kbd" | "samp" | "tt" => {
                        if s.pre.is_none() {
                            s.code = s.code.saturating_sub(1)
                        }
                    }
                    "a" => s.link = None,
                    "button" => s.inl.push(Inline::Text(" ]".into())),
                    "pre" => {
                        if let Some((lang, buf)) = s.pre.take() {
                            let lines: Vec<String> = buf.trim_matches('\n').lines().map(|l| l.to_string()).collect();
                            if !lines.is_empty() {
                                s.blocks.push(Block::Code(lang, lines));
                            }
                        }
                    }
                    "blockquote" => {
                        s.flush();
                        s.quote = s.quote.saturating_sub(1);
                    }
                    "li" => {
                        if s.in_li {
                            s.close_item();
                        }
                    }
                    "ul" | "ol" => {
                        if s.in_li {
                            s.close_item();
                        }
                        s.depth = s.depth.saturating_sub(1);
                        if s.depth == 0 {
                            s.close_list();
                        }
                    }
                    "td" | "th" => {
                        if s.cell {
                            let v = std::mem::take(&mut s.inl);
                            s.cell = false;
                            if let Some(t) = s.table.as_mut() {
                                t.row.push(v);
                            }
                        }
                    }
                    "tr" => {
                        if let Some(t) = s.table.as_mut() {
                            let row = std::mem::take(&mut t.row);
                            if !row.is_empty() {
                                if t.row_th && t.head.is_empty() && t.rows.is_empty() {
                                    t.head = row;
                                } else {
                                    t.rows.push(row);
                                }
                            }
                        }
                    }
                    "table" => {
                        if let Some(t) = s.table.take() {
                            let (head, rows) = if t.head.is_empty() && !t.rows.is_empty() {
                                let mut r = t.rows;
                                let h = r.remove(0);
                                (h, r)
                            } else {
                                (t.head, t.rows)
                            };
                            if !head.is_empty() {
                                s.blocks.push(Block::Table(head, rows));
                            }
                        }
                    }
                    _ if BLOCOS.contains(&n) => s.flush(),
                    _ => {}
                }
            }
        }
        if s.blocks.len() > 400 {
            break;
        }
    }
    s.flush();
    s.close_list();
    s.blocks
}

impl S {
    fn close_item(&mut self) {
        self.in_li = false;
        let mut v = std::mem::take(&mut self.inl);
        if let Some(Inline::Text(t)) = v.last_mut() {
            *t = t.trim_end().to_string();
        }
        if self.depth > 1 {
            v.insert(0, Inline::Text("– ".into()));
        }
        if !v.is_empty() {
            if let Some((_, items)) = self.list.as_mut() {
                items.push(v);
            }
        }
    }
}
