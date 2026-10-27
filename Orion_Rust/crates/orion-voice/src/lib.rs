//! Captura e inferência locais (REGRAS.md: palavra de ativação só abre o microfone; detecção local).
use std::collections::VecDeque;
use std::time::{Duration, Instant};

pub const RATE: u32 = 16_000;
pub const FRAME: usize = 512;

/// Reamostra o fluxo mono preservando a fase entre callbacks de tamanhos diferentes.
pub struct Resampler {
    rate: u32,
    phase: u64,
    sum: f64,
    count: u32,
}
impl Resampler {
    pub fn new(rate: u32) -> anyhow::Result<Self> {
        anyhow::ensure!(rate >= RATE, "microfone precisa oferecer ao menos 16 kHz");
        Ok(Self { rate, phase: 0, sum: 0.0, count: 0 })
    }
    pub fn push(&mut self, sample: f32) -> Option<i16> {
        self.sum += sample.clamp(-1.0, 1.0) as f64;
        self.count += 1;
        self.phase += RATE as u64;
        if self.phase < self.rate as u64 {
            return None;
        }
        self.phase -= self.rate as u64;
        let out = (self.sum / self.count as f64 * i16::MAX as f64) as i16;
        self.sum = 0.0;
        self.count = 0;
        Some(out)
    }
}

/// VAD por energia calibrado com o ruído ambiente, alternativa sem modelo Silero.
pub struct EnergyVad {
    pub threshold: f64,
}
impl EnergyVad {
    pub fn calibrated(noise: &[i16]) -> Self {
        Self { threshold: (energy(noise) * 3.0).max(350.0) }
    }
    pub fn speech(&self, samples: &[i16]) -> bool {
        energy(samples) >= self.threshold
    }
}
pub fn energy(samples: &[i16]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / samples.len() as f64).sqrt()
}

/// Teto local com relógio monotônico; pausa/pânico são conferidos antes de chamar.
pub struct ActivationLimit {
    events: VecDeque<Instant>,
    maximum: usize,
}
impl ActivationLimit {
    pub fn new(maximum: usize) -> Self {
        Self { events: VecDeque::new(), maximum }
    }
    pub fn allow(&mut self, now: Instant) -> bool {
        while self.events.front().is_some_and(|t| now.duration_since(*t) >= Duration::from_secs(3600)) {
            self.events.pop_front();
        }
        if self.events.len() >= self.maximum {
            return false;
        }
        self.events.push_back(now);
        true
    }
}

#[cfg(windows)]
mod native;
#[cfg(windows)]
pub use native::*;
#[cfg(windows)]
mod duplex;
#[cfg(windows)]
pub use duplex::{play_audio,play_interruptible};

/// Interrupção exige fala sustentada após o AEC, nunca um pico isolado ou palmas.
pub struct BargeInGate {
    consecutive: usize,
    threshold: f64,
}
impl BargeInGate {
    pub fn new(threshold: f64) -> Self { Self { consecutive: 0, threshold: threshold.max(600.0) } }
    pub fn feed(&mut self, pcm: &[i16]) -> bool {
        self.consecutive = if energy(pcm) >= self.threshold { self.consecutive + 1 } else { 0 };
        self.consecutive >= 6 // 192 ms de fala em quadros de 32 ms.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reamostra_44100_sem_deriva() {
        let mut r = Resampler::new(44100).unwrap();
        let out: Vec<_> = (0..44100).filter_map(|_| r.push(0.25)).collect();
        assert_eq!(out.len(), 16000);
        assert!(out.iter().all(|v| (*v - 8191).abs() <= 1));
    }
    #[test]
    fn vad_e_teto_por_hora() {
        let v = EnergyVad::calibrated(&[100; 512]);
        assert!(!v.speech(&[100; 512]));
        assert!(v.speech(&[1000; 512]));
        let mut limit = ActivationLimit::new(1);
        let now = Instant::now();
        assert!(limit.allow(now));
        assert!(!limit.allow(now));
        assert!(limit.allow(now + Duration::from_secs(3600)));
    }
    #[test]
    fn interrupcao_exige_fala_continua_depois_do_eco() {
        let mut gate = BargeInGate::new(900.0);
        assert!(!gate.feed(&[5000; FRAME])); // uma palma não interrompe
        assert!(!gate.feed(&[50; FRAME]));
        for _ in 0..5 { assert!(!gate.feed(&[2000; FRAME])); }
        assert!(gate.feed(&[2000; FRAME]));
    }
}
