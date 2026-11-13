//! Nível do som que o Orion está falando, em faixas de frequência, para a esfera reagir à voz.
//! Calculado fora do callback de áudio (que não pode alocar nem esperar): uma thread leve lê a
//! posição tocada e mede a janela atual. Só o número sai daqui; o áudio nunca é guardado.
use std::sync::OnceLock;

pub const BANDS: usize = 8;
/// Centros das faixas em Hz (escala aproximadamente logarítmica até perto de 8 kHz).
const CENTERS: [f32; BANDS] = [140.0, 260.0, 450.0, 750.0, 1200.0, 1900.0, 3000.0, 4600.0];
pub(crate) const WINDOW: usize = 512;

static SINK: OnceLock<fn(Option<[f32; BANDS]>)> = OnceLock::new();

/// Registra quem recebe o nível (o servidor liga isto à esfera do app). `None` = parou de falar.
pub fn on_level(f: fn(Option<[f32; BANDS]>)) {
    let _ = SINK.set(f);
}

#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn publish(v: Option<[f32; BANDS]>) {
    if let Some(f) = SINK.get() {
        f(v);
    }
}

#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn has_sink() -> bool {
    SINK.get().is_some()
}

/// Energia de cada faixa na janela que termina em `fim` (Goertzel: barato e sem FFT).
pub fn measure(samples: &[f32], end: usize, rate: u32) -> [f32; BANDS] {
    let end = end.min(samples.len());
    let start = end.saturating_sub(WINDOW);
    let window = &samples[start..end];
    let mut out = [0.0f32; BANDS];
    if window.len() < 64 {
        return out;
    }
    let n = window.len() as f32;
    for (k, f) in CENTERS.iter().enumerate() {
        let w = 2.0 * std::f32::consts::PI * f / rate as f32;
        let c = 2.0 * w.cos();
        let (mut s1, mut s2) = (0.0f32, 0.0f32);
        for (i, x) in window.iter().enumerate() {
            // janela de Hann para não vazar energia entre faixas
            let h = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / (n - 1.0)).cos();
            let s0 = x * h + c * s1 - s2;
            s2 = s1;
            s1 = s0;
        }
        let power = (s1 * s1 + s2 * s2 - c * s1 * s2).max(0.0);
        out[k] = (power.sqrt() / (n * 0.25)).min(4.0);
    }
    out
}
