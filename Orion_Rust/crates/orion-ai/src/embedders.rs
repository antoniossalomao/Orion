//! Embeddings pela API gratuita do Gemini, com lote, nova tentativa e espera crescente.
//!
//! Quando a API cai ou a cota acaba, o erro sobe e a memória segue só por palavra-chave: o que
//! faltou entra depois em `embed_pending`. A chave vai em cabeçalho, nunca na URL.

use crate::saidas;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

pub const GEMINI_BASE: &str = "https://generativelanguage.googleapis.com/v1beta";
/// Limite do batchEmbedContents.
pub const MAX_LOTE: usize = 100;
const RETENTAVEL: [u16; 5] = [429, 500, 502, 503, 504];

pub struct GeminiEmbedder {
    pub dim: usize,
    model: String,
    key: String,
    base: String,
    client: reqwest::blocking::Client,
    retries: u32,
    backoff_s: f64,
}

impl GeminiEmbedder {
    pub fn new(api_key: &str, model: &str, dim: usize) -> anyhow::Result<Self> {
        anyhow::ensure!(!api_key.is_empty(), "chave da API de embeddings vazia");
        Ok(GeminiEmbedder {
            dim,
            model: model.into(),
            key: api_key.into(),
            base: GEMINI_BASE.into(),
            client: reqwest::blocking::Client::builder().timeout(Duration::from_secs(30)).build()?,
            retries: 3,
            backoff_s: 1.0,
        })
    }

    pub fn com_base(mut self, base: &str, retries: u32, backoff_s: f64) -> Self {
        self.base = base.into();
        self.retries = retries;
        self.backoff_s = backoff_s;
        self
    }

    fn embed_task(&self, texts: &[String], tarefa: &str) -> anyhow::Result<Vec<Vec<f32>>> {
        let mut saida = vec![];
        for lote in texts.chunks(MAX_LOTE) {
            saida.extend(self.lote(lote, tarefa)?);
        }
        Ok(saida)
    }

    fn lote(&self, texts: &[String], tarefa: &str) -> anyhow::Result<Vec<Vec<f32>>> {
        let corpo = json!({"requests": texts.iter().map(|t| json!({
            "model": format!("models/{}", self.model),
            "content": {"parts": [{"text": t}]},
            "taskType": tarefa,
            "outputDimensionality": self.dim,
        })).collect::<Vec<_>>()});
        let url = format!("{}/models/{}:batchEmbedContents", self.base.trim_end_matches('/'), self.model);
        let enviados = corpo.to_string().len();
        for tentativa in 0..=self.retries {
            let inicio = Instant::now();
            let r = self.client.post(&url).header("x-goog-api-key", &self.key).json(&corpo).send();
            let (erro, retry_after) = match r {
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    let ra = resp.headers().get("retry-after").and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<f64>().ok());
                    let bytes = resp.bytes().unwrap_or_default();
                    // cada tentativa conta na cota (REGRAS.md: toda saída de dado fica registrada)
                    saidas::registrar("gemini", "embed", status < 400, inicio.elapsed().as_secs_f64() * 1000.0, &self.model, enviados, bytes.len(), "texto");
                    if status < 400 {
                        return self.vetores(&bytes, texts.len());
                    }
                    if !RETENTAVEL.contains(&status) {
                        let curto: String = String::from_utf8_lossy(&bytes).chars().take(200).collect();
                        anyhow::bail!("embeddings recusados (HTTP {status}): {curto}");
                    }
                    (format!("HTTP {status}"), ra.unwrap_or(0.0))
                }
                Err(e) => {
                    saidas::registrar("gemini", "embed", false, inicio.elapsed().as_secs_f64() * 1000.0, &self.model, enviados, 0, "texto");
                    (if e.is_timeout() { "ReadTimeout".into() } else { "ConnectError".into() }, 0.0)
                }
            };
            if tentativa == self.retries {
                anyhow::bail!("embeddings indisponíveis depois de {} tentativas ({erro})", tentativa + 1);
            }
            let espera = (self.backoff_s * 2f64.powi(tentativa as i32)).max(retry_after);
            tracing::warn!("embeddings: {erro}, nova tentativa em {espera:.1}s");
            std::thread::sleep(Duration::from_secs_f64(espera));
        }
        anyhow::bail!("inalcançável")
    }

    fn vetores(&self, corpo: &[u8], esperados: usize) -> anyhow::Result<Vec<Vec<f32>>> {
        let v: Value = serde_json::from_slice(corpo).map_err(|e| anyhow::anyhow!("resposta de embeddings fora do formato: {e}"))?;
        let lista = v["embeddings"].as_array().ok_or_else(|| anyhow::anyhow!("resposta de embeddings fora do formato: 'embeddings'"))?;
        let vetores: Vec<Vec<f32>> = lista
            .iter()
            .map(|e| e["values"].as_array().map(|a| a.iter().filter_map(Value::as_f64).map(|x| x as f32).collect()).unwrap_or_default())
            .collect();
        if vetores.len() != esperados || vetores.iter().any(|v| v.len() != self.dim) {
            anyhow::bail!("esperava {esperados} vetores de {} dimensões, veio {}", self.dim, vetores.len());
        }
        Ok(vetores)
    }
}

impl orion_store::Embedder for GeminiEmbedder {
    fn dim(&self) -> usize {
        self.dim
    }
    fn embed(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
        self.embed_task(texts, "RETRIEVAL_DOCUMENT")
    }
    fn embed_query(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        Ok(self.embed_task(&[text.to_string()], "RETRIEVAL_QUERY")?.remove(0))
    }
}
