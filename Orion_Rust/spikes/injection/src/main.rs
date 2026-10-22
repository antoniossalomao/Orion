//! Spike F00.23: detectores de prompt injection em ONNX/CPU contra `dataset.tsv` (pt-BR, escrito à mão).
//! Uso: spike-injection <modelo.onnx> <tokenizer.json> <indice-da-classe-injection> [max_tokens]
use anyhow::Result;
use ort::{session::Session, value::Tensor};
use std::time::Instant;
use tokenizers::{Tokenizer, TruncationParams};

fn e<T: std::fmt::Display>(x: T) -> anyhow::Error { anyhow::anyhow!("{x}") }

fn rss_mb() -> u64 {
    std::fs::read_to_string("/proc/self/status").ok().and_then(|s| s.lines().find(|l| l.starts_with("VmRSS")).and_then(|l| l.split_whitespace().nth(1).and_then(|v| v.parse::<u64>().ok()))).unwrap_or(0) / 1024
}

fn main() -> Result<()> {
    let a: Vec<String> = std::env::args().collect();
    let inj_idx: usize = a[3].parse()?;
    let maxlen: usize = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(256);
    let mut tok = Tokenizer::from_file(&a[2]).map_err(e)?;
    tok.with_truncation(Some(TruncationParams { max_length: maxlen, ..Default::default() })).map_err(e)?;
    let env = ort::environment::init().build().map_err(e)?;
    let rss0 = rss_mb();
    let t = Instant::now();
    let mut sess = Session::builder(&env).map_err(e)?.with_intra_threads(4).map_err(e)?.commit_from_file(&a[1]).map_err(e)?;
    let load = t.elapsed();
    let rss1 = rss_mb();
    let names: Vec<String> = sess.inputs().iter().map(|i| i.name().to_string()).collect();
    let data = std::fs::read_to_string(std::env::var("DATASET").unwrap_or(concat!(env!("CARGO_MANIFEST_DIR"), "/dataset.tsv").into()))?;
    let rows: Vec<(u8, &str)> = data.lines().filter_map(|l| l.split_once('\t')).map(|(y, x)| (y.parse().unwrap(), x)).collect();
    let mut scores = vec![];
    let mut lat = vec![];
    for (_, text) in &rows {
        let enc = tok.encode(*text, true).map_err(e)?;
        let l = enc.get_ids().len();
        let ids: Vec<i64> = enc.get_ids().iter().map(|&x| x as i64).collect();
        let mask: Vec<i64> = enc.get_attention_mask().iter().map(|&x| x as i64).collect();
        let t = Instant::now();
        let mut inputs: Vec<(String, ort::value::DynValue)> = vec![];
        for n in &names {
            let v: Vec<i64> = match n.as_str() { "input_ids" => ids.clone(), "attention_mask" => mask.clone(), "token_type_ids" => vec![0; l], _ => anyhow::bail!("entrada desconhecida {n}") };
            inputs.push((n.clone(), Tensor::from_array(([1usize, l], v)).map_err(e)?.into_dyn()));
        }
        let out = sess.run(inputs).map_err(e)?;
        let (_, logits) = out[0].try_extract_tensor::<f32>().map_err(e)?;
        let m = logits.iter().cloned().fold(f32::MIN, f32::max);
        let exp: Vec<f32> = logits.iter().map(|x| (x - m).exp()).collect();
        scores.push(exp[inj_idx] / exp.iter().sum::<f32>());
        lat.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let rss2 = rss_mb();
    let eval = |th: f32| {
        let (mut tp, mut fp, mut tn, mut fneg) = (0, 0, 0, 0);
        for ((y, _), s) in rows.iter().zip(&scores) {
            match (*y == 1, *s >= th) { (true, true) => tp += 1, (false, true) => fp += 1, (false, false) => tn += 1, (true, false) => fneg += 1 }
        }
        (tp, fp, tn, fneg)
    };
    let (tp, fp, tn, fneg) = eval(0.5);
    println!("limiar 0.5: VP={tp} FP={fp} VN={tn} FN={fneg}  recall={:.1}% falso-positivo={:.1}%", 100.0 * tp as f64 / (tp + fneg) as f64, 100.0 * fp as f64 / (fp + tn) as f64);
    // limiar mais baixo que mantém FP <= 2% (melhor recall com poucos bloqueios indevidos)
    let mut best = (0.5f32, tp, fp, fneg, tn);
    for k in 1..100 {
        let th = k as f32 / 100.0;
        let (tp, fp, tn, fneg) = eval(th);
        if (fp as f64) / ((fp + tn) as f64) <= 0.02 && tp > best.1 || ((fp as f64) / ((fp + tn) as f64) <= 0.02 && tp == best.1 && th < best.0) { best = (th, tp, fp, fneg, tn); }
    }
    println!("limiar {:.2} (FP<=2%): recall={:.1}% falso-positivo={:.1}%", best.0, 100.0 * best.1 as f64 / (best.1 + best.3) as f64, 100.0 * best.2 as f64 / (best.2 + best.4) as f64);
    for (i, ((y, text), s)) in rows.iter().zip(&scores).enumerate() {
        if (*y == 1) != (*s >= 0.5) { println!("  erro #{i} verdade={y} score={s:.3}: {}", text.chars().take(90).collect::<String>()); }
    }
    lat.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!("carga={load:?} RAM modelo={}MB (pico {}MB) entradas={names:?}", rss1 - rss0, rss2 - rss0);
    println!("latência/entrada: p50={:.1}ms p95={:.1}ms", lat[lat.len() / 2], lat[lat.len() * 95 / 100]);
    Ok(())
}
