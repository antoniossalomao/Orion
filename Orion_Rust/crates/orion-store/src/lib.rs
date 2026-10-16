//! Memória do Orion: sessões, fatos e documentos num único arquivo SQLite.
//!
//! Porte de `orion/memory/` (Python). Busca híbrida = FTS5 (palavra-chave, sempre disponível) +
//! vetores (quando há embedder), fundidos por RRF. Se o embedder cair, a busca segue só por
//! palavra-chave e os vetores faltantes entram depois em `embed_pending()`. O esquema é o mesmo
//! do Python durante a transição: os dois lados abrem o mesmo `orion.db`.

pub mod documents;
pub mod facts;
pub mod library;
pub mod ops;
pub mod schema;
pub mod scope;
pub mod search;
pub mod sessions;
pub mod text;
pub mod vectors;

use parking_lot::{Mutex, ReentrantMutex, ReentrantMutexGuard};
use rusqlite::{Connection, OptionalExtension};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub use rusqlite;
pub use scope::{current as current_scope, with_scope, DataScope};

pub type Clock = orion_policy::Clock;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// Equivale ao `KeyError` do Python: o registro não existe (ou não é deste escopo).
    #[error("não encontrado: {0}")]
    NotFound(String),
    /// Equivale ao `ValueError` do Python: pedido inválido.
    #[error("{0}")]
    Invalid(String),
    #[error("banco: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, StoreError>;

pub fn invalid<T>(msg: impl Into<String>) -> Result<T> {
    Err(StoreError::Invalid(msg.into()))
}

pub fn not_found<T>(what: impl ToString) -> Result<T> {
    Err(StoreError::NotFound(what.to_string()))
}

/// Gera vetores de embedding. `embed_query` usa o modo de consulta, quando o modelo tem.
pub trait Embedder: Send + Sync {
    fn dim(&self) -> usize;
    fn embed(&self, texts: &[String]) -> anyhow::Result<Vec<Vec<f32>>>;
    fn embed_query(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        Ok(self.embed(&[text.to_string()])?.remove(0))
    }
}

/// Reordena candidatos da busca (cross-encoder local). Nota entre 0 e 1 por trecho.
pub trait Reranker: Send + Sync {
    fn score(&self, pergunta: &str, trechos: &[String]) -> anyhow::Result<Vec<f32>>;
}

pub struct Store {
    pub path: PathBuf,
    conn: ReentrantMutex<Connection>,
    embedder: Option<Arc<dyn Embedder>>,
    clock: Clock,
    index: Mutex<vectors::VectorIndex>,
    pub reranker: Mutex<Option<Arc<dyn Reranker>>>,
    /// Há embedder: a busca vetorial está ligada.
    pub vectors_available: bool,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_with(path, None, orion_policy::wall_clock())
    }

    pub fn open_with(path: impl AsRef<Path>, embedder: Option<Arc<dyn Embedder>>, clock: Clock) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let memoria = path.as_os_str() == ":memory:";
        if !memoria {
            if let Some(pai) = path.parent() {
                std::fs::create_dir_all(pai).map_err(|e| StoreError::Other(e.to_string()))?;
            }
        }
        let conn = if memoria { Connection::open_in_memory()? } else { Connection::open(&path)? };
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let store = Store {
            path,
            conn: ReentrantMutex::new(conn),
            vectors_available: embedder.is_some(),
            embedder,
            clock,
            index: Mutex::new(vectors::VectorIndex::default()),
            reranker: Mutex::new(None),
        };
        store.init_schema()?;
        if let Some(e) = &store.embedder {
            store.ensure_vector_dim(e.dim())?;
        }
        Ok(store)
    }

    pub fn now(&self) -> f64 {
        (self.clock)()
    }

    pub fn clock(&self) -> Clock {
        self.clock.clone()
    }

    /// A conexão, travada (reentrante: a mesma thread pode pedir de novo).
    pub fn conn(&self) -> ReentrantMutexGuard<'_, Connection> {
        self.conn.lock()
    }

    /// Transação: confirma se `f` deu certo, desfaz se falhou. Dentro de outra transação da
    /// mesma thread, vira parte dela (como o `_tx` do Python com `RLock`).
    pub fn tx<T>(&self, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        let c = self.conn.lock();
        if !c.is_autocommit() {
            return f(&c);
        }
        c.execute_batch("BEGIN")?;
        match f(&c) {
            Ok(v) => {
                c.execute_batch("COMMIT")?;
                Ok(v)
            }
            Err(e) => {
                let _ = c.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    fn init_schema(&self) -> Result<()> {
        let c = self.conn();
        let existe: Option<i64> = c
            .query_row("SELECT 1 FROM sqlite_master WHERE type='table' AND name='meta'", [], |r| r.get(0))
            .optional()?;
        if existe.is_none() {
            c.execute_batch(&format!("BEGIN;{}COMMIT;", schema::ddl()))?;
            c.execute("INSERT INTO meta VALUES ('schema_version', ?)", [schema::SCHEMA_VERSION.to_string()])?;
            return Ok(());
        }
        let mut versao: u32 = c
            .query_row("SELECT value FROM meta WHERE key='schema_version'", [], |r| r.get::<_, String>(0))?
            .parse()
            .map_err(|_| StoreError::Other("schema_version inválida".into()))?;
        if versao > schema::SCHEMA_VERSION {
            return Err(StoreError::Other(format!(
                "banco no esquema v{versao}, este Orion só entende até v{}",
                schema::SCHEMA_VERSION
            )));
        }
        if versao < 17 {
            let audit: Option<i64> = c
                .query_row("SELECT 1 FROM sqlite_master WHERE type='table' AND name='audit'", [], |r| r.get(0))
                .optional()?;
            if audit.is_some() {
                // banco da linhagem antiga da `main`: o Python converte (memory/legacy_main.py)
                return Err(StoreError::Other(
                    "banco da linhagem antiga da main: abra uma vez com o Orion em Python para converter".into(),
                ));
            }
        }
        while versao < schema::SCHEMA_VERSION {
            let sql = schema::migration(versao)
                .ok_or_else(|| StoreError::Other(format!("esquema v{versao} sem migração para v{}", versao + 1)))?;
            // DDL do SQLite é transacional: falhou no meio, volta tudo
            let r = c.execute_batch(&format!(
                "BEGIN;{sql}UPDATE meta SET value='{}' WHERE key='schema_version';COMMIT;",
                versao + 1
            ));
            if let Err(e) = r {
                let _ = c.execute_batch("ROLLBACK");
                return Err(e.into());
            }
            tracing::info!("banco migrado: esquema v{} → v{}", versao, versao + 1);
            versao += 1;
        }
        Ok(())
    }

    fn meta_raw(c: &Connection, chave: &str) -> Result<Option<String>> {
        Ok(c.query_row("SELECT value FROM meta WHERE key=?", [chave], |r| r.get(0)).optional()?)
    }

    fn ensure_vector_dim(&self, dim: usize) -> Result<()> {
        self.tx(|c| {
            match Self::meta_raw(c, "embed_dim")? {
                Some(atual) if atual.parse::<usize>().ok() != Some(dim) => Err(StoreError::Other(format!(
                    "embedder de {dim} dimensões, mas o índice tem {atual}: use reset_vectors()"
                ))),
                Some(_) => Ok(()),
                None => {
                    c.execute("INSERT INTO meta VALUES ('embed_dim', ?)", [dim.to_string()])?;
                    Ok(())
                }
            }
        })
    }

    /// Descarta os vetores (troca de modelo de embedding); `embed_pending` refaz.
    pub fn reset_vectors(&self) -> Result<()> {
        self.tx(|c| {
            c.execute_batch(
                "DELETE FROM vectors; DELETE FROM meta WHERE key='embed_dim';
                 UPDATE facts SET embedded=0; UPDATE chunks SET embedded=0;",
            )?;
            Ok(())
        })?;
        self.index.lock().invalidar(None);
        if let Some(e) = &self.embedder {
            self.ensure_vector_dim(e.dim())?;
        }
        Ok(())
    }

    pub fn ping(&self) -> bool {
        self.conn().query_row("SELECT 1", [], |r| r.get::<_, i64>(0)).is_ok_and(|v| v == 1)
    }

    /// Consulta livre: linhas como objetos JSON (equivale a `dict(row)` do Python).
    pub fn query(&self, sql: &str, params: &[&dyn rusqlite::ToSql]) -> Result<Vec<Map<String, Value>>> {
        let c = self.conn();
        query_maps(&c, sql, params)
    }

    // ── contadores (cota diária das CLIs delegadas etc.) ──────────────────
    pub fn counter_get(&self, chave: &str) -> i64 {
        Self::meta_raw(&self.conn(), &format!("counter:{chave}"))
            .ok()
            .flatten()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    }

    pub fn counter_incr(&self, chave: &str, por: i64) -> Result<i64> {
        let k = format!("counter:{chave}");
        self.tx(|c| {
            c.execute(
                "INSERT INTO meta(key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = CAST(value AS INTEGER) + ?3",
                rusqlite::params![k, por.to_string(), por],
            )?;
            Ok(Self::meta_raw(c, &k)?.and_then(|v| v.parse().ok()).unwrap_or(0))
        })
    }

    pub fn counter_set(&self, chave: &str, valor: i64) -> Result<()> {
        self.tx(|c| {
            c.execute(
                "INSERT INTO meta(key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [format!("counter:{chave}"), valor.to_string()],
            )?;
            Ok(())
        })
    }

    /// Contadores cujo nome começa com `prefixo` (nome sem o prefixo interno).
    pub fn counters_with_prefix(&self, prefixo: &str) -> Result<Vec<(String, i64)>> {
        let c = self.conn();
        let mut st = c.prepare("SELECT key, value FROM meta WHERE key LIKE ? ESCAPE '\\'")?;
        let rows = st.query_map([format!("counter:{}%", text::like(prefixo))], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        let mut out = vec![];
        for r in rows {
            let (k, v) = r?;
            out.push((k["counter:".len()..].to_string(), v.parse().unwrap_or(0)));
        }
        Ok(out)
    }

    // ── valores livres no `meta` (estado dos modos) ───────────────────────
    pub fn meta_get(&self, chave: &str) -> Option<String> {
        Self::meta_raw(&self.conn(), &format!("valor:{chave}")).ok().flatten()
    }

    /// `None` apaga. Chave com prefixo próprio: não colide com contador nem com `watch:`.
    pub fn meta_set(&self, chave: &str, valor: Option<&str>) -> Result<()> {
        let k = format!("valor:{chave}");
        self.tx(|c| {
            match valor {
                None => c.execute("DELETE FROM meta WHERE key=?", [&k])?,
                Some(v) => c.execute(
                    "INSERT INTO meta(key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    [&k, v],
                )?,
            };
            Ok(())
        })
    }

    // ── backup e restauração ──────────────────────────────────────────────
    /// Cópia consistente do banco (API de backup do SQLite, com o app no ar).
    pub fn backup_to(&self, destino: &Path) -> Result<PathBuf> {
        if let Some(pai) = destino.parent() {
            std::fs::create_dir_all(pai).map_err(|e| StoreError::Other(e.to_string()))?;
        }
        let tmp = destino.with_file_name(format!(
            "{}.tmp",
            destino.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
        ));
        let _ = std::fs::remove_file(&tmp);
        {
            let mut copia = Connection::open(&tmp)?;
            let c = self.conn();
            let b = rusqlite::backup::Backup::new(&c, &mut copia)?;
            b.run_to_completion(256, std::time::Duration::from_millis(0), None)?;
        }
        std::fs::rename(&tmp, destino).map_err(|e| StoreError::Other(e.to_string()))?;
        Ok(destino.to_path_buf())
    }

    /// Um backup por dia (nome com a data), mantendo os últimos `manter`.
    pub fn daily_backup(&self, pasta: &Path, manter: usize) -> Result<Option<PathBuf>> {
        let agora = chrono::DateTime::from_timestamp(self.now() as i64, 0)
            .unwrap_or_default()
            .with_timezone(&chrono::Local);
        let hoje = agora.format("%Y%m%d").to_string();
        let _ = std::fs::create_dir_all(pasta);
        let mut existentes: Vec<PathBuf> = std::fs::read_dir(pasta)
            .map(|d| d.filter_map(|e| e.ok().map(|e| e.path())).collect())
            .unwrap_or_default();
        existentes.retain(|p| {
            let n = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            n.starts_with("orion-") && n.ends_with(".db")
        });
        let mut feito = None;
        if !existentes.iter().any(|p| p.to_string_lossy().contains(&format!("orion-{hoje}-"))) {
            let carimbo = agora.format("%Y%m%d-%H%M%S").to_string();
            let p = self.backup_to(&pasta.join(format!("orion-{carimbo}.db")))?;
            existentes.push(p.clone());
            feito = Some(p);
        }
        existentes.sort();
        if manter > 0 && existentes.len() > manter {
            for velho in &existentes[..existentes.len() - manter] {
                let _ = std::fs::remove_file(velho);
            }
        }
        Ok(feito)
    }

    /// Abre o backup só para leitura, confere integridade e devolve as contagens.
    pub fn verify_backup(arquivo: &Path) -> Result<Vec<(String, i64)>> {
        let conn = Connection::open_with_flags(arquivo, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let ok: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        if ok != "ok" {
            return invalid("backup corrompido (integrity_check)");
        }
        let versao: Option<String> = conn
            .query_row("SELECT value FROM meta WHERE key='schema_version'", [], |r| r.get(0))
            .optional()
            .map_err(|_| StoreError::Invalid("backup com esquema diferente".into()))?;
        let v: u32 = versao.and_then(|v| v.parse().ok()).unwrap_or(0);
        if !(1..=schema::SCHEMA_VERSION).contains(&v) {
            return invalid("backup com esquema diferente");
        }
        let mut tabelas = vec!["sessions", "messages", "facts", "documents", "chunks"];
        if v >= 2 {
            tabelas.extend(["reminders", "schedules", "tasks", "numbers", "prompts", "edges"]);
        }
        tabelas
            .into_iter()
            .map(|t| Ok((t.to_string(), conn.query_row(&format!("SELECT COUNT(*) FROM {t}"), [], |r| r.get(0))?)))
            .collect()
    }

    /// Confere o backup e o põe no lugar do banco (substituição atômica).
    pub fn restore(backup: &Path, destino: &Path) -> Result<Store> {
        Self::verify_backup(backup)?;
        let io = |e: std::io::Error| StoreError::Other(e.to_string());
        if let Some(pai) = destino.parent() {
            std::fs::create_dir_all(pai).map_err(io)?;
        }
        let nome = destino.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let tmp = destino.with_file_name(format!("{nome}.restore"));
        std::fs::copy(backup, &tmp).map_err(io)?;
        for sufixo in ["-wal", "-shm"] {
            let _ = std::fs::remove_file(destino.with_file_name(format!("{nome}{sufixo}")));
        }
        std::fs::rename(&tmp, destino).map_err(io)?;
        Store::open(destino)
    }
}

/// Converte um valor do SQLite para JSON.
pub fn sql_to_json(v: rusqlite::types::ValueRef<'_>) -> Value {
    use rusqlite::types::ValueRef::*;
    match v {
        Null => Value::Null,
        Integer(i) => Value::from(i),
        Real(f) => serde_json::Number::from_f64(f).map(Value::Number).unwrap_or(Value::Null),
        Text(t) => Value::String(String::from_utf8_lossy(t).into_owned()),
        Blob(b) => Value::String(hex::encode(b)),
    }
}

/// Linhas como objetos JSON, na ordem das colunas.
pub fn query_maps(c: &Connection, sql: &str, params: &[&dyn rusqlite::ToSql]) -> Result<Vec<Map<String, Value>>> {
    let mut st = c.prepare(sql)?;
    let nomes: Vec<String> = st.column_names().iter().map(|s| s.to_string()).collect();
    let mut rows = st.query(params)?;
    let mut out = vec![];
    while let Some(r) = rows.next()? {
        let mut m = Map::new();
        for (i, n) in nomes.iter().enumerate() {
            m.insert(n.clone(), sql_to_json(r.get_ref(i)?));
        }
        out.push(m);
    }
    Ok(out)
}

/// Primeira linha como objeto JSON.
pub fn query_map_one(c: &Connection, sql: &str, params: &[&dyn rusqlite::ToSql]) -> Result<Option<Map<String, Value>>> {
    Ok(query_maps(c, sql, params)?.into_iter().next())
}

/// `uuid.uuid4().hex`.
pub fn uuid_hex() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// `json.dumps` do Python (separadores com espaço), para colunas JSON compartilhadas.
pub fn py_json(v: &Value) -> String {
    orion_policy::pyjson::dumps_ascii(v, false)
}
