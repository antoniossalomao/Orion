//! Ingestão limitada e recuperável: originais no banco, texto isolado por projeto.
//!
//! O texto sai num processo à parte (`orion extrair-documento`), com prazo e teto de bytes; o
//! original não é interpretado no processo do servidor.

use orion_store::{with_scope, DataScope, Store};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const MAX_UPLOAD: usize = 6 * 1024 * 1024;
pub const MAX_TOTAL: i64 = 256 * 1024 * 1024;
pub const PRAZO_EXTRACAO_S: f64 = 12.0;
pub const DEPOIS_DA_EXTRACAO: i64 = 85;
/// O upload espera isto pela indexação; se demora, segue em segundo plano.
pub const ESPERA_S: f64 = 2.0;

fn andamento(memory: &Store, id: &str, pct: i64) {
    let _ = memory.tx(|c| {
        c.execute("UPDATE uploads SET progress=? WHERE id=?", orion_store::rusqlite::params![pct.clamp(0, 100), id])?;
        Ok(())
    });
}

/// Roda o extrator num processo à parte e repassa o andamento.
fn extrair(raw: &[u8], kind: &str, avisar: &dyn Fn(i64)) -> Value {
    let falha = json!({"error": "document_extraction_failed"});
    let Ok(exe) = std::env::current_exe() else { return falha };
    let dir = std::env::temp_dir().join(format!("orion-document-{}", orion_store::uuid_hex()));
    if std::fs::create_dir_all(&dir).is_err() {
        return falha;
    }
    let entrada = dir.join("input");
    let r = (|| -> Option<Value> {
        std::fs::write(&entrada, raw).ok()?;
        let mut cmd = Command::new(exe);
        cmd.arg("extrair-documento").arg(&entrada).arg(kind).env_clear().stdout(Stdio::piped()).stderr(Stdio::null());
        for k in ["SYSTEMROOT", "WINDIR", "TEMP", "TMP", "PATH"] {
            if let Some(v) = std::env::var_os(k) {
                cmd.env(k, v);
            }
        }
        let mut filho = cmd.spawn().ok()?;
        let saida = filho.stdout.take()?;
        let (tx, rx) = std::sync::mpsc::channel::<String>();
        std::thread::spawn(move || {
            let mut total = 0usize;
            for l in BufReader::new(saida.take(14 * 1024 * 1024)).lines().map_while(Result::ok) {
                total += l.len();
                if total > 14 * 1024 * 1024 || tx.send(l).is_err() {
                    break;
                }
            }
        });
        let limite = Instant::now() + Duration::from_secs_f64(PRAZO_EXTRACAO_S);
        let mut ultima = None;
        loop {
            let resta = limite.saturating_duration_since(Instant::now());
            if resta.is_zero() {
                let _ = filho.kill();
                let _ = filho.wait();
                return Some(json!({"error": "document_extraction_timeout"}));
            }
            match rx.recv_timeout(resta) {
                Ok(l) => {
                    if l.starts_with("{\"progress\"") {
                        if let Some(p) = serde_json::from_str::<Value>(&l).ok().and_then(|v| v["progress"].as_i64()) {
                            avisar(p);
                        }
                    } else {
                        ultima = Some(l);
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(_) => break, // o processo fechou a saída
            }
        }
        let ok = filho.wait().ok()?.success();
        if ok { serde_json::from_str(&ultima?).ok() } else { None }
    })();
    let _ = std::fs::remove_dir_all(&dir);
    r.unwrap_or(falha)
}

/// Extrai e indexa o documento (bloqueante). Devolve `false` se o documento não existe.
pub fn process(memory: &Arc<Store>, id: &str, project_id: Option<&str>) -> bool {
    let Ok(rows) = memory.query("SELECT * FROM uploads WHERE id=? AND project_id IS ?", &[&id, &project_id]) else { return false };
    let Some(row) = rows.first() else { return false };
    let (nome, kind) = (row["name"].as_str().unwrap_or("").to_string(), row["kind"].as_str().unwrap_or("").to_string());
    let raw: Vec<u8> = memory.conn().query_row("SELECT raw FROM uploads WHERE id=?", [id], |r| r.get(0)).unwrap_or_default();
    let _ = memory.tx(|c| {
        c.execute("UPDATE uploads SET status='processing',error=NULL,progress=0 WHERE id=?", [id])?;
        Ok(())
    });
    let m2 = memory.clone();
    let id2 = id.to_string();
    let r = extrair(&raw, &kind, &move |pct| andamento(&m2, &id2, pct * DEPOIS_DA_EXTRACAO / 100));
    let texto = r["text"].as_str().map(String::from);
    if let Some(t) = &texto {
        andamento(memory, id, DEPOIS_DA_EXTRACAO);
        let indexou = with_scope(DataScope::project(project_id.map(String::from), false), || memory.index_document(&format!("upload:{id}/{nome}"), &nome, t));
        if let Err(e) = indexou {
            tracing::error!("indexação do documento falhou: {e}");
        }
    }
    let erro = r["error"].as_str().map(String::from);
    let _ = memory.tx(|c| {
        c.execute(
            "UPDATE uploads SET status=?,error=?,progress=100 WHERE id=?",
            orion_store::rusqlite::params![if texto.is_some() { "ready" } else { "error" }, erro, id],
        )?;
        Ok(())
    });
    true
}

/// Documento que ficou `processing`/`received` porque o Orion caiu no meio vira erro (com "Tentar
/// de novo"), em vez de uma barra de progresso parada para sempre.
pub fn recuperar_interrompidos(memory: &Store) -> usize {
    memory
        .tx(|c| Ok(c.execute("UPDATE uploads SET status='error', error='document_interrupted', progress=100 WHERE status IN ('processing','received')", [])?))
        .unwrap_or(0)
}

pub fn linhas(memory: &Store, project_id: Option<&str>) -> Result<Vec<serde_json::Map<String, Value>>, orion_store::StoreError> {
    memory.query(
        "SELECT id,name,kind,status,error,progress,created_at,length(raw) AS size FROM uploads WHERE project_id IS ? ORDER BY created_at DESC LIMIT 200",
        &[&project_id],
    )
}
