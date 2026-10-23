//! Spike F00.17: SQLCipher + FTS5 + sqlite-vec + log de eventos encadeado por hash, num único arquivo.
use anyhow::{ensure, Result};
use rusqlite::{ffi::sqlite3_auto_extension, params, Connection};
use sha2::{Digest, Sha256};
use std::time::Instant;

const DIM: usize = 384;

fn lcg(seed: &mut u64) -> f32 {
    *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    ((*seed >> 33) as f32 / (1u64 << 31) as f32) - 0.5
}

fn vec_bytes(seed: &mut u64) -> Vec<u8> {
    let v: Vec<f32> = (0..DIM).map(|_| lcg(seed)).collect();
    let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    v.iter().flat_map(|x| (x / n).to_le_bytes()).collect()
}

fn append_event(db: &Connection, kind: &str, payload: &str) -> Result<()> {
    let (seq, prev): (i64, String) = db.query_row(
        "SELECT COALESCE(MAX(seq),0), COALESCE((SELECT hash FROM events ORDER BY seq DESC LIMIT 1),'') FROM events",
        [], |r| Ok((r.get(0)?, r.get(1)?)))?;
    let seq = seq + 1;
    let mut h = Sha256::new();
    h.update(format!("{seq}|{kind}|{payload}|{prev}"));
    db.execute("INSERT INTO events(seq,device,kind,payload,prev_hash,hash) VALUES (?,?,?,?,?,?)",
        params![seq, "pc-1", kind, payload, prev, hex::encode(h.finalize())])?;
    Ok(())
}

fn verify_chain(db: &Connection) -> Result<Option<i64>> {
    let mut st = db.prepare("SELECT seq,kind,payload,prev_hash,hash FROM events ORDER BY seq")?;
    let mut prev = String::new();
    let rows = st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?, r.get::<_, String>(4)?)))?;
    for r in rows {
        let (seq, kind, payload, p, hash) = r?;
        let mut h = Sha256::new();
        h.update(format!("{seq}|{kind}|{payload}|{prev}"));
        if p != prev || hex::encode(h.finalize()) != hash { return Ok(Some(seq)); }
        prev = hash;
    }
    Ok(None)
}

fn main() -> Result<()> {
    unsafe {
        sqlite3_auto_extension(Some(std::mem::transmute(sqlite_vec::sqlite3_vec_init as *const ())));
    }
    let path = std::env::temp_dir().join("spike_banco.db");
    for ext in ["", "-wal", "-shm"] { let _ = std::fs::remove_file(format!("{}{ext}", path.display())); }
    let db = Connection::open(&path)?;
    db.pragma_update(None, "key", "senha-de-teste-do-spike")?;
    db.pragma_update(None, "journal_mode", "WAL")?;
    let cipher: String = db.query_row("PRAGMA cipher_version", [], |r| r.get(0))?;
    let (ver, vecv): (String, String) = db.query_row("SELECT sqlite_version(), vec_version()", [], |r| Ok((r.get(0)?, r.get(1)?)))?;
    println!("sqlite={ver} sqlcipher={cipher} sqlite-vec={vecv}");
    db.execute_batch(&format!("
        CREATE TABLE events(seq INTEGER PRIMARY KEY, ts TEXT DEFAULT CURRENT_TIMESTAMP, device TEXT, kind TEXT, payload TEXT, prev_hash TEXT, hash TEXT);
        CREATE TRIGGER events_no_update BEFORE UPDATE ON events BEGIN SELECT RAISE(ABORT,'log de eventos é append-only'); END;
        CREATE TRIGGER events_no_delete BEFORE DELETE ON events BEGIN SELECT RAISE(ABORT,'log de eventos é append-only'); END;
        CREATE VIRTUAL TABLE docs USING fts5(title, body, tokenize='unicode61 remove_diacritics 2');
        CREATE VIRTUAL TABLE vecs USING vec0(embedding float[{DIM}] distance_metric=cosine);
    "))?;
    let n = 10_000usize;
    let mut seed = 42u64;
    let t = Instant::now();
    db.execute_batch("BEGIN")?;
    let words = ["vault", "nota", "agente", "banco", "projeto", "sincronia", "modelo", "voz", "grafo", "rotina", "orçamento", "câmera"];
    for i in 0..n {
        let body: Vec<&str> = (0..40).map(|j| words[(i * 7 + j * 13 + (i >> 3)) % words.len()]).collect();
        db.execute("INSERT INTO docs(rowid,title,body) VALUES (?,?,?)", params![i as i64, format!("nota {i}"), body.join(" ")])?;
        db.execute("INSERT INTO vecs(rowid,embedding) VALUES (?,?)", params![i as i64, vec_bytes(&mut seed)])?;
        append_event(&db, "nota.criada", &format!("{{\"id\":{i}}}"))?;
    }
    db.execute_batch("COMMIT")?;
    println!("inserir {n} (fts5+vec0+evento): {:?}", t.elapsed());

    let q = vec_bytes(&mut seed);
    let mut lat = vec![];
    for _ in 0..50 {
        let t = Instant::now();
        let ids: Vec<i64> = db.prepare("SELECT rowid FROM vecs WHERE embedding MATCH ? AND k = 10 ORDER BY distance")?
            .query_map([&q], |r| r.get(0))?.collect::<Result<_, _>>()?;
        ensure!(ids.len() == 10);
        lat.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    lat.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!("KNN k=10 em {n}x{DIM}: p50={:.2}ms p95={:.2}ms", lat[25], lat[47]);
    let t = Instant::now();
    let c: i64 = db.query_row("SELECT count(*) FROM docs WHERE docs MATCH 'agente AND sincronia'", [], |r| r.get(0))?;
    println!("FTS5 'agente AND sincronia': {c} linhas em {:?}", t.elapsed());
    // Híbrido: FTS5 filtra, vec0 reordena dentro do mesmo arquivo/transação.
    let t = Instant::now();
    let hyb: Vec<i64> = db.prepare("SELECT v.rowid FROM vecs v WHERE v.embedding MATCH ? AND k = 5 AND v.rowid IN (SELECT rowid FROM docs WHERE docs MATCH 'banco') ORDER BY distance")?
        .query_map([&q], |r| r.get(0))?.collect::<Result<_, _>>()?;
    println!("híbrido (FTS5 como filtro do vec0): {} linhas em {:?}", hyb.len(), t.elapsed());

    // Log append-only e cadeia de hash.
    ensure!(db.execute("UPDATE events SET payload='x' WHERE seq=1", []).is_err());
    ensure!(db.execute("DELETE FROM events WHERE seq=1", []).is_err());
    println!("append-only: UPDATE/DELETE bloqueados por trigger");
    ensure!(verify_chain(&db)?.is_none());
    println!("cadeia de hash íntegra ({n} eventos)");
    db.execute_batch("DROP TRIGGER events_no_update")?;
    db.execute("UPDATE events SET payload='{\"id\":999}' WHERE seq=5000", [])?;
    println!("adulteração do evento 5000 detectada em seq={:?}", verify_chain(&db)?);
    drop(db);

    // Criptografia em repouso.
    let head = std::fs::read(&path)?;
    ensure!(!head.starts_with(b"SQLite format 3"));
    println!("arquivo cifrado: cabeçalho não é 'SQLite format 3' ({} MiB)", head.len() / (1 << 20));
    let wrong = Connection::open(&path)?;
    wrong.pragma_update(None, "key", "senha-errada")?;
    ensure!(wrong.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0)).is_err());
    println!("senha errada: leitura recusada");
    let ok = Connection::open(&path)?;
    ok.pragma_update(None, "key", "senha-de-teste-do-spike")?;
    let c: i64 = ok.query_row("SELECT count(*) FROM vecs", [], |r| r.get(0))?;
    println!("reabrir com a senha certa: {c} vetores");
    Ok(())
}
