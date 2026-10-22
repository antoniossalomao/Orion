//! Medição da busca (B-05): latência de FTS5, vetores e busca híbrida sobre um `orion.db` real
//! ou sintético, para comparar com o Python (`scripts/benchmark_rust_python.py`).
//!
//! Protocolo com o script (para o RSS ser medido de fora, em qualquer sistema): o processo
//! escreve `READY <fase>` no stdout e espera uma linha no stdin antes de seguir; no fim escreve
//! `RESULT <json>`. Fases: `aberto` (banco aberto), `frio` (primeira busca vetorial: a matriz
//! carregou) e `quente` (depois das buscas).
//!
//!     cargo run --release -p orion-store --example bench_busca -- \
//!         --db <orion.db> --dim 384 --consultas consultas.json --reps 3 [--sem-pausa]
//!
//! O embedder é o de mentira do benchmark (palavras → vetor por hash), idêntico ao do Python:
//! mede a memória e a busca, não um modelo de embeddings.

use orion_store::search::Kind;
use orion_store::{Embedder, Store};
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::sync::Arc;
use std::time::Instant;

fn crc32(b: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &x in b {
        c ^= x as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { (c >> 1) ^ 0xEDB8_8320 } else { c >> 1 };
        }
    }
    !c
}

/// Mesmo algoritmo do `FakeEmbedder` do script: palavras minúsculas e sem acento, `crc32 % dim`.
struct Hash {
    dim: usize,
    re: regex::Regex,
}

impl Embedder for Hash {
    fn dim(&self) -> usize {
        self.dim
    }
    fn embed(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>> {
        use unicode_normalization::UnicodeNormalization;
        Ok(texts
            .iter()
            .map(|t| {
                let plano: String = t.to_lowercase().nfkd().filter(|c| c.is_ascii()).collect();
                let mut v = vec![0.0f32; self.dim];
                for m in self.re.find_iter(&plano) {
                    v[crc32(m.as_str().as_bytes()) as usize % self.dim] += 1.0;
                }
                // desempate: textos de mesmas palavras não podem empatar no cosseno
                let marca = crc32(&[t.as_bytes(), b"#"].concat()) as usize % self.dim;
                v[marca] += 0.05 * ((crc32(t.as_bytes()) % 1000) as f32 / 1000.0);
                v
            })
            .collect())
    }
}

fn pausa(fase: &str, sem_pausa: bool) {
    if sem_pausa {
        return;
    }
    println!("READY {fase}");
    std::io::stdout().flush().ok();
    let mut linha = String::new();
    std::io::stdin().lock().read_line(&mut linha).ok();
}

fn percentil(ordenado: &[f64], p: f64) -> f64 {
    if ordenado.is_empty() {
        return 0.0;
    }
    let i = ((ordenado.len() as f64 - 1.0) * p).round() as usize;
    ordenado[i.min(ordenado.len() - 1)]
}

fn estatisticas(ms: &mut [f64]) -> Value {
    ms.sort_by(|a, b| a.total_cmp(b));
    let media = ms.iter().sum::<f64>() / ms.len().max(1) as f64;
    json!({"n": ms.len(), "min_ms": ms.first(), "p50_ms": percentil(ms, 0.5), "p95_ms": percentil(ms, 0.95),
           "p99_ms": percentil(ms, 0.99), "max_ms": ms.last(), "media_ms": media})
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let pega = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let db = pega("--db").ok_or_else(|| anyhow::anyhow!("falta --db"))?;
    let dim: usize = pega("--dim").map(|d| d.parse()).transpose()?.unwrap_or(384);
    let reps: usize = pega("--reps").map(|d| d.parse()).transpose()?.unwrap_or(3);
    let k: usize = pega("--k").map(|d| d.parse()).transpose()?.unwrap_or(8);
    let sem_pausa = args.iter().any(|a| a == "--sem-pausa");
    let consultas: Vec<String> = serde_json::from_str(&std::fs::read_to_string(pega("--consultas").ok_or_else(|| anyhow::anyhow!("falta --consultas"))?)?)?;

    let t0 = Instant::now();
    let emb = Arc::new(Hash { dim, re: regex::Regex::new(r"\w+")? });
    let store = Store::open_with(&db, Some(emb), orion_policy::wall_clock())?;
    let abrir_ms = t0.elapsed().as_secs_f64() * 1000.0;
    pausa("aberto", sem_pausa);

    // fts: sem embedder o `search` não usa vetor; aqui o mesmo `Store` serve só de comparação
    // com a busca por palavra-chave pura (abre outro, sem embedder, no mesmo arquivo).
    let so_fts = Store::open(&db)?;
    let todos = [Kind::Fact, Kind::Chunk, Kind::Message];

    let rodar = |s: &Store| -> (Vec<f64>, Vec<Vec<(String, i64)>>) {
        let mut ms = vec![];
        let mut topo = vec![];
        for r in 0..reps {
            for q in &consultas {
                let t = Instant::now();
                let hits = s.search(q, k, &todos).unwrap_or_default();
                ms.push(t.elapsed().as_secs_f64() * 1000.0);
                if r == 0 {
                    topo.push(hits.iter().map(|h| (h.kind.as_str().to_string(), h.id)).collect());
                }
            }
        }
        (ms, topo)
    };

    let (mut fts_ms, fts_topo) = rodar(&so_fts);

    // primeira busca vetorial: carrega as matrizes de fatos e trechos
    let t = Instant::now();
    store.search(&consultas[0], k, &todos).ok();
    let frio_ms = t.elapsed().as_secs_f64() * 1000.0;
    pausa("frio", sem_pausa);

    let (mut hib_ms, hib_topo) = rodar(&store);
    pausa("quente", sem_pausa);

    let r = json!({
        "motor": "rust",
        "abrir_ms": abrir_ms,
        "primeira_busca_vetorial_ms": frio_ms,
        "fts": estatisticas(&mut fts_ms),
        "hibrida": estatisticas(&mut hib_ms),
        "topo_fts": fts_topo,
        "topo_hibrida": hib_topo,
    });
    println!("RESULT {r}");
    Ok(())
}
