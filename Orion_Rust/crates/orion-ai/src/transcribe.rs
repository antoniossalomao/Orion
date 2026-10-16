//! Transcrição de voz por API gratuita (Whisper no Groq).
//!
//! Formato da API de transcrição da OpenAI (`POST /audio/transcriptions`, multipart). A chave vai
//! só no cabeçalho; nenhuma mensagem de erro leva a chave nem a URL. O áudio que sai fica no
//! registro de saídas.

use crate::saidas;
use std::time::Duration;

pub const GROQ: &str = "https://api.groq.com/openai/v1";
/// O limite de download de arquivo do bot do Telegram.
pub const MAX_AUDIO: usize = 20 * 1024 * 1024;

#[derive(Debug, Clone, thiserror::Error)]
#[error("{0}")]
pub struct TranscribeError(pub String);

#[derive(Clone)]
pub struct Transcriber {
    key: String,
    url: String,
    provedor: String,
    model: String,
    language: String,
    timeout: Duration,
}

impl Transcriber {
    pub fn new(api_key: &str, base_url: &str, model: &str) -> anyhow::Result<Self> {
        anyhow::ensure!(!api_key.is_empty(), "chave de transcrição vazia");
        Ok(Transcriber {
            key: api_key.into(),
            url: format!("{}/audio/transcriptions", base_url.trim_end_matches('/')),
            provedor: saidas::provedor_da_url(base_url, ""),
            model: model.into(),
            language: "pt".into(),
            timeout: Duration::from_secs(90),
        })
    }

    fn checar(audio: &[u8]) -> Result<(), TranscribeError> {
        if audio.is_empty() {
            return Err(TranscribeError("áudio vazio".into()));
        }
        if audio.len() > MAX_AUDIO {
            return Err(TranscribeError("áudio grande demais".into()));
        }
        Ok(())
    }

    fn texto(status: u16, corpo: &[u8]) -> Result<String, TranscribeError> {
        if status >= 400 {
            return Err(TranscribeError(format!("a transcrição falhou (HTTP {status})")));
        }
        let v: serde_json::Value = serde_json::from_slice(corpo).map_err(|_| TranscribeError("resposta da transcrição inválida".into()))?;
        let t = v.get("text").and_then(|t| t.as_str()).unwrap_or("").trim().to_string();
        if t.is_empty() {
            return Err(TranscribeError("não entendi nada neste áudio".into()));
        }
        Ok(t)
    }

    pub async fn transcribe(&self, audio: Vec<u8>, filename: &str, mime: &str) -> Result<String, TranscribeError> {
        Self::checar(&audio)?;
        let mut m = saidas::medir(&self.provedor, "transcribe", &self.model, audio.len(), "audio");
        let parte = reqwest::multipart::Part::bytes(audio).file_name(filename.to_string()).mime_str(mime).map_err(|_| TranscribeError("tipo de áudio inválido".into()))?;
        let form = reqwest::multipart::Form::new()
            .text("model", self.model.clone())
            .text("language", self.language.clone())
            .text("response_format", "json")
            .text("temperature", "0")
            .part("file", parte);
        let r = reqwest::Client::new()
            .post(&self.url)
            .bearer_auth(&self.key)
            .multipart(form)
            .timeout(self.timeout)
            .send()
            .await
            .map_err(|e| TranscribeError(format!("a transcrição falhou: {}", if e.is_timeout() { "ReadTimeout" } else { "ConnectError" })))?;
        let status = r.status().as_u16();
        let corpo = r.bytes().await.unwrap_or_default();
        m.bytes_in = corpo.len();
        let t = Self::texto(status, &corpo)?;
        m.ok = true;
        Ok(t)
    }

    /// Mesma coisa para ferramentas síncronas.
    pub fn transcribe_sync(&self, audio: Vec<u8>, filename: &str, mime: &str) -> Result<String, TranscribeError> {
        Self::checar(&audio)?;
        let mut m = saidas::medir(&self.provedor, "transcribe", &self.model, audio.len(), "audio");
        let parte = reqwest::blocking::multipart::Part::bytes(audio).file_name(filename.to_string()).mime_str(mime).map_err(|_| TranscribeError("tipo de áudio inválido".into()))?;
        let form = reqwest::blocking::multipart::Form::new()
            .text("model", self.model.clone())
            .text("language", self.language.clone())
            .text("response_format", "json")
            .text("temperature", "0")
            .part("file", parte);
        let r = reqwest::blocking::Client::new()
            .post(&self.url)
            .bearer_auth(&self.key)
            .multipart(form)
            .timeout(self.timeout)
            .send()
            .map_err(|e| TranscribeError(format!("a transcrição falhou: {}", if e.is_timeout() { "ReadTimeout" } else { "ConnectError" })))?;
        let status = r.status().as_u16();
        let corpo = r.bytes().unwrap_or_default();
        m.bytes_in = corpo.len();
        let t = Self::texto(status, &corpo)?;
        m.ok = true;
        Ok(t)
    }
}
