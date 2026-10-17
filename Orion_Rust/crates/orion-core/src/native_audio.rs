//! Adaptadores Windows (REGRAS.md: opt-in não liga sozinho; toda saída de dado fica registrada).
use crate::wake::{AudioSource, Detector};
use parking_lot::Mutex;
use std::{path::Path, sync::Arc};

pub struct CpalSource(Option<orion_voice::Microphone>);
impl CpalSource {
    pub fn open(device: &str) -> Result<Self, String> {
        orion_voice::Microphone::open(device).map(|mic| Self(Some(mic))).map_err(|e| e.to_string())
    }
}
impl AudioSource for CpalSource {
    fn read_frame(&mut self) -> Option<Vec<u8>> {
        self.0.as_ref()?.read_frame()
    }
    fn close(&mut self) {
        self.0.take();
    }
}
pub struct OpenWakeWord(orion_voice::OrionWake);
impl OpenWakeWord {
    pub fn load(path: &Path, threshold: f32) -> Result<Self, String> {
        orion_voice::OrionWake::load(path, threshold).map(Self).map_err(|e| e.to_string())
    }
}
impl Detector for OpenWakeWord {
    fn reset(&mut self) {
        self.0.reset()
    }
    fn feed(&mut self, pcm: &[u8]) -> bool {
        self.0.feed(pcm)
    }
}

pub fn transcriber(model: &Path, gpu: bool, fallback: Option<orion_ai::transcribe::Transcriber>) -> anyhow::Result<orion_ai::transcribe::Transcriber> {
    let context = Arc::new(Mutex::new(orion_voice::LocalWhisper::load(model, gpu)?));
    Ok(orion_ai::transcribe::Transcriber::with_local(
        Arc::new(move |audio| {
            let decoded = decode(audio).map_err(|_| orion_ai::transcribe::TranscribeError("não foi possível decodificar o áudio local".into()))?;
            let text = context.lock().transcribe(decoded).map_err(|_| orion_ai::transcribe::TranscribeError("transcrição local falhou".into()))?;
            if text.is_empty() {
                return Err(orion_ai::transcribe::TranscribeError("áudio sem fala".into()));
            }
            Ok(text)
        }),
        fallback,
    ))
}
fn decode(audio: Vec<u8>) -> anyhow::Result<Vec<u8>> {
    if let Ok(spec) = hound::WavReader::new(std::io::Cursor::new(&audio)).map(|reader| reader.spec()) {
        if spec.channels == 1 && spec.sample_rate == 16000 && spec.bits_per_sample == 16 {
            return Ok(audio);
        }
    }
    let dir = tempfile::tempdir()?;
    let input = dir.path().join("input.audio");
    let output = dir.path().join("output.wav");
    std::fs::write(&input, audio)?;
    let exe = crate::util::which("ffmpeg").ok_or_else(|| anyhow::anyhow!("ffmpeg necessário para áudio comprimido"))?;
    let result = crate::util::rodar(
        &[
            exe.to_string_lossy().into_owned(),
            "-nostdin".into(),
            "-v".into(),
            "error".into(),
            "-i".into(),
            input.to_string_lossy().into_owned(),
            "-t".into(),
            "300".into(),
            "-ac".into(),
            "1".into(),
            "-ar".into(),
            "16000".into(),
            "-c:a".into(),
            "pcm_s16le".into(),
            output.to_string_lossy().into_owned(),
        ],
        None,
        std::time::Duration::from_secs(30),
        Some(crate::util::env_limpo()),
    )
    .map_err(|_| anyhow::anyhow!("ffmpeg interrompido ou indisponível"))?;
    anyhow::ensure!(result.codigo == Some(0), "decodificação falhou");
    Ok(std::fs::read(output)?)
}
