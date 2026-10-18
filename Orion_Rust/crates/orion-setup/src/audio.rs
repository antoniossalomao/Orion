//! Trilha instrumental ambiente espacial do `Orion Setup.exe`.
//!
//! Em vez de um zumbido estático de osciladores contínuos, sintetiza em tempo real
//! uma peça instrumental calma, espacial e contemplativa (estilo planetário / Brian Eno / Stellardrone):
//! - Progressão harmônica em 4 acordes lentos (Am9 → Fmaj7(#11) → Cmaj9 → Em7), trocando a cada 7.2 s
//! - Pad etéreo suave com ataque e queda lentos (envelope senoidal por compasso)
//! - Dedilhado cristalino de piano elétrico / celesta espacial (com harmônicos suaves de 2ª e 3ª ordem
//!   e decaimento exponencial natural)
//! - Reverberação espacial estéreo (rede de 4 delays Schroeder + filtro passa-baixas quente em 1.8 kHz)
//! - Fade-in de 4 s ao abrir e fade-out de 0.5 s ao fechar.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::f32::consts::TAU;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub const TARGET_GAIN: f32 = 0.028;
pub const FADE_IN_S: f32 = 4.0;
pub const FADE_OUT_S: f32 = 0.5;
pub const LOWPASS_HZ: f32 = 1800.0;

/// Duração de cada compasso/acorde lento (7.2 segundos = 8 notas de arpejo de 0.9 s).
const CHORD_DURATION_S: f32 = 7.2;
const NOTE_STEP_S: f32 = 0.9;

/// 4 acordes espaciais (frequências em Hz das 4 vozes do pad):
/// 0: Am9        (A2, E3, G3, B3)
/// 1: Fmaj7(#11) (F2, C3, E3, B3)
/// 2: Cmaj9      (C3, G3, B3, D4)
/// 3: Em7        (E2, B2, D3, G3)
const PAD_CHORDS: [[f32; 4]; 4] = [
    [110.00, 164.81, 196.00, 246.94],
    [87.31, 130.81, 164.81, 246.94],
    [130.81, 196.00, 246.94, 293.66],
    [82.41, 123.47, 146.83, 196.00],
];

/// Melodia/arpejo lento de celesta/Rhodes (8 tempos por acorde; `0.0` = pausa respirando).
const MELODY_NOTES: [[f32; 8]; 4] = [
    // Am9: E4, A4, B4, C5, —, G4, E4, —
    [329.63, 440.00, 493.88, 523.25, 0.0, 392.00, 329.63, 0.0],
    // Fmaj7(#11): C4, E4, G4, B4, —, A4, E4, —
    [261.63, 329.63, 392.00, 493.88, 0.0, 440.00, 329.63, 0.0],
    // Cmaj9: G4, B4, D5, E5, —, B4, G4, —
    [392.00, 493.88, 587.33, 659.25, 0.0, 493.88, 392.00, 0.0],
    // Em7: B3, E4, G4, B4, —, D5, B4, —
    [246.94, 329.63, 392.00, 493.88, 0.0, 587.33, 493.88, 0.0],
];

/// Voz polifônica para as notas dedilhadas (permite que as notas anteriores continuem soando em cauda).
#[derive(Clone, Copy)]
struct PluckVoice {
    freq: f32,
    phase: f32,
    age_s: f32,
    active: bool,
}

impl PluckVoice {
    const fn inactive() -> Self {
        Self {
            freq: 0.0,
            phase: 0.0,
            age_s: 99.0,
            active: false,
        }
    }
}

/// Sintetizador instrumental espacial determinístico.
pub struct AmbientSynth {
    sample_rate: f32,
    pad_phases: [f32; 4],
    shimmer_phases: [f32; 4],
    plucks: [PluckVoice; 4],
    next_pluck_slot: usize,
    last_step_idx: usize,
    reverb_buf_a: Vec<f32>,
    reverb_buf_b: Vec<f32>,
    reverb_buf_c: Vec<f32>,
    rev_idx_a: usize,
    rev_idx_b: usize,
    rev_idx_c: usize,
    lp_state: f32,
    lp_alpha: f32,
    elapsed_s: f32,
    closing: bool,
    close_elapsed_s: f32,
    gain_at_close: f32,
}

impl AmbientSynth {
    pub fn new(sample_rate: u32) -> Self {
        let sr = (sample_rate.max(8_000)) as f32;
        let rc = 1.0 / (TAU * LOWPASS_HZ);
        let dt = 1.0 / sr;
        let lp_alpha = dt / (rc + dt);

        // Três linhas de delay em números primos para criar o efeito de reverberação espacial (cosmos)
        let len_a = ((sr * 0.173) as usize).max(64);
        let len_b = ((sr * 0.281) as usize).max(64);
        let len_c = ((sr * 0.419) as usize).max(64);

        let mut plucks = [PluckVoice::inactive(); 4];
        // Já dispara a primeira nota suave ao iniciar
        plucks[0] = PluckVoice {
            freq: MELODY_NOTES[0][0],
            phase: 0.0,
            age_s: 0.0,
            active: true,
        };

        Self {
            sample_rate: sr,
            pad_phases: [0.0; 4],
            shimmer_phases: [0.13, 0.37, 0.61, 0.83],
            plucks,
            next_pluck_slot: 1,
            last_step_idx: 0,
            reverb_buf_a: vec![0.0; len_a],
            reverb_buf_b: vec![0.0; len_b],
            reverb_buf_c: vec![0.0; len_c],
            rev_idx_a: 0,
            rev_idx_b: 0,
            rev_idx_c: 0,
            lp_state: 0.0,
            lp_alpha,
            elapsed_s: 0.0,
            closing: false,
            close_elapsed_s: 0.0,
            gain_at_close: 0.0,
        }
    }

    pub fn current_envelope(&self) -> f32 {
        let fade_in = (self.elapsed_s / FADE_IN_S).clamp(0.0, 1.0) * TARGET_GAIN;
        if !self.closing {
            fade_in
        } else {
            let rem = (1.0 - self.close_elapsed_s / FADE_OUT_S).clamp(0.0, 1.0);
            self.gain_at_close * rem
        }
    }

    pub fn start_fade_out(&mut self) {
        if !self.closing {
            self.gain_at_close = self.current_envelope();
            self.closing = true;
            self.close_elapsed_s = 0.0;
        }
    }

    pub fn next_sample(&mut self) -> f32 {
        let dt = 1.0 / self.sample_rate;
        self.elapsed_s += dt;
        if self.closing {
            self.close_elapsed_s += dt;
        }
        let env = self.current_envelope();

        // Posição na progressão de 4 acordes
        let cycle_s = self.elapsed_s % (CHORD_DURATION_S * 4.0);
        let chord_idx = ((cycle_s / CHORD_DURATION_S) as usize).min(3);
        let chord_pos = (cycle_s % CHORD_DURATION_S) / CHORD_DURATION_S; // 0.0..1.0

        // Dispara nota do arpejo a cada NOTE_STEP_S (0.9 s)
        let global_step = (self.elapsed_s / NOTE_STEP_S) as usize;
        if global_step != self.last_step_idx {
            self.last_step_idx = global_step;
            let step_in_chord = global_step % 8;
            let note_hz = MELODY_NOTES[chord_idx][step_in_chord];
            if note_hz > 1.0 {
                self.plucks[self.next_pluck_slot] = PluckVoice {
                    freq: note_hz,
                    phase: 0.0,
                    age_s: 0.0,
                    active: true,
                };
                self.next_pluck_slot = (self.next_pluck_slot + 1) % self.plucks.len();
            }
        }

        // 1. Pad quente com envelope de respiração suave ao longo de cada acorde (evita cortes na troca)
        let pad_swell = (chord_pos * std::f32::consts::PI).sin().powf(0.65);
        let chord_freqs = PAD_CHORDS[chord_idx];
        let mut pad_mix = 0.0_f32;
        for i in 0..4 {
            let f = chord_freqs[i];
            let p1 = self.pad_phases[i];
            let p2 = self.shimmer_phases[i];
            // Fundamental + quinta/oitava muito sutil com leve detune (chorus espacial)
            let w1 = (p1 * TAU).sin();
            let w2 = (p2 * TAU).sin() * 0.35;
            pad_mix += (w1 + w2) * 0.22;

            self.pad_phases[i] = (p1 + f * dt).fract();
            self.shimmer_phases[i] = (p2 + (f * 1.0018) * dt).fract();
        }
        pad_mix *= pad_swell;

        // 2. Notas dedilhadas (timbre de Rhodes / Celesta suave: fundamental + 2º harmônico + 3º harmônico)
        let mut pluck_mix = 0.0_f32;
        for voice in &mut self.plucks {
            if !voice.active {
                continue;
            }
            voice.age_s += dt;
            if voice.age_s > 4.2 {
                voice.active = false;
                continue;
            }
            // Ataque macio de 18 ms (sem clique) + queda exponencial longa
            let attack = (voice.age_s / 0.018).clamp(0.0, 1.0);
            let decay_body = (-voice.age_s * 1.35).exp();
            let decay_bell = (-voice.age_s * 4.5).exp();
            let p = voice.phase * TAU;
            let sig = p.sin() * decay_body
                + (p * 2.0).sin() * 0.22 * decay_bell
                + (p * 3.0).sin() * 0.07 * decay_bell;
            pluck_mix += sig * attack * 0.55;
            voice.phase = (voice.phase + voice.freq * dt).fract();
        }

        let dry = pad_mix * 0.55 + pluck_mix * 0.75;

        // 3. Reverberação espacial (3 linhas de eco difuso com realimentação suave)
        let ra = self.reverb_buf_a[self.rev_idx_a];
        let rb = self.reverb_buf_b[self.rev_idx_b];
        let rc = self.reverb_buf_c[self.rev_idx_c];
        let wet = (ra + rb + rc) * 0.33;

        self.reverb_buf_a[self.rev_idx_a] = dry + wet * 0.46;
        self.reverb_buf_b[self.rev_idx_b] = dry + ra * 0.42;
        self.reverb_buf_c[self.rev_idx_c] = (ra + rb) * 0.35;

        self.rev_idx_a += 1;
        if self.rev_idx_a >= self.reverb_buf_a.len() {
            self.rev_idx_a = 0;
        }
        self.rev_idx_b += 1;
        if self.rev_idx_b >= self.reverb_buf_b.len() {
            self.rev_idx_b = 0;
        }
        self.rev_idx_c += 1;
        if self.rev_idx_c >= self.reverb_buf_c.len() {
            self.rev_idx_c = 0;
        }

        let combined = dry * 0.65 + wet * 0.55;
        // 4. Filtro passa-baixas quente
        self.lp_state += self.lp_alpha * (combined - self.lp_state);
        // Ganho musical confortável (env de 0.028 escalado suavemente para audibilidade agradável)
        (self.lp_state * (env * 2.2)).clamp(-0.25, 0.25)
    }
}

pub struct AmbientPlayer {
    closing: Arc<AtomicBool>,
    _stream: Option<cpal::Stream>,
}

impl AmbientPlayer {
    /// Inicia a música ambiente imediatamente ao abrir o instalador.
    pub fn start() -> Self {
        let closing = Arc::new(AtomicBool::new(false));
        let c2 = closing.clone();
        let stream = Self::try_open_stream(c2);
        Self {
            closing,
            _stream: stream,
        }
    }

    pub fn fade_out(&self) {
        self.closing.store(true, Ordering::SeqCst);
    }

    fn try_open_stream(closing: Arc<AtomicBool>) -> Option<cpal::Stream> {
        let host = cpal::default_host();
        let device = host.default_output_device()?;
        let config = device.default_output_config().ok()?;
        let channels = config.channels() as usize;
        let sr = config.sample_rate();
        let sample_format = config.sample_format();
        let stream_cfg: cpal::StreamConfig = config.into();
        let mut synth = AmbientSynth::new(sr);

        let err_fn = |err| {
            tracing::debug!("áudio ambiente: {err}");
        };

        let stream = match sample_format {
            cpal::SampleFormat::F32 => device
                .build_output_stream(
                    stream_cfg,
                    move |data: &mut [f32], _| {
                        if closing.load(Ordering::Relaxed) {
                            synth.start_fade_out();
                        }
                        for frame in data.chunks_mut(channels.max(1)) {
                            let s = synth.next_sample();
                            for ch in frame.iter_mut() {
                                *ch = s;
                            }
                        }
                    },
                    err_fn,
                    None,
                )
                .ok()?,
            cpal::SampleFormat::I16 => device
                .build_output_stream(
                    stream_cfg,
                    move |data: &mut [i16], _| {
                        if closing.load(Ordering::Relaxed) {
                            synth.start_fade_out();
                        }
                        for frame in data.chunks_mut(channels.max(1)) {
                            let s = (synth.next_sample() * i16::MAX as f32) as i16;
                            for ch in frame.iter_mut() {
                                *ch = s;
                            }
                        }
                    },
                    err_fn,
                    None,
                )
                .ok()?,
            _ => return None,
        };
        let _ = stream.play();
        Some(stream)
    }
}

/// Controlador de música ambiente com suporte a ativar/silenciar na barra de título.
pub struct SoundController {
    enabled: bool,
    player: Option<AmbientPlayer>,
}

impl Default for SoundController {
    fn default() -> Self {
        Self::new()
    }
}

impl SoundController {
    pub fn new() -> Self {
        Self {
            enabled: false,
            player: None,
        }
    }

    pub fn start_enabled() -> Self {
        Self {
            enabled: true,
            player: Some(AmbientPlayer::start()),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn toggle(&mut self) {
        if self.enabled {
            if let Some(p) = &self.player {
                p.fade_out();
            }
            self.player = None;
            self.enabled = false;
        } else {
            self.player = Some(AmbientPlayer::start());
            self.enabled = true;
        }
    }

    pub fn fade_out(&self) {
        if let Some(p) = &self.player {
            p.fade_out();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn som_sobe_ate_0_028_em_4s_e_faz_fade_out_em_meio_segundo() {
        let sr = 48_000;
        let mut s = AmbientSynth::new(sr);
        assert_eq!(s.current_envelope(), 0.0);
        // Avança 2 segundos -> metade do ganho alvo
        for _ in 0..(sr * 2) {
            let _ = s.next_sample();
        }
        assert!((s.current_envelope() - TARGET_GAIN * 0.5).abs() < 0.001);
        // Avança mais 2.5 segundos -> estabiliza em 0.028
        for _ in 0..(sr * 5 / 2) {
            let _ = s.next_sample();
        }
        assert!((s.current_envelope() - TARGET_GAIN).abs() < 1e-5);
        // Inicia fade-out de 0.5s
        s.start_fade_out();
        for _ in 0..(sr / 2 + 100) {
            let _ = s.next_sample();
        }
        assert!(s.current_envelope() <= 1e-5);
    }
}
