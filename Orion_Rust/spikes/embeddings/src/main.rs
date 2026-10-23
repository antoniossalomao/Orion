//! Spike F00.24: multilingual-e5-small int8 contra FTS5 puro no vault real.
//! Consulta = campo `resumo:` do frontmatter (paráfrase curta da nota); alvo = a própria nota,
//! indexada sem o frontmatter. Uso: spike-embeddings <vault> <modelo.onnx> <tokenizer.json> [N]
use anyhow::Result;
use ort::{session::Session, value::Tensor};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tokenizers::{Tokenizer, TruncationParams, PaddingParams};

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if e.file_name().to_string_lossy().starts_with('.') { continue; }
        if p.is_dir() { walk(&p, out) } else if p.extension().is_some_and(|x| x == "md") { out.push(p) }
    }
}

struct Doc { key: String, title: String, body: String, resumo: String }

fn parse(root: &Path, path: &Path) -> Option<Doc> {
    let t = std::fs::read_to_string(path).ok()?;
    let rest = t.strip_prefix("---\n")?;
    let end = rest.find("\n---")?;
    let (fm, body) = (&rest[..end], &rest[end + 4..]);
    let resumo = fm.lines().find_map(|l| l.strip_prefix("resumo:"))?.trim().trim_matches('"').trim_matches('\'').to_string();
    if resumo.chars().count() < 40 || body.chars().count() < 300 { return None; }
    let title = path.file_stem()?.to_string_lossy().to_string();
    // a chave é o caminho relativo: dois `README.md` em pastas diferentes não podem colidir
    let key = path.strip_prefix(root).unwrap_or(path).to_string_lossy().to_string();
    Some(Doc { key, title, body: body.to_string(), resumo })
}

fn embed(sess: &mut Session, tok: &Tokenizer, texts: &[String]) -> Result<Vec<Vec<f32>>> {
    let enc = tok.encode_batch(texts.to_vec(), true).map_err(|e| anyhow::anyhow!("{e}"))?;
    let (b, l) = (enc.len(), enc[0].get_ids().len());
    let ids: Vec<i64> = enc.iter().flat_map(|e| e.get_ids().iter().map(|&x| x as i64)).collect();
    let mask: Vec<i64> = enc.iter().flat_map(|e| e.get_attention_mask().iter().map(|&x| x as i64)).collect();
    let out = sess.run(ort::inputs![
        "input_ids" => Tensor::from_array(([b, l], ids))?,
        "attention_mask" => Tensor::from_array(([b, l], mask.clone()))?,
        "token_type_ids" => Tensor::from_array(([b, l], vec![0i64; b * l]))?,
    ])?;
    let (shape, data) = out[0].try_extract_tensor::<f32>()?;
    let h = shape[2] as usize;
    let mut res = Vec::with_capacity(b);
    for i in 0..b {
        let mut v = vec![0f32; h];
        let mut n = 0f32;
        for j in 0..l {
            if mask[i * l + j] == 1 {
                n += 1.0;
                for k in 0..h { v[k] += data[(i * l + j) * h + k]; }
            }
        }
        let mut norm = 0f32;
        for x in v.iter_mut() { *x /= n; norm += *x * *x; }
        let norm = norm.sqrt();
        for x in v.iter_mut() { *x /= norm; }
        res.push(v);
    }
    Ok(res)
}

fn rss_mb() -> u64 {
    std::fs::read_to_string("/proc/self/status").ok().and_then(|s| s.lines().find(|l| l.starts_with("VmRSS")).and_then(|l| l.split_whitespace().nth(1).and_then(|v| v.parse::<u64>().ok()))).unwrap_or(0) / 1024
}

fn rank_of(order: &[usize], target: usize) -> Option<usize> { order.iter().position(|&x| x == target) }

fn main() -> Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let maxq: usize = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(300);
    let mut files = Vec::new();
    walk(Path::new(&a[1]), &mut files);
    files.sort();
    let root = Path::new(&a[1]);
    let docs: Vec<Doc> = files.iter().filter_map(|p| parse(root, p)).collect();
    // Corpus: todas as notas do vault (as sem `resumo` entram como distratores).
    let corpus: Vec<(String, String, String)> = files.iter().filter_map(|p| {
        let t = std::fs::read_to_string(p).ok()?;
        let body = match t.strip_prefix("---\n").and_then(|r| r.find("\n---").map(|e| r[e + 4..].to_string())) { Some(b) => b, None => t };
        let key = p.strip_prefix(root).unwrap_or(p).to_string_lossy().to_string();
        Some((key, p.file_stem()?.to_string_lossy().to_string(), body))
    }).collect();
    let idx_of: HashMap<&str, usize> = corpus.iter().enumerate().map(|(i, (k, _, _))| (k.as_str(), i)).collect();
    let step = (docs.len() / maxq).max(1);
    let queries: Vec<&Doc> = docs.iter().step_by(step).take(maxq).collect();
    println!("corpus={} notas, consultas={} (resumo -> nota)", corpus.len(), queries.len());

    // FTS5
    let db = rusqlite::Connection::open_in_memory()?;
    db.execute_batch("CREATE VIRTUAL TABLE f USING fts5(title, body, tokenize='unicode61 remove_diacritics 2');")?;
    let t0 = Instant::now();
    for (i, (_, t, b)) in corpus.iter().enumerate() {
        db.execute("INSERT INTO f(rowid,title,body) VALUES (?,?,?)", rusqlite::params![i as i64, t, b])?;
    }
    let fts_index = t0.elapsed();
    let fts = |q: &str| -> Vec<usize> {
        let toks: Vec<String> = q.split(|c: char| !c.is_alphanumeric()).filter(|w| w.chars().count() >= 3).map(|w| format!("\"{w}\"")).collect();
        if toks.is_empty() { return vec![]; }
        let mut st = db.prepare("SELECT rowid FROM f WHERE f MATCH ? ORDER BY bm25(f, 3.0, 1.0) LIMIT 50").unwrap();
        st.query_map([toks.join(" OR ")], |r| r.get::<_, i64>(0)).unwrap().filter_map(|x| x.ok()).map(|x| x as usize).collect()
    };

    // Embeddings
    let mut tok = Tokenizer::from_file(&a[3]).map_err(|e| anyhow::anyhow!("{e}"))?;
    tok.with_truncation(Some(TruncationParams { max_length: 512, ..Default::default() })).map_err(|e| anyhow::anyhow!("{e}"))?;
    tok.with_padding(Some(PaddingParams::default()));
    let rss0 = rss_mb();
    let env = ort::environment::init().build().map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut sess = Session::builder(&env).map_err(|e| anyhow::anyhow!("{e}"))?.with_intra_threads(4).map_err(|e| anyhow::anyhow!("{e}"))?.commit_from_file(&a[2]).map_err(|e| anyhow::anyhow!("{e}"))?;
    let rss_model = rss_mb();
    let t0 = Instant::now();
    let mut emb: Vec<Vec<f32>> = Vec::with_capacity(corpus.len());
    for chunk in corpus.chunks(8) {
        let texts: Vec<String> = chunk.iter().map(|(_, t, b)| format!("passage: {t}. {}", b.chars().take(1500).collect::<String>())).collect();
        emb.extend(embed(&mut sess, &tok, &texts)?);
    }
    let emb_index = t0.elapsed();
    let rss_peak = rss_mb();

    let (mut r_f, mut r_e, mut r_h) = (vec![], vec![], vec![]);
    let mut qlat = vec![];
    for d in &queries {
        let target = idx_of[d.key.as_str()];
        let order_f = fts(&d.resumo);
        let t = Instant::now();
        let q = embed(&mut sess, &tok, &[format!("query: {}", d.resumo)])?.remove(0);
        let mut sc: Vec<(usize, f32)> = emb.iter().enumerate().map(|(i, e)| (i, e.iter().zip(&q).map(|(x, y)| x * y).sum())).collect();
        sc.sort_by(|x, y| y.1.partial_cmp(&x.1).unwrap());
        let order_e: Vec<usize> = sc.iter().take(50).map(|x| x.0).collect();
        qlat.push(t.elapsed().as_secs_f64() * 1000.0);
        let mut rrf: HashMap<usize, f64> = HashMap::new();
        for (r, i) in order_f.iter().enumerate() { *rrf.entry(*i).or_default() += 1.0 / (60.0 + r as f64); }
        for (r, i) in order_e.iter().enumerate() { *rrf.entry(*i).or_default() += 1.0 / (60.0 + r as f64); }
        let mut h: Vec<_> = rrf.into_iter().collect();
        h.sort_by(|x, y| y.1.partial_cmp(&x.1).unwrap());
        let order_h: Vec<usize> = h.into_iter().map(|x| x.0).collect();
        r_f.push(rank_of(&order_f, target));
        r_e.push(rank_of(&order_e, target));
        r_h.push(rank_of(&order_h, target));
    }
    let report = |name: &str, r: &[Option<usize>]| {
        let n = r.len() as f64;
        let at = |k: usize| r.iter().filter(|x| x.is_some_and(|p| p < k)).count() as f64 / n;
        let mrr: f64 = r.iter().map(|x| x.map_or(0.0, |p| 1.0 / (p as f64 + 1.0))).sum::<f64>() / n;
        println!("{name:8} R@1={:.3} R@5={:.3} R@10={:.3} MRR={:.3}", at(1), at(5), at(10), mrr);
    };
    report("FTS5", &r_f);
    report("e5-small", &r_e);
    report("hibrido", &r_h);
    qlat.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!("indexar FTS5: {fts_index:?}; embeddings: {emb_index:?} ({:.1} notas/s)", corpus.len() as f64 / emb_index.as_secs_f64());
    println!("latencia consulta e5 (embed+varredura): p50={:.1}ms p95={:.1}ms", qlat[qlat.len() / 2], qlat[qlat.len() * 95 / 100]);
    println!("RAM: base={rss0}MB apos modelo={rss_model}MB pico={rss_peak}MB; vetores={:.1}MB (384 f32)", emb.len() as f64 * 384.0 * 4.0 / 1e6);
    Ok(())
}
