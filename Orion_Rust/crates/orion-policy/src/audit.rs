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
