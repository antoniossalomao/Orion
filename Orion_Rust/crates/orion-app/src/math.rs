//! Fórmulas LaTeX diagramadas no próprio app, sem WebView (ADR 0004).
//! Entende frações empilhadas, raízes (com índice), expoentes e índices, somatórios/produtos/limites com limites
//! empilhados, integrais, `\left … \right`, matrizes (`pmatrix`, `bmatrix`, `vmatrix`…), `cases`, `align`,
//! acentos (`\hat`, `\bar`, `\vec`…), `\text{}`, letras gregas e os símbolos mais comuns.
//! Sem medição real de texto: as larguras são estimadas por caractere. Se a fórmula não fecha (chaves soltas),
//! `typeset` devolve `None` e o chamador mostra a versão em texto (a fórmula nunca some).

use crate::theme as t;
use iced::alignment::{Horizontal, Vertical};
use iced::mouse;
use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke, Text};
use iced::{Font, Point, Rectangle, Renderer, Theme};

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Var,
    Num,
    Op,
    Rel,
    Punct,
    Sym,
    Up,
    Bold,
    Big,
}

#[derive(Clone, Copy, PartialEq)]
enum Align {
    Center,
    Left,
    Pairs,
}

#[derive(Clone, Copy)]
enum Acc {
    Hat,
    Bar,
    Vec,
    Dot,
    DDot,
    Tilde,
    Under,
}

enum Node {
    Seq(Vec<Node>),
    Ch(String, Kind),
    Frac(Box<Node>, Box<Node>),
    Sqrt(Box<Node>, Option<Box<Node>>),
    Script(Box<Node>, Option<Box<Node>>, Option<Box<Node>>),
    Limits(Box<Node>, Option<Box<Node>>, Option<Box<Node>>),
    Delim(char, Box<Node>, char),
    Grid(Vec<Vec<Node>>, Align),
    Accent(Acc, Box<Node>),
    Gap(f32),
}

// ---------------------------------------------------------------- análise

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Top,
    Brace,
    Right,
    Cell,
}

struct P {
    c: Vec<char>,
    i: usize,
}

const SYMS: &[(&str, &str, Kind)] = &[
    ("alpha", "α", Kind::Var), ("beta", "β", Kind::Var), ("gamma", "γ", Kind::Var), ("delta", "δ", Kind::Var),
    ("epsilon", "ε", Kind::Var), ("varepsilon", "ε", Kind::Var), ("zeta", "ζ", Kind::Var), ("eta", "η", Kind::Var),
    ("theta", "θ", Kind::Var), ("vartheta", "ϑ", Kind::Var), ("iota", "ι", Kind::Var), ("kappa", "κ", Kind::Var),
    ("lambda", "λ", Kind::Var), ("mu", "μ", Kind::Var), ("nu", "ν", Kind::Var), ("xi", "ξ", Kind::Var),
    ("pi", "π", Kind::Var), ("rho", "ρ", Kind::Var), ("sigma", "σ", Kind::Var), ("tau", "τ", Kind::Var),
    ("upsilon", "υ", Kind::Var), ("phi", "φ", Kind::Var), ("varphi", "φ", Kind::Var), ("chi", "χ", Kind::Var),
    ("psi", "ψ", Kind::Var), ("omega", "ω", Kind::Var),
    ("Gamma", "Γ", Kind::Up), ("Delta", "Δ", Kind::Up), ("Theta", "Θ", Kind::Up), ("Lambda", "Λ", Kind::Up),
    ("Xi", "Ξ", Kind::Up), ("Pi", "Π", Kind::Up), ("Sigma", "Σ", Kind::Up), ("Phi", "Φ", Kind::Up),
    ("Psi", "Ψ", Kind::Up), ("Omega", "Ω", Kind::Up),
    ("times", "×", Kind::Op), ("div", "÷", Kind::Op), ("cdot", "·", Kind::Op), ("pm", "±", Kind::Op), ("mp", "∓", Kind::Op),
    ("ast", "∗", Kind::Op), ("circ", "∘", Kind::Op), ("bullet", "•", Kind::Op), ("cup", "∪", Kind::Op), ("cap", "∩", Kind::Op),
    ("wedge", "∧", Kind::Op), ("vee", "∨", Kind::Op), ("oplus", "⊕", Kind::Op), ("otimes", "⊗", Kind::Op), ("setminus", "∖", Kind::Op),
    ("leq", "≤", Kind::Rel), ("le", "≤", Kind::Rel), ("geq", "≥", Kind::Rel), ("ge", "≥", Kind::Rel), ("neq", "≠", Kind::Rel),
    ("ne", "≠", Kind::Rel), ("approx", "≈", Kind::Rel), ("equiv", "≡", Kind::Rel), ("sim", "∼", Kind::Rel), ("simeq", "≃", Kind::Rel),
    ("cong", "≅", Kind::Rel), ("propto", "∝", Kind::Rel), ("in", "∈", Kind::Rel), ("notin", "∉", Kind::Rel), ("subset", "⊂", Kind::Rel),
    ("subseteq", "⊆", Kind::Rel), ("supset", "⊃", Kind::Rel), ("supseteq", "⊇", Kind::Rel), ("ll", "≪", Kind::Rel), ("gg", "≫", Kind::Rel),
    ("perp", "⊥", Kind::Rel), ("parallel", "∥", Kind::Rel), ("mid", "∣", Kind::Rel),
    ("to", "→", Kind::Rel), ("rightarrow", "→", Kind::Rel), ("leftarrow", "←", Kind::Rel), ("leftrightarrow", "↔", Kind::Rel),
    ("Rightarrow", "⇒", Kind::Rel), ("Leftarrow", "⇐", Kind::Rel), ("Leftrightarrow", "⇔", Kind::Rel), ("mapsto", "↦", Kind::Rel),
    ("implies", "⟹", Kind::Rel), ("iff", "⟺", Kind::Rel), ("uparrow", "↑", Kind::Sym), ("downarrow", "↓", Kind::Sym),
    ("infty", "∞", Kind::Sym), ("partial", "∂", Kind::Sym), ("nabla", "∇", Kind::Sym), ("forall", "∀", Kind::Sym),
    ("exists", "∃", Kind::Sym), ("emptyset", "∅", Kind::Sym), ("varnothing", "∅", Kind::Sym), ("neg", "¬", Kind::Sym),
    ("lnot", "¬", Kind::Sym), ("angle", "∠", Kind::Sym), ("ldots", "…", Kind::Sym), ("dots", "…", Kind::Sym), ("cdots", "⋯", Kind::Sym),
    ("vdots", "⋮", Kind::Sym), ("ddots", "⋱", Kind::Sym), ("prime", "′", Kind::Sym), ("hbar", "ℏ", Kind::Sym), ("ell", "ℓ", Kind::Sym),
    ("aleph", "ℵ", Kind::Sym),
];

const BIG: &[(&str, &str, bool)] = &[
    ("sum", "∑", true), ("prod", "∏", true), ("coprod", "∐", true), ("bigcup", "⋃", true), ("bigcap", "⋂", true), ("bigoplus", "⨁", true),
    ("int", "∫", false), ("oint", "∮", false), ("iint", "∬", false), ("iiint", "∭", false),
];

const LIMITS_WORDS: &[&str] = &["lim", "max", "min", "sup", "inf", "limsup", "liminf", "argmax", "argmin"];
const FUNCS: &[&str] = &["sin", "cos", "tan", "cot", "sec", "csc", "arcsin", "arccos", "arctan", "sinh", "cosh", "tanh", "log", "ln", "lg", "exp", "det", "dim", "ker", "arg", "gcd", "deg", "Pr", "mod", "Re", "Im"];

impl P {
    fn peek(&self) -> Option<char> {
        self.c.get(self.i).copied()
    }
    fn starts(&self, s: &str) -> bool {
        s.chars().enumerate().all(|(k, ch)| self.c.get(self.i + k) == Some(&ch))
    }
    fn skip_ws(&mut self) {
        while self.peek().is_some_and(|c| c.is_whitespace()) {
            self.i += 1;
        }
    }
    fn name(&mut self) -> String {
        // depois de `\`: letras, ou um único caractere
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_alphabetic() {
                s.push(c);
                self.i += 1;
            } else {
                break;
            }
        }
        if s.is_empty() {
            if let Some(c) = self.peek() {
                s.push(c);
                self.i += 1;
            }
        }
        s
    }
    /// Texto cru de um grupo `{...}` (com chaves aninhadas).
    fn raw_group(&mut self) -> Result<String, ()> {
        self.skip_ws();
        if self.peek() != Some('{') {
            return Err(());
        }
        self.i += 1;
        let (mut prof, mut out) = (1, String::new());
        while let Some(c) = self.peek() {
            self.i += 1;
            match c {
                '{' => prof += 1,
                '}' => {
                    prof -= 1;
                    if prof == 0 {
                        return Ok(out);
                    }
                }
                _ => {}
            }
            out.push(c);
        }
        Err(())
    }
    /// Argumento: grupo `{...}`, comando `\x` ou um único caractere.
    fn arg(&mut self) -> Result<Node, ()> {
        self.skip_ws();
        match self.peek() {
            Some('{') => {
                self.i += 1;
                let n = self.seq(Mode::Brace)?;
                if self.peek() != Some('}') {
                    return Err(());
                }
                self.i += 1;
                Ok(n)
            }
            Some('\\') => {
                let mut items = vec![];
                self.command(&mut items)?;
                Ok(Node::Seq(items))
            }
            Some(c) => {
                self.i += 1;
                let kind = if c.is_ascii_digit() { Kind::Num } else if c.is_alphabetic() { Kind::Var } else { Kind::Sym };
                Ok(Node::Ch(c.to_string(), kind))
            }
            None => Err(()),
        }
    }
    fn read_delim(&mut self) -> char {
        self.skip_ws();
        match self.peek() {
            Some('.') => {
                self.i += 1;
                '\0'
            }
            Some('\\') => {
                self.i += 1;
                match self.name().as_str() {
                    "{" | "lbrace" => '{',
                    "}" | "rbrace" => '}',
                    "langle" => '⟨',
                    "rangle" => '⟩',
                    "lfloor" => '⌊',
                    "rfloor" => '⌋',
                    "lceil" => '⌈',
                    "rceil" => '⌉',
                    "|" | "Vert" => '‖',
                    "vert" | "lvert" | "rvert" => '|',
                    _ => '\0',
                }
            }
            Some(c) => {
                self.i += 1;
                c
            }
            None => '\0',
        }
    }
    fn seq(&mut self, mode: Mode) -> Result<Node, ()> {
        let mut items: Vec<Node> = vec![];
        loop {
            self.skip_ws();
            let Some(c) = self.peek() else {
                return if mode == Mode::Top { Ok(Node::Seq(items)) } else { Err(()) };
            };
            match mode {
                Mode::Brace if c == '}' => break,
                Mode::Right if self.starts("\\right") => break,
                Mode::Cell if c == '&' || self.starts("\\\\") || self.starts("\\end") => break,
                _ => {}
            }
            match c {
                '{' => {
                    self.i += 1;
                    let n = self.seq(Mode::Brace)?;
                    if self.peek() != Some('}') {
                        return Err(());
                    }
                    self.i += 1;
                    items.push(n);
                }
                '}' => return Err(()),
                '^' | '_' => {
                    self.i += 1;
                    let a = Box::new(self.arg()?);
                    let up = c == '^';
                    match items.pop() {
                        Some(Node::Script(b, mut sp, mut sb)) if (up && sp.is_none()) || (!up && sb.is_none()) => {
                            if up { sp = Some(a) } else { sb = Some(a) }
                            items.push(Node::Script(b, sp, sb));
                        }
                        Some(Node::Limits(b, mut sp, mut sb)) if (up && sp.is_none()) || (!up && sb.is_none()) => {
                            if up { sp = Some(a) } else { sb = Some(a) }
                            items.push(Node::Limits(b, sp, sb));
                        }
                        Some(prev) => {
                            let (sp, sb) = if up { (Some(a), None) } else { (None, Some(a)) };
                            items.push(Node::Script(Box::new(prev), sp, sb));
                        }
                        None => {
                            let (sp, sb) = if up { (Some(a), None) } else { (None, Some(a)) };
                            items.push(Node::Script(Box::new(Node::Seq(vec![])), sp, sb));
                        }
                    }
                }
                '\\' => {
                    self.i += 1;
                    if self.peek() == Some('\\') {
                        self.i += 1;
                        items.push(Node::Gap(1.0));
                    } else {
                        self.i -= 1;
                        self.command(&mut items)?;
                    }
                }
                '&' => self.i += 1,
                '~' => {
                    self.i += 1;
                    items.push(Node::Gap(0.3));
                }
                '\'' => {
                    self.i += 1;
                    let prev = items.pop().unwrap_or(Node::Seq(vec![]));
                    items.push(Node::Script(Box::new(prev), Some(Box::new(Node::Ch("′".into(), Kind::Sym))), None));
                }
                c if c.is_ascii_digit() => {
                    let mut s = String::new();
                    while let Some(d) = self.peek() {
                        if d.is_ascii_digit() || (d == '.' && self.c.get(self.i + 1).is_some_and(|x| x.is_ascii_digit())) {
                            s.push(d);
                            self.i += 1;
                        } else {
                            break;
                        }
                    }
                    items.push(Node::Ch(s, Kind::Num));
                }
                c if c.is_alphabetic() => {
                    self.i += 1;
                    items.push(Node::Ch(c.to_string(), Kind::Var));
                }
                '+' => {
                    self.i += 1;
                    items.push(Node::Ch("+".into(), Kind::Op));
                }
                '-' => {
                    self.i += 1;
                    items.push(Node::Ch("−".into(), Kind::Op));
                }
                '*' => {
                    self.i += 1;
                    items.push(Node::Ch("∗".into(), Kind::Op));
                }
                '=' | '<' | '>' => {
                    self.i += 1;
                    items.push(Node::Ch(c.to_string(), Kind::Rel));
                }
                ',' | ';' | ':' => {
                    self.i += 1;
                    items.push(Node::Ch(c.to_string(), Kind::Punct));
                }
                _ => {
                    self.i += 1;
                    items.push(Node::Ch(c.to_string(), Kind::Sym));
                }
            }
        }
        Ok(Node::Seq(items))
    }

    fn command(&mut self, items: &mut Vec<Node>) -> Result<(), ()> {
        self.i += 1; // a barra
        let name = self.name();
        let n = name.as_str();
        match n {
            "frac" | "dfrac" | "tfrac" | "cfrac" => {
                let a = self.arg()?;
                let b = self.arg()?;
                items.push(Node::Frac(Box::new(a), Box::new(b)));
            }
            "binom" => {
                let a = self.arg()?;
                let b = self.arg()?;
                let stack = Node::Grid(vec![vec![a], vec![b]], Align::Center);
                items.push(Node::Delim('(', Box::new(stack), ')'));
            }
            "sqrt" => {
                self.skip_ws();
                let mut idx = None;
                if self.peek() == Some('[') {
                    self.i += 1;
                    let n = self.seq_until_bracket()?;
                    idx = Some(Box::new(n));
                }
                let a = self.arg()?;
                items.push(Node::Sqrt(Box::new(a), idx));
            }
            "left" => {
                let open = self.read_delim();
                let body = self.seq(Mode::Right)?;
                if !self.starts("\\right") {
                    return Err(());
                }
                self.i += 6;
                let close = self.read_delim();
                items.push(Node::Delim(open, Box::new(body), close));
            }
            "begin" => {
                let env = self.raw_group()?;
                self.env(&env, items)?;
            }
            "text" | "textrm" | "mathrm" | "operatorname" | "mbox" | "textit" | "mathit" | "textsf" | "mathsf" => {
                let s = self.raw_group()?;
                items.push(Node::Ch(s, Kind::Up));
            }
            "mathbf" | "textbf" | "boldsymbol" | "bm" => {
                let s = self.raw_group()?;
                items.push(Node::Ch(s, Kind::Bold));
            }
            "mathbb" | "mathcal" | "mathfrak" | "mathscr" => {
                let s = self.raw_group()?;
                let mapped: String = s
                    .chars()
                    .map(|c| match (n, c) {
                        ("mathbb", 'R') => 'ℝ', ("mathbb", 'N') => 'ℕ', ("mathbb", 'Z') => 'ℤ', ("mathbb", 'Q') => 'ℚ', ("mathbb", 'C') => 'ℂ',
                        ("mathbb", 'P') => 'ℙ', ("mathbb", 'E') => '𝔼', ("mathcal", 'L') => 'ℒ', ("mathcal", 'H') => 'ℋ', ("mathcal", 'F') => 'ℱ',
                        _ => c,
                    })
                    .collect();
                items.push(Node::Ch(mapped, Kind::Up));
            }
            "hat" | "widehat" => items.push(Node::Accent(Acc::Hat, Box::new(self.arg()?))),
            "bar" | "overline" => items.push(Node::Accent(Acc::Bar, Box::new(self.arg()?))),
            "vec" | "overrightarrow" => items.push(Node::Accent(Acc::Vec, Box::new(self.arg()?))),
            "dot" => items.push(Node::Accent(Acc::Dot, Box::new(self.arg()?))),
            "ddot" => items.push(Node::Accent(Acc::DDot, Box::new(self.arg()?))),
            "tilde" | "widetilde" => items.push(Node::Accent(Acc::Tilde, Box::new(self.arg()?))),
            "underline" => items.push(Node::Accent(Acc::Under, Box::new(self.arg()?))),
            "quad" => items.push(Node::Gap(1.0)),
            "qquad" => items.push(Node::Gap(2.0)),
            "," => items.push(Node::Gap(0.17)),
            ";" => items.push(Node::Gap(0.28)),
            " " => items.push(Node::Gap(0.3)),
            "!" => items.push(Node::Gap(-0.1)),
            "{" | "}" | "%" | "&" | "#" | "$" | "_" => items.push(Node::Ch(n.to_string(), Kind::Sym)),
            "|" => items.push(Node::Ch("‖".into(), Kind::Sym)),
            "displaystyle" | "textstyle" | "scriptstyle" | "limits" | "nolimits" | "bigl" | "bigr" | "Bigl" | "Bigr" | "big" | "Big" => {}
            _ => {
                if let Some((_, g, big_limits)) = BIG.iter().find(|(k, _, _)| *k == n) {
                    let base = Box::new(Node::Ch(g.to_string(), Kind::Big));
                    items.push(if *big_limits { Node::Limits(base, None, None) } else { Node::Script(base, None, None) });
                } else if LIMITS_WORDS.contains(&n) {
                    items.push(Node::Limits(Box::new(Node::Ch(n.to_string(), Kind::Up)), None, None));
                } else if FUNCS.contains(&n) {
                    items.push(Node::Ch(n.to_string(), Kind::Up));
                    items.push(Node::Gap(0.12));
                } else if let Some((_, g, k)) = SYMS.iter().find(|(k, _, _)| *k == n) {
                    items.push(Node::Ch(g.to_string(), *k));
                } else {
                    items.push(Node::Ch(format!("\\{n}"), Kind::Up));
                }
            }
        }
        Ok(())
    }

    fn seq_until_bracket(&mut self) -> Result<Node, ()> {
        let ini = self.i;
        while self.peek().is_some_and(|c| c != ']') {
            self.i += 1;
        }
        if self.peek() != Some(']') {
            return Err(());
        }
        let inner: String = self.c[ini..self.i].iter().collect();
        self.i += 1;
        let mut sub = P { c: inner.chars().collect(), i: 0 };
        sub.seq(Mode::Top)
    }

    fn env(&mut self, env: &str, items: &mut Vec<Node>) -> Result<(), ()> {
        self.skip_ws();
        if env == "array" || env == "alignat" {
            // especificação das colunas: `{cc}`
            let _ = self.raw_group();
        }
        let mut rows: Vec<Vec<Node>> = vec![];
        let mut row: Vec<Node> = vec![];
        loop {
            let cell = self.seq(Mode::Cell)?;
            row.push(cell);
            if self.starts("&") {
                self.i += 1;
            } else if self.starts("\\\\") {
                self.i += 2;
                self.skip_ws();
                if self.peek() == Some('[') {
                    while self.peek().is_some_and(|c| c != ']') {
                        self.i += 1;
                    }
                    self.i += 1;
                }
                rows.push(std::mem::take(&mut row));
            } else if self.starts("\\end") {
                rows.push(std::mem::take(&mut row));
                break;
            } else {
                return Err(());
            }
        }
        self.i += 4;
        let _ = self.raw_group()?;
        // última linha vazia (`\\` antes do `\end`) some
        if rows.last().is_some_and(|r| r.len() == 1 && matches!(&r[0], Node::Seq(v) if v.is_empty())) && rows.len() > 1 {
            rows.pop();
        }
        let (grid, open, close) = match env {
            "pmatrix" => (Align::Center, '(', ')'),
            "bmatrix" => (Align::Center, '[', ']'),
            "Bmatrix" => (Align::Center, '{', '}'),
            "vmatrix" => (Align::Center, '|', '|'),
            "Vmatrix" => (Align::Center, '‖', '‖'),
            "cases" => (Align::Left, '{', '\0'),
            "align" | "align*" | "aligned" | "alignat" | "split" | "eqnarray" | "flalign" => (Align::Pairs, '\0', '\0'),
            _ => (Align::Center, '\0', '\0'),
        };
        let g = Node::Grid(rows, grid);
        items.push(if open == '\0' && close == '\0' { g } else { Node::Delim(open, Box::new(g), close) });
        Ok(())
    }
}

// ---------------------------------------------------------------- diagramação

#[derive(Clone)]
enum Cmd {
    Text { x: f32, y: f32, s: String, size: f32, font: Font },
    Line { a: (f32, f32), b: (f32, f32), w: f32 },
    Poly { pts: Vec<(f32, f32)>, w: f32 },
}

#[derive(Clone)]
struct Lay {
    w: f32,
    asc: f32,
    desc: f32,
    cmds: Vec<Cmd>,
}

impl Lay {
    fn empty() -> Lay {
        Lay { w: 0.0, asc: 0.0, desc: 0.0, cmds: vec![] }
    }
    fn put(&mut self, mut o: Lay, dx: f32, dy: f32) {
        for c in &mut o.cmds {
            match c {
                Cmd::Text { x, y, .. } => {
                    *x += dx;
                    *y += dy;
                }
                Cmd::Line { a, b, .. } => {
                    a.0 += dx;
                    a.1 += dy;
                    b.0 += dx;
                    b.1 += dy;
                }
                Cmd::Poly { pts, .. } => {
                    for p in pts {
                        p.0 += dx;
                        p.1 += dy;
                    }
                }
            }
        }
        self.asc = self.asc.max(o.asc - dy);
        self.desc = self.desc.max(o.desc + dy);
        self.cmds.append(&mut o.cmds);
    }
}

fn cw(c: char) -> f32 {
    match c {
        'i' | 'l' | 'j' | 't' | 'f' | 'I' | '|' | '!' | '\'' | '′' => 0.30,
        'm' | 'w' | 'M' | 'W' => 0.80,
        ' ' => 0.25,
        '.' | ',' | ':' | ';' => 0.26,
        '0'..='9' => 0.50,
        '(' | ')' | '[' | ']' => 0.34,
        'A'..='Z' => 0.68,
        'a'..='z' => 0.48,
        '+' | '−' | '=' | '<' | '>' | '×' | '±' | '∓' | '≤' | '≥' | '≠' | '≈' | '→' | '←' | '⇒' | '⇔' | '∈' | '∉' | '∗' | '÷' | '≡' | '∼' | '⊂' | '⊆' => 0.70,
        'α'..='ω' | 'Α'..='Ω' => 0.56,
        '∑' | '∏' | '∐' | '⋃' | '⋂' | '⨁' => 0.95,
        '∫' | '∮' | '∬' | '∭' => 0.55,
        '∞' => 0.80,
        _ => 0.58,
    }
}

fn text_w(s: &str, size: f32) -> f32 {
    s.chars().map(cw).sum::<f32>() * size
}

fn font_of(kind: Kind) -> Font {
    match kind {
        Kind::Var => Font { style: iced::font::Style::Italic, ..t::SERIF },
        Kind::Bold => t::SERIF_SEMI,
        _ => t::SERIF,
    }
}

fn lay_ch(text: &str, kind: Kind, s: f32, binary: bool) -> Lay {
    let (size, shift) = match kind {
        Kind::Big => {
            let tall = text.chars().any(|c| matches!(c, '∫' | '∮' | '∬' | '∭'));
            (s * if tall { 1.7 } else { 1.35 }, 0.30 * s)
        }
        _ => (s, 0.0),
    };
    let (lp, rp) = match kind {
        Kind::Op if binary => (0.22 * s, 0.22 * s),
        Kind::Rel => (0.30 * s, 0.30 * s),
        Kind::Punct => (0.0, 0.20 * s),
        Kind::Up if text.chars().count() > 1 => (0.0, 0.04 * s),
        _ => (0.0, 0.0),
    };
    let w = text_w(text, size);
    let mut l = Lay { w: lp + w + rp, asc: 0.75 * size - shift, desc: 0.22 * size + shift, cmds: vec![] };
    l.cmds.push(Cmd::Text { x: lp, y: shift, s: text.to_string(), size, font: font_of(kind) });
    l
}

fn lay(n: &Node, s: f32) -> Lay {
    match n {
        Node::Gap(f) => Lay { w: f * s, ..Lay::empty() },
        Node::Ch(text, kind) => lay_ch(text, *kind, s, true),
        Node::Seq(items) => {
            let mut out = Lay::empty();
            let mut x = 0.0;
            let mut prev_operand = false;
            for it in items {
                let l = match it {
                    Node::Ch(text, kind) => lay_ch(text, *kind, s, prev_operand),
                    other => lay(other, s),
                };
                prev_operand = match it {
                    Node::Ch(_, Kind::Op | Kind::Rel | Kind::Punct) | Node::Gap(_) => false,
                    Node::Ch(_, Kind::Sym) => true,
                    _ => true,
                };
                let w = l.w;
                out.put(l, x, 0.0);
                x += w;
            }
            out.w = x;
            out
        }
        Node::Frac(a, b) => {
            let (na, nb) = (lay(a, s * 0.88), lay(b, s * 0.88));
            let (axis, gap, bar) = (0.30 * s, 0.16 * s, 0.06 * s);
            let w = na.w.max(nb.w) + 0.24 * s;
            let mut out = Lay { w, asc: 0.0, desc: 0.0, cmds: vec![] };
            let (nw, dw) = (na.w, nb.w);
            let (n_base, d_base) = (-(axis + gap + na.desc), -axis + gap + nb.asc);
            out.put(na, (w - nw) / 2.0, n_base);
            out.put(nb, (w - dw) / 2.0, d_base);
            out.cmds.push(Cmd::Line { a: (0.06 * s, -axis), b: (w - 0.06 * s, -axis), w: bar });
            out.asc = out.asc.max(axis + bar);
            out.desc = out.desc.max(0.0);
            out
        }
        Node::Sqrt(body, idx) => {
            let b = lay(body, s);
            let pad = 0.14 * s;
            let (top, bottom) = (-(b.asc + pad), b.desc + 0.04 * s);
            let sign_w = 0.62 * s;
            let (iw, il) = match idx {
                Some(i) => {
                    let l = lay(i, s * 0.55);
                    (l.w, Some(l))
                }
                None => (0.0, None),
            };
            let x0 = (iw - 0.3 * s).max(0.0);
            let mut out = Lay { w: x0 + sign_w + b.w + 0.06 * s, asc: b.asc + pad + 0.04 * s, desc: b.desc, cmds: vec![] };
            let mid = top + (bottom - top) * 0.58;
            out.cmds.push(Cmd::Poly {
                pts: vec![(x0 + 0.04 * s, mid), (x0 + 0.20 * s, bottom), (x0 + 0.42 * s, top), (x0 + sign_w + b.w + 0.04 * s, top)],
                w: 1.1,
            });
            let bw = b.w;
            out.put(b, x0 + sign_w, 0.0);
            if let Some(il) = il {
                out.put(il, 0.0, top + 0.45 * s);
            }
            out.w = x0 + sign_w + bw + 0.06 * s;
            out
        }
        Node::Script(base, sup, sub) => {
            let b = lay(base, s);
            let (sp, sb) = (sup.as_ref().map(|n| lay(n, s * 0.7)), sub.as_ref().map(|n| lay(n, s * 0.7)));
            let mut up = (b.asc - 0.25 * s).max(0.42 * s);
            let mut dn = (b.desc + 0.15 * s).max(0.25 * s);
            if let (Some(p), Some(q)) = (&sp, &sb) {
                let need = p.desc + q.asc + 0.1 * s;
                if up + dn < need {
                    dn = need - up;
                    up = up.max(0.42 * s);
                }
            }
            let bw = b.w;
            let mut out = Lay::empty();
            out.put(b, 0.0, 0.0);
            let sw = sp.as_ref().map_or(0.0, |l| l.w).max(sb.as_ref().map_or(0.0, |l| l.w));
            if let Some(p) = sp {
                out.put(p, bw + 0.03 * s, -up);
            }
            if let Some(q) = sb {
                out.put(q, bw + 0.03 * s, dn);
            }
            out.w = bw + sw + 0.06 * s;
            out
        }
        Node::Limits(base, sup, sub) => {
            let b = lay(base, s);
            let (sp, sb) = (sup.as_ref().map(|n| lay(n, s * 0.7)), sub.as_ref().map(|n| lay(n, s * 0.7)));
            let w = b.w.max(sp.as_ref().map_or(0.0, |l| l.w)).max(sb.as_ref().map_or(0.0, |l| l.w));
            let (ba, bd, bw) = (b.asc, b.desc, b.w);
            let mut out = Lay { w, ..Lay::empty() };
            out.put(b, (w - bw) / 2.0, 0.0);
            if let Some(p) = sp {
                let pw = p.w;
                let dy = -(ba + 0.12 * s + p.desc);
                out.put(p, (w - pw) / 2.0, dy);
            }
            if let Some(q) = sb {
                let qw = q.w;
                let dy = bd + 0.12 * s + q.asc;
                out.put(q, (w - qw) / 2.0, dy);
            }
            out
        }
        Node::Delim(open, body, close) => {
            let b = lay(body, s);
            let axis = 0.30 * s;
            let half = (b.asc - axis).max(b.desc + axis).max(0.55 * s) + 0.08 * s;
            let (top, bottom) = (-axis - half, -axis + half);
            let dw = |c: char| if c == '\0' { 0.0 } else { 0.34 * s };
            let (lw, rw) = (dw(*open), dw(*close));
            let bw = b.w;
            let mut out = Lay { w: lw + bw + rw + 0.04 * s, asc: -top, desc: bottom, cmds: vec![] };
            if *open != '\0' {
                delim(&mut out.cmds, *open, 0.02 * s, top, bottom, true, s);
            }
            out.put(b, lw, 0.0);
            if *close != '\0' {
                delim(&mut out.cmds, *close, lw + bw + 0.02 * s, top, bottom, false, s);
            }
            out.asc = -top;
            out.desc = bottom;
            out
        }
        Node::Grid(rows, align) => {
            let cells: Vec<Vec<Lay>> = rows.iter().map(|r| r.iter().map(|n| lay(n, s)).collect()).collect();
            let ncols = cells.iter().map(|r| r.len()).max().unwrap_or(0);
            let mut cwid = vec![0.0f32; ncols];
            for r in &cells {
                for (k, c) in r.iter().enumerate() {
                    cwid[k] = cwid[k].max(c.w);
                }
            }
            let gap_after = |k: usize| -> f32 {
                match align {
                    Align::Pairs => if k % 2 == 0 { 0.12 * s } else { 1.6 * s },
                    _ => 1.0 * s,
                }
            };
            let mut xs = vec![0.0f32; ncols];
            let mut x = 0.0;
            for k in 0..ncols {
                xs[k] = x;
                x += cwid[k] + if k + 1 < ncols { gap_after(k) } else { 0.0 };
            }
            let w = x;
            let rh: Vec<(f32, f32)> = cells.iter().map(|r| (r.iter().map(|c| c.asc).fold(0.0, f32::max).max(0.6 * s), r.iter().map(|c| c.desc).fold(0.0, f32::max).max(0.2 * s))).collect();
            let rgap = 0.35 * s;
            let total: f32 = rh.iter().map(|(a, d)| a + d).sum::<f32>() + rgap * rh.len().saturating_sub(1) as f32;
            let axis = 0.30 * s;
            let mut y = -axis - total / 2.0;
            let mut out = Lay { w, asc: axis + total / 2.0, desc: total / 2.0 - axis, cmds: vec![] };
            for (ri, r) in cells.into_iter().enumerate() {
                let base = y + rh[ri].0;
                for (k, c) in r.into_iter().enumerate() {
                    let dx = match align {
                        Align::Center => (cwid[k] - c.w) / 2.0,
                        Align::Left => 0.0,
                        Align::Pairs => if k % 2 == 0 { cwid[k] - c.w } else { 0.0 },
                    };
                    out.put(c, xs[k] + dx, base);
                }
                y += rh[ri].0 + rh[ri].1 + rgap;
            }
            out.asc = axis + total / 2.0;
            out.desc = total / 2.0 - axis;
            out
        }
        Node::Accent(kind, body) => {
            let b = lay(body, s);
            let (w, asc, desc) = (b.w, b.asc, b.desc);
            let top = -(asc + 0.10 * s);
            let mut out = Lay { w, asc, desc, cmds: vec![] };
            let cx = w / 2.0;
            match kind {
                Acc::Hat => out.cmds.push(Cmd::Poly { pts: vec![(cx - 0.20 * s, top + 0.12 * s), (cx, top), (cx + 0.20 * s, top + 0.12 * s)], w: 1.0 }),
                Acc::Bar => out.cmds.push(Cmd::Line { a: (0.0, top + 0.04 * s), b: (w, top + 0.04 * s), w: 1.0 }),
                Acc::Vec => {
                    out.cmds.push(Cmd::Line { a: (0.0, top + 0.06 * s), b: (w, top + 0.06 * s), w: 1.0 });
                    out.cmds.push(Cmd::Poly { pts: vec![(w - 0.14 * s, top - 0.02 * s), (w, top + 0.06 * s), (w - 0.14 * s, top + 0.14 * s)], w: 1.0 });
                }
                Acc::Dot => out.cmds.push(Cmd::Line { a: (cx - 0.01 * s, top + 0.05 * s), b: (cx + 0.01 * s, top + 0.05 * s), w: 2.2 }),
                Acc::DDot => {
                    for dx in [-0.09, 0.09] {
                        out.cmds.push(Cmd::Line { a: (cx + dx * s - 0.01 * s, top + 0.05 * s), b: (cx + dx * s + 0.01 * s, top + 0.05 * s), w: 2.2 });
                    }
                }
                Acc::Tilde => out.cmds.push(Cmd::Poly {
                    pts: (0..=8).map(|k| { let u = k as f32 / 8.0; (cx - 0.22 * s + u * 0.44 * s, top + 0.06 * s - (u * std::f32::consts::TAU).sin() * 0.04 * s) }).collect(),
                    w: 1.0,
                }),
                Acc::Under => out.cmds.push(Cmd::Line { a: (0.0, desc + 0.08 * s), b: (w, desc + 0.08 * s), w: 1.0 }),
            }
            out.put(b, 0.0, 0.0);
            match kind {
                Acc::Under => out.desc = desc + 0.12 * s,
                _ => out.asc = asc + 0.18 * s,
            }
            out
        }
    }
}

/// Delimitador esticado (parêntese, colchete, chave, barra, ângulo, piso/teto) entre `top` e `bottom`.
fn delim(cmds: &mut Vec<Cmd>, c: char, x: f32, top: f32, bottom: f32, left: bool, s: f32) {
    let (xl, xr) = (x + 0.04 * s, x + 0.30 * s);
    let (near, far) = if left { (xr, xl) } else { (xl, xr) }; // `near` encosta no conteúdo
    let mid = (top + bottom) / 2.0;
    let h = bottom - top;
    let w = 1.1;
    match c {
        '(' | ')' => {
            let pts = (0..=14)
                .map(|k| {
                    let u = k as f32 / 14.0;
                    (near + (far - near) * (std::f32::consts::PI * u).sin() * 0.9, top + h * u)
                })
                .collect();
            cmds.push(Cmd::Poly { pts, w });
        }
        '[' | ']' => cmds.push(Cmd::Poly { pts: vec![(near, top), (far, top), (far, bottom), (near, bottom)], w }),
        '⌊' | '⌋' => cmds.push(Cmd::Poly { pts: vec![(far, top), (far, bottom), (near, bottom)], w }),
        '⌈' | '⌉' => cmds.push(Cmd::Poly { pts: vec![(near, top), (far, top), (far, bottom)], w }),
        '{' | '}' => {
            let xm = (near + far) / 2.0;
            cmds.push(Cmd::Poly {
                pts: vec![(near, top), (xm, top + 0.06 * h), (xm, mid - 0.07 * h), (far, mid), (xm, mid + 0.07 * h), (xm, bottom - 0.06 * h), (near, bottom)],
                w,
            });
        }
        '|' => cmds.push(Cmd::Line { a: ((xl + xr) / 2.0, top), b: ((xl + xr) / 2.0, bottom), w }),
        '‖' => {
            let xm = (xl + xr) / 2.0;
            cmds.push(Cmd::Line { a: (xm - 0.04 * s, top), b: (xm - 0.04 * s, bottom), w });
            cmds.push(Cmd::Line { a: (xm + 0.04 * s, top), b: (xm + 0.04 * s, bottom), w });
        }
        '⟨' | '⟩' => cmds.push(Cmd::Poly { pts: vec![(near, top), (far, mid), (near, bottom)], w }),
        other => cmds.push(Cmd::Text { x, y: mid + 0.33 * s, s: other.to_string(), size: s, font: t::SERIF }),
    }
}

#[derive(Clone)]
pub struct Formula {
    lay: Lay,
    pad: f32,
}

impl Formula {
    pub fn height(&self) -> f32 {
        self.lay.asc + self.lay.desc + self.pad * 2.0
    }
}

thread_local! {
    static CACHE: std::cell::RefCell<std::collections::VecDeque<((String, u32), Option<Formula>)>> = const { std::cell::RefCell::new(std::collections::VecDeque::new()) };
}
/// Diagrama a fórmula com cache limitado (REGRAS.md: portabilidade).
pub fn typeset(src: &str, size: f32) -> Option<Formula> {
    CACHE.with(|cache| {
        let key = (src.to_string(), size.to_bits());
        let mut cache = cache.borrow_mut();
        if let Some(index) = cache.iter().position(|(k, _)| *k == key) {
            let item = cache.remove(index).unwrap();
            let result = item.1.clone(); cache.push_back(item); return result;
        }
        let result = typeset_uncached(src, size);
        if cache.len() >= 64 { cache.pop_front(); }
        cache.push_back((key, result.clone()));
        result
    })
}
fn typeset_uncached(src: &str, size: f32) -> Option<Formula> {
    let mut s = src.trim();
    for (a, b) in [("$$", "$$"), ("\\[", "\\]"), ("\\(", "\\)"), ("$", "$")] {
        if let Some(r) = s.strip_prefix(a).and_then(|r| r.strip_suffix(b)) {
            s = r.trim();
            break;
        }
    }
    if s.is_empty() || s.len() > 4000 {
        return None;
    }
    let mut p = P { c: s.chars().collect(), i: 0 };
    let n = p.seq(Mode::Top).ok()?;
    if p.i < p.c.len() {
        return None;
    }
    let l = lay(&n, size);
    if l.w <= 0.0 {
        return None;
    }
    Some(Formula { lay: l, pad: size * 0.35 })
}

impl<M> canvas::Program<M> for Formula {
    type State = ();
    fn draw(&self, _s: &(), renderer: &Renderer, _th: &Theme, bounds: Rectangle, _c: mouse::Cursor) -> Vec<Geometry> {
        let mut f = Frame::new(renderer, bounds.size());
        let k = (bounds.width / (self.lay.w + 4.0)).min(1.0);
        let ox = (bounds.width - self.lay.w * k) / 2.0;
        let by = self.pad + self.lay.asc * k;
        let p = |x: f32, y: f32| Point::new(ox + x * k, by + y * k);
        for c in &self.lay.cmds {
            match c {
                Cmd::Text { x, y, s, size, font } => {
                    f.fill_text(Text {
                        content: s.clone(),
                        position: p(*x, *y - 0.33 * size),
                        color: t::FG,
                        size: (size * k).into(),
                        font: *font,
                        align_x: Horizontal::Left.into(),
                        align_y: Vertical::Center,
                        ..Text::default()
                    });
                }
                Cmd::Line { a, b, w } => f.stroke(&Path::line(p(a.0, a.1), p(b.0, b.1)), Stroke::default().with_width(*w).with_color(t::FG)),
                Cmd::Poly { pts, w } => {
                    let path = Path::new(|bd| {
                        for (i, q) in pts.iter().enumerate() {
                            let pt = p(q.0, q.1);
                            if i == 0 { bd.move_to(pt) } else { bd.line_to(pt) }
                        }
                    });
                    f.stroke(&path, Stroke::default().with_width(*w).with_color(t::FG).with_line_join(canvas::LineJoin::Round).with_line_cap(canvas::LineCap::Round));
                }
            }
        }
        vec![f.into_geometry()]
    }
}
