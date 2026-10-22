//! Cifra do banco com SQLCipher (só com a feature `sqlcipher`).

use rusqlite::Connection;
use std::path::Path;

/// Aplica a chave bruta de 256 bits e confere que ela abre o banco. O `PRAGMA key` precisa ser o
/// primeiro comando da conexão; a leitura de `sqlite_master` falha com chave errada.
pub fn apply_key(conn: &Connection, key: &[u8; 32]) -> rusqlite::Result<()> {
    // formato `x'hex'` = chave bruta, sem derivação PBKDF2 do SQLCipher
    conn.pragma_update(None, "key", format!("x'{}'", hex::encode(key)))?;
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0)).map(|_| ())
}

/// Copia um banco em claro para um arquivo cifrado novo (`sqlcipher_export`). O original não é
/// tocado: quem chama troca os arquivos só depois de abrir o cifrado e conferir.
pub fn encrypt_copy(plain: &Path, encrypted: &Path, key: &[u8; 32]) -> rusqlite::Result<()> {
    if encrypted.exists() {
        return Err(rusqlite::Error::InvalidPath(encrypted.to_path_buf()));
    }
    let conn = Connection::open(plain)?;
    conn.execute("ATTACH DATABASE ?1 AS cifrado KEY ?2", rusqlite::params![encrypted.to_string_lossy(), format!("x'{}'", hex::encode(key))])?;
    conn.query_row("SELECT sqlcipher_export('cifrado')", [], |_| Ok(()))?;
    conn.execute("DETACH DATABASE cifrado", [])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Store;

    const KEY: [u8; 32] = [7; 32];

    #[test]
    fn banco_cifrado_nao_abre_sem_chave_nem_com_chave_errada() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("orion.db");
        {
            let s = Store::open_encrypted(&path, &KEY, None, orion_policy::wall_clock()).unwrap();
            s.conn().execute("CREATE TABLE segredo (v TEXT)", []).unwrap();
            s.conn().execute("INSERT INTO segredo VALUES ('texto-que-nao-pode-vazar')", []).unwrap();
        }
        let bytes = std::fs::read(&path).unwrap();
        assert!(!bytes.starts_with(b"SQLite format 3"), "arquivo não pode ser SQLite em claro");
        assert!(!bytes.windows(10).any(|w| w == b"texto-que-"), "conteúdo vazou em claro");
        assert!(Store::open(&path).is_err(), "sem chave não abre");
        assert!(Store::open_encrypted(&path, &[8; 32], None, orion_policy::wall_clock()).is_err(), "chave errada não abre");
        let s = Store::open_encrypted(&path, &KEY, None, orion_policy::wall_clock()).unwrap();
        let v: String = s.conn().query_row("SELECT v FROM segredo", [], |r| r.get(0)).unwrap();
        assert_eq!(v, "texto-que-nao-pode-vazar");
    }

    #[test]
    fn backup_de_banco_cifrado_sai_cifrado_com_a_mesma_chave() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::open_encrypted(dir.path().join("orion.db"), &KEY, None, orion_policy::wall_clock()).unwrap();
        s.conn().execute("CREATE TABLE t (v TEXT)", []).unwrap();
        s.conn().execute("INSERT INTO t VALUES ('so-no-backup-cifrado')", []).unwrap();
        let bak = s.backup_to(&dir.path().join("b").join("orion-x.db")).unwrap();
        let bytes = std::fs::read(&bak).unwrap();
        assert!(!bytes.starts_with(b"SQLite format 3"), "backup não pode ser SQLite em claro");
        assert!(!bytes.windows(10).any(|w| w == b"so-no-back"));
        assert!(Store::verify_backup(&bak).is_err(), "conferir backup cifrado ainda não é suportado, e diz isso");
        let r = Store::open_encrypted(&bak, &KEY, None, orion_policy::wall_clock()).unwrap();
        let v: String = r.conn().query_row("SELECT v FROM t", [], |r| r.get(0)).unwrap();
        assert_eq!(v, "so-no-backup-cifrado");
    }

    #[test]
    fn migra_banco_em_claro_para_cifrado_sem_perder_dados() {
        let dir = tempfile::tempdir().unwrap();
        let (plain, enc) = (dir.path().join("claro.db"), dir.path().join("cifrado.db"));
        {
            let s = Store::open(&plain).unwrap();
            s.conn().execute("CREATE TABLE t (v TEXT)", []).unwrap();
            s.conn().execute("INSERT INTO t VALUES ('dado')", []).unwrap();
        }
        encrypt_copy(&plain, &enc, &KEY).unwrap();
        assert!(Store::open(&plain).is_ok(), "original intacto");
        let s = Store::open_encrypted(&enc, &KEY, None, orion_policy::wall_clock()).unwrap();
        let v: String = s.conn().query_row("SELECT v FROM t", [], |r| r.get(0)).unwrap();
        assert_eq!(v, "dado");
        assert!(encrypt_copy(&plain, &enc, &KEY).is_err(), "não sobrescreve cifrado existente");
    }
}
