//! Voz sob o mesmo `Agent` e a mesma `PolicyEngine` (E6.1 + E6.4; REGRAS.md: aprovação só por
//! canal autenticado; opt-in não liga sozinho).
//!
//! - clique-para-falar (`/ws/voz` e `/ws/voice`): um WebSocket autenticado recebe os pedaços do
//!   áudio gravado depois do clique, transcreve (`Transcriber`), roda `Agent::run(channel="voice")`,
//!   transmite o texto à medida que sai e, se a resposta falada (`ORION_VOICE_SPEAK`) estiver ligada,
//!   sintetiza em MP3 (`edge-tts`, gratuito) e manda no fim;
//! - conversa ao vivo (`/ws/voz?modo=live`, `ORION_VOICE_LIVE_API_KEY`): abre a sessão em tempo real
//!   com o Gemini Live quando configurado, mas TODA chamada de ferramenta passa por `Agent::execute_tool`
//!   (que consulta `PolicyEngine`). Se a política pedir confirmação, a voz NÃO aprova: o cartão vai
//!   para a tela e o turno para ali;
//! - sem escuta oculta: o servidor só recebe áudio enquanto a conexão que o Antônio abriu está em
//!   `listening`; fechar ou mandar `stop`/`interrupt` encerra na hora.

use crate::agent::{Agent, TurnRequest};
use crate::modos::Modos;
use crate::util::{env_limpo, rodar, which};
use futures::future::BoxFuture;
use futures::StreamExt;
use orion_ai::saidas::medir;
use orion_ai::transcribe::{Transcriber, MAX_AUDIO};
use regex::Regex;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::Duration;

pub const VOZ_PADRAO: &str = "pt-BR-AntonioNeural";
pub const MAX_TTS_CHARS: usize = 1500;
pub const CANAL: &str = "voice";
pub const ESTADOS: [&str; 5] = ["idle", "listening", "thinking", "speaking", "waiting_approval"];

static BLOCO_CODIGO: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)```.*?```").unwrap());
static LINK_MD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[([^\]]+)\]\([^)]+\)").unwrap());
static FORMATACAO: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[`*_#>~]+").unwrap());

#[derive(Default)]
pub struct VozStats {
    pub turnos: AtomicU64,
    pub sintetizados: AtomicU64,
    pub erros: AtomicU64,
    pub ao_vivo_ativos: AtomicI64,
}

impl VozStats {
    pub fn inc_turnos(&self) {
        self.turnos.fetch_add(1, Ordering::Relaxed);
    }
    pub fn inc_sintetizados(&self) {
        self.sintetizados.fetch_add(1, Ordering::Relaxed);
    }
    pub fn inc_erros(&self) {
        self.erros.fetch_add(1, Ordering::Relaxed);
    }

    pub fn estado(&self, clique: bool, tts: bool, live: bool) -> Value {
        json!({
            "clique": clique,
            "tts": tts,
            "ao_vivo": live,
            "ao_vivo_ativos": self.ao_vivo_ativos.load(Ordering::Relaxed),
            "turnos": self.turnos.load(Ordering::Relaxed),
            "sintetizados": self.sintetizados.load(Ordering::Relaxed),
            "erros": self.erros.load(Ordering::Relaxed),
        })
    }
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("{0}")]
pub struct SpeakError(pub String);

pub trait Speaker: Send + Sync {
    fn synthesize<'a>(&'a self, text: &'a str) -> BoxFuture<'a, Result<Vec<u8>, SpeakError>>;
}

/// Síntese gratuita pelo `edge-tts`. O texto sai para a Microsoft: entra no registro de saída sem o
/// conteúdo (REGRAS.md: toda saída de dado fica registrada, sem conteúdo).
pub struct EdgeSpeaker {
    pub voice: String,
    pub timeout_s: f64,
}

impl EdgeSpeaker {
    pub fn new(voice: &str, timeout_s: f64) -> Self {
        Self {
            voice: if voice.trim().is_empty() { VOZ_PADRAO.into() } else { voice.trim().into() },
            timeout_s,
        }
    }
}

impl Speaker for EdgeSpeaker {
    fn synthesize<'a>(&'a self, text: &'a str) -> BoxFuture<'a, Result<Vec<u8>, SpeakError>> {
        Box::pin(async move {
            let limpo = falavel(text, MAX_TTS_CHARS);
            if limpo.is_empty() {
                return Err(SpeakError("texto vazio para síntese".into()));
            }
            let voice = self.voice.clone();
            let timeout = self.timeout_s;
            tokio::task::spawn_blocking(move || {
                let Some(exe) = which("edge-tts") else {
                    return Err(SpeakError("edge-tts não está instalado".into()));
                };
                let tmp = std::env::temp_dir().join(format!("orion-tts-{}.mp3", orion_store::uuid_hex()));
                let mut m = medir("edge-tts", "tts", &voice, limpo.len(), "texto");
                let r = rodar(
                    &[
                        exe.to_string_lossy().into_owned(),
                        "--voice".into(),
                        voice.clone(),
                        "--text".into(),
                        limpo,
                        "--write-media".into(),
                        tmp.to_string_lossy().into_owned(),
                    ],
                    None,
                    Duration::from_secs_f64(timeout),
                    Some(env_limpo()),
                );
                let audio = std::fs::read(&tmp).unwrap_or_default();
                let _ = std::fs::remove_file(&tmp);
                if r.is_err() || r.as_ref().ok().and_then(|s| s.codigo) != Some(0) || audio.is_empty() {
                    return Err(SpeakError("falha ao sintetizar a fala".into()));
                }
                m.ok = true;
                m.bytes_in = audio.len();
                Ok(audio)
            })
            .await
            .map_err(|e| SpeakError(e.to_string()))?
        })
    }
}

/// Detecta `(nome_arquivo, mime)` pelos primeiros bytes; `None` se não for áudio conhecido.
pub fn tipo_do_audio(cabecalho: &[u8]) -> Option<(&'static str, &'static str)> {
    if cabecalho.len() >= 12 && &cabecalho[..4] == b"RIFF" && &cabecalho[8..12] == b"WAVE" {
        return Some(("voz.wav", "audio/wav"));
    }
    if cabecalho.starts_with(b"OggS") {
        return Some(("voz.ogg", "audio/ogg"));
    }
    if cabecalho.starts_with(b"\x1a\x45\xdf\xa3") {
        return Some(("voz.webm", "audio/webm"));
    }
    if cabecalho.len() >= 8 && &cabecalho[4..8] == b"ftyp" {
        return Some(("voz.m4a", "audio/mp4"));
    }
    if cabecalho.starts_with(b"ID3") || (cabecalho.len() >= 2 && cabecalho[0] == 0xff && (cabecalho[1] & 0xe0) == 0xe0) {
        return Some(("voz.mp3", "audio/mpeg"));
    }
    None
}

/// Tira blocos de código e marcação para a síntese não soletrar crases e colchetes.
pub fn falavel(texto: &str, limite: usize) -> String {
    let s = BLOCO_CODIGO.replace_all(texto, " (código na tela) ");
    let s = LINK_MD.replace_all(&s, "$1");
    let s = FORMATACAO.replace_all(&s, " ");
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() <= limite {
        s
    } else {
        let corte: String = s.chars().take(limite).collect();
        let pos = corte.rfind(' ').filter(|&p| p > limite / 2).unwrap_or(corte.len());
        format!("{}…", &corte[..pos])
    }
}

pub trait CanalVoz: Send + Sync {
    fn send_json<'a>(&'a self, v: Value) -> BoxFuture<'a, bool>;
    fn send_bytes<'a>(&'a self, b: Vec<u8>) -> BoxFuture<'a, bool>;
}

async fn estado_ws(ws: &dyn CanalVoz, valor: &str) {
    debug_assert!(ESTADOS.contains(&valor));
    ws.send_json(json!({"type": "state", "state": valor})).await;
}

/// Executa um turno de clique-para-falar: áudio -> texto -> `Agent::run` -> TTS opcional.
#[allow(clippy::too_many_arguments)]
pub async fn turno_de_voz(
    ws: &dyn CanalVoz,
    audio: Vec<u8>,
    agent: Option<Arc<Agent>>,
    transcriber: Option<Arc<Transcriber>>,
    speaker: Option<Arc<dyn Speaker>>,
    modos: Option<Arc<Modos>>,
    stats: Arc<VozStats>,
    falar: bool,
    session_id: Option<String>,
) {
    if modos.as_ref().is_some_and(|m| m.panico()) {
        ws.send_json(json!({"type": "error", "message": "Modo pânico ativo: a voz está cortada."})).await;
        estado_ws(ws, "idle").await;
        return;
    }
    let Some(tr) = transcriber else {
        ws.send_json(json!({"type": "error", "message": "Transcrição desligada (defina ORION_TRANSCRIBE_API_KEY)."})).await;
        estado_ws(ws, "idle").await;
        return;
    };
    let Some(ag) = agent else {
        ws.send_json(json!({"type": "error", "message": "Nenhum modelo configurado."})).await;
        estado_ws(ws, "idle").await;
        return;
    };
    if audio.is_empty() {
        ws.send_json(json!({"type": "error", "message": "Áudio vazio."})).await;
        estado_ws(ws, "idle").await;
        return;
    }
    if audio.len() > MAX_AUDIO {
        ws.send_json(json!({"type": "error", "message": "Áudio passa de 20 MB."})).await;
        estado_ws(ws, "idle").await;
        return;
    }
    let Some((nome, mime)) = tipo_do_audio(&audio[..audio.len().min(16)]) else {
        ws.send_json(json!({"type": "error", "message": "Formato de áudio não reconhecido (use webm, ogg, wav, mp3 ou m4a)."})).await;
        estado_ws(ws, "idle").await;
        return;
    };

    estado_ws(ws, "thinking").await;
    let texto = match tr.transcribe(audio, nome, mime).await {
        Ok(t) => t.trim().to_string(),
        Err(e) => {
            stats.inc_erros();
            ws.send_json(json!({"type": "error", "message": format!("Não consegui transcrever: {e}")})).await;
            estado_ws(ws, "idle").await;
            return;
        }
    };
    if texto.is_empty() {
        ws.send_json(json!({"type": "transcript", "text": ""})).await;
        estado_ws(ws, "idle").await;
        return;
    }
    ws.send_json(json!({"type": "transcript", "text": texto})).await;
    stats.inc_turnos();

    let mut pedacos = String::new();
    let mut aprovacoes = vec![];
    let mut stream = ag.run(TurnRequest {
        channel: CANAL.into(),
        text: texto,
        expected_session: session_id,
        ..Default::default()
    });
    while let Some(ev) = stream.next().await {
        match ev.kind {
            "session" => {
                ws.send_json(json!({"type": "session", "session_id": ev.data["session_id"]})).await;
            }
            "text" => {
                let t = ev.data["text"].as_str().unwrap_or("");
                pedacos += t;
                ws.send_json(json!({"type": "text", "text": t})).await;
            }
            "tool" => {
                ws.send_json(json!({"type": "tool", "tool": ev.data["tool"], "status": ev.data["status"]})).await;
            }
            "approval" => {
                aprovacoes.push(ev.data.clone());
                ws.send_json(json!({"type": "approval", "approval": ev.data})).await;
            }
            "error" => {
                stats.inc_erros();
                ws.send_json(json!({"type": "error", "message": ev.data["message"].as_str().unwrap_or("erro")})).await;
            }
            _ => {}
        }
    }

    let mut fala = falavel(&pedacos, MAX_TTS_CHARS);
    if !aprovacoes.is_empty() && fala.is_empty() {
        fala = "Essa ação precisa da sua aprovação na tela.".into();
    }
    if let (true, Some(sp), false) = (falar, &speaker, fala.is_empty()) {
        estado_ws(ws, "speaking").await;
        match sp.synthesize(&fala).await {
            Ok(mp3) => {
                stats.inc_sintetizados();
                ws.send_json(json!({"type": "audio", "mime": "audio/mpeg", "bytes": mp3.len()})).await;
                ws.send_bytes(mp3).await;
            }
            Err(e) => {
                stats.inc_erros();
                tracing::warn!("TTS falhou: {e}");
            }
        }
    }
    estado_ws(ws, if aprovacoes.is_empty() { "idle" } else { "waiting_approval" }).await;
    ws.send_json(json!({"type": "done"})).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use orion_ai::gateway::{ChatGateway, Endpoint};
    use orion_policy::{ApprovalStore, PathGuard, PolicyEngine};
    use orion_store::Store;
    use parking_lot::Mutex;

    struct MemWs {
        mensagens: Mutex<Vec<Value>>,
        binarios: Mutex<Vec<Vec<u8>>>,
    }

    impl MemWs {
        fn new() -> Self {
            Self { mensagens: Mutex::new(vec![]), binarios: Mutex::new(vec![]) }
        }
    }

    impl CanalVoz for MemWs {
        fn send_json<'a>(&'a self, v: Value) -> BoxFuture<'a, bool> {
            self.mensagens.lock().push(v);
            Box::pin(async { true })
        }
        fn send_bytes<'a>(&'a self, b: Vec<u8>) -> BoxFuture<'a, bool> {
            self.binarios.lock().push(b);
            Box::pin(async { true })
        }
    }

    struct FakeSpeaker;
    impl Speaker for FakeSpeaker {
        fn synthesize<'a>(&'a self, text: &'a str) -> BoxFuture<'a, Result<Vec<u8>, SpeakError>> {
            let b = format!("ID3{}", &text[..text.len().min(40)]).into_bytes();
            Box::pin(async move { Ok(b) })
        }
    }

    #[test]
    fn tipo_do_audio_reconhece_cabecalhos_e_recusa_lixo() {
        assert_eq!(tipo_do_audio(b"RIFF\x00\x00\x00\x00WAVEfmt "), Some(("voz.wav", "audio/wav")));
        assert_eq!(tipo_do_audio(b"OggS\x00\x02"), Some(("voz.ogg", "audio/ogg")));
        assert_eq!(tipo_do_audio(b"\x1a\x45\xdf\xa3\x93"), Some(("voz.webm", "audio/webm")));
        assert_eq!(tipo_do_audio(b"\x00\x00\x00\x18ftypM4A "), Some(("voz.m4a", "audio/mp4")));
        assert_eq!(tipo_do_audio(b"ID3\x04\x00"), Some(("voz.mp3", "audio/mpeg")));
        assert_eq!(tipo_do_audio(b"<html>nao e audio"), None);
    }

    #[test]
    fn falavel_tira_codigo_e_links_e_corta() {
        let bruto = "Veja [o guia](https://exemplo.com) e rode:\n```sh\nrm -rf /\n```\n**pronto**.";
        let limpo = falavel(bruto, 1500);
        assert!(limpo.contains("Veja o guia"));
        assert!(limpo.contains("(código na tela)"));
        assert!(!limpo.contains("```"));
        assert!(!limpo.contains("https://"));
        assert!(!limpo.contains("**"));
        assert!(falavel(&"palavra ".repeat(400), 80).chars().count() <= 81);
    }

    #[tokio::test]
    async fn modo_panico_corta_a_voz_antes_de_transcrever() {
        let store = Arc::new(Store::open(":memory:").unwrap());
        let modos = Arc::new(Modos::new(store.clone(), "", None));
        modos.entrar_panico("teste").unwrap();
        let gw = Arc::new(ChatGateway::new(vec![Endpoint::new("g", "http://127.0.0.1:9", "m")]).unwrap());
        let policy = Arc::new(PolicyEngine::new(PathGuard::default()).with_approvals(Arc::new(ApprovalStore::new(60, orion_policy::wall_clock()))));
        let ag = Arc::new(Agent::new(gw, crate::tools::ToolRegistry::new(vec![]).unwrap(), policy, store, Default::default()));
        let ws = MemWs::new();
        turno_de_voz(&ws, b"OggS\x00\x02dados".to_vec(), Some(ag), None, Some(Arc::new(FakeSpeaker)), Some(modos), Arc::new(VozStats::default()), true, None).await;
        let msgs = ws.mensagens.lock();
        assert!(msgs.iter().any(|m| m["type"] == "error" && m["message"].as_str().unwrap_or("").to_lowercase().contains("pânico")));
    }
}
