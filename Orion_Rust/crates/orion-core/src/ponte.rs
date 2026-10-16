//! O lado do servidor da ponte: a ponte conectada (no máximo uma), os comandos que o servidor lhe
//! manda e a captura do "O que é isso?".
//!
//! REGRAS.md: o servidor só consegue pedir `abrir` (uma tela do Orion) e `colar` (texto que o
//! Antônio já viu e aprovou na tela). Nada que leia a tela ou a área de transferência parte daqui.

use parking_lot::Mutex;
use rand::RngCore;
use regex::Regex;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::LazyLock;
use tokio::sync::mpsc;

pub const MAX_COLAR: usize = 8000;
static ROTA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^#/[a-z0-9/_-]{0,40}$").unwrap());

/// Comando fora da lista, ou com argumento que não passa na validação.
#[derive(Debug, Clone, thiserror::Error)]
#[error("{0}")]
pub struct ComandoInvalido(pub String);

/// O comando normalizado (só as chaves conhecidas), ou o motivo da recusa.
pub fn validar(comando: &Value) -> Result<Value, ComandoInvalido> {
    let erro = |m: &str| ComandoInvalido(m.into());
    match comando.get("cmd").and_then(Value::as_str) {
        Some("abrir") => {
            let rota = comando.get("rota").and_then(Value::as_str).unwrap_or("#/");
            if !ROTA.is_match(rota) {
                return Err(erro("rota inválida"));
            }
            Ok(json!({"cmd": "abrir", "rota": rota}))
        }
        Some("colar") => match comando.get("texto").and_then(Value::as_str) {
            Some(t) if !t.trim().is_empty() && t.chars().count() <= MAX_COLAR => Ok(json!({"cmd": "colar", "texto": t})),
            _ => Err(ComandoInvalido(format!("texto vazio ou maior que {MAX_COLAR} caracteres"))),
        },
        _ => Err(erro("comando desconhecido")),
    }
}

pub type Fila = mpsc::Receiver<Option<Value>>;

/// Uma conexão por vez: parear de novo (ou abrir a ponte duas vezes) derruba a antiga.
pub struct PonteHub {
    atual: Mutex<Option<(u64, mpsc::Sender<Option<Value>>)>>,
    desde: Mutex<Option<f64>>,
    enviados: AtomicU64,
    geracao: AtomicU64,
}

impl Default for PonteHub {
    fn default() -> Self {
        PonteHub { atual: Mutex::new(None), desde: Mutex::new(None), enviados: AtomicU64::new(0), geracao: AtomicU64::new(0) }
    }
}

impl PonteHub {
    pub fn conectada(&self) -> bool {
        self.atual.lock().is_some()
    }

    /// Chamada pelo handler do WebSocket. Devolve o identificador da conexão e a fila.
    pub fn anexar(&self) -> (u64, Fila) {
        let (tx, rx) = mpsc::channel(50);
        let id = self.geracao.fetch_add(1, Ordering::SeqCst) + 1;
        let antiga = self.atual.lock().replace((id, tx));
        *self.desde.lock() = Some(crate::util::agora());
        if let Some((_, a)) = antiga {
            let _ = a.try_send(None); // o handler antigo fecha o socket dele
        }
        (id, rx)
    }

    pub fn soltar(&self, id: u64) {
        let mut g = self.atual.lock();
        if g.as_ref().is_some_and(|(i, _)| *i == id) {
            *g = None;
            *self.desde.lock() = None;
        }
    }

    /// Valida e enfileira. `Ok(false)` se não há ponte conectada. Seguro de qualquer thread (a
    /// escuta de palmas chama daqui).
    pub fn enviar(&self, comando: &Value) -> Result<bool, ComandoInvalido> {
        let pronto = validar(comando)?;
        let Some(tx) = self.atual.lock().as_ref().map(|(_, t)| t.clone()) else { return Ok(false) };
        // ponte travada: melhor perder o comando que acumular
        if tx.try_send(Some(pronto)).is_ok() {
            self.enviados.fetch_add(1, Ordering::SeqCst);
        }
        Ok(true)
    }

    pub fn estado(&self) -> Value {
        json!({"conectada": self.conectada(), "desde": *self.desde.lock(), "enviados": self.enviados.load(Ordering::SeqCst)})
    }
}

// ── captura do "O que é isso?" ─────────────────────────────────────────────

pub const TTL_S: f64 = 300.0;
pub const MAX_GUARDADAS: usize = 3;

pub fn tipo_da_imagem(d: &[u8]) -> Option<&'static str> {
    if d.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if d.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if d.len() >= 12 && &d[..4] == b"RIFF" && &d[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

/// A captura fica **só na memória** do servidor local, por 5 minutos; nada vai a disco.
#[derive(Default)]
pub struct Imagens {
    itens: Mutex<HashMap<String, (f64, Vec<u8>, &'static str)>>,
}

impl Imagens {
    fn podar(m: &mut HashMap<String, (f64, Vec<u8>, &'static str)>) {
        let agora = crate::util::agora();
        m.retain(|_, v| agora - v.0 < TTL_S);
        while m.len() > MAX_GUARDADAS {
            if let Some(k) = m.iter().min_by(|a, b| a.1 .0.total_cmp(&b.1 .0)).map(|(k, _)| k.clone()) {
                m.remove(&k);
            }
        }
    }

    pub fn guardar(&self, dados: &[u8], max: usize) -> Result<String, String> {
        if dados.is_empty() {
            return Err("imagem vazia".into());
        }
        if dados.len() > max {
            return Err(format!("imagem passa de {} MB", max / 1024 / 1024));
        }
        let mime = tipo_da_imagem(dados).ok_or("só JPEG, PNG ou WebP")?;
        let mut b = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut b);
        let id = base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, b);
        let mut g = self.itens.lock();
        g.insert(id.clone(), (crate::util::agora(), dados.to_vec(), mime));
        Self::podar(&mut g);
        Ok(id)
    }

    pub fn pegar(&self, id: &str) -> Option<(Vec<u8>, &'static str)> {
        let mut g = self.itens.lock();
        Self::podar(&mut g);
        g.get(id).map(|v| (v.1.clone(), v.2))
    }
}
