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

pub const SEGREDO: &str = "ORION_VOZ_GEMINI_KEY";
pub const MODELO: &str = "gemini-3.8-flash-tts";
pub const VOZ: &str = "Charon";
pub const TOM: &str = "[calmly]";
const BASE: &str = "https://generativelanguage.googleapis.com";
const TOKENS_POR_SEGUNDO: f64 = 25.0;

/// Preço do áudio de saída por milhão de tokens na data (`ano`). O Google dobra em 2027.
pub fn preco_por_milhao(ano: i32) -> f64 {
    if ano <= 2026 { 9.0 } else { 18.0 }
}

pub fn custo_usd(segundos: f64, ano: i32) -> f64 {
    segundos * TOKENS_POR_SEGUNDO * preco_por_milhao(ano) / 1_000_000.0
}

/// Último motivo de voz indisponível; `seq` muda a cada aviso novo para o app mostrar uma vez só.
static AVISO: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));
static AVISO_SEQ: AtomicU64 = AtomicU64::new(0);

pub fn aviso() -> (u64, Option<String>) {
    (AVISO_SEQ.load(Ordering::SeqCst), AVISO.lock().clone())
}

fn indisponivel(motivo: &str) -> SpeakError {
    let msg = format!("A voz está indisponível: {motivo}");
    *AVISO.lock() = Some(msg.clone());
    AVISO_SEQ.fetch_add(1, Ordering::SeqCst);
    SpeakError(msg)
}

fn limpar_aviso() {
    let mut a = AVISO.lock();
    if a.is_some() {
        *a = None;
        AVISO_SEQ.fetch_add(1, Ordering::SeqCst);
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Gasto {
    pub mes: String,
    pub segundos: f64,
    pub usd: f64,
}

fn mes_atual() -> String {
    let d = Utc::now();
    format!("{:04}-{:02}", d.year(), d.month())
}

pub struct GeminiSpeaker {
    orcamento_usd: f64,
    arquivo: PathBuf,
    gasto: Mutex<Gasto>,
    /// Dólares previstos das falas em andamento: entram na conta antes de chamar o Google, para
    /// falas simultâneas (Telegram e leitura em voz alta, por exemplo) não passarem do teto.
    reservado: Mutex<f64>,
    client: reqwest::Client,
    base: String,
    chave: Box<dyn Fn() -> Option<String> + Send + Sync>,
}

impl GeminiSpeaker {
    /// `pasta` guarda `voz-gasto.json` (só segundos e dólares do mês, nunca texto).
    pub fn new(pasta: &std::path::Path, orcamento_usd: f64) -> Self {
        Self::com(pasta, orcamento_usd, BASE.into(), Box::new(|| orion_ai::secrets::get_secret(SEGREDO).filter(|k| !k.trim().is_empty())))
    }

    pub fn com(pasta: &std::path::Path, orcamento_usd: f64, base: String, chave: Box<dyn Fn() -> Option<String> + Send + Sync>) -> Self {
        let arquivo = pasta.join("voz-gasto.json");
        let gasto = std::fs::read(&arquivo).ok().and_then(|b| serde_json::from_slice::<Gasto>(&b).ok()).unwrap_or_default();
        GeminiSpeaker {
            orcamento_usd,
            arquivo,
            gasto: Mutex::new(gasto),
            reservado: Mutex::new(0.0),
            client: reqwest::Client::builder().timeout(Duration::from_secs(60)).build().unwrap_or_default(),
            base,
            chave,
        }
    }

    /// Gasto do mês corrente (zera ao virar o mês).
    pub fn gasto(&self) -> Gasto {
        let mut g = self.gasto.lock();
        let mes = mes_atual();
        if g.mes != mes {
            *g = Gasto { mes, ..Default::default() };
        }
        g.clone()
    }

    pub fn orcamento_usd(&self) -> f64 {
        self.orcamento_usd
    }

    fn registrar(&self, segundos: f64) {
        let mut g = self.gasto.lock();
        let mes = mes_atual();
        if g.mes != mes {
            *g = Gasto { mes, ..Default::default() };
        }
        g.segundos += segundos;
        g.usd += custo_usd(segundos, Utc::now().year());
        if let Ok(b) = serde_json::to_vec(&*g) {
            let tmp = self.arquivo.with_extension("json.tmp");
            if std::fs::write(&tmp, b).is_ok() {
                let _ = std::fs::rename(&tmp, &self.arquivo);
            }
        }
    }

    async fn gerar(&self, texto: &str) -> Result<Vec<u8>, SpeakError> {
        let Some(chave) = (self.chave)() else {
            return Err(indisponivel("falta a chave da voz (Configurações › Voz › Chave da voz)."));
        };
        // reserva o custo previsto (~15 caracteres por segundo de fala, com folga) junto com a checagem
        let previsto = custo_usd(texto.chars().count() as f64 / 15.0 * 1.3 + 1.0, Utc::now().year());
        {
            let g = self.gasto();
            let mut r = self.reservado.lock();
            if g.usd + *r + previsto > self.orcamento_usd {
                return Err(indisponivel(&format!("o crédito de voz deste mês (US$ {:.2}) acabou. Volta no dia 1º.", self.orcamento_usd)));
            }
            *r += previsto;
        }
        let resultado = self.chamar(&chave, texto).await;
        {
            let mut r = self.reservado.lock();
            *r = (*r - previsto).max(0.0);
        }
        resultado
    }

    async fn chamar(&self, chave: &str, texto: &str) -> Result<Vec<u8>, SpeakError> {
        let corpo = json!({
            "contents": [{"parts": [{"text": format!("{TOM} {texto}")}]}],
            "generationConfig": {
                "responseModalities": ["AUDIO"],
                "speechConfig": {"voiceConfig": {"prebuiltVoiceConfig": {"voiceName": VOZ}}}
            }
        });
        let mut m = medir("gemini-tts", "tts", MODELO, texto.len(), "texto");
        let resp = self
            .client
            .post(format!("{}/v1beta/models/{MODELO}:generateContent", self.base))
            .header("x-goog-api-key", chave)
            .json(&corpo)
            .send()
            .await
            .map_err(|_| indisponivel("o Google não respondeu. Confira a internet."))?;
        let status = resp.status().as_u16();
        let body: Value = resp.json().await.unwrap_or(Value::Null);
        match status {
            200 => {}
            429 => return Err(indisponivel("o limite da API de voz foi atingido (cota ou crédito do Google). Tente mais tarde.")),
            400 | 401 | 403 => {
                let detalhe = body["error"]["message"].as_str().unwrap_or("");
                return Err(indisponivel(if detalhe.to_lowercase().contains("billing") || detalhe.to_lowercase().contains("credit") {
                    "o crédito da conta Google da voz acabou."
                } else {
                    "o Google recusou a chave da voz."
                }));
            }
            503 => return Err(indisponivel("o Google está sobrecarregado agora. Tente de novo em instantes.")),
            n => return Err(indisponivel(&format!("o Google devolveu um erro (HTTP {n})."))),
        }
        let parte = body["candidates"][0]["content"]["parts"]
            .as_array()
            .and_then(|ps| ps.iter().find(|p| p["inlineData"]["data"].is_string()))
            .ok_or_else(|| indisponivel("o Google não devolveu áudio."))?;
        let mime = parte["inlineData"]["mimeType"].as_str().unwrap_or("").to_lowercase();
        let dados = base64::engine::general_purpose::STANDARD
            .decode(parte["inlineData"]["data"].as_str().unwrap_or(""))
            .map_err(|_| indisponivel("o áudio devolvido veio corrompido."))?;
        let (wav, segundos) = para_wav(&dados, &mime).ok_or_else(|| indisponivel("formato de áudio desconhecido."))?;
        m.ok = true;
        m.bytes_in = wav.len();
        self.registrar(segundos);
        limpar_aviso();
        Ok(wav)
    }
}

impl Speaker for GeminiSpeaker {
    fn synthesize<'a>(&'a self, text: &'a str) -> BoxFuture<'a, Result<Vec<u8>, SpeakError>> {
        Box::pin(async move {
            let limpo = falavel(text, MAX_TTS_CHARS);
            if limpo.is_empty() {
                return Err(SpeakError("texto vazio para síntese".into()));
            }
            self.gerar(&limpo).await
        })
    }
}

/// Devolve WAV e a duração. O Gemini manda WAV pronto (`audio/wav`) ou PCM 16 bits cru
/// (`audio/L16;rate=24000`), que ganha cabeçalho aqui.
pub fn para_wav(dados: &[u8], mime: &str) -> Option<(Vec<u8>, f64)> {
    if dados.len() >= 44 && &dados[..4] == b"RIFF" && &dados[8..12] == b"WAVE" {
        let canais = u16::from_le_bytes([dados[22], dados[23]]).max(1) as f64;
        let taxa = u32::from_le_bytes([dados[24], dados[25], dados[26], dados[27]]).max(1) as f64;
        let bits = u16::from_le_bytes([dados[34], dados[35]]).max(8) as f64;
        let seg = (dados.len() - 44) as f64 / (taxa * canais * bits / 8.0);
        return Some((dados.to_vec(), seg));
    }
    let mime = mime.to_lowercase();
    if mime.starts_with("audio/l16") || mime.starts_with("audio/pcm") {
        let taxa = mime
            .split(';')
            .find_map(|p| p.trim().strip_prefix("rate=").and_then(|r| r.parse::<u32>().ok()))
            .unwrap_or(24_000);
        let mut w = Vec::with_capacity(dados.len() + 44);
        w.extend_from_slice(b"RIFF");
        w.extend_from_slice(&(36 + dados.len() as u32).to_le_bytes());
        w.extend_from_slice(b"WAVEfmt ");
        w.extend_from_slice(&16u32.to_le_bytes());
        w.extend_from_slice(&1u16.to_le_bytes());
        w.extend_from_slice(&1u16.to_le_bytes());
        w.extend_from_slice(&taxa.to_le_bytes());
        w.extend_from_slice(&(taxa * 2).to_le_bytes());
        w.extend_from_slice(&2u16.to_le_bytes());
        w.extend_from_slice(&16u16.to_le_bytes());
        w.extend_from_slice(b"data");
        w.extend_from_slice(&(dados.len() as u32).to_le_bytes());
        w.extend_from_slice(dados);
        return Some((w, dados.len() as f64 / (taxa as f64 * 2.0)));
    }
    None
}
