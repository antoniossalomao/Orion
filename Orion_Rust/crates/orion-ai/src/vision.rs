//! Visão: pergunta sobre uma imagem ao mesmo gateway de modelos (API compatível com a da OpenAI).
//!
//! Uma imagem, uma pergunta, um texto. O texto que volta é conteúdo não confiável
//! (REGRAS.md: conteúdo externo é dado, não instrução). Chave só em cabeçalho; nenhum erro leva a
//! URL nem a chave. A imagem que sai fica no registro de saídas.

use crate::gateway::Endpoint;
use crate::saidas;
use base64::Engine as _;
use serde_json::{json, Value};
use std::time::Duration;

/// Bytes; imagem maior que isso o provedor gratuito costuma recusar.
pub const MAX_IMAGEM: usize = 6 * 1024 * 1024;
pub const MAX_RESPOSTA: usize = 6000;
pub const PERGUNTA_PADRAO: &str = "Descreva o que você vê, em português, e transcreva o texto legível.";

pub fn mime(ext: &str) -> Option<&'static str> {
    match ext {
        ".jpg" | ".jpeg" => Some("image/jpeg"),
        ".png" => Some("image/png"),
        ".webp" => Some("image/webp"),
        ".gif" => Some("image/gif"),
        _ => None,
    }
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("{0}")]
pub struct VisionError(pub String);

pub struct Vision {
    pub endpoints: Vec<Endpoint>,
}

impl Vision {
    pub fn new(endpoints: Vec<Endpoint>) -> anyhow::Result<Self> {
        anyhow::ensure!(!endpoints.is_empty(), "ao menos um endpoint");
        Ok(Vision { endpoints })
    }

    /// Síncrono (as ferramentas rodam fora do laço assíncrono).
    pub fn describe(&self, imagem: &[u8], mime: &str, pergunta: &str) -> Result<String, VisionError> {
        if imagem.is_empty() {
            return Err(VisionError("imagem vazia".into()));
        }
        if imagem.len() > MAX_IMAGEM {
            return Err(VisionError(format!("imagem passa de {} MB", MAX_IMAGEM / 1024 / 1024)));
        }
        let url = format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(imagem));
        let p: String = pergunta.trim().chars().take(2000).collect();
        let mensagens = json!([{"role": "user", "content": [
            {"type": "text", "text": if p.is_empty() { PERGUNTA_PADRAO.to_string() } else { p }},
            {"type": "image_url", "image_url": {"url": url}},
        ]}]);
        let mut falhas = vec![];
        let cliente = reqwest::blocking::Client::new();
        for ep in &self.endpoints {
            match Self::perguntar(&cliente, ep, &mensagens) {
                Ok(t) => return Ok(t),
                Err(e) => falhas.push(format!("{}: {e}", ep.name)),
            }
        }
        Err(VisionError(if falhas.is_empty() { "nenhum endpoint respondeu".into() } else { falhas.join("; ") }))
    }

    fn perguntar(cliente: &reqwest::blocking::Client, ep: &Endpoint, mensagens: &Value) -> Result<String, VisionError> {
        let corpo = json!({"model": ep.model, "messages": mensagens, "stream": false});
        let txt = corpo.to_string();
        let mut m = saidas::medir("gateway:visao", "vision", &ep.model, txt.len(), "imagem");
        let mut req = cliente
            .post(format!("{}/chat/completions", ep.base_url.trim_end_matches('/')))
            .header("content-type", "application/json")
            .timeout(Duration::from_secs_f64(ep.timeout_s))
            .body(txt);
        if let Some(k) = &ep.api_key {
            req = req.bearer_auth(k);
        }
        // a mensagem do cliente HTTP leva a URL: só o tipo do erro
        let resp = req.send().map_err(|e| VisionError(if e.is_timeout() { "ReadTimeout".into() } else { "ConnectError".into() }))?;
        let status = resp.status().as_u16();
        let bytes = resp.bytes().unwrap_or_default();
        m.bytes_in = bytes.len();
        if status >= 400 {
            return Err(VisionError(format!("HTTP {status}")));
        }
        let v: Value = serde_json::from_slice(&bytes).map_err(|_| VisionError("resposta em formato inesperado".into()))?;
        let c = &v["choices"][0]["message"]["content"];
        let texto = match c {
            Value::String(s) => s.clone(),
            Value::Array(partes) => partes.iter().filter_map(|p| p["text"].as_str()).collect(),
            Value::Null => String::new(),
            _ => return Err(VisionError("resposta em formato inesperado".into())),
        };
        let texto = texto.trim().to_string();
        if texto.is_empty() {
            return Err(VisionError("resposta vazia (o modelo aceita imagem?)".into()));
        }
        m.ok = true;
        Ok(texto.chars().take(MAX_RESPOSTA).collect())
    }
}
