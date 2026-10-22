//! Audit: redação de segredos e limite de chamadas.
//!
//! REGRAS.md: segredos nunca em log, audit ou processo filho. `redact` mascara por nome de
//! chave e por padrões de token conhecidos, e trunca textos longos.

use crate::Clock;
use regex::Regex;
use serde_json::{Map, Value};
use std::collections::{HashMap, VecDeque};
use std::sync::{LazyLock, Mutex};

pub const MASCARA: &str = "***";

static CHAVES_SECRETAS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(token|secret|passw|senha|api[_-]?key|authorization|cookie|bearer)").unwrap()
});
static PADROES_TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"(gsk_[A-Za-z0-9]{20,}|AIza[0-9A-Za-z_\-]{30,}|sk-[A-Za-z0-9_\-]{20,}|",
        r"ghp_[A-Za-z0-9]{30,}|\d{8,10}:[A-Za-z0-9_\-]{30,}|",
        r"eyJ[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{10,}\.[A-Za-z0-9_\-]{5,})"
    ))
    .unwrap()
});

/// Mascara padrões de token conhecidos num texto (sem truncar).
pub fn mask_tokens(texto: &str) -> String {
    PADROES_TOKEN.replace_all(texto, MASCARA).into_owned()
}

static EMAIL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}").unwrap());
static CPF: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b\d{3}\.?\d{3}\.?\d{3}-?\d{2}\b").unwrap());
static CNPJ: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b\d{2}\.?\d{3}\.?\d{3}/?\d{4}-?\d{2}\b").unwrap());
static TELEFONE: LazyLock<Regex> = LazyLock::new(|| {
    // só formatos de telefone de verdade: (11) 91234-5678, +55 11 91234-5678, 11 91234-5678, 91234-5678
    Regex::new(r"\(\d{2}\)\s?9?\d{4}-?\d{4}|\+55[\s-]?\d{2}[\s-]?9?\d{4}-?\d{4}|\b\d{2}\s9\d{4}-?\d{4}\b|\b9?\d{4}-\d{4}\b").unwrap()
});
static CARTAO: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(?:\d[ -]?){12,18}\d\b").unwrap());

fn digitos(s: &str) -> Vec<u32> {
    s.chars().filter_map(|c| c.to_digit(10)).collect()
}

/// Dígitos verificadores do CPF: evita mascarar qualquer número de 11 dígitos.
fn cpf_valido(s: &str) -> bool {
    let d = digitos(s);
    if d.len() != 11 || d.iter().all(|x| *x == d[0]) {
        return false;
    }
    let dv = |n: usize| {
        let soma: u32 = d[..n].iter().enumerate().map(|(i, x)| x * (n as u32 + 1 - i as u32)).sum();
        let r = (soma * 10) % 11;
        if r == 10 { 0 } else { r }
    };
    dv(9) == d[9] && dv(10) == d[10]
}

fn cnpj_valido(s: &str) -> bool {
    let d = digitos(s);
    if d.len() != 14 || d.iter().all(|x| *x == d[0]) {
        return false;
    }
    let dv = |n: usize| {
        let pesos: Vec<u32> = if n == 12 { vec![5, 4, 3, 2, 9, 8, 7, 6, 5, 4, 3, 2] } else { vec![6, 5, 4, 3, 2, 9, 8, 7, 6, 5, 4, 3, 2] };
        let soma: u32 = d[..n].iter().zip(&pesos).map(|(x, p)| x * p).sum();
        let r = soma % 11;
        if r < 2 { 0 } else { 11 - r }
    };
    dv(12) == d[12] && dv(13) == d[13]
}

/// Algoritmo de Luhn: só número de cartão de verdade é mascarado.
fn luhn(s: &str) -> bool {
    let d = digitos(s);
    if !(13..=19).contains(&d.len()) {
        return false;
    }
    let soma: u32 = d.iter().rev().enumerate().map(|(i, x)| if i % 2 == 1 { let y = x * 2; if y > 9 { y - 9 } else { y } } else { *x }).sum();
    soma % 10 == 0
}

/// Mascara dados pessoais num texto de log: e-mail, CPF e CNPJ (com dígitos verificadores),
/// telefone brasileiro e número de cartão (Luhn). Use em **log e audit**, não em cartões de
/// aprovação, onde o usuário precisa ver o destino.
pub fn mask_pii(texto: &str) -> String {
    let t = EMAIL.replace_all(texto, MASCARA);
    let t = CNPJ.replace_all(&t, |c: &regex::Captures| if cnpj_valido(&c[0]) { MASCARA.to_string() } else { c[0].to_string() }).into_owned();
    let t = CARTAO.replace_all(&t, |c: &regex::Captures| if luhn(&c[0]) { MASCARA.to_string() } else { c[0].to_string() }).into_owned();
    let t = CPF.replace_all(&t, |c: &regex::Captures| if cpf_valido(&c[0]) { MASCARA.to_string() } else { c[0].to_string() }).into_owned();
    TELEFONE.replace_all(&t, MASCARA).into_owned()
}

/// `redact` + máscara de dados pessoais nos textos. Usada no audit, que fica gravado.
pub fn redact_pii(valor: &Value, limite: usize) -> Value {
    fn pii(v: Value) -> Value {
        match v {
            Value::String(s) => Value::String(mask_pii(&s)),
            Value::Array(a) => Value::Array(a.into_iter().map(pii).collect()),
            Value::Object(m) => Value::Object(m.into_iter().map(|(k, v)| (k, pii(v))).collect()),
            outro => outro,
        }
    }
    pii(redact(valor, limite))
}

/// `true` se o nome da chave indica segredo.
pub fn chave_secreta(nome: &str) -> bool {
    CHAVES_SECRETAS.is_match(nome)
}

/// Cópia de `valor` sem segredos e com textos limitados a `limite` caracteres.
pub fn redact(valor: &Value, limite: usize) -> Value {
    match valor {
        Value::Object(m) => {
            let mut out = Map::new();
            for (k, v) in m {
                let novo = if chave_secreta(k) { Value::String(MASCARA.into()) } else { redact(v, limite) };
                out.insert(k.clone(), novo);
            }
            Value::Object(out)
        }
        Value::Array(a) => Value::Array(a.iter().map(|v| redact(v, limite)).collect()),
        Value::String(s) => {
            let texto = mask_tokens(s);
            if texto.chars().count() <= limite {
                Value::String(texto)
            } else {
                let mut t: String = texto.chars().take(limite).collect();
                t.push('…');
                Value::String(t)
            }
        }
        outro => outro.clone(),
    }
}

/// Janela deslizante por ferramenta (portado do legado), thread-safe.
pub struct RateLimiter {
    limites: Mutex<HashMap<String, (u32, u64)>>,
    janelas: Mutex<HashMap<String, VecDeque<f64>>>,
    clock: Clock,
}

impl RateLimiter {
    pub fn new(limites: HashMap<String, (u32, u64)>, clock: Clock) -> Self {
        RateLimiter { limites: Mutex::new(limites), janelas: Mutex::new(HashMap::new()), clock }
    }

    pub fn set_limit(&self, ferramenta: &str, limite: (u32, u64)) {
        self.limites.lock().unwrap().insert(ferramenta.into(), limite);
    }

    /// Registra a chamada e devolve `None`, ou o motivo se estourou o limite.
    pub fn check(&self, ferramenta: &str) -> Option<String> {
        let (maximo, janela) = *self.limites.lock().unwrap().get(ferramenta)?;
        let agora = (self.clock)();
        let mut janelas = self.janelas.lock().unwrap();
        let fila = janelas.entry(ferramenta.into()).or_default();
        while fila.front().is_some_and(|t| agora - t > janela as f64) {
            fila.pop_front();
        }
        if fila.len() >= maximo as usize {
            let espera = (janela as f64 - (agora - fila[0])) as i64;
            return Some(format!(
                "limite: '{ferramenta}' atingiu {maximo} chamadas em {janela}s (aguarde ~{espera}s)"
            ));
        }
        fila.push_back(agora);
        None
    }
}
