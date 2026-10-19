//! Animações do protótipo (§26.1): curva `--ease` e valores de progresso por camada.
//! O `view` só lê; quem escreve é `root`, a cada quadro, a partir das marcas de tempo do `App`.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub static PAGE: Prog = Prog::new();
pub static POP: Prog = Prog::new();
pub static MODAL: Prog = Prog::new();
pub static SCRIM: Prog = Prog::new();
pub static OVER: Prog = Prog::new();
pub static DRAWER: Prog = Prog::new();
pub static TOAST: Prog = Prog::new();
pub static MORE: Prog = Prog::new();
pub static TOOLS: Prog = Prog::new();
pub static SWITCH: Prog = Prog::new();
pub static SWITCH_KEY: Mutex<String> = Mutex::new(String::new());

pub struct Prog(AtomicU32);
impl Prog {
    pub const fn new() -> Self {
        Prog(AtomicU32::new(0x3f80_0000)) // 1.0
    }
    pub fn set(&self, v: f32) {
        self.0.store(v.to_bits(), Ordering::Relaxed);
    }
    pub fn get(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }
}

/// `cubic-bezier(.16, 1, .3, 1)`.
pub fn ease(x: f32) -> f32 {
    let (x1, y1, x2, y2) = (0.16f32, 1.0f32, 0.3f32, 1.0f32);
    let x = x.clamp(0.0, 1.0);
    if x <= 0.0 || x >= 1.0 {
        return x;
    }
    let bx = |t: f32| 3.0 * (1.0 - t).powi(2) * t * x1 + 3.0 * (1.0 - t) * t * t * x2 + t.powi(3);
    let by = |t: f32| 3.0 * (1.0 - t).powi(2) * t * y1 + 3.0 * (1.0 - t) * t * t * y2 + t.powi(3);
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..24 {
        let m = (lo + hi) / 2.0;
        if bx(m) < x {
            lo = m;
        } else {
            hi = m;
        }
    }
    by((lo + hi) / 2.0)
}

/// Progresso 0→1 de uma animação que começou em `at` e dura `ms`. Sem marca ou com movimento reduzido: 1.
pub fn progress(at: Option<Instant>, now: Instant, ms: u64, reduced: bool) -> f32 {
    match at {
        Some(t) if !reduced => ease((now.saturating_duration_since(t).as_secs_f32() * 1000.0 / ms as f32).min(1.0)),
        _ => 1.0,
    }
}

pub fn active(at: Option<Instant>, now: Instant, ms: u64) -> bool {
    at.map(|t| now.saturating_duration_since(t) < Duration::from_millis(ms)).unwrap_or(false)
}
