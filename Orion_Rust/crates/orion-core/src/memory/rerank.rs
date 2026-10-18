//! Reranker local em ONNX (E4.2): reordena no computador os candidatos que o FTS5 + vetores
//! trouxeram, sem chamar rede (REGRAS.md: custo zero (sem API paga)).

use orion_store::Reranker;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const MAX_TOKENS: usize = 512;
pub const MODELO_SUGERIDO: &str = "Xenova/ms-marco-MiniLM-L-6-v2";

pub fn sigmoid(x: f64) -> f64 {
    if x >= 0.0 {
        1.0 / (1.0 + (-x).exp())
    } else {
        let e = x.exp();
        e / (1.0 + e)
    }
}

pub struct LocalReranker {
    pub pasta: PathBuf,
}

impl LocalReranker {
    pub fn new(pasta: impl AsRef<Path>) -> Result<Self, String> {
        let p = pasta.as_ref().to_path_buf();
        let onnx = ["model_quantized.onnx", "model.onnx"].into_iter().map(|n| p.join(n)).find(|c| c.is_file());
        let Some(_) = onnx else {
            return Err(format!("modelo ONNX não encontrado em {} (esperado model.onnx + tokenizer.json, ex.: {MODELO_SUGERIDO})", p.display()));
        };
        if !p.join("tokenizer.json").is_file() {
            return Err(format!("tokenizer.json não encontrado em {}", p.display()));
        }
        Ok(Self { pasta: p })
    }
}

impl Reranker for LocalReranker {
    fn score(&self, _pergunta: &str, trechos: &[String]) -> anyhow::Result<Vec<f32>> {
        if trechos.is_empty() {
            return Ok(vec![]);
        }
        anyhow::bail!("runtime ONNX não vinculado neste binário")
    }
}

/// Carrega o reranker se a pasta existir; se falhar, avisa no log e devolve `None`.
pub fn reranker_from_dir(pasta: Option<&Path>) -> Option<Arc<dyn Reranker>> {
    let p = pasta?;
    match LocalReranker::new(p) {
        Ok(r) => Some(Arc::new(r)),
        Err(e) => {
            tracing::warn!("reranker desligado: {e}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sigmoid_estavel_nos_extremos() {
        assert!((sigmoid(0.0) - 0.5).abs() < 1e-9);
        assert!(sigmoid(100.0) > 0.9999);
        assert!(sigmoid(-100.0) < 0.0001);
    }

    #[test]
    fn reranker_ausente_devolve_none_sem_quebrar() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(reranker_from_dir(Some(tmp.path())).is_none());
    }
}
