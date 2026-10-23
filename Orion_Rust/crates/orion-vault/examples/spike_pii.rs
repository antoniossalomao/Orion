//! Spike F00.27: segredos e PII em notas reais (REGRAS.md: segredo e PII nunca vão para o vault).
//! Uso: cargo run -p orion-vault --example spike_pii --release -- <pasta-do-vault>
//! Mede quantas notas cada regra marca e imprime amostras mascaradas para revisão manual.
use orion_vault::scanner::SecretScanner;
use regex::Regex;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().is_some_and(|x| x == "md") {
            out.push(p);
        }
    }
}

fn digits(s: &str) -> Vec<u32> {
    s.chars().filter_map(|c| c.to_digit(10)).collect()
}

fn cpf_valid(s: &str) -> bool {
    let d = digits(s);
    if d.len() != 11 || d.iter().all(|x| *x == d[0]) {
        return false;
    }
    (9..11).all(|n| {
        let sum: u32 = d[..n]
            .iter()
            .enumerate()
            .map(|(i, x)| x * (n as u32 + 1 - i as u32))
            .sum();
        let dv = (sum * 10 % 11) % 10;
        dv == d[n]
    })
}

fn cnpj_valid(s: &str) -> bool {
    let d = digits(s);
    if d.len() != 14 || d.iter().all(|x| *x == d[0]) {
        return false;
    }
    let w1 = [5, 4, 3, 2, 9, 8, 7, 6, 5, 4, 3, 2];
    let w2 = [6, 5, 4, 3, 2, 9, 8, 7, 6, 5, 4, 3, 2];
    let dv = |w: &[u32], n: usize| {
        let s: u32 = d[..n].iter().zip(w).map(|(a, b)| a * b).sum();
        let r = s % 11;
        if r < 2 {
            0
        } else {
            11 - r
        }
    };
    dv(&w1, 12) == d[12] && dv(&w2, 13) == d[13]
}

fn luhn(s: &str) -> bool {
    let d = digits(s);
    if !(13..=19).contains(&d.len()) {
        return false;
    }
    let sum: u32 = d
        .iter()
        .rev()
        .enumerate()
        .map(|(i, x)| {
            if i % 2 == 1 {
                let y = x * 2;
                if y > 9 {
                    y - 9
                } else {
                    y
                }
            } else {
                *x
            }
        })
        .sum();
    sum % 10 == 0
}

fn mask(s: &str) -> String {
    let n = s.chars().count();
    if n <= 4 {
        return "*".repeat(n);
    }
    let head: String = s.chars().take(2).collect();
    format!("{head}{}", "*".repeat(n - 2))
}

struct Rule {
    name: &'static str,
    re: Regex,
    check: Option<fn(&str) -> bool>,
}

fn main() {
    let root = std::env::args().nth(1).expect("pasta do vault");
    let mut files = Vec::new();
    walk(Path::new(&root), &mut files);
    let rules = vec![
        Rule {
            name: "cpf",
            re: Regex::new(r"\b\d{3}\.?\d{3}\.?\d{3}-?\d{2}\b").unwrap(),
            check: Some(cpf_valid),
        },
        Rule {
            name: "cnpj",
            re: Regex::new(r"\b\d{2}\.?\d{3}\.?\d{3}/?\d{4}-?\d{2}\b").unwrap(),
            check: Some(cnpj_valid),
        },
        Rule {
            name: "cartao",
            re: Regex::new(r"\b(?:\d[ -]?){13,19}\b").unwrap(),
            check: Some(luhn),
        },
        Rule {
            name: "rg_rotulado",
            re: Regex::new(r"(?i)\brg\b[^\n\d]{0,12}(\d{1,2}\.?\d{3}\.?\d{3}(?:-?[0-9xX])?)")
                .unwrap(),
            check: None,
        },
        Rule {
            name: "agencia_conta",
            re: Regex::new(
                r"(?i)\b(?:ag(?:[eê]ncia)?\.?|conta(?: corrente)?|c/c)\s*:?\s*\d{3,6}(?:-?\d)?\b",
            )
            .unwrap(),
            check: None,
        },
        Rule {
            name: "pix_chave_aleatoria",
            re: Regex::new(r"(?i)\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b")
                .unwrap(),
            check: None,
        },
        Rule {
            name: "pix_rotulado",
            re: Regex::new(r"(?i)\bpix\b[^\n]{0,20}[:=]\s*\S{5,}").unwrap(),
            check: None,
        },
        Rule {
            name: "telefone_br",
            re: Regex::new(r"(?:\+?55\s?)?\(?\b\d{2}\)?\s?9?\d{4}-?\d{4}\b").unwrap(),
            check: None,
        },
        Rule {
            name: "email",
            re: Regex::new(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b").unwrap(),
            check: None,
        },
        Rule {
            name: "cep",
            re: Regex::new(r"(?i)\bcep\b\D{0,4}\d{5}-?\d{3}\b").unwrap(),
            check: None,
        },
    ];
    let mut notes_hit: BTreeMap<&str, usize> = BTreeMap::new();
    let mut matches: BTreeMap<&str, usize> = BTreeMap::new();
    let mut samples: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    let mut secret_hits: BTreeMap<String, usize> = BTreeMap::new();
    let mut bytes = 0usize;
    let t = Instant::now();
    for f in &files {
        let Ok(text) = std::fs::read_to_string(f) else {
            continue;
        };
        bytes += text.len();
        if let Err(e) = SecretScanner::scan(&text) {
            *secret_hits.entry(e.to_string()).or_default() += 1;
        }
        for r in &rules {
            let mut any = false;
            for m in r.re.find_iter(&text) {
                if r.check.is_some_and(|c| !c(m.as_str())) {
                    continue;
                }
                any = true;
                *matches.entry(r.name).or_default() += 1;
                let v = samples.entry(r.name).or_default();
                if v.len() < 5 {
                    v.push(format!(
                        "{} => {}",
                        f.strip_prefix(&root).unwrap_or(f).display(),
                        mask(m.as_str())
                    ));
                }
            }
            if any {
                *notes_hit.entry(r.name).or_default() += 1;
            }
        }
    }
    let dt = t.elapsed();
    println!("notas: {} ({} KiB) em {:?}", files.len(), bytes / 1024, dt);
    for r in &rules {
        println!(
            "{:22} notas={:4} ocorrencias={:4}",
            r.name,
            notes_hit.get(r.name).unwrap_or(&0),
            matches.get(r.name).unwrap_or(&0)
        );
        for s in samples.get(r.name).into_iter().flatten() {
            println!("    {s}");
        }
    }
    println!("--- SecretScanner (regras atuais), notas bloqueadas por tipo ---");
    for (k, v) in &secret_hits {
        println!("{v:4} {k}");
    }
}
