//! REGRAS.md: áudio só existe no quadro atual; captura e voz começam desligadas.
use crate::{Resampler, FRAME};
use anyhow::{anyhow, Context};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::{
    path::Path,
    sync::mpsc::{sync_channel, Receiver},
    time::Duration,
};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

pub struct Microphone {
    _stream: cpal::Stream,
    frames: Receiver<Vec<u8>>,
}
impl Microphone {
    pub fn open(device_name: &str) -> anyhow::Result<Self> {
        let host = cpal::default_host();
        let device = if device_name.is_empty() {
            host.default_input_device()
        } else {
            host.input_devices()?.find(|d| d.description().is_ok_and(|desc| desc.name() == device_name))
        }
        .ok_or_else(|| anyhow!("microfone não encontrado"))?;
        let supported = device.default_input_config()?;
        let config: cpal::StreamConfig = supported.clone().into();
        let (tx, frames) = sync_channel(4); // teto de 128 ms: nunca acumula áudio ambiente
        let channels = config.channels as usize;
        let mut resample = Resampler::new(config.sample_rate)?;
        let mut frame = Vec::with_capacity(FRAME * 2);
        let mut push = move |sample: f32| {
            if let Some(v) = resample.push(sample) {
                frame.extend_from_slice(&v.to_le_bytes());
                if frame.len() == FRAME * 2 {
                    let next = std::mem::replace(&mut frame, Vec::with_capacity(FRAME * 2));
                    let _ = tx.try_send(next); // consumidor atrasado: descarta, nunca bloqueia callback
                }
            }
        };
        let error = |_: cpal::Error| {};
        let stream = match supported.sample_format() {
            cpal::SampleFormat::F32 => device.build_input_stream(
                config,
                move |data: &[f32], _| {
                    for chunk in data.chunks_exact(channels) {
                        push(chunk.iter().sum::<f32>() / channels as f32);
                    }
                },
                error,
                None,
            )?,
            cpal::SampleFormat::I16 => device.build_input_stream(
                config,
                move |data: &[i16], _| {
                    for chunk in data.chunks_exact(channels) {
                        push(chunk.iter().map(|v| *v as f32 / 32768.0).sum::<f32>() / channels as f32);
                    }
                },
                error,
                None,
            )?,
            cpal::SampleFormat::U16 => device.build_input_stream(
                config,
                move |data: &[u16], _| {
                    for chunk in data.chunks_exact(channels) {
                        push(chunk.iter().map(|v| (*v as f32 - 32768.0) / 32768.0).sum::<f32>() / channels as f32);
                    }
                },
                error,
                None,
            )?,
            _ => return Err(anyhow!("formato do microfone não suportado")),
        };
        stream.play()?;
        Ok(Self { _stream: stream, frames })
    }
    pub fn read_frame(&self) -> Option<Vec<u8>> {
        self.frames.recv_timeout(Duration::from_millis(250)).ok()
    }
    pub fn discard(&self) {
        while self.frames.try_recv().is_ok() {}
    }
}

pub struct OrionWake {
    model: oww_rs::oww::OwwModel,
    buffered: Vec<f32>,
}
impl OrionWake {
    pub fn load(path: &Path, threshold: f32) -> anyhow::Result<Self> {
        Self::load_checked(path, threshold, false)
    }
    /// Só para o comando explícito de calibração; não liga agente, rede nem ferramentas.
    pub fn load_for_calibration(path: &Path) -> anyhow::Result<Self> {
        Self::load_checked(path, 0.5, true)
    }
    fn load_checked(path: &Path, threshold: f32, calibration: bool) -> anyhow::Result<Self> {
        let (_, manifest) = orion_local::wake::load(path.parent().context("pasta do modelo ausente")?, calibration)?;
        anyhow::ensure!(path.file_name().is_some_and(|n| n == "orion.onnx"), "use o bundle orion.onnx/orion.json");
        anyhow::ensure!(path.is_file(), "configure o modelo ONNX treinado para Orion");
        // A biblioteca usa assert internamente ao validar ONNX. Um modelo inválido desliga a escuta.
        anyhow::ensure!(calibration || (manifest.threshold - threshold).abs() < 0.0001, "limiar configurado difere do limiar avaliado no bundle Orion");
        let calibrated_threshold = manifest.threshold;
        let model = std::panic::catch_unwind(|| oww_rs::oww::OwwModel::from_file(path, "Orion".into(), calibrated_threshold))
            .map_err(|_| anyhow!("modelo openWakeWord inválido"))??;
        Ok(Self { model, buffered: Vec::new() })
    }
    pub fn reset(&mut self) {
        self.buffered.clear();
        self.model.detections_buffer.clear();
    }
    pub fn feed(&mut self, pcm: &[u8]) -> bool {
        self.buffered.extend(pcm.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0));
        let mut found = false;
        while self.buffered.len() >= 1280 {
            let chunk: Vec<_> = self.buffered.drain(..1280).collect();
            found |= self.model.detection(chunk).detected;
        }
        found
    }
}

pub struct LocalWhisper {
    context: WhisperContext,
}
impl LocalWhisper {
    pub fn load(path: &Path, gpu: bool) -> anyhow::Result<Self> {
        anyhow::ensure!(!gpu || cfg!(feature = "vulkan"), "esta compilação não inclui Whisper Vulkan; desative GPU ou instale o pacote Vulkan");
        let mut params = WhisperContextParameters::default();
        params.use_gpu(gpu);
        let context = WhisperContext::new_with_params(path.to_str().context("caminho do modelo inválido")?, params)
            .map_err(|_| anyhow!("não foi possível carregar o modelo Whisper"))?;
        Ok(Self { context })
    }
    pub fn transcribe(&self, audio: Vec<u8>) -> anyhow::Result<String> {
        let mut reader = hound::WavReader::new(std::io::Cursor::new(audio)).context("Whisper local exige WAV PCM; converta o áudio comprimido antes")?;
        let spec = reader.spec();
        anyhow::ensure!(spec.sample_rate == 16000 && spec.channels == 1 && spec.bits_per_sample == 16, "Whisper local exige PCM mono 16 kHz/16 bits");
        let samples = reader.samples::<i16>().map(|v| v.map(|v| v as f32 / 32768.0)).collect::<Result<Vec<_>, _>>()?;
        let mut state = self.context.create_state()?;
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some("pt"));
        params.set_translate(false);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        state.full(params, &samples)?;
        Ok(state.as_iter().map(|s| s.to_string()).collect::<Vec<_>>().join("").trim().to_string())
    }
}
