//! Arquivo de backup autenticado: banco + perfil, sem auth.db, APIs ou tokens.
//! A chave mestra aleatória é embrulhada por senha/recuperação, inclusive com banco em claro.
use crate::dbkey::{KeyFile, MasterKey};
use anyhow::{ensure, Context};
use base64::Engine;
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Key, Nonce,
};
use rand::RngCore;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn encode(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}
fn decode(v: &Value) -> anyhow::Result<Vec<u8>> {
    Ok(base64::engine::general_purpose::STANDARD.decode(v.as_str().context("backup inválido")?)?)
}

pub fn save(store: &orion_store::Store, data: &Path, folder: &Path, key: &MasterKey) -> anyhow::Result<PathBuf> {
    snapshot(store, data, folder, key, format!("orion-{}.orion-backup", chrono::Local::now().format("%Y-%m-%d")))
}

/// Exportação manual sempre captura o estado atual, sem reutilizar a cópia diária.
pub fn export(store: &orion_store::Store, data: &Path, folder: &Path, key: &MasterKey) -> anyhow::Result<PathBuf> {
    snapshot(
        store,
        data,
        folder,
        key,
        format!("orion-{}-{}.orion-backup", chrono::Local::now().format("%Y-%m-%d-%H%M%S"), uuid::Uuid::new_v4().simple()),
    )
}
fn snapshot(store: &orion_store::Store, data: &Path, folder: &Path, key: &MasterKey, name: String) -> anyhow::Result<PathBuf> {
    std::fs::create_dir_all(folder)?;
    let destination = folder.join(name);
    if destination.exists() {
        decrypt_with_key(&std::fs::read(&destination)?, key)?;
        return Ok(destination);
    }
    let temp = tempfile::tempdir_in(data)?;
    let db = store.backup_to(&temp.path().join("orion.db"))?;
    let keypath = if crate::dbopen::is_encrypted(&data.join("orion.db")) {
        data.join(crate::dbkey::KEY_FILE)
    } else {
        data.join("orion-recovery.key")
    };
    let wrapped: Value = serde_json::from_slice(&std::fs::read(keypath)?)?;
    let profile = std::fs::read(data.join(crate::onboarding::FILE))?;
    let plain = zeroize::Zeroizing::new(json!({"database":encode(&std::fs::read(db)?),"profile":encode(&profile)}).to_string());
    let mut nonce = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce);
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key.as_ref()))
        .encrypt(Nonce::from_slice(&nonce), plain.as_bytes())
        .map_err(|_| anyhow::anyhow!("não foi possível cifrar o backup"))?;
    let file = serde_json::to_vec(&json!({"version":1,"key":wrapped,"nonce":encode(&nonce),"ciphertext":encode(&cipher)}))?;
    let mut out = tempfile::NamedTempFile::new_in(folder)?;
    use std::io::Write;
    out.write_all(&file)?;
    out.as_file().sync_all()?;
    out.persist_noclobber(&destination)?;
    // Confere o conteúdo antes de podar cópias anteriores.
    let restored = decrypt_with_key(&std::fs::read(&destination)?, key)?;
    ensure!(restored["profile"].as_str().is_some(), "a verificação do backup falhou");
    let mut files = std::fs::read_dir(folder)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name().is_some_and(|n| {
                let n = n.to_string_lossy();
                n.starts_with("orion-") && n.ends_with(".orion-backup")
            })
        })
        .collect::<Vec<_>>();
    files.sort();
    let remove = files.len().saturating_sub(7);
    for file in files.into_iter().take(remove) {
        std::fs::remove_file(file)?;
    }
    Ok(destination)
}
fn decrypt_with_key(bytes: &[u8], key: &MasterKey) -> anyhow::Result<Value> {
    let envelope: Value = serde_json::from_slice(bytes)?;
    ensure!(envelope["version"] == 1, "versão de backup inválida");
    let nonce = decode(&envelope["nonce"])?;
    ensure!(nonce.len() == 12, "nonce inválido");
    let cipher = decode(&envelope["ciphertext"])?;
    let plain = zeroize::Zeroizing::new(
        ChaCha20Poly1305::new(Key::from_slice(key.as_ref()))
            .decrypt(Nonce::from_slice(&nonce), cipher.as_slice())
            .map_err(|_| anyhow::anyhow!("senha incorreta ou backup alterado"))?,
    );
    Ok(serde_json::from_slice(&plain)?)
}

/// Abre e verifica em memória. A restauração nunca substitui uma instalação existente.
pub fn restore(backup: &Path, target: &Path, password: &str, recovery: bool, new_password: &str) -> anyhow::Result<()> {
    ensure!(
        (crate::auth::SENHA_MIN..=crate::auth::SENHA_MAX).contains(&new_password.chars().count()),
        "a nova senha precisa ter de 12 a 256 caracteres"
    );
    ensure!(!target.exists(), "a pasta de restauração já existe; escolha uma pasta nova");
    let bytes = std::fs::read(backup)?;
    let envelope: Value = serde_json::from_slice(&bytes)?;
    let mut wrapped: KeyFile = serde_json::from_value(envelope["key"].clone())?;
    let key = if recovery {
        wrapped.unlock_recovery(password)?
    } else {
        wrapped.unlock_password(password)?
    };
    let content = decrypt_with_key(&bytes, &key)?;
    let db = decode(&content["database"])?;
    let profile = decode(&content["profile"])?;
    let mut local_profile: crate::onboarding::Profile = serde_json::from_slice(&profile)?;
    let parent = target.parent().context("pasta de restauração inválida")?;
    let stage = tempfile::tempdir_in(parent)?;
    std::fs::write(stage.path().join("orion.db"), &db)?;
    let encrypted = !db.starts_with(b"SQLite format 3\0");
    wrapped.change_password(&key, new_password)?;
    wrapped.save(&stage.path().join(if encrypted { crate::dbkey::KEY_FILE } else { "orion-recovery.key" }))?;
    #[cfg(feature = "sqlcipher")]
    if encrypted {
        drop(orion_store::Store::open_encrypted(&stage.path().join("orion.db"), &key, None, orion_policy::wall_clock())?);
    }
    #[cfg(not(feature = "sqlcipher"))]
    ensure!(!encrypted, "este build não abre bancos SQLCipher");
    if !encrypted {
        drop(orion_store::Store::open(&stage.path().join("orion.db"))?);
    }
    {
        let auth = crate::auth::AuthService::open(&stage.path().join("auth.db"), &local_profile.user, 30.0 * 86400.0)?;
        auth.set_password(new_password)?;
    }
    // Credenciais antigas e tokens externos não entram no backup.
    local_profile.hello = false;
    local_profile.telegram_2fa = false;
    local_profile.drive_backup = false;
    local_profile.backup = Some(target.join("backups"));
    std::fs::write(stage.path().join(crate::onboarding::FILE), serde_json::to_vec_pretty(&local_profile)?)?;
    std::fs::rename(stage.path(), target)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn backup_cifrado_recupera_dados_sem_sobrescrever() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir(&data).unwrap();
        let keys = crate::dbkey::create("senha-do-teste-123", crate::dbkey::Kdf::TESTE).unwrap();
        keys.file.save(&data.join("orion-recovery.key")).unwrap();
        let profile = crate::onboarding::Profile {
            version: 1,
            user: "antonio".into(),
            name: "Antônio".into(),
            language: "pt-BR".into(),
            vault: None,
            projects: None,
            backup: None,
            telegram_users: vec![],
            telegram_2fa: false,
            drive_backup: false,
            hello: false,
        };
        std::fs::write(data.join(crate::onboarding::FILE), serde_json::to_vec(&profile).unwrap()).unwrap();
        let store = orion_store::Store::open(&data.join("orion.db")).unwrap();
        store
            .conn()
            .execute_batch("CREATE TABLE prova(v TEXT);INSERT INTO prova VALUES('dado-pessoal-de-teste')")
            .unwrap();
        let archive = save(&store, &data, &dir.path().join("backups"), &keys.master).unwrap();
        assert!(!String::from_utf8_lossy(&std::fs::read(&archive).unwrap()).contains("dado-pessoal-de-teste"));
        assert!(restore(&archive, &dir.path().join("errado"), "senha-errada", false, "nova-senha-do-teste").is_err());
        let target = dir.path().join("restaurado");
        restore(&archive, &target, &keys.recovery_code, true, "nova-senha-do-teste").unwrap();
        assert!(restore(&archive, &target, &keys.recovery_code, true, "nova-senha-do-teste").is_err());
        let restored = orion_store::Store::open(&target.join("orion.db")).unwrap();
        let value: String = restored.conn().query_row("SELECT v FROM prova", [], |r| r.get(0)).unwrap();
        assert_eq!(value, "dado-pessoal-de-teste");
    }
}
