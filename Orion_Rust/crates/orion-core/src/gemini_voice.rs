//! Voz do Orion: Gemini TTS com a voz Charon em tom calmo (decisão de 13/11/2026 com o Antônio).
//!
//! - Chave própria (`ORION_VOZ_GEMINI_KEY`), separada de `ORION_KEY_GEMINI`: o gateway do chat nunca
//!   usa a chave da voz, então as respostas de texto não gastam o crédito da voz.
//! - Sem reserva: se a chave faltar, o crédito do mês acabar ou o Google recusar, a fala NÃO cai para
//!   outra voz. O erro diz que a voz está indisponível e fica em [`aviso`] para o app mostrar.
//! - Teto mensal em dólares (`ORION_VOICE_TTS_BUDGET_USD`, padrão 10), contado pelo áudio gerado:
//!   25 tokens por segundo, US$ 9 por milhão até 31/12/2026 e US$ 18 depois.
//! - O tom vai como marcação entre colchetes antes do texto. Instrução em frase ("Diga com voz
//!   calma:") é lida em voz alta pelo modelo e `systemInstruction` não é aceito.
use crate::voice::{falavel, SpeakError, Speaker, MAX_TTS_CHARS};
use base64::Engine;
use chrono::{Datelike, Utc};
use futures::future::BoxFuture;
use orion_ai::saidas::medir;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::LazyLock;
use std::time::Duration;

pub const SECRET_NAME: &str = "ORION_VOZ_GEMINI_KEY";
pub const MODEL: &str = "gemini-3.8-flash-tts";
pub const VOICE: &str = "Charon";
pub const TONE: &str = "[calmly]";
const BASE: &str = "https://generativelanguage.googleapis.com";
const TOKENS_PER_SECOND: f64 = 25.0;

/// Preço do áudio de saída por milhão de tokens na data (`ano`). O Google dobra em 2027.
pub fn price_per_million(year: i32) -> f64 {
    if year <= 2026 { 9.0 } else { 18.0 }
}

pub fn cost_usd(seconds: f64, year: i32) -> f64 {
    seconds * TOKENS_PER_SECOND * price_per_million(year) / 1_000_000.0
}

/// Último motivo de voz indisponível; `seq` muda a cada aviso novo para o app mostrar uma vez só.
static NOTICE: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));
static NOTICE_SEQ: AtomicU64 = AtomicU64::new(0);

pub fn notice() -> (u64, Option<String>) {
    (NOTICE_SEQ.load(Ordering::SeqCst), NOTICE.lock().clone())
}

fn unavailable(reason: &str) -> SpeakError {
    let msg = format!("A voz está indisponível: {reason}");
    *NOTICE.lock() = Some(msg.clone());
    NOTICE_SEQ.fetch_add(1, Ordering::SeqCst);
    SpeakError(msg)
}

fn clear_notice() {
    let mut a = NOTICE.lock();
    if a.is_some() {
        *a = None;
        NOTICE_SEQ.fetch_add(1, Ordering::SeqCst);
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Spend {
    pub month: String,
    pub seconds: f64,
    pub usd: f64,
}

fn current_month() -> String {
    let d = Utc::now();
    format!("{:04}-{:02}", d.year(), d.month())
}

pub struct GeminiSpeaker {
    budget_usd: f64,
    file: PathBuf,
    spend: Mutex<Spend>,
    /// Dólares previstos das falas em andamento: entram na conta antes de chamar o Google, para
    /// falas simultâneas (Telegram e leitura em voz alta, por exemplo) não passarem do teto.
    reserved: Mutex<f64>,
    client: reqwest::Client,
    base: String,
    key: Box<dyn Fn() -> Option<String> + Send + Sync>,
}

impl GeminiSpeaker {
    /// `pasta` guarda `voz-gasto.json` (só segundos e dólares do mês, nunca texto).
    pub fn new(dir: &std::path::Path, budget_usd: f64) -> Self {
        Self::with_parts(dir, budget_usd, BASE.into(), Box::new(|| orion_ai::secrets::get_secret(SECRET_NAME).filter(|k| !k.trim().is_empty())))
    }

    pub fn with_parts(dir: &std::path::Path, budget_usd: f64, base: String, key: Box<dyn Fn() -> Option<String> + Send + Sync>) -> Self {
        let file = dir.join("voz-gasto.json");
        let spend = std::fs::read(&file).ok().and_then(|b| serde_json::from_slice::<Spend>(&b).ok()).unwrap_or_default();
        GeminiSpeaker {
            budget_usd,
            file,
            spend: Mutex::new(spend),
            reserved: Mutex::new(0.0),
            client: reqwest::Client::builder().timeout(Duration::from_secs(60)).build().unwrap_or_default(),
            base,
            key,
        }
    }

    /// Gasto do mês corrente (zera ao virar o mês).
    pub fn spend(&self) -> Spend {
        let mut g = self.spend.lock();
        let month = current_month();
        if g.month != month {
            *g = Spend { month, ..Default::default() };
        }
        g.clone()
    }

    pub fn budget_usd(&self) -> f64 {
        self.budget_usd
    }

    fn record(&self, seconds: f64) {
        let mut g = self.spend.lock();
        let month = current_month();
        if g.month != month {
            *g = Spend { month, ..Default::default() };
        }
        g.seconds += seconds;
        g.usd += cost_usd(seconds, Utc::now().year());
        if let Ok(b) = serde_json::to_vec(&*g) {
            let tmp = self.file.with_extension("json.tmp");
            if std::fs::write(&tmp, b).is_ok() {
                let _ = std::fs::rename(&tmp, &self.file);
            }
        }
    }

    async fn generate(&self, text: &str) -> Result<Vec<u8>, SpeakError> {
        let Some(key) = (self.key)() else {
            return Err(unavailable("falta a chave da voz (Configurações › Voz › Chave da voz)."));
        };
        // reserva o custo previsto (~15 caracteres por segundo de fala, com folga) junto com a checagem.
        // O teto da fala vai até 3x o previsto, mas nunca além do que sobra no mês: vira
        // `maxOutputTokens`, então fala lenta ou números por extenso não estouram o teto mensal.
        let year = Utc::now().year();
        let estimate = cost_usd(text.chars().count() as f64 / 15.0 * 1.3 + 1.0, year);
        let cap = {
            let g = self.spend();
            let mut r = self.reserved.lock();
            let left = self.budget_usd - g.usd - *r;
            if estimate > left {
                return Err(unavailable(&format!("o crédito de voz deste mês (US$ {:.2}) acabou. Volta no dia 1º.", self.budget_usd)));
            }
            let cap = (estimate * 3.0).min(left);
            *r += cap;
            cap
        };
        // devolve a reserva em qualquer saída, inclusive quando a fala é cancelada no meio
        let _reservation = Reservation { total: &self.reserved, amount: cap };
        let max_tokens = ((cap / price_per_million(year) * 1_000_000.0).floor() as u64).max(1);
        self.call(&key, text, max_tokens).await
    }

    async fn call(&self, key: &str, text: &str, max_tokens: u64) -> Result<Vec<u8>, SpeakError> {
        let body = json!({
            "contents": [{"parts": [{"text": format!("{TONE} {text}")}]}],
            "generationConfig": {
                "responseModalities": ["AUDIO"],
                "maxOutputTokens": max_tokens,
                "speechConfig": {"voiceConfig": {"prebuiltVoiceConfig": {"voiceName": VOICE}}}
            }
        });
        let mut m = medir("gemini-tts", "tts", MODEL, text.len(), "texto");
        let resp = self
            .client
            .post(format!("{}/v1beta/models/{MODEL}:generateContent", self.base))
            .header("x-goog-api-key", key)
            .json(&body)
            .send()
            .await
            .map_err(|_| unavailable("o Google não respondeu. Confira a internet."))?;
        let status = resp.status().as_u16();
        let body: Value = resp.json().await.unwrap_or(Value::Null);
        match status {
            200 => {}
            429 => return Err(unavailable("o limite da API de voz foi atingido (cota ou crédito do Google). Tente mais tarde.")),
            400 | 401 | 403 => {
                let detail = body["error"]["message"].as_str().unwrap_or("");
                return Err(unavailable(if detail.to_lowercase().contains("billing") || detail.to_lowercase().contains("credit") {
                    "o crédito da conta Google da voz acabou."
                } else {
                    "o Google recusou a chave da voz."
                }));
            }
            503 => return Err(unavailable("o Google está sobrecarregado agora. Tente de novo em instantes.")),
            n => return Err(unavailable(&format!("o Google devolveu um erro (HTTP {n})."))),
        }
        let part = body["candidates"][0]["content"]["parts"]
            .as_array()
            .and_then(|ps| ps.iter().find(|p| p["inlineData"]["data"].is_string()))
            .ok_or_else(|| unavailable("o Google não devolveu áudio."))?;
        let mime = part["inlineData"]["mimeType"].as_str().unwrap_or("").to_lowercase();
        let data = base64::engine::general_purpose::STANDARD
            .decode(part["inlineData"]["data"].as_str().unwrap_or(""))
            .map_err(|_| unavailable("o áudio devolvido veio corrompido."))?;
        let (wav, seconds) = to_wav(&data, &mime).ok_or_else(|| unavailable("formato de áudio desconhecido."))?;
        m.ok = true;
        m.bytes_in = wav.len();
        self.record(seconds);
        clear_notice();
        Ok(wav)
    }
}

/// Libera a reserva do custo previsto ao sair de `gerar`, mesmo se o futuro for abortado.
struct Reservation<'a> {
    total: &'a Mutex<f64>,
    amount: f64,
}

impl Drop for Reservation<'_> {
    fn drop(&mut self) {
        let mut r = self.total.lock();
        *r = (*r - self.amount).max(0.0);
    }
}

impl Speaker for GeminiSpeaker {
    fn synthesize<'a>(&'a self, text: &'a str) -> BoxFuture<'a, Result<Vec<u8>, SpeakError>> {
        Box::pin(async move {
            let clean = falavel(text, MAX_TTS_CHARS);
            if clean.is_empty() {
                return Err(SpeakError("texto vazio para síntese".into()));
            }
            self.generate(&clean).await
        })
    }
}

/// Devolve WAV e a duração. O Gemini manda WAV pronto (`audio/wav`) ou PCM 16 bits cru
/// (`audio/L16;rate=24000`), que ganha cabeçalho aqui.
pub fn to_wav(data: &[u8], mime: &str) -> Option<(Vec<u8>, f64)> {
    if data.len() >= 44 && &data[..4] == b"RIFF" && &data[8..12] == b"WAVE" {
        let channels = u16::from_le_bytes([data[22], data[23]]).max(1) as f64;
        let rate = u32::from_le_bytes([data[24], data[25], data[26], data[27]]).max(1) as f64;
        let bits = u16::from_le_bytes([data[34], data[35]]).max(8) as f64;
        let secs = (data.len() - 44) as f64 / (rate * channels * bits / 8.0);
        return Some((data.to_vec(), secs));
    }
    let mime = mime.to_lowercase();
    if mime.starts_with("audio/l16") || mime.starts_with("audio/pcm") {
        let rate = mime
            .split(';')
            .find_map(|p| p.trim().strip_prefix("rate=").and_then(|r| r.parse::<u32>().ok()))
            .unwrap_or(24_000);
        let mut w = Vec::with_capacity(data.len() + 44);
        w.extend_from_slice(b"RIFF");
        w.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
        w.extend_from_slice(b"WAVEfmt ");
        w.extend_from_slice(&16u32.to_le_bytes());
        w.extend_from_slice(&1u16.to_le_bytes());
        w.extend_from_slice(&1u16.to_le_bytes());
        w.extend_from_slice(&rate.to_le_bytes());
        w.extend_from_slice(&(rate * 2).to_le_bytes());
        w.extend_from_slice(&2u16.to_le_bytes());
        w.extend_from_slice(&16u16.to_le_bytes());
        w.extend_from_slice(b"data");
        w.extend_from_slice(&(data.len() as u32).to_le_bytes());
        w.extend_from_slice(data);
        return Some((w, data.len() as f64 / (rate as f64 * 2.0)));
    }
    None
}
