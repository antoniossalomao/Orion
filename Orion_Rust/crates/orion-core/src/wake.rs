//! Escuta da palavra de ativação ("Orion") no computador (E6.2; REGRAS.md: wake word é só local;
//! áudio só sai depois do "Orion").
//!
//! - enquanto a palavra não é dita, os quadros de 32 ms passam só pelo detector local e são
//!   descartados; nada vai para disco, banco ou rede;
//! - depois da ativação, grava até o silêncio (ou `max_record_s`), manda o WAV ao transcritor, passa
//!   o texto pelo mesmo `Agent` e toca o MP3 se o TTS estiver ligado;
//! - acordar por voz NUNCA aprova ferramenta; se o turno pedir aprovação, o listener avisa em voz alta
//!   que o cartão ficou na tela ou no Telegram;
//! - modo pânico e não perturbe suspendem a escuta na hora, e há pausa própria (`POST /voz/escuta`).

use crate::agent::{Agent, TurnRequest};
use crate::palmas::{acao_das_palmas, ClapDetector, PICO_MINIMO};
use crate::util::{env_limpo, rodar, which};
use crate::voice::{falavel, Speaker, VozStats};
use futures::StreamExt;
use orion_ai::transcribe::Transcriber;
use orion_store::Store;
use parking_lot::Mutex;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use unicode_normalization::UnicodeNormalization;

pub const SAMPLE_RATE: u32 = 16_000;
pub const FRAME_SAMPLES: usize = 512; // 32 ms a 16 kHz
pub const FRAME_BYTES: usize = FRAME_SAMPLES * 2; // int16 mono
pub const FRAME_S: f64 = FRAME_SAMPLES as f64 / SAMPLE_RATE as f64;
pub const MAX_UTTERANCE_S: f64 = 15.0;
pub const MIN_SPEECH_S: f64 = 0.25;
pub const PRE_ROLL_FRAMES: usize = 10; // ~320 ms antes do pico da palavra
pub const COOLDOWN_S: f64 = 1.5;
pub const PAUSA_CHAVE: &str = "voz:pausa";
pub const PLAYERS: [&str; 5] = ["ffplay", "mpv", "afplay", "mpg123", "paplay"];

pub trait Detector: Send + Sync {
    fn reset(&mut self);
    fn feed(&mut self, pcm_int16: &[u8]) -> bool;
}

pub trait AudioSource: Send {
    fn read_frame(&mut self) -> Option<Vec<u8>>;
    fn close(&mut self);
}

/// Energia RMS de um quadro int16 little-endian (0..32768).
pub fn rms(quadro: &[u8]) -> f64 {
    let n = quadro.len() / 2;
    if n == 0 {
        return 0.0;
    }
    let soma: f64 = quadro
        .chunks_exact(2)
        .map(|b| {
            let v = i16::from_le_bytes([b[0], b[1]]) as f64;
            v * v
        })
        .sum();
    (soma / n as f64).sqrt()
}

/// Empacota PCM 16-bit mono em WAV em memória (nada vai para o disco).
pub fn pcm_para_wav(pcm: &[u8], rate: u32) -> Vec<u8> {
    let byte_rate = rate * 2;
    let data_len = pcm.len() as u32;
    let riff_len = 36 + data_len;
    let mut out = Vec::with_capacity(44 + pcm.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&riff_len.to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // PCM fmt chunk
    out.extend_from_slice(&1u16.to_le_bytes()); // audio format = 1 (PCM)
    out.extend_from_slice(&1u16.to_le_bytes()); // 1 canal
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    out.extend_from_slice(pcm);
    out
}

pub fn normalizar(texto: &str) -> String {
    texto
        .nfd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .collect::<String>()
        .to_lowercase()
        .trim()
        .to_string()
}

pub fn argv_player(caminho: &str) -> Option<Vec<String>> {
    for nome in PLAYERS {
        let Some(exe) = which(nome) else { continue };
        let exe = exe.to_string_lossy().into_owned();
        return Some(match nome {
            "ffplay" => vec![exe, "-nodisp".into(), "-autoexit".into(), "-loglevel".into(), "quiet".into(), caminho.into()],
            "mpv" => vec![exe, "--no-video".into(), "--really-quiet".into(), caminho.into()],
            "mpg123" => vec![exe, "-q".into(), caminho.into()],
            _ => vec![exe, caminho.into()],
        });
    }
    None
}

/// Toca o MP3 num reprodutor do sistema e apaga o arquivo temporário no fim.
pub fn tocar_mp3(mp3: &[u8], timeout_s: f64) -> bool {
    if mp3.is_empty() || argv_player("x").is_none() {
        return false;
    }
    let tmp = std::env::temp_dir().join(format!("orion-fala-{}.mp3", orion_store::uuid_hex()));
    if std::fs::write(&tmp, mp3).is_err() {
        return false;
    }
    let r = argv_player(&tmp.to_string_lossy())
        .and_then(|argv| rodar(&argv, None, Duration::from_secs_f64(timeout_s), Some(env_limpo())).ok())
        .is_some_and(|s| s.codigo == Some(0));
    let _ = std::fs::remove_file(&tmp);
    r
}

pub struct DetectorInerte;

impl Detector for DetectorInerte {
    fn reset(&mut self) {}
    fn feed(&mut self, _: &[u8]) -> bool {
        false
    }
}

pub struct FfmpegAudioSource {
    filho: Option<Child>,
}

impl FfmpegAudioSource {
    pub fn open(device: Option<i32>) -> Result<Self, String> {
        let exe = which("ffmpeg").ok_or_else(|| "ffmpeg não encontrado para abrir o microfone".to_string())?.to_string_lossy().into_owned();
        let d = device.map(|i| i.to_string()).unwrap_or_default();
        let entrada = if cfg!(windows) {
            vec!["-f".into(), "dshow".into(), "-i".into(), format!("audio={}", if d.is_empty() { "default" } else { &d })]
        } else if cfg!(target_os = "macos") {
            vec!["-f".into(), "avfoundation".into(), "-i".into(), format!(":{}", if d.is_empty() { "0" } else { &d })]
        } else {
            vec!["-f".into(), "pulse".into(), "-i".into(), if d.is_empty() { "default".into() } else { d }]
        };
        let mut cmd = Command::new(exe);
        cmd.args(["-nostdin", "-hide_banner", "-loglevel", "error"])
            .args(entrada)
            .args(["-ac", "1", "-ar", &SAMPLE_RATE.to_string(), "-f", "s16le", "pipe:1"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .env_clear()
            .envs(env_limpo());
        let filho = cmd.spawn().map_err(|e| format!("microfone indisponível: {e}"))?;
        Ok(Self { filho: Some(filho) })
    }
}

impl AudioSource for FfmpegAudioSource {
    fn read_frame(&mut self) -> Option<Vec<u8>> {
        let out = self.filho.as_mut()?.stdout.as_mut()?;
        let mut buf = vec![0u8; FRAME_BYTES];
        out.read_exact(&mut buf).ok()?;
        Some(buf)
    }

    fn close(&mut self) {
        if let Some(mut f) = self.filho.take() {
            let _ = f.kill();
            let _ = f.wait();
        }
    }
}

#[derive(Clone)]
pub struct WakeConfig {
    pub phrase: String,
    pub threshold: f64,
    pub silence_rms: f64,
    pub silence_s: f64,
    pub max_record_s: f64,
    pub speech_wait_s: f64,
    pub clap_enabled: bool,
    pub clap_peak: i32,
}

impl Default for WakeConfig {
    fn default() -> Self {
        Self {
            phrase: "orion".into(),
            threshold: 0.5,
            silence_rms: 350.0,
            silence_s: 1.0,
            max_record_s: MAX_UTTERANCE_S,
            speech_wait_s: 3.0,
            clap_enabled: false,
            clap_peak: PICO_MINIMO,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct WakeStats {
    pub ativacoes: u64,
    pub palmas: u64,
    pub turnos: u64,
    pub ignorados: u64,
    pub erros: u64,
    pub ultimo_erro: String,
}

pub type OnVoiceTurn = Arc<dyn Fn(&[u8]) + Send + Sync>;
pub type OnClap = Arc<dyn Fn() + Send + Sync>;
pub type PlayFn = Arc<dyn Fn(&[u8]) -> bool + Send + Sync>;
pub type SourceFactory = Arc<dyn Fn() -> Result<Box<dyn AudioSource>, String> + Send + Sync>;

pub struct WakeListener {
    pub detector: Mutex<Box<dyn Detector>>,
    pub source_factory: SourceFactory,
    pub on_utterance: Mutex<OnVoiceTurn>,
    pub on_clap: Option<OnClap>,
    pub memory: Option<Arc<Store>>,
    pub config: WakeConfig,
    pub claps: Option<Mutex<ClapDetector>>,
    pub stats: Mutex<WakeStats>,
    rodando: AtomicBool,
    suspensa: AtomicBool,
    pausa_mem: AtomicBool,
    parar: AtomicBool,
    tempo_s: Mutex<f64>,
}

impl WakeListener {
    pub fn new(
        detector: Box<dyn Detector>,
        source_factory: SourceFactory,
        on_utterance: OnVoiceTurn,
        memory: Option<Arc<Store>>,
        config: WakeConfig,
        on_clap: Option<OnClap>,
    ) -> Self {
        let claps = config.clap_enabled.then(|| Mutex::new(ClapDetector::new(config.clap_peak)));
        Self {
            detector: Mutex::new(detector),
            source_factory,
            on_utterance: Mutex::new(on_utterance),
            on_clap,
            memory,
            config,
            claps,
            stats: Mutex::new(WakeStats::default()),
            rodando: AtomicBool::new(false),
            suspensa: AtomicBool::new(false),
            pausa_mem: AtomicBool::new(false),
            parar: AtomicBool::new(false),
            tempo_s: Mutex::new(0.0),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_runtime(
        detector: Box<dyn Detector>,
        source_factory: SourceFactory,
        agent: Arc<Agent>,
        transcriber: Arc<Transcriber>,
        speaker: Option<Arc<dyn Speaker>>,
        memory: Arc<Store>,
        config: WakeConfig,
        voz_stats: Option<Arc<VozStats>>,
        play_fn: Option<PlayFn>,
        modos: Option<Arc<crate::modos::Modos>>,
    ) -> Arc<Self> {
        let tocar: PlayFn = play_fn.unwrap_or_else(|| Arc::new(|mp3| tocar_mp3(mp3, 60.0)));
        let handle = tokio::runtime::Handle::try_current().ok();
        let (ag, tr, sp, vs, tc) = (agent, transcriber, speaker, voz_stats, tocar);
        let h_utt = handle.clone();

        let listener = Arc::new(Self::new(
            detector,
            source_factory,
            Arc::new(|_| {}),
            Some(memory),
            config.clone(),
            config.clap_enabled.then(|| -> OnClap {
                let m = modos.clone();
                let sp_c = sp.clone();
                let tc_c = tc.clone();
                let h_c = handle.clone();
                Arc::new(move || {
                    // REGRAS.md: palmas só abrem; nunca aprovam nem executam
                    if acao_das_palmas(m.as_deref()) != "abrir" {
                        return;
                    }
                    tracing::info!(target: "orion.audit", "palmas_detectadas acao=abrir");
                    if let (Some(s), Some(h)) = (&sp_c, &h_c) {
                        if let Ok(mp3) = h.block_on(s.synthesize("Estou ouvindo.")) {
                            tc_c(&mp3);
                        }
                    }
                })
            }),
        ));

        let weak = Arc::downgrade(&listener);
        *listener.on_utterance.lock() = Arc::new(move |wav: &[u8]| {
            let Some(me) = weak.upgrade() else { return };
            let Some(h) = &h_utt else { return };
            let wav_vec = wav.to_vec();
            let (ag, tr, sp, vs, tc) = (ag.clone(), tr.clone(), sp.clone(), vs.clone(), tc.clone());
            h.block_on(async move {
                let texto = match tr.transcribe(wav_vec, "wake.wav", "audio/wav").await {
                    Ok(t) => t.trim().to_string(),
                    Err(e) => {
                        let mut st = me.stats.lock();
                        st.erros += 1;
                        st.ultimo_erro = format!("transcrição: {e}");
                        if let Some(v) = &vs {
                            v.inc_erros();
                        }
                        return;
                    }
                };
                if texto.chars().count() < 2 {
                    me.stats.lock().ignorados += 1;
                    return;
                }
                me.stats.lock().turnos += 1;
                if let Some(v) = &vs {
                    v.inc_turnos();
                }
                tracing::info!(target: "orion.audit", "wake_turno caracteres={}", texto.chars().count());
                let mut pedacos = String::new();
                let mut pediu_aprovacao = false;
                let mut stream = ag.run(TurnRequest { channel: "voice".into(), text: texto, ..Default::default() });
                while let Some(ev) = stream.next().await {
                    if ev.kind == "text" {
                        pedacos += ev.data["text"].as_str().unwrap_or("");
                    } else if ev.kind == "approval" {
                        pediu_aprovacao = true;
                    }
                }
                let mut resposta = falavel(&pedacos, 1500);
                if pediu_aprovacao {
                    let aviso = "Essa ação precisa da sua aprovação na tela ou no Telegram.";
                    resposta = if resposta.is_empty() { aviso.into() } else { format!("{resposta} {aviso}") };
                }
                if let (false, Some(s)) = (resposta.is_empty(), &sp) {
                    match s.synthesize(&resposta).await {
                        Ok(mp3) => {
                            if let Some(v) = &vs {
                                v.inc_sintetizados();
                            }
                            tc(&mp3);
                        }
                        Err(_) => {
                            if let Some(v) = &vs {
                                v.inc_erros();
                            }
                        }
                    }
                }
            });
        });

        listener
    }

    pub fn pausada(&self) -> bool {
        match &self.memory {
            Some(m) => m.counter_get(PAUSA_CHAVE) > 0,
            None => self.pausa_mem.load(Ordering::SeqCst),
        }
    }

    pub fn pausar(&self, ativa: bool) {
        match &self.memory {
            Some(m) => {
                let _ = m.counter_set(PAUSA_CHAVE, ativa as i64);
            }
            None => self.pausa_mem.store(ativa, Ordering::SeqCst),
        }
    }

    /// Chamada pelo modo pânico / não perturbe para cortar a escuta na hora.
    pub fn set_suspended(&self, ativa: bool) {
        self.suspensa.store(ativa, Ordering::SeqCst);
    }

    pub fn estado(&self) -> Value {
        let s = self.stats.lock().clone();
        let rodando = self.rodando.load(Ordering::SeqCst);
        let pausada = self.pausada() || self.suspensa.load(Ordering::SeqCst);
        json!({
            "ligada": true,
            "rodando": rodando,
            "pausada": pausada,
            "escutando": rodando && !pausada,
            "palavra": self.config.phrase,
            "ativacoes": s.ativacoes,
            "palmas": s.palmas,
            "palmas_ativas": self.claps.is_some(),
            "turnos": s.turnos,
            "ignorados": s.ignorados,
            "erros": s.erros,
            "ultimo_erro": s.ultimo_erro,
        })
    }

    /// Processa até uma ativação (usado nos testes e pelo laço em segundo plano).
    pub fn step(&self, src: &mut dyn AudioSource) -> bool {
        let mut pre_roll: VecDeque<Vec<u8>> = VecDeque::with_capacity(PRE_ROLL_FRAMES);
        while !self.parar.load(Ordering::SeqCst) {
            if self.pausada() || self.suspensa.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(200));
                continue;
            }
            let Some(quadro) = src.read_frame() else { return false };
            if quadro.len() != FRAME_BYTES {
                continue;
            }
            if pre_roll.len() >= PRE_ROLL_FRAMES {
                pre_roll.pop_front();
            }
            pre_roll.push_back(quadro.clone());
            let t = {
                let mut ts = self.tempo_s.lock();
                *ts += FRAME_S;
                *ts
            };
            if let Some(claps) = &self.claps {
                if claps.lock().feed(&quadro, t) {
                    self.stats.lock().palmas += 1;
                    self.detector.lock().reset();
                    if let Some(cb) = &self.on_clap {
                        cb();
                    }
                    if let Some(wav) = self.gravar_pedido(src, VecDeque::new()) {
                        (self.on_utterance.lock())(&wav);
                    } else {
                        self.stats.lock().ignorados += 1;
                    }
                    return true;
                }
            }
            if self.detector.lock().feed(&quadro) {
                self.stats.lock().ativacoes += 1;
                self.detector.lock().reset();
                if let Some(claps) = &self.claps {
                    claps.lock().reset();
                }
                if let Some(wav) = self.gravar_pedido(src, pre_roll) {
                    (self.on_utterance.lock())(&wav);
                } else {
                    self.stats.lock().ignorados += 1;
                }
                return true;
            }
        }
        false
    }

    fn gravar_pedido(&self, src: &mut dyn AudioSource, pre_roll: VecDeque<Vec<u8>>) -> Option<Vec<u8>> {
        let max_quadros = (self.config.max_record_s / FRAME_S).max(1.0) as usize;
        let espera_quadros = (self.config.speech_wait_s / FRAME_S).max(1.0) as usize;
        let silencio_quadros = (self.config.silence_s / FRAME_S).max(1.0) as usize;
        let min_fala = (MIN_SPEECH_S / FRAME_S).max(1.0) as usize;

        let mut gravados: Vec<Vec<u8>> = pre_roll.into_iter().collect();
        let mut falou = 0usize;
        let mut silencio = 0usize;
        for i in 0..max_quadros {
            if self.parar.load(Ordering::SeqCst) || self.pausada() || self.suspensa.load(Ordering::SeqCst) {
                return None;
            }
            let Some(q) = src.read_frame() else { break };
            if q.len() != FRAME_BYTES {
                continue;
            }
            gravados.push(q.clone());
            if rms(&q) >= self.config.silence_rms {
                falou += 1;
                silencio = 0;
            } else if falou > 0 {
                silencio += 1;
                if falou >= min_fala && silencio >= silencio_quadros {
                    break;
                }
            } else if i + 1 >= espera_quadros {
                return None; // ativou mas ninguém falou nada
            }
        }
        if falou < min_fala {
            return None;
        }
        let pcm: Vec<u8> = gravados.into_iter().flatten().collect();
        Some(pcm_para_wav(&pcm, SAMPLE_RATE))
    }

    pub fn start(self: &Arc<Self>) {
        if self.rodando.load(Ordering::SeqCst) {
            return;
        }
        self.parar.store(false, Ordering::SeqCst);
        let me = self.clone();
        let _ = std::thread::Builder::new().name("orion-wake".into()).spawn(move || me.laco());
    }

    pub fn stop(&self) {
        self.parar.store(true, Ordering::SeqCst);
        self.rodando.store(false, Ordering::SeqCst);
    }

    fn laco(&self) {
        let mut src = match (self.source_factory)() {
            Ok(s) => s,
            Err(e) => {
                let mut st = self.stats.lock();
                st.erros += 1;
                st.ultimo_erro = e.clone();
                tracing::error!("escuta local desligada: {e}");
                return;
            }
        };
        self.rodando.store(true, Ordering::SeqCst);
        while !self.parar.load(Ordering::SeqCst) {
            if !self.step(&mut *src) && !self.parar.load(Ordering::SeqCst) {
                break;
            }
            if !self.parar.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_secs_f64(COOLDOWN_S));
            }
        }
        self.rodando.store(false, Ordering::SeqCst);
        src.close();
    }
}

pub fn listar_microfones() -> Vec<(usize, String)> {
    vec![]
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeDetector {
        disparar_em: usize,
        quadros: usize,
        resets: usize,
    }

    impl Detector for FakeDetector {
        fn reset(&mut self) {
            self.resets += 1;
        }
        fn feed(&mut self, _: &[u8]) -> bool {
            self.quadros += 1;
            self.quadros == self.disparar_em
        }
    }

    struct FakeSource {
        quadros: VecDeque<Vec<u8>>,
    }

    impl AudioSource for FakeSource {
        fn read_frame(&mut self) -> Option<Vec<u8>> {
            self.quadros.pop_front()
        }
        fn close(&mut self) {}
    }

    fn quadro(amplitude: i16) -> Vec<u8> {
        let mut b = Vec::with_capacity(FRAME_BYTES);
        for _ in 0..FRAME_SAMPLES {
            b.extend_from_slice(&amplitude.to_le_bytes());
        }
        b
    }

    #[test]
    fn rms_mede_silencio_e_fala() {
        assert_eq!(rms(&quadro(0)), 0.0);
        assert!(rms(&quadro(4000)) > 3900.0);
        assert_eq!(rms(b""), 0.0);
    }

    #[test]
    fn pcm_para_wav_gera_cabecalho_16khz_mono() {
        let mut pcm = quadro(1000);
        pcm.extend_from_slice(&quadro(1000));
        let wav = pcm_para_wav(&pcm, SAMPLE_RATE);
        assert!(wav.starts_with(b"RIFF"));
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]), SAMPLE_RATE);
        assert_eq!(&wav[44..], pcm.as_slice());
    }

    #[test]
    fn antes_da_palavra_nenhum_audio_sai() {
        let recebidos = Arc::new(Mutex::new(vec![]));
        let r2 = recebidos.clone();
        let mut seq = VecDeque::new();
        for _ in 0..20 {
            seq.push_back(quadro(5000));
        }
        let mut src = FakeSource { quadros: seq };
        let w = WakeListener::new(
            Box::new(FakeDetector { disparar_em: 999, quadros: 0, resets: 0 }),
            Arc::new(|| Err("nao usado".into())),
            Arc::new(move |b| r2.lock().push(b.to_vec())),
            None,
            WakeConfig::default(),
            None,
        );
        assert!(!w.step(&mut src));
        assert!(recebidos.lock().is_empty());
        assert_eq!(w.stats.lock().ativacoes, 0);
    }

    #[test]
    fn ativacao_grava_fala_ate_o_silencio_e_entrega_wav() {
        let recebidos = Arc::new(Mutex::new(vec![]));
        let r2 = recebidos.clone();
        let mut seq = VecDeque::new();
        for _ in 0..5 {
            seq.push_back(quadro(0));
        }
        for _ in 0..12 {
            seq.push_back(quadro(4000));
        }
        for _ in 0..10 {
            seq.push_back(quadro(0));
        }
        let mut src = FakeSource { quadros: seq };
        let cfg = WakeConfig { silence_s: 0.2, silence_rms: 350.0, ..Default::default() };
        let w = WakeListener::new(
            Box::new(FakeDetector { disparar_em: 5, quadros: 0, resets: 0 }),
            Arc::new(|| Err("nao usado".into())),
            Arc::new(move |b| r2.lock().push(b.to_vec())),
            None,
            cfg,
            None,
        );
        assert!(w.step(&mut src));
        let rec = recebidos.lock();
        assert_eq!(rec.len(), 1);
        assert!(rec[0].starts_with(b"RIFF"));
        assert_eq!(w.stats.lock().ativacoes, 1);
    }

    #[test]
    fn duas_palmas_acordam_e_entregam_fala_sem_wake_word() {
        let recebidos = Arc::new(Mutex::new(vec![]));
        let r2 = recebidos.clone();
        let palmas = Arc::new(Mutex::new(0usize));
        let p2 = palmas.clone();
        let mut seq = VecDeque::new();
        for &(amp, n) in &[(0i16, 4usize), (9000, 2), (0, 6), (9000, 2), (0, 2), (4000, 12), (0, 10)] {
            for _ in 0..n {
                seq.push_back(quadro(amp));
            }
        }
        let mut src = FakeSource { quadros: seq };
        let cfg = WakeConfig { silence_s: 0.2, silence_rms: 350.0, clap_enabled: true, clap_peak: 5000, ..Default::default() };
        let w = WakeListener::new(
            Box::new(FakeDetector { disparar_em: 9999, quadros: 0, resets: 0 }),
            Arc::new(|| Err("nao usado".into())),
            Arc::new(move |b| r2.lock().push(b.to_vec())),
            None,
            cfg,
            Some(Arc::new(move || *p2.lock() += 1)),
        );
        assert!(w.step(&mut src));
        assert_eq!(*palmas.lock(), 1);
        assert_eq!(w.stats.lock().palmas, 1);
        assert_eq!(w.stats.lock().ativacoes, 0);
        assert_eq!(recebidos.lock().len(), 1);
    }
}
