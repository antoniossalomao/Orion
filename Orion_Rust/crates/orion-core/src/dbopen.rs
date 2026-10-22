//! Abertura do banco da memória, cifrado ou não (F02.01).
//!
//! Se existe `orion.key` ao lado do `orion.db`, o banco é SQLCipher e só abre com a chave mestra,
//! que sai da senha (ou do código de recuperação) e fica **só na memória** deste processo. Sem
//! `orion.key` o comportamento é o de antes (SQLite em claro, compartilhado com o Python).

use crate::auth::SENHA_MIN;
#[cfg_attr(not(feature = "sqlcipher"), allow(unused_imports))]
use crate::dbkey::{self, Kdf, KeyError, KeyFile, MasterKey, KEY_FILE};
use orion_store::{Store, StoreError};
use parking_lot::Mutex;
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

static MASTER: Mutex<Option<MasterKey>> = Mutex::new(None);

pub fn key_path(db: &Path) -> PathBuf {
    db.with_file_name(KEY_FILE)
}

fn tmp_path(db: &Path) -> PathBuf {
    db.with_extension("db.enc")
}

pub fn plain_backup_path(db: &Path) -> PathBuf {
    db.with_extension("db.plain.bak")
}

/// O banco foi cifrado? (Existe `orion.key` e o `orion.db` não é SQLite em claro.)
pub fn is_encrypted(db: &Path) -> bool {
    key_path(db).exists() && !is_plain_sqlite(db)
}

fn is_plain_sqlite(db: &Path) -> bool {
    use std::io::Read;
    let mut head = [0u8; 16];
    std::fs::File::open(db).and_then(|mut f| f.read_exact(&mut head)).is_ok() && &head == b"SQLite format 3\0"
}

pub fn is_unlocked() -> bool {
    MASTER.lock().is_some()
}

/// Apaga a chave da memória deste processo (bloqueio). Conexões já abertas continuam com a chave
/// que o SQLCipher guarda por dentro; para trancar de verdade, o processo precisa terminar.
pub fn lock() {
    *MASTER.lock() = None;
}

pub fn master_key() -> Option<MasterKey> {
    MASTER.lock().as_ref().map(|k| Zeroizing::new(**k))
}

fn guard(k: MasterKey) {
    *MASTER.lock() = Some(k);
}

pub fn unlock_with_password(db: &Path, password: &str) -> Result<(), KeyError> {
    guard(KeyFile::load(&key_path(db))?.unlock_password(password)?);
    Ok(())
}

pub fn unlock_with_recovery(db: &Path, code: &str) -> Result<(), KeyError> {
    guard(KeyFile::load(&key_path(db))?.unlock_recovery(code)?);
    Ok(())
}

/// Retoma uma migração interrompida entre as duas renomeações.
fn finish_interrupted(db: &Path) {
    if !db.exists() && tmp_path(db).exists() && key_path(db).exists() {
        let _ = std::fs::rename(tmp_path(db), db);
    }
}

pub fn open_store(db: &Path) -> orion_store::Result<Store> {
    open_store_with(db, None, orion_policy::wall_clock())
}

pub fn open_store_with(db: &Path, embedder: Option<std::sync::Arc<dyn orion_store::Embedder>>, clock: orion_policy::Clock) -> orion_store::Result<Store> {
    finish_interrupted(db);
    if !is_encrypted(db) {
        return Store::open_with(db, embedder, clock);
    }
    #[cfg(feature = "sqlcipher")]
    {
        let key = master_key().ok_or_else(|| StoreError::Other("banco cifrado e bloqueado: informe a senha (ORION_DB_PASSWORD ou o prompt)".into()))?;
        Store::open_encrypted(db, &key, embedder, clock)
    }
    #[cfg(not(feature = "sqlcipher"))]
    {
        let _ = (embedder, clock);
        Err(StoreError::Other("o banco é cifrado, mas este executável foi compilado sem SQLCipher".into()))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MigrateError {
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Key(#[from] KeyError),
    #[error("banco: {0}")]
    Db(String),
    #[error("disco: {0}")]
    Io(String),
}

fn io(e: std::io::Error) -> MigrateError {
    MigrateError::Io(e.to_string())
}

/// Cifra o `orion.db` existente. O original em claro vira `orion.db.plain.bak` (apague com
/// [`delete_plain_backup`] depois de conferir). Devolve o código de recuperação, que deve ser
/// mostrado uma única vez. Falha sem tocar em nada se o banco já é cifrado.
#[cfg(feature = "sqlcipher")]
pub fn encrypt_database(db: &Path, password: &str, kdf: Kdf) -> Result<Zeroizing<String>, MigrateError> {
    if password.chars().count() < SENHA_MIN {
        return Err(MigrateError::Invalid(format!("a senha precisa de pelo menos {SENHA_MIN} caracteres")));
    }
    if is_encrypted(db) {
        return Err(MigrateError::Invalid("o banco já está cifrado".into()));
    }
    if !is_plain_sqlite(db) {
        return Err(MigrateError::Invalid(format!("{} não é um banco SQLite em claro", db.display())));
    }
    // um orion.key sem banco cifrado é sobra de tentativa interrompida (o código nunca foi mostrado)
    let _ = std::fs::remove_file(key_path(db));
    let _ = std::fs::remove_file(tmp_path(db));

    let created = dbkey::create(password, kdf)?;
    {
        let conn = orion_store::rusqlite::Connection::open(db).map_err(|e| MigrateError::Db(e.to_string()))?;
        // zera o WAL: sem isso parte dos dados ficaria só no -wal e fora da cópia cifrada
        conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(())).map_err(|e| MigrateError::Db(e.to_string()))?;
    }
    orion_store::crypto::encrypt_copy(db, &tmp_path(db), &created.master).map_err(|e| MigrateError::Db(e.to_string()))?;

    // confere que a cópia abre e tem as mesmas tabelas antes de trocar qualquer arquivo
    let contar = |c: &orion_store::rusqlite::Connection| c.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0));
    let esperado = contar(&orion_store::rusqlite::Connection::open(db).map_err(|e| MigrateError::Db(e.to_string()))?).map_err(|e| MigrateError::Db(e.to_string()))?;
    let cifrado = Store::open_encrypted(&tmp_path(db), &created.master, None, orion_policy::wall_clock()).map_err(|e| MigrateError::Db(e.to_string()))?;
    let obtido = contar(&cifrado.conn()).map_err(|e| MigrateError::Db(e.to_string()))?;
    drop(cifrado);
    if esperado != obtido {
        let _ = std::fs::remove_file(tmp_path(db));
        return Err(MigrateError::Db(format!("a cópia cifrada tem {obtido} objetos, o original {esperado}")));
    }

    // troca: chave primeiro (sem ela o cifrado seria irrecuperável), depois os dois arquivos
    created.file.save(&key_path(db))?;
    let bak = plain_backup_path(db);
    let _ = std::fs::remove_file(&bak);
    std::fs::rename(db, &bak).map_err(io)?;
    std::fs::rename(tmp_path(db), db).map_err(io)?;
    // -wal e -shm do banco antigo não podem ser aplicados ao novo
    for ext in ["db-wal", "db-shm"] {
        let _ = std::fs::remove_file(db.with_extension(ext));
    }
    guard(created.master.clone());
    Ok(created.recovery_code)
}

/// Sobrescreve com zeros e apaga o `orion.db.plain.bak`. (Em SSD isso não garante apagar de vez;
/// vale como higiene, não como destruição forense.)
pub fn delete_plain_backup(db: &Path) -> Result<bool, MigrateError> {
    use std::io::Write;
    let bak = plain_backup_path(db);
    let Ok(meta) = std::fs::metadata(&bak) else { return Ok(false) };
    let mut f = std::fs::OpenOptions::new().write(true).open(&bak).map_err(io)?;
    let zeros = [0u8; 64 * 1024];
    let mut resta = meta.len();
    while resta > 0 {
        let n = resta.min(zeros.len() as u64) as usize;
        f.write_all(&zeros[..n]).map_err(io)?;
        resta -= n as u64;
    }
    f.sync_all().map_err(io)?;
    drop(f);
    std::fs::remove_file(&bak).map_err(io)?;
    Ok(true)
}

/// Troca a senha do banco (a chave mestra e o banco não mudam). Exige a senha atual.
pub fn change_password(db: &Path, current: &str, new: &str) -> Result<(), MigrateError> {
    if new.chars().count() < SENHA_MIN {
        return Err(MigrateError::Invalid(format!("a senha precisa de pelo menos {SENHA_MIN} caracteres")));
    }
    let mut file = KeyFile::load(&key_path(db))?;
    let master = file.unlock_password(current)?;
    file.change_password(&master, new)?;
    file.save(&key_path(db))?;
    Ok(())
}

/// Troca a senha usando o código de recuperação (esqueci a senha). Exige o código.
pub fn reset_password_with_recovery(db: &Path, code: &str, new: &str) -> Result<(), MigrateError> {
    if new.chars().count() < SENHA_MIN {
        return Err(MigrateError::Invalid(format!("a senha precisa de pelo menos {SENHA_MIN} caracteres")));
    }
    let mut file = KeyFile::load(&key_path(db))?;
    let master = file.unlock_recovery(code)?;
    file.change_password(&master, new)?;
    file.save(&key_path(db))?;
    Ok(())
}

/// Gera outro código de recuperação (o anterior deixa de valer). Exige a senha.
pub fn new_recovery_code(db: &Path, password: &str) -> Result<Zeroizing<String>, MigrateError> {
    let mut file = KeyFile::load(&key_path(db))?;
    let master = file.unlock_password(password)?;
    let code = file.new_recovery(&master)?;
    file.save(&key_path(db))?;
    Ok(code)
}

#[cfg(all(test, feature = "sqlcipher"))]
mod tests {
    use super::*;

    /// A chave mestra é global do processo: os testes que a usam rodam um de cada vez.
    static SERIAL: Mutex<()> = Mutex::new(());

    fn banco_em_claro(dir: &Path) -> PathBuf {
        let db = dir.join("orion.db");
        let s = Store::open(&db).unwrap();
        s.conn().execute("CREATE TABLE extra (v TEXT)", []).unwrap();
        s.conn().execute("INSERT INTO extra VALUES ('segredo-da-memoria')", []).unwrap();
        db
    }

    #[test]
    fn cifra_o_banco_existente_e_so_abre_com_senha_ou_recuperacao() {
        let _g = SERIAL.lock();
        let tmp = tempfile::tempdir().unwrap();
        let db = banco_em_claro(tmp.path());
        let code = encrypt_database(&db, "senha-forte-123", Kdf::TESTE).unwrap();
        assert!(is_encrypted(&db));
        assert!(!std::fs::read(&db).unwrap().windows(18).any(|w| w == b"segredo-da-memoria"));
        lock();
        assert!(open_store(&db).is_err(), "bloqueado não abre");
        assert!(unlock_with_password(&db, "senha-errada-123").is_err() && !is_unlocked());
        unlock_with_password(&db, "senha-forte-123").unwrap();
        let v: String = open_store(&db).unwrap().conn().query_row("SELECT v FROM extra", [], |r| r.get(0)).unwrap();
        assert_eq!(v, "segredo-da-memoria");
        lock();
        unlock_with_recovery(&db, &code).unwrap();
        assert!(open_store(&db).is_ok());
        lock();
        assert!(master_key().is_none());
        // original em claro guardado até o dono apagar
        assert!(plain_backup_path(&db).exists());
        assert!(delete_plain_backup(&db).unwrap() && !plain_backup_path(&db).exists());
        assert!(!delete_plain_backup(&db).unwrap());
    }

    #[test]
    fn nao_cifra_duas_vezes_nem_com_senha_fraca() {
        let _g = SERIAL.lock();
        let tmp = tempfile::tempdir().unwrap();
        let db = banco_em_claro(tmp.path());
        assert!(matches!(encrypt_database(&db, "curta", Kdf::TESTE), Err(MigrateError::Invalid(_))));
        assert!(!key_path(&db).exists() && is_plain_sqlite(&db));
        encrypt_database(&db, "senha-forte-123", Kdf::TESTE).unwrap();
        assert!(matches!(encrypt_database(&db, "senha-forte-123", Kdf::TESTE), Err(MigrateError::Invalid(_))));
    }

    #[test]
    fn tentativa_interrompida_com_chave_sobrando_pode_ser_refeita() {
        let _g = SERIAL.lock();
        let tmp = tempfile::tempdir().unwrap();
        let db = banco_em_claro(tmp.path());
        dbkey::create("outra-senha-123", Kdf::TESTE).unwrap().file.save(&key_path(&db)).unwrap();
        assert!(!is_encrypted(&db), "chave sem banco cifrado não conta");
        encrypt_database(&db, "senha-forte-123", Kdf::TESTE).unwrap();
        lock();
        assert!(unlock_with_password(&db, "outra-senha-123").is_err());
        unlock_with_password(&db, "senha-forte-123").unwrap();
    }

    #[test]
    fn troca_de_senha_e_novo_codigo() {
        let _g = SERIAL.lock();
        let tmp = tempfile::tempdir().unwrap();
        let db = banco_em_claro(tmp.path());
        let velho = encrypt_database(&db, "senha-forte-123", Kdf::TESTE).unwrap();
        assert!(change_password(&db, "errada-errada-1", "nova-senha-456").is_err());
        assert!(change_password(&db, "senha-forte-123", "curta").is_err());
        change_password(&db, "senha-forte-123", "nova-senha-456").unwrap();
        lock();
        assert!(unlock_with_password(&db, "senha-forte-123").is_err());
        unlock_with_password(&db, "nova-senha-456").unwrap();
        let novo = new_recovery_code(&db, "nova-senha-456").unwrap();
        lock();
        assert!(unlock_with_recovery(&db, &velho).is_err());
        reset_password_with_recovery(&db, &novo, "reset-senha-789").unwrap();
        unlock_with_password(&db, "reset-senha-789").unwrap();
        assert!(open_store(&db).is_ok());
        lock();
    }
}
