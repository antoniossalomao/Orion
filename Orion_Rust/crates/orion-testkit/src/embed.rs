//! Embedder de mentira dos testes (`FakeEmbedder` de `tests/memory/conftest.py`): sinônimos viram
//! o mesmo "conceito", como um modelo real faz e a busca por palavra-chave não faz.

use orion_store::Embedder;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

const PARADAS: &str = "a o as os um uma uns umas de do da dos das em no na nos nas por para com sem sobre entre e ou mas que se como qual quais quem onde quando é ser foi são era eu me meu minha meus minhas você seu sua tu te ele ela isso isto esse essa este esta já não sim mais muito tem tinha ter há vai vou sei fala falou disse";

fn plano(t: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    t.to_lowercase().nfkd().filter(|c| c.is_ascii()).collect()
}

fn conceito(tok: &str) -> &str {
    match tok {
        "estudo" | "estuda" | "estudar" | "faculdade" | "universidade" | "unimar" | "curso" => "ESTUDAR",
        "moro" | "mora" | "morar" | "cidade" | "residencia" => "MORAR",
        "carro" | "veiculo" | "automovel" => "VEICULO",
        t => t,
    }
}

pub fn crc32(b: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &x in b {
        c ^= x as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { (c >> 1) ^ 0xEDB8_8320 } else { c >> 1 };
        }
    }
    !c
}

#[derive(Default)]
pub struct FakeEmbedder {
    /// `true`: a API de embedding "caiu".
    pub quebrado: AtomicBool,
    pub chamadas: AtomicUsize,
}

impl FakeEmbedder {
    pub fn novo() -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self::default())
    }
    pub fn chamadas(&self) -> usize {
        self.chamadas.load(Ordering::SeqCst)
    }
    pub fn quebrar(&self, sim: bool) {
        self.quebrado.store(sim, Ordering::SeqCst);
    }
}

impl Embedder for FakeEmbedder {
    fn dim(&self) -> usize {
        128
    }
    fn embed(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
        if self.quebrado.load(Ordering::SeqCst) {
            anyhow::bail!("API de embedding fora do ar");
        }
        self.chamadas.fetch_add(1, Ordering::SeqCst);
        let paradas: std::collections::HashSet<String> = PARADAS.split_whitespace().map(plano).collect();
        let re = regex::Regex::new(r"\w+").unwrap();
        Ok(texts
            .iter()
            .map(|t| {
                let mut v = vec![0.0f32; 128];
                for m in re.find_iter(&plano(t)) {
                    if paradas.contains(m.as_str()) {
                        continue; // modelos reais dão peso baixo a palavras funcionais
                    }
                    v[crc32(conceito(m.as_str()).as_bytes()) as usize % 128] += 1.0;
                }
                v
            })
            .collect())
    }
}
