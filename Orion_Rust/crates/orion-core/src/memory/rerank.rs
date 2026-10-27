//! Reranker local em ONNX (E4.2): reordena no computador os candidatos que o FTS5 + vetores
//! trouxeram, sem chamar rede (REGRAS.md: custo zero (sem API paga)).

use orion_store::Reranker;
use tract_onnx::prelude::*;
use parking_lot::Mutex;
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
    tokenizer: tokenizers::Tokenizer,
    model: Mutex<Option<Arc<TypedRunnableModel>>>,
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
        let mut tokenizer=tokenizers::Tokenizer::from_file(p.join("tokenizer.json")).map_err(|e|e.to_string())?;
        tokenizer.with_truncation(Some(tokenizers::utils::truncation::TruncationParams{max_length:MAX_TOKENS,..Default::default()})).map_err(|e|e.to_string())?;
        tokenizer.with_padding(Some(tokenizers::utils::padding::PaddingParams{strategy:tokenizers::utils::padding::PaddingStrategy::Fixed(MAX_TOKENS),..Default::default()}));
        Ok(Self { pasta: p,tokenizer,model:Mutex::new(None) })
    }
}

impl Reranker for LocalReranker {
    fn score(&self, pergunta:&str, trechos:&[String])->anyhow::Result<Vec<f32>>{
        if trechos.is_empty(){return Ok(vec![]);}
        anyhow::ensure!(trechos.len()<=100,"candidatos demais para reranking");
        let mut guard=self.model.lock();
        if guard.is_none(){
            let path=["model.onnx","model_quantized.onnx"].into_iter().map(|n|self.pasta.join(n)).find(|p|p.is_file()).ok_or_else(||anyhow::anyhow!("modelo ONNX ausente"))?;
            let mut graph=tract_onnx::onnx().model_for_path(path)?;for i in 0..graph.input_outlets()?.len(){graph=graph.with_input_fact(i,i64::fact([1,MAX_TOKENS]).into())?;}
            *guard=Some(graph.into_optimized()?.into_runnable()?);
        }
        let model=guard.as_ref().unwrap();
        let mut scores=Vec::with_capacity(trechos.len());
        for text in trechos{
            let encoded=self.tokenizer.encode((pergunta,text.as_str()),true).map_err(|e|anyhow::anyhow!(e.to_string()))?;
            let n=encoded.len();
            let ids:Tensor=tract_ndarray::Array2::from_shape_vec((1,n),encoded.get_ids().iter().map(|v|*v as i64).collect())?.into();
            let mask:Tensor=tract_ndarray::Array2::from_shape_vec((1,n),encoded.get_attention_mask().iter().map(|v|*v as i64).collect())?.into();
            let types:Tensor=tract_ndarray::Array2::from_shape_vec((1,n),encoded.get_type_ids().iter().map(|v|*v as i64).collect())?.into();
            let mut inputs=tvec![];
            for outlet in model.model().input_outlets()?{
                let name=&model.model().node(outlet.node).name;
                inputs.push(match name.as_str(){"input_ids"=>ids.clone().into(),"attention_mask"=>mask.clone().into(),"token_type_ids"=>types.clone().into(),_=>anyhow::bail!("entrada ONNX não suportada: {name}")});
            }
            let outputs=model.run(inputs)?;
            let score=*outputs[0].to_plain_array_view::<f32>()?.first().ok_or_else(||anyhow::anyhow!("reranker sem logits"))?;
            anyhow::ensure!(score.is_finite(),"logit inválido");scores.push(sigmoid(score as f64) as f32);
        }
        Ok(scores)
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
