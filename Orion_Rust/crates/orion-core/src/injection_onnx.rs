//! Classificador de injeção de prompt com ModernBERT multilíngue (F04.09), local e em CPU.
//!
//! Modelo escolhido no spike F00.23: Horizon-Labs/prompt-injection-guard-small, int8, Apache-2.0
//! (docs/spikes/README.md). Os pesos não vêm no pacote: ficam em `<pasta>/model_quantized.onnx` e
//! `<pasta>/tokenizer.json`, instalados por escolha sua, e são conferidos por SHA-256 antes de carregar
//! (REGRAS.md: pesos de modelo são verificados). Sem os arquivos, o guarda continua na heurística.
//! Pontuação final = a maior entre a heurística e o modelo; falhou em classificar = bloqueado
//! (REGRAS.md: conteúdo externo é dado; o guarda falha fechado).

use orion_ai::injection::{Heuristic, InjectionClassifier, InjectionError};
use parking_lot::Mutex;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokenizers::{Tokenizer, TruncationParams};

pub const MODEL_FILE: &str = "model_quantized.onnx";
pub const TOKENIZER_FILE: &str = "tokenizer.json";
const MODEL_SHA256: &str = "61cd0741cdbfcc229e32e1de1392f352736d87d1e8b3c2b67ad9836bb7910d5d";
const TOKENIZER_SHA256: &str = "47834a7dbbb0c4324fe9569fe47955241594a9ee539c9e478755d6fbc51e5f46";
/// Índice da classe INJECTION na saída (config.json: 0 = SAFE, 1 = INJECTION).
const INJECTION_CLASS: usize = 1;
/// Tokens por janela. Textos longos são analisados em janelas e vale a pior; acima do teto, erro.
const WINDOW: usize = 512;
/// Janelas por texto (limita a latência: ~25 ms por janela de 512 tokens em CPU; 64 janelas ≈ 1,6 s e ~28 mil tokens).
/// Texto maior que isso não é classificado por inteiro, então é recusado (falha fechada), nunca liberado pela metade.
const MAX_WINDOWS: usize = 64;

/// Pasta padrão dos pesos: `<recursos>/injection`, ou `ORION_INJECTION_MODEL`.
pub fn default_dir(configured: Option<&Path>) -> Option<PathBuf> {
    configured.map(Path::to_path_buf).or_else(|| crate::vision_runtime::runtime_dir().map(|d| d.join("injection")))
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut f = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut h = Sha256::new();
    std::io::copy(&mut f, &mut h).map_err(|e| e.to_string())?;
    Ok(hex::encode(h.finalize()))
}

pub struct ModernBert {
    session: Mutex<ort::session::Session>,
    tokenizer: Tokenizer,
    inputs: Vec<String>,
}

impl ModernBert {
    /// Carrega e confere os pesos. Erro descreve o que falta, sem caminhos de dados do usuário no texto.
    pub fn load(dir: &Path) -> Result<Self, String> {
        let (model, tok) = (dir.join(MODEL_FILE), dir.join(TOKENIZER_FILE));
        for (p, esperado) in [(&model, MODEL_SHA256), (&tok, TOKENIZER_SHA256)] {
            if !p.is_file() {
                return Err(format!("modelo de injeção não instalado: falta {}", p.file_name().unwrap_or_default().to_string_lossy()));
            }
            if sha256_file(p)? != esperado {
                return Err(format!("{} não confere com o SHA-256 esperado; reinstale o modelo", p.file_name().unwrap_or_default().to_string_lossy()));
            }
        }
        let mut tokenizer = Tokenizer::from_file(&tok).map_err(|e| e.to_string())?;
        tokenizer
            .with_truncation(Some(TruncationParams { max_length: WINDOW, stride: 64, ..Default::default() }))
            .map_err(|e| e.to_string())?;
        // Reusa o ambiente do ONNX Runtime da visão (mesma biblioteca instalada em `ORION_RUNTIME_DIR`).
        let session = crate::vision_runtime::cpu_session(&model)?;
        let inputs = session.inputs().iter().map(|i| i.name().to_string()).collect();
        Ok(ModernBert { session: Mutex::new(session), tokenizer, inputs })
    }

    fn score_window(&self, ids: &[u32], mask: &[u32]) -> Result<f32, InjectionError> {
        let err = |e: &dyn std::fmt::Display| InjectionError(format!("classificador de injeção: {e}"));
        let l = ids.len();
        let mut feed: Vec<(String, ort::value::DynValue)> = vec![];
        for n in &self.inputs {
            let v: Vec<i64> = match n.as_str() {
                "input_ids" => ids.iter().map(|&x| x as i64).collect(),
                "attention_mask" => mask.iter().map(|&x| x as i64).collect(),
                "token_type_ids" => vec![0; l],
                outro => return Err(InjectionError(format!("classificador de injeção: entrada desconhecida {outro}"))),
            };
            feed.push((n.clone(), ort::value::Tensor::from_array(([1usize, l], v)).map_err(|e| err(&e))?.into_dyn()));
        }
        let mut s = self.session.lock();
        let out = s.run(feed).map_err(|e| err(&e))?;
        let (_, logits) = out[0].try_extract_tensor::<f32>().map_err(|e| err(&e))?;
        if logits.len() <= INJECTION_CLASS {
            return Err(InjectionError("classificador de injeção: saída inesperada".into()));
        }
        let m = logits.iter().cloned().fold(f32::MIN, f32::max);
        let exp: Vec<f32> = logits.iter().map(|x| (x - m).exp()).collect();
        let p = exp[INJECTION_CLASS] / exp.iter().sum::<f32>();
        if p.is_nan() {
            return Err(InjectionError("classificador de injeção: pontuação inválida".into()));
        }
        Ok(p)
    }
}

impl InjectionClassifier for ModernBert {
    fn name(&self) -> &str {
        "modernbert"
    }
    fn score(&self, text: &str) -> Result<f32, InjectionError> {
        let enc = self.tokenizer.encode(text, true).map_err(|e| InjectionError(format!("classificador de injeção: {e}")))?;
        // O primeiro encoding é a janela inicial; os `overflowing` são as seguintes (stride de 64).
        if enc.get_overflowing().len() + 1 > MAX_WINDOWS {
            return Err(InjectionError("classificador de injeção: texto longo demais para classificar por inteiro".into()));
        }
        let mut worst = self.score_window(enc.get_ids(), enc.get_attention_mask())?;
        for o in enc.get_overflowing() {
            worst = worst.max(self.score_window(o.get_ids(), o.get_attention_mask())?);
        }
        Ok(worst)
    }
}

/// Leva a probabilidade do modelo à escala do `Guard` (aviso a partir de 0,5; bloqueio a partir de 0,8).
/// No conjunto do spike F00.23: p ≥ 0,5 acerta 92,8% com 0% de falso positivo (bloqueia); p ≥ 0,09 acerta 99%
/// com 1% (só avisa). O conjunto é curto e escrito à mão, então os números são otimistas (docs/spikes/README.md).
fn calibrate(p: f32) -> f32 {
    if p >= 0.5 {
        p.max(0.8)
    } else if p >= 0.09 {
        0.5
    } else {
        p
    }
}

/// Maior pontuação entre a heurística (padrões conhecidos) e o modelo (reformulações e outras línguas).
pub struct Combined {
    model: ModernBert,
}

impl Combined {
    pub fn new(model: ModernBert) -> Self {
        Combined { model }
    }
}

impl InjectionClassifier for Combined {
    fn name(&self) -> &str {
        "heuristica+modernbert"
    }
    fn score(&self, text: &str) -> Result<f32, InjectionError> {
        let h = Heuristic.score(text)?;
        Ok(h.max(calibrate(self.model.score(text)?)))
    }
}

/// Guarda com o modelo quando os pesos estão instalados; senão, o guarda padrão (heurística).
pub fn guard(dir: Option<&Path>) -> (orion_ai::injection::Guard, Result<(), String>) {
    use orion_ai::injection::Guard;
    let Some(dir) = default_dir(dir) else {
        return (Guard::standard(), Err("pasta do modelo de injeção indisponível".into()));
    };
    match ModernBert::load(&dir) {
        Ok(m) => (Guard::new(std::sync::Arc::new(Combined::new(m)), 0.5, 0.8), Ok(())),
        Err(e) => (Guard::standard(), Err(e)),
    }
}
