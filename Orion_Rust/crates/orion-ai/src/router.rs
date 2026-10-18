//! Roteamento por tipo de tarefa: pergunta curta vai ao modelo rápido, trabalho pesado ao forte.
//!
//! Pontuação explicável: cada sinal soma pontos e o motivo acompanha a decisão. Sem modelo no
//! meio: decidir qual modelo usar não pode gastar cota. `#pesado`, `#rapido` ou `#visao` no
//! começo da mensagem força a camada.

use regex::Regex;
use std::sync::LazyLock;
use unicode_normalization::UnicodeNormalization;

pub const RAPIDO: &str = "rapido";
pub const PESADO: &str = "pesado";
pub const VISAO: &str = "visao";
pub const CAMADAS: [&str; 3] = [RAPIDO, PESADO, VISAO];
pub const LIMIAR_PESADO: i32 = 3;

pub fn sem_acento(s: &str) -> String {
    s.nfd().filter(|c| c.is_ascii()).collect::<String>().to_lowercase()
}

static FORCA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*#(pesado|rapido|visao)\b").unwrap());
static CODIGO: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)```|\bTraceback\b|\bStack ?trace\b|^\s*at \S+\(.*:\d+\)|\bException\b|\bError:").unwrap()
});
static ITEM_NUMERADO: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^\s*\d+[.)]\s+\S").unwrap());

const TRABALHO: &[&str] = &[
    "analis", "compar", "refator", "arquitet", "planej", "otimiz", "depur", "debug", "implement",
    "audit", "revis", "projet", "diagnostic", "migr", "passo a passo", "em detalhe",
    "detalhadamente", "escreva um", "escreva uma", "elabor", "estrateg", "trade-off", "tradeoff",
    "pros e contras",
];
const TEMA_TECNICO: &[&str] = &["codigo", "algoritmo", "diagrama", "uml", "arquitetura", "banco de dados", "sql"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rota {
    pub camada: String,
    pub motivo: String,
}

/// Escolhe a camada para a mensagem. Determinístico.
pub fn classificar(texto: &str, imagens: usize) -> Rota {
    let t = sem_acento(texto);
    if let Some(m) = FORCA.captures(&t) {
        return Rota { camada: m[1].to_string(), motivo: format!("forçado por #{}", &m[1]) };
    }
    if imagens > 0 {
        return Rota { camada: VISAO.into(), motivo: format!("{imagens} imagem(ns) na mensagem") };
    }
    let mut pontos = 0;
    let mut sinais: Vec<String> = vec![];
    let mut somar = |n: i32, sinal: String| {
        pontos += n;
        sinais.push(format!("{sinal} (+{n})"));
    };
    let n = texto.trim().chars().count();
    if n > 1200 {
        somar(3, format!("texto longo ({n} caracteres)"));
    } else if n > 500 {
        somar(2, format!("texto médio ({n} caracteres)"));
    } else if n > 250 {
        somar(1, format!("texto de {n} caracteres"));
    }
    if CODIGO.is_match(texto) {
        somar(2, "código ou erro colado".into());
    }
    let mut achados: Vec<&str> = TRABALHO.iter().copied().filter(|r| t.contains(r)).collect();
    if !achados.is_empty() {
        achados.sort();
        let n = (2 * achados.len() as i32).min(4);
        somar(n, format!("pede {}", achados.iter().take(3).copied().collect::<Vec<_>>().join(", ")));
    }
    if ITEM_NUMERADO.find_iter(texto).count() >= 3 || texto.matches('?').count() >= 3 {
        somar(1, "várias perguntas ou itens".into());
    }
    if TEMA_TECNICO.iter().any(|x| t.contains(x)) {
        somar(1, "tema técnico".into());
    }
    if pontos >= LIMIAR_PESADO {
        return Rota { camada: PESADO.into(), motivo: sinais.join("; ") };
    }
    let motivo = if sinais.is_empty() { "pergunta ou comando curto".to_string() } else { sinais.join("; ") };
    Rota { camada: RAPIDO.into(), motivo }
}
