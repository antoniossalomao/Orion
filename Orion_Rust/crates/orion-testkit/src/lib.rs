//! Apoio dos testes de paridade com o Python (ORION.md: "paridade antes de melhoria").
//!
//! O `httpx.MockTransport` dos testes Python vira [`http::servidor`]; o relógio injetável vira
//! [`Relogio`]. Só entra como `dev-dependency`.

pub mod http;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub type Clock = Arc<dyn Fn() -> f64 + Send + Sync>;

/// Relógio de mentira, ajustável (segundos desde a época).
#[derive(Clone)]
pub struct Relogio(Arc<AtomicU64>);

impl Relogio {
    pub fn em(t: f64) -> Self {
        Relogio(Arc::new(AtomicU64::new(t.to_bits())))
    }
    pub fn get(&self) -> f64 {
        f64::from_bits(self.0.load(Ordering::SeqCst))
    }
    pub fn set(&self, t: f64) {
        self.0.store(t.to_bits(), Ordering::SeqCst);
    }
    pub fn avanca(&self, s: f64) {
        self.set(self.get() + s);
    }
    pub fn clock(&self) -> Clock {
        let a = self.0.clone();
        Arc::new(move || f64::from_bits(a.load(Ordering::SeqCst)))
    }
}
