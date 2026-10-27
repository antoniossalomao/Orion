//! Legenda ao vivo de reuniões e aulas no computador (E11.7; REGRAS.md: opt-in explícito para o que
//! observa ou age).
//!
//! Captura blocos curtos de áudio (16 kHz mono PCM), transcreve no computador e mantém só as
//! últimas linhas em memória para a tela ou CLI ler. Nada vai para o disco nem para a rede.

use crate::util::{env_limpo, which};
use crate::wake::{pcm_para_wav, rms};
use parking_lot::Mutex;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub const RATE: u32 = 16_000;
pub const BLOCO: usize = RATE as usize * 3; // 3 s por fatia
pub const MAX_LINHAS: usize = 40;

#[derive(Debug, Clone, thiserror::Error)]
#[error("{0}")]
pub struct LegendaError(pub String);

pub fn extrair_texto_vosk(final_json: &str) -> String {
    serde_json::from_str::<Value>(final_json)
        .ok()
        .and_then(|v| v.get("text").and_then(Value::as_str).map(|s| s.trim().to_string()))
        .unwrap_or_default()
}

pub type TranscritorPcm = Arc<dyn Fn(&[u8]) -> String + Send + Sync>;

pub fn criar_transcritor(modelo: &str) -> Result<TranscritorPcm, LegendaError> {
    let m = modelo.trim();
    if m.is_empty() {
        return Err(LegendaError("defina ORION_SUBTITLE_MODEL (pasta Vosk ou modelo Whisper)".into()));
    }
    let p = PathBuf::from(m);
    if !p.exists() {
        return Err(LegendaError(format!("modelo de legenda indisponível ({m}): confira ORION_SUBTITLE_MODEL")));
    }
    #[cfg(windows)]
    {
        // O binário Windows já contém Whisper; legenda não precisa de executável auxiliar.
        let model=Arc::new(Mutex::new(None::<orion_voice::LocalWhisper>));
        return Ok(Arc::new(move|pcm:&[u8]|{
            let mut guard=model.lock();
            if guard.is_none(){match orion_voice::LocalWhisper::load(&p,false){Ok(m)=>*guard=Some(m),Err(_)=>return String::new()}}
            guard.as_ref().unwrap().transcribe(pcm_para_wav(pcm,RATE)).unwrap_or_default()
        }));
    }
    #[cfg(not(windows))]
    let cli = which("whisper-cli").or_else(|| which("whisper-cpp")).ok_or_else(|| {
        LegendaError(format!("modelo de legenda indisponível ({m}): instale whisper-cli ou confira ORION_SUBTITLE_MODEL"))
    })?;
    #[cfg(not(windows))]
    {Ok(Arc::new(move |pcm: &[u8]| {
        let wav = pcm_para_wav(pcm, RATE);
        let tmp = std::env::temp_dir().join(format!("orion-legenda-{}.wav", orion_store::uuid_hex()));
        if std::fs::write(&tmp, wav).is_err() {
            return String::new();
        }
        let r = crate::util::rodar(
            &[
                cli.to_string_lossy().into_owned(),
                "-m".into(),
                p.to_string_lossy().into_owned(),
                "-f".into(),
                tmp.to_string_lossy().into_owned(),
                "-nt".into(),
                "-np".into(),
            ],
            None,
            std::time::Duration::from_secs(15),
            Some(env_limpo()),
        );
        let _ = std::fs::remove_file(&tmp);
        r.ok()
            .filter(|s| s.codigo == Some(0))
            .map(|s| String::from_utf8_lossy(&s.stdout).trim().to_string())
            .unwrap_or_default()
    }))}
}

fn argv_captura_audio(dispositivo: &str) -> Option<Vec<String>> {
    let exe = which("ffmpeg")?.to_string_lossy().into_owned();
    let dev = dispositivo.trim();
    let entrada = if cfg!(windows) {
        vec!["-f".into(), "dshow".into(), "-i".into(), format!("audio={}", if dev.is_empty() { "default" } else { dev })]
    } else if cfg!(target_os = "macos") {
        vec!["-f".into(), "avfoundation".into(), "-i".into(), format!(":{}", if dev.is_empty() { "0" } else { dev })]
    } else {
        vec!["-f".into(), "pulse".into(), "-i".into(), if dev.is_empty() { "default".into() } else { dev.into() }]
    };
    Some(
        [
            vec![exe, "-nostdin".into(), "-hide_banner".into(), "-loglevel".into(), "error".into()],
            entrada,
            vec!["-ac".into(), "1".into(), "-ar".into(), RATE.to_string(), "-f".into(), "s16le".into(), "pipe:1".into()],
        ]
        .concat(),
    )
}

pub struct Legenda {
    pub transcrever: TranscritorPcm,
    pub limiar_rms: f64,
    linhas: Mutex<VecDeque<Value>>,
    pub rodando: AtomicBool,
}

impl Legenda {
    pub fn new(transcrever: TranscritorPcm, limiar_rms: f64) -> Self {
        Self {
            transcrever,
            limiar_rms,
            linhas: Mutex::new(VecDeque::with_capacity(MAX_LINHAS)),
            rodando: AtomicBool::new(false),
        }
    }

    pub fn alimentar(&self, pcm: &[u8], agora: Option<f64>) -> Option<String> {
        if pcm.is_empty() || rms(pcm) < self.limiar_rms {
            return None;
        }
        let bruto = (self.transcrever)(pcm);
        let texto = bruto.trim().to_string();
        if texto.is_empty() {
            return None;
        }
        let ts = agora.unwrap_or_else(crate::util::agora);
        let mut g = self.linhas.lock();
        if g.len() >= MAX_LINHAS {
            g.pop_front();
        }
        g.push_back(json!({
            "ts": ts,
            "hora": crate::util::local(ts).format("%H:%M:%S").to_string(),
            "texto": texto.chars().take(500).collect::<String>(),
        }));
        Some(texto)
    }

    pub fn estado(&self) -> Value {
        let g = self.linhas.lock();
        let v: Vec<Value> = g.iter().cloned().collect();
        json!({"ativa": self.rodando.load(Ordering::SeqCst), "linhas": v})
    }

    pub fn limpar(&self) {
        self.linhas.lock().clear();
    }
}

/// Captura blocos do dispositivo de áudio em segundo plano até `parar()` ser chamado.
pub fn iniciar_thread(legenda: Arc<Legenda>, dispositivo: &str) -> Arc<dyn Fn() + Send + Sync> {
    iniciar_thread_guarded(legenda,dispositivo,Arc::new(||false))
}
pub fn iniciar_thread_guarded(legenda:Arc<Legenda>,dispositivo:&str,paused:Arc<dyn Fn()->bool+Send+Sync>)->Arc<dyn Fn()+Send+Sync>{
    let parar_flag = Arc::new(AtomicBool::new(false));
    let p2 = parar_flag.clone();
    let dev = dispositivo.to_string();
    std::thread::Builder::new()
        .name("orion-legenda".into())
        .spawn(move || {
            #[cfg(windows)]
            {
                let mut mic=None;
                let mut buf=Vec::with_capacity(BLOCO*2);
                while !p2.load(Ordering::SeqCst){
                    if paused(){mic.take();buf.clear();legenda.rodando.store(false,Ordering::SeqCst);std::thread::sleep(std::time::Duration::from_millis(100));continue;}
                    if mic.is_none(){match orion_voice::Microphone::open(&dev){Ok(m)=>{mic=Some(m);legenda.rodando.store(true,Ordering::SeqCst);},Err(_)=>break}}
                    if let Some(frame)=mic.as_ref().unwrap().read_frame(){buf.extend(frame);if buf.len()>=BLOCO*2{if !paused(){let _=legenda.alimentar(&buf,None);}buf.clear();}}
                }
                legenda.rodando.store(false,Ordering::SeqCst);return;
            }
            #[cfg(not(windows))]
            {let Some(argv) = argv_captura_audio(&dev) else {
                legenda.rodando.store(false, Ordering::SeqCst);
                return;
            };
            let mut cmd = Command::new(&argv[0]);
            cmd.args(&argv[1..]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
            cmd.env_clear().envs(env_limpo());
            let Ok(mut filho) = cmd.spawn() else {
                legenda.rodando.store(false, Ordering::SeqCst);
                return;
            };
            let Some(mut out) = filho.stdout.take() else {
                let _ = filho.kill();
                legenda.rodando.store(false, Ordering::SeqCst);
                return;
            };
            legenda.rodando.store(true, Ordering::SeqCst);
            let mut buf = vec![0u8; BLOCO * 2];
            while !p2.load(Ordering::SeqCst) && !paused() {
                match out.read_exact(&mut buf) {
                    Ok(()) => {
                        let _ = legenda.alimentar(&buf, None);
                    }
                    Err(_) => break,
                }
            }
            legenda.rodando.store(false, Ordering::SeqCst);
            let _ = filho.kill();
            let _ = filho.wait();}
        })
        .ok();
    Arc::new(move || parar_flag.store(true, Ordering::SeqCst))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legenda_ignora_silencio_e_guarda_ultimas_linhas() {
        let leg = Legenda::new(Arc::new(|_| "olá mundo".into()), 100.0);
        assert_eq!(leg.alimentar(&vec![0u8; 640], Some(1_700_000_000.0)), None);
        let mut alto = Vec::with_capacity(640);
        for _ in 0..320 {
            alto.extend_from_slice(&2000i16.to_le_bytes());
        }
        assert_eq!(leg.alimentar(&alto, Some(1_700_000_000.0)).as_deref(), Some("olá mundo"));
        let est = leg.estado();
        assert_eq!(est["linhas"].as_array().unwrap().len(), 1);
        assert_eq!(est["linhas"][0]["texto"], json!("olá mundo"));
        leg.limpar();
        assert!(leg.estado()["linhas"].as_array().unwrap().is_empty());
    }
}
