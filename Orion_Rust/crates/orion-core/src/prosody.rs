//! Tom da voz por prosódia (energia e altura), só local e só na fala que o Orion já ouviu.
//!
//! Heurística leve, sem modelo: não diz "emoção", diz se a fala veio calma, animada ou tensa em
//! relação à própria linha de base de Antônio. O resultado só descreve o turno; não decide nada.
//! (REGRAS.md: voz e foto não ganham poder novo; a análise é local e não sai do computador.)

/// Taxa de amostragem do PCM da captura nativa.
pub const RATE: usize = 16_000;
const FRAME: usize = 480; // 30 ms
const MIN_PITCH_HZ: usize = 70;
const MAX_PITCH_HZ: usize = 400;
/// Quadros mais baixos que isto contam como pausa.
const VOICED_RMS: f64 = 500.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Prosody {
    /// RMS médio dos quadros com fala.
    pub energy: f64,
    /// Altura média da voz, em Hz; 0 quando não deu para medir.
    pub pitch_hz: f64,
    /// Variação da altura (desvio relativo): fala monótona dá valor baixo.
    pub pitch_var: f64,
    /// Fração dos quadros com fala.
    pub voiced: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mood {
    Calm,
    Excited,
    Tense,
    Low,
}

impl Mood {
    pub fn label(self) -> &'static str {
        match self {
            Mood::Calm => "calm",
            Mood::Excited => "excited",
            Mood::Tense => "tense",
            Mood::Low => "low",
        }
    }
}

fn samples(pcm: &[u8]) -> Vec<f32> {
    pcm.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32).collect()
}

fn frame_rms(f: &[f32]) -> f64 {
    (f.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / f.len().max(1) as f64).sqrt()
}

/// Altura por autocorrelação; `None` se o quadro não é periódico o bastante.
fn pitch(f: &[f32]) -> Option<f64> {
    let (lo, hi) = (RATE / MAX_PITCH_HZ, RATE / MIN_PITCH_HZ);
    let e0: f64 = f.iter().map(|v| (*v as f64).powi(2)).sum();
    if e0 <= 0.0 || hi >= f.len() {
        return None;
    }
    let (mut best_lag, mut best) = (0, 0.0);
    for lag in lo..=hi {
        let r: f64 = f.iter().zip(&f[lag..]).map(|(a, b)| *a as f64 * *b as f64).sum();
        if r > best {
            best = r;
            best_lag = lag;
        }
    }
    (best / e0 > 0.3 && best_lag > 0).then(|| RATE as f64 / best_lag as f64)
}

/// Mede o PCM mono de 16 kHz (s16le). `None` se quase não houve fala.
pub fn measure(pcm: &[u8]) -> Option<Prosody> {
    let s = samples(pcm);
    let (mut energies, mut pitches, mut frames) = (vec![], vec![], 0usize);
    for f in s.chunks_exact(FRAME) {
        frames += 1;
        let e = frame_rms(f);
        if e >= VOICED_RMS {
            energies.push(e);
            if let Some(p) = pitch(f) {
                pitches.push(p);
            }
        }
    }
    if energies.len() < 5 {
        return None;
    }
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    let pitch_hz = if pitches.is_empty() { 0.0 } else { mean(&pitches) };
    let pitch_var = if pitches.len() < 3 || pitch_hz == 0.0 {
        0.0
    } else {
        (pitches.iter().map(|p| (p - pitch_hz).powi(2)).sum::<f64>() / pitches.len() as f64).sqrt() / pitch_hz
    };
    Some(Prosody { energy: mean(&energies), pitch_hz, pitch_var, voiced: energies.len() as f64 / frames as f64 })
}

/// Compara o turno com a linha de base de Antônio (média dos turnos anteriores da sessão).
/// Sem base, só `Calm`: não há como saber o que é fora do normal.
pub fn classify(now: &Prosody, base: Option<&Prosody>) -> Mood {
    let Some(b) = base else { return Mood::Calm };
    let loud = now.energy / b.energy.max(1.0);
    let high = if b.pitch_hz > 0.0 && now.pitch_hz > 0.0 { now.pitch_hz / b.pitch_hz } else { 1.0 };
    if loud > 1.4 && high > 1.1 {
        Mood::Tense
    } else if loud > 1.2 || (high > 1.15 && now.pitch_var > b.pitch_var * 1.3) {
        Mood::Excited
    } else if loud < 0.7 && now.pitch_var < b.pitch_var * 0.8 {
        Mood::Low
    } else {
        Mood::Calm
    }
}
