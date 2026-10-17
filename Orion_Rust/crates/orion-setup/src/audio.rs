//! Som ambiente em tempo real do `Orion Setup.exe` (`ORION_INSTALADOR_ESPECIFICACAO.md`, §9).
//!
//! - Começa ao abrir o instalador (sem botão).
//! - Volume sobe de 0 a `0.028` (ganho linear) em 4 s; ao fechar faz fade-out em 0.5 s.
//! - Síntese em tempo real: 4 osciladores (2 senoidais + 2 triangulares com desafinação em cents
//!   e LFO de ganho) → filtro passa-baixas em 900 Hz → saída.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::f32::consts::TAU;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub const TARGET_GAIN: f32 = 0.028;
pub const FADE_IN_S: f32 = 4.0;
pub const FADE_OUT_S: f32 = 0.5;
pub const LOWPASS_HZ: f32 = 900.0;

#[derive(Clone, Copy)]
enum Wave {
    Sine,
    Triangle,
}

#[derive(Clone, Copy)]
struct VoiceSpec {
    base_hz: f32,
    detune_cents: f32,
    wave: Wave,
    base_gain: f32,
    lfo_hz: f32,
    lfo_depth: f32,
}

const VOICES: [VoiceSpec; 4] = [
    VoiceSpec { base_hz: 110.0, detune_cents: -6.0, wave: Wave::Sine, base_gain: 0.8, lfo_hz: 0.04, lfo_depth: 0.35 },
    VoiceSpec { base_hz: 164.8, detune_cents: -2.0, wave: Wave::Triangle, base_gain: 0.8, lfo_hz: 0.065, lfo_depth: 0.35 },
    VoiceSpec { base_hz: 220.0, detune_cents: 2.0, wave: Wave::Sine, base_gain: 0.8, lfo_hz: 0.09, lfo_depth: 0.35 },
    VoiceSpec { base_hz: 329.6, detune_cents: 6.0, wave: Wave::Triangle, base_gain: 0.25, lfo_hz: 0.115, lfo_depth: 0.35 },
];

/// Sintetizador puro determinístico (testável sem hardware de áudio).
pub struct AmbientSynth {
    sample_rate: f32,
    phases: [f32; 4],
    lfo_phases: [f32; 4],
    freqs: [f32; 4],
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
        let freqs = VOICES.map(|v| v.base_hz * 2.0_f32.powf(v.detune_cents / 1200.0));
        let rc = 1.0 / (TAU * LOWPASS_HZ);
        let dt = 1.0 / sr;
        let lp_alpha = dt / (rc + dt);
        Self {
            sample_rate: sr,
            phases: [0.0; 4],
            lfo_phases: [0.0; 4],
            freqs,
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
        let mut mix = 0.0_f32;
        for i in 0..4 {
            let spec = VOICES[i];
            let p = self.phases[i];
            let osc = match spec.wave {
                Wave::Sine => (p * TAU).sin(),
                Wave::Triangle => 1.0 - 4.0 * ((p + 0.25).fract() - 0.5).abs(),
            };
            let lfo = (self.lfo_phases[i] * TAU).sin();
            let voice_gain = spec.base_gain * (1.0 + spec.lfo_depth * lfo);
            mix += osc * voice_gain;

            self.phases[i] = (p + self.freqs[i] * dt).fract();
            self.lfo_phases[i] = (self.lfo_phases[i] + spec.lfo_hz * dt).fract();
        }
        mix *= 0.25;
        // Filtro passa-baixas em 900 Hz
        self.lp_state += self.lp_alpha * (mix - self.lp_state);
        self.lp_state * env
    }
}

pub struct AmbientPlayer {
    closing: Arc<AtomicBool>,
    _stream: Option<cpal::Stream>,
}

impl AmbientPlayer {
    /// Inicia o som ambiente imediatamente ao abrir o instalador.
    pub fn start() -> Self {
        let closing = Arc::new(AtomicBool::new(false));
        let c2 = closing.clone();
        let stream = Self::try_open_stream(c2);
        Self { closing, _stream: stream }
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

/// Controlador de som ambiente com suporte a ativar/silenciar na barra de título.
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
