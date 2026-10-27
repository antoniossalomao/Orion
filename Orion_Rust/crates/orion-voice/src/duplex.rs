//! REGRAS.md: áudio só existe no quadro atual; voz nunca aprova. Referência é só a fala do Orion.
use crate::{BargeInGate, Microphone, Resampler, FRAME, RATE};
use aec3::{nodes::audio::AudioFormat, pipelines::linear};
use anyhow::{anyhow, ensure};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use symphonia::core::{
    audio::SampleBuffer,
    codecs::DecoderOptions,
    formats::FormatOptions,
    io::{MediaSourceStream, MediaSourceStreamOptions},
    meta::MetadataOptions,
    probe::Hint,
};

const TEN_MS: usize = 160;
const REFERENCE_LIMIT: usize = RATE as usize / 4;

fn decode_mp3(bytes: &[u8]) -> anyhow::Result<Vec<f32>> {
    ensure!(bytes.len() <= 10 * 1024 * 1024, "TTS grande demais");
    let source = MediaSourceStream::new(Box::new(std::io::Cursor::new(bytes.to_vec())), MediaSourceStreamOptions::default());
    let mut hint = Hint::new();
    hint.with_extension(if bytes.starts_with(b"RIFF"){"wav"}else{"mp3"});
    let mut format = symphonia::default::get_probe()
        .format(&hint, source, &FormatOptions::default(), &MetadataOptions::default())?
        .format;
    let track = format.default_track().ok_or_else(|| anyhow!("TTS sem faixa de áudio"))?;
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs().make(&track.codec_params, &DecoderOptions::default())?;
    let mut output = Vec::new();
    let mut resampler = None;
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.into()),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = decoder.decode(&packet)?;
        let spec = *decoded.spec();
        let resample = resampler.get_or_insert(Resampler::new(spec.rate)?);
        let mut buffer = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
        buffer.copy_interleaved_ref(decoded);
        for channels in buffer.samples().chunks_exact(spec.channels.count()) {
            if let Some(sample) = resample.push(channels.iter().sum::<f32>() / channels.len() as f32) {
                output.push(sample as f32 / 32768.0);
            }
        }
        ensure!(output.len() <= RATE as usize * 60, "TTS excedeu 60 segundos");
    }
    ensure!(!output.is_empty(), "TTS vazio");
    Ok(output)
}

struct Playback {
    stream: cpal::Stream,
    done: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
    failed: Arc<AtomicBool>,
}

fn output<T: cpal::SizedSample + cpal::FromSample<f32>>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    samples: Vec<f32>,
    reference: Arc<Mutex<VecDeque<f32>>>,
    done: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
    failed: Arc<AtomicBool>,
) -> anyhow::Result<cpal::Stream> {
    let rate = config.sample_rate;
    let channels = config.channels as usize;
    let mut position = 0.0f64;
    let mut resampler = Resampler::new(rate)?;
    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], _| {
            // Nunca esperar lock no callback de áudio; limite de 250 ms para referência em RAM.
            let mut queue = reference.try_lock();
            for frame in data.chunks_exact_mut(channels) {
                let sample = if stopped.load(Ordering::Relaxed) || position as usize >= samples.len() {
                    done.store(true, Ordering::SeqCst);
                    0.0
                } else {
                    let i = position as usize;
                    let fraction = (position - i as f64) as f32;
                    let a = samples[i];
                    let b = samples.get(i + 1).copied().unwrap_or(a);
                    position += RATE as f64 / rate as f64;
                    a + (b - a) * fraction
                };
                frame.fill(T::from_sample(sample));
                if let Some(value) = resampler.push(sample) {
                    if let Some(queue) = &mut queue {
                        if queue.len() >= REFERENCE_LIMIT {
                            queue.pop_front();
                        }
                        queue.push_back(value as f32 / 32768.0);
                    }
                }
            }
        },
        move |_: cpal::Error| {
            failed.store(true, Ordering::SeqCst);
        },
        None,
    )?;
    Ok(stream)
}

impl Playback {
    fn start(samples: Vec<f32>, reference: Arc<Mutex<VecDeque<f32>>>) -> anyhow::Result<Self> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or_else(|| anyhow!("saída de áudio ausente"))?;
        let supported = device.default_output_config()?;
        let config: cpal::StreamConfig = supported.clone().into();
        let done = Arc::new(AtomicBool::new(false));
        let stopped = Arc::new(AtomicBool::new(false));
        let failed = Arc::new(AtomicBool::new(false));
        let stream = match supported.sample_format() {
            cpal::SampleFormat::F32 => output::<f32>(&device, config, samples, reference, done.clone(), stopped.clone(), failed.clone()),
            cpal::SampleFormat::I16 => output::<i16>(&device, config, samples, reference, done.clone(), stopped.clone(), failed.clone()),
            cpal::SampleFormat::U16 => output::<u16>(&device, config, samples, reference, done.clone(), stopped.clone(), failed.clone()),
            _ => Err(anyhow!("formato de saída não suportado")),
        }?;
        stream.play()?;
        Ok(Self {
            stream,
            done,
            stopped,
            failed,
        })
    }
    fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        let _ = self.stream.pause();
    }
}

/// Retorna PCM mono 16 kHz somente se o usuário interrompeu. Pausa/pânico descartam tudo.
pub fn play_interruptible(mp3: &[u8], device: &str, delay_ms: i32, noise_threshold: f64, abort: &(dyn Fn() -> bool + Sync)) -> anyhow::Result<Option<Vec<u8>>> {
    if abort() {
        return Ok(None);
    }
    let samples = decode_mp3(mp3)?;
    if abort() {
        return Ok(None);
    }
    let mic = Microphone::open(device)?;
    let reference = Arc::new(Mutex::new(VecDeque::with_capacity(REFERENCE_LIMIT)));
    let format = AudioFormat::ten_ms(RATE, 1);
    let mut aec = linear::builder(format, format)
        .initial_delay_ms(delay_ms)
        .enable_gain_controller2(false)
        .enable_post_filter(false)
        .build()
        .map_err(|e| anyhow!("AEC3: {e}"))?;
    let playback = Playback::start(samples, reference.clone())?;
    let start = Instant::now();
    let mut gate = BargeInGate::new(noise_threshold.max(900.0));
    let mut raw = VecDeque::new();
    let mut cleaned = VecDeque::new();
    let mut pre_roll: VecDeque<Vec<u8>> = VecDeque::new();
    let mut recording: Option<(Instant, Vec<u8>, usize)> = None;
    while start.elapsed() < Duration::from_secs(75) {
        if abort() {
            playback.stop();
            return Ok(None);
        }
        ensure!(!playback.failed.load(Ordering::SeqCst), "saída de áudio foi desconectada");
        let Some(frame) = mic.read_frame() else {
            return Err(anyhow!("microfone desconectado durante a fala"));
        };
        raw.extend(frame.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0));
        while raw.len() >= TEN_MS {
            let capture: Vec<f32> = raw.drain(..TEN_MS).collect();
            let render: Vec<f32> = {
                let mut queue = reference.lock();
                (0..TEN_MS).map(|_| queue.pop_front().unwrap_or(0.0)).collect()
            };
            let mut output = [0.0; TEN_MS];
            aec.handle_render_frame(&render).map_err(|e| anyhow!("AEC3: {e}"))?;
            if aec.process_capture_frame(&capture, &mut output).map_err(|e| anyhow!("AEC3: {e}"))? {
                cleaned.extend(output.into_iter().map(|v| (v.clamp(-1.0, 1.0) * 32767.0) as i16));
            }
        }
        while cleaned.len() >= FRAME {
            let values: Vec<i16> = cleaned.drain(..FRAME).collect();
            let pcm: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
            if let Some((began, bytes, silence)) = &mut recording {
                bytes.extend_from_slice(&pcm);
                *silence = if crate::energy(&values) < 600.0 { *silence + 1 } else { 0 };
                if *silence >= 32 || began.elapsed() >= Duration::from_secs(15) {
                    return Ok(Some(std::mem::take(bytes)));
                }
            } else {
                if pre_roll.len() >= 10 {
                    pre_roll.pop_front();
                }
                pre_roll.push_back(pcm);
                if start.elapsed() >= Duration::from_millis(300) && gate.feed(&values) {
                    playback.stop();
                    reference.lock().clear();
                    recording = Some((Instant::now(), pre_roll.drain(..).flatten().collect(), 0));
                }
            }
        }
        if recording.is_none() && playback.done.load(Ordering::SeqCst) {
            return Ok(None);
        }
    }
    playback.stop();
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aec3_aceita_quadros_de_dez_ms_sem_audio_externo() {
        let format = AudioFormat::ten_ms(RATE, 1);
        let mut aec = linear::builder(format, format).build().unwrap();
        for _ in 0..50 {
            aec.handle_render_frame(&[0.0; TEN_MS]).unwrap();
            let mut output = [0.0; TEN_MS];
            if aec.process_capture_frame(&[0.0; TEN_MS], &mut output).unwrap() {
                assert!(output.iter().all(|v| v.is_finite() && v.abs() < 0.001));
            }
        }
    }
}

/// Reprodução explícita sem abrir o microfone; pânico e parada interrompem o áudio.
pub fn play_audio(bytes:&[u8],abort:&(dyn Fn()->bool+Sync))->anyhow::Result<()>{
    if abort(){return Ok(());}
    let playback=Playback::start(decode_mp3(bytes)?,Arc::new(Mutex::new(VecDeque::new())))?;
    let start=Instant::now();
    while !playback.done.load(Ordering::SeqCst) && !playback.failed.load(Ordering::SeqCst) && start.elapsed()<Duration::from_secs(75) && !abort(){std::thread::sleep(Duration::from_millis(20));}
    ensure!(!playback.failed.load(Ordering::SeqCst),"falha na saída de áudio");Ok(())
}
