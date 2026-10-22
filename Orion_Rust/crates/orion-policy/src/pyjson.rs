//! Serialização JSON igual à do `json.dumps` do Python (separadores `", "` e `": "`,
//! `ensure_ascii=False`, `sort_keys` opcional).
//!
//! O hash das aprovações e o `binding` precisam ser os mesmos nos dois lados durante a
//! transição, porque o banco é compartilhado.

use serde_json::Value;
use std::fmt::Write;

/// `json.dumps(valor, sort_keys=sort_keys, ensure_ascii=False)`.
pub fn dumps(valor: &Value, sort_keys: bool) -> String {
    let mut s = String::new();
    escrever(&mut s, valor, sort_keys, false);
    s
}

/// `json.dumps(valor, sort_keys=sort_keys)` com `ensure_ascii=True` (padrão do Python).
pub fn dumps_ascii(valor: &Value, sort_keys: bool) -> String {
    let mut s = String::new();
    escrever(&mut s, valor, sort_keys, true);
    s
}

fn escrever(s: &mut String, v: &Value, sort: bool, ascii: bool) {
    match v {
        Value::Null => s.push_str("null"),
        Value::Bool(b) => s.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => {
            if let Some(f) = n.as_f64().filter(|_| n.is_f64()) {
                s.push_str(&float_repr(f));
            } else {
                let _ = write!(s, "{n}");
            }
        }
        Value::String(t) => string(s, t, ascii),
        Value::Array(a) => {
            s.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    s.push_str(", ");
                }
                escrever(s, x, sort, ascii);
            }
            s.push(']');
        }
        Value::Object(m) => {
            s.push('{');
            let mut chaves: Vec<&String> = m.keys().collect();
            if sort {
                // Python ordena por ponto de código; `String` em Rust compara por bytes UTF-8,
                // que preserva a mesma ordem.
                chaves.sort();
            }
            for (i, k) in chaves.into_iter().enumerate() {
                if i > 0 {
                    s.push_str(", ");
                }
                string(s, k, ascii);
                s.push_str(": ");
                escrever(s, &m[k], sort, ascii);
            }
            s.push('}');
        }
    }
}

/// `repr(float)` do Python: `1.0`, `0.5`, `1e+20`, `inf` vira `Infinity`.
pub fn float_repr(f: f64) -> String {
    if f.is_nan() {
        return "NaN".into();
    }
    if f.is_infinite() {
        return if f > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    if f == f.trunc() && f.abs() < 1e16 {
        return format!("{f:.1}");
    }
    let r = format!("{f}");
    if f.abs() >= 1e16 || (f != 0.0 && f.abs() < 1e-4) {
        // Python usa notação científica com expoente de dois dígitos: 1e-05, 1.5e+20.
        let e = format!("{f:e}");
        if let Some((mant, exp)) = e.split_once('e') {
            let (sinal, digitos) = match exp.strip_prefix('-') {
                Some(d) => ('-', d),
                None => ('+', exp),
            };
            return format!("{mant}e{sinal}{digitos:0>2}");
        }
    }
    r
}

fn string(s: &mut String, t: &str, ascii: bool) {
    s.push('"');
    for c in t.chars() {
        match c {
            '"' => s.push_str("\\\""),
            '\\' => s.push_str("\\\\"),
            '\n' => s.push_str("\\n"),
            '\r' => s.push_str("\\r"),
            '\t' => s.push_str("\\t"),
            '\u{08}' => s.push_str("\\b"),
            '\u{0c}' => s.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                let _ = write!(s, "\\u{:04x}", c as u32);
            }
            c if ascii && (c as u32) > 0x7e => {
                let mut buf = [0u16; 2];
                for u in c.encode_utf16(&mut buf) {
                    let _ = write!(s, "\\u{:04x}", u);
                }
            }
            c => s.push(c),
        }
    }
    s.push('"');
}
