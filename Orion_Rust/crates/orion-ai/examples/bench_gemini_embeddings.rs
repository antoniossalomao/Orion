//! Benchmark F00.24 / issue #75: embeddings do Gemini contra FTS5 puro no vault real, na mesma tarefa do
//! spike local (`spikes/embeddings`, e5-small ONNX): consulta = campo `resumo:` da nota, alvo = a nota.
//! Serve para decidir a origem dos embeddings (local ONNX × API Gemini) com os MESMOS dados.
//!
//!     ORION_EMBED_API_KEY=... cargo run --release -p orion-ai --example bench_gemini_embeddings -- \
//!         <vault> [consultas=100] [modelo=gemini-embedding-001] [dim=768]
//!
//! Sem a chave, roda só o FTS5 (útil para conferir o corpus). O texto das notas SAI do computador para
//! o Google: rode só com um vault que você aceite enviar (REGRAS.md: toda saída de dado fica registrada).
//! Não é teste nem faz parte do build do produto.

use orion_store::rusqlite;
use orion_store::Embedder;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if e.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        if p.is_dir() {
            walk(&p, out)
        } else if p.extension().is_some_and(|x| x == "md") {
            out.push(p)
        }
    }
}

struct Query {
    key: String,
    resumo: String,
}

/// (chave, título, corpo, resumo opcional) de uma nota; o corpo não inclui o frontmatter.
fn parse(root: &Path, path: &Path) -> Option<(String, String, String, Option<String>)> {
    let t = std::fs::read_to_string(path).ok()?;
    let key = path.strip_prefix(root).unwrap_or(path).to_string_lossy().to_string();
    let title = path.file_stem()?.to_string_lossy().to_string();
    let Some(rest) = t.strip_prefix("---\n") else { return Some((key, title, t, None)) };
    let Some(end) = rest.find("\n---") else { return Some((key, title, t, None)) };
    let resumo = rest[..end]
        .lines()
        .find_map(|l| l.strip_prefix("resumo:"))
        .map(|r| r.trim().trim_matches('"').trim_matches('\'').to_string());
    Some((key, title, rest[end + 4..].to_string(), resumo))
}

fn rank_of(order: &[usize], target: usize) -> Option<usize> {
    order.iter().position(|&x| x == target)
}

fn report(name: &str, r: &[Option<usize>]) {
    let n = r.len() as f64;
    let at = |k: usize| r.iter().filter(|x| x.is_some_and(|p| p < k)).count() as f64 / n;
    let mrr: f64 = r.iter().map(|x| x.map_or(0.0, |p| 1.0 / (p as f64 + 1.0))).sum::<f64>() / n;
    println!("{name:10} R@1={:.3} R@5={:.3} R@10={:.3} MRR={:.3}", at(1), at(5), at(10), mrr);
}

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().collect();
    anyhow::ensure!(a.len() >= 2, "uso: bench_gemini_embeddings <vault> [consultas] [modelo] [dim]");
    let root = Path::new(&a[1]);
    let maxq: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(100);
    let model = a.get(3).cloned().unwrap_or_else(|| "gemini-embedding-001".into());
    let dim: usize = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(768);
    let mut files = vec![];
    walk(root, &mut files);
    files.sort();
    let corpus: Vec<(String, String, String, Option<String>)> = files.iter().filter_map(|p| parse(root, p)).collect();
    let idx_of: HashMap<&str, usize> = corpus.iter().enumerate().map(|(i, c)| (c.0.as_str(), i)).collect();
    // Mesmo critério do spike local: resumo com 40+ caracteres e corpo com 300+.
    let cands: Vec<Query> = corpus
        .iter()
        .filter_map(|(k, _, b, r)| r.as_ref().filter(|r| r.chars().count() >= 40 && b.chars().count() >= 300).map(|r| Query { key: k.clone(), resumo: r.clone() }))
        .collect();
    let step = (cands.len() / maxq.max(1)).max(1);
    let queries: Vec<&Query> = cands.iter().step_by(step).take(maxq).collect();
    println!("corpus={} notas, consultas={} (resumo -> nota)", corpus.len(), queries.len());
    anyhow::ensure!(!queries.is_empty(), "nenhuma nota com `resumo:` longo o bastante");

    let db = rusqlite::Connection::open_in_memory()?;
    db.execute_batch("CREATE VIRTUAL TABLE f USING fts5(title, body, tokenize='unicode61 remove_diacritics 2');")?;
    for (i, (_, t, b, _)) in corpus.iter().enumerate() {
        db.execute("INSERT INTO f(rowid,title,body) VALUES (?,?,?)", rusqlite::params![i as i64, t, b])?;
    }
    let fts = |q: &str| -> Vec<usize> {
        let toks: Vec<String> = q.split(|c: char| !c.is_alphanumeric()).filter(|w| w.chars().count() >= 3).map(|w| format!("\"{w}\"")).collect();
        if toks.is_empty() {
            return vec![];
        }
        let mut st = db.prepare("SELECT rowid FROM f WHERE f MATCH ? ORDER BY bm25(f, 3.0, 1.0) LIMIT 50").unwrap();
        st.query_map([toks.join(" OR ")], |r| r.get::<_, i64>(0)).unwrap().filter_map(|x| x.ok()).map(|x| x as usize).collect()
    };

    // Variante de produção: a consulta que a memória monta (`fts_query`: sem stopwords, prefixo nos termos longos).
    let fts_prod = |q: &str| -> Vec<usize> {
        let Some(m) = orion_store::text::fts_query(q) else { return vec![] };
        let Ok(mut st) = db.prepare("SELECT rowid FROM f WHERE f MATCH ? ORDER BY bm25(f, 3.0, 1.0) LIMIT 50") else { return vec![] };
        st.query_map([m], |r| r.get::<_, i64>(0)).map(|rows| rows.filter_map(|x| x.ok()).map(|x| x as usize).collect()).unwrap_or_default()
    };
    let r_prod: Vec<Option<usize>> = queries.iter().map(|q| rank_of(&fts_prod(&q.resumo), idx_of[q.key.as_str()])).collect();
    report("FTS5+prod", &r_prod);

    let key = std::env::var("ORION_EMBED_API_KEY").unwrap_or_default();
    let gem = if key.is_empty() { None } else { Some(orion_ai::embedders::GeminiEmbedder::new(&key, &model, dim)?) };
    let mut emb: Vec<Vec<f32>> = vec![];
    let mut index_time = Duration::ZERO;
    if let Some(g) = &gem {
        let t0 = Instant::now();
        let textos: Vec<String> = corpus.iter().map(|(_, t, b, _)| format!("{t}. {}", b.chars().take(1500).collect::<String>())).collect();
        for lote in textos.chunks(50) {
            emb.extend(g.embed(lote)?);
            std::thread::sleep(Duration::from_millis(700)); // cota gratuita: ~100 requisições/min
        }
        index_time = t0.elapsed();
    } else {
        println!("sem ORION_EMBED_API_KEY: só FTS5");
    }

    let (mut r_f, mut r_e, mut r_h) = (vec![], vec![], vec![]);
    let mut lat = vec![];
    for q in &queries {
        let target = idx_of[q.key.as_str()];
        let order_f = fts(&q.resumo);
        r_f.push(rank_of(&order_f, target));
        let Some(g) = &gem else { continue };
        let t = Instant::now();
        let v = g.embed_query(&q.resumo)?;
        let mut sc: Vec<(usize, f32)> = emb
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let dot: f32 = e.iter().zip(&v).map(|(x, y)| x * y).sum();
                let n = (e.iter().map(|x| x * x).sum::<f32>().sqrt() * v.iter().map(|x| x * x).sum::<f32>().sqrt()).max(1e-9);
                (i, dot / n)
            })
            .collect();
        sc.sort_by(|x, y| y.1.total_cmp(&x.1));
        lat.push(t.elapsed().as_secs_f64() * 1000.0);
        let order_e: Vec<usize> = sc.iter().take(50).map(|x| x.0).collect();
        let mut rrf: HashMap<usize, f64> = HashMap::new();
        for (r, i) in order_f.iter().enumerate() {
            *rrf.entry(*i).or_default() += 1.0 / (60.0 + r as f64);
        }
        for (r, i) in order_e.iter().enumerate() {
            *rrf.entry(*i).or_default() += 1.0 / (60.0 + r as f64);
        }
        let mut h: Vec<_> = rrf.into_iter().collect();
        h.sort_by(|x, y| y.1.total_cmp(&x.1));
        r_e.push(rank_of(&order_e, target));
        r_h.push(rank_of(&h.into_iter().map(|x| x.0).collect::<Vec<_>>(), target));
        std::thread::sleep(Duration::from_millis(700));
    }
    report("FTS5", &r_f);
    if gem.is_some() {
        report(&model, &r_e);
        report("hibrido", &r_h);
        lat.sort_by(|x, y| x.total_cmp(y));
        println!("indexar {} notas: {index_time:?}; consulta (rede + varredura): p50={:.0}ms p95={:.0}ms", corpus.len(), lat[lat.len() / 2], lat[lat.len() * 95 / 100]);
        println!("Referência local (spike F00.24, e5-small int8, 300 consultas): FTS5 R@1=0,723 | e5 R@1=0,680 | híbrido R@1=0,787 MRR=0,858; consulta 10 ms; modelo 118 MB.");
    }
    Ok(())
}
