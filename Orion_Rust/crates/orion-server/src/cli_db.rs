//! Banco cifrado: senha na subida, migração, troca de senha, recuperação, logs cifrados e
//! bloqueio quando o Windows trava (F02). A lógica fica em `orion_core::{dbkey, dbopen, logcrypt}`.

use orion_core::config::Settings;
#[cfg(feature = "sqlcipher")]
use orion_core::dbkey::Kdf;
use orion_core::dbkey::KeyError;
use orion_core::dbopen::{self, MigrateError};
use std::io::IsTerminal;

fn senha_nova(stdin: bool) -> anyhow::Result<String> {
    if stdin {
        return Ok(super::ler_linha());
    }
    let a = rpassword::prompt_password("Senha nova do banco (mínimo 12 caracteres): ")?;
    if a != rpassword::prompt_password("Repita a senha: ")? {
        anyhow::bail!("as senhas não conferem");
    }
    Ok(a)
}

fn pedir(msg: &str, stdin: bool) -> anyhow::Result<String> {
    Ok(if stdin { super::ler_linha() } else { rpassword::prompt_password(msg)? })
}

/// Destrava o banco se ele for cifrado: `ORION_DB_PASSWORD` (e já tira do ambiente do processo) ou
/// o prompt no terminal. Sem nenhum dos dois, falha em vez de abrir algo em claro.
pub fn unlock_if_needed(s: &Settings) -> anyhow::Result<()> {
    let db = s.db_path();
    if !dbopen::is_encrypted(&db) || dbopen::is_unlocked() {
        return Ok(());
    }
    if let Ok(senha) = std::env::var("ORION_DB_PASSWORD") {
        std::env::remove_var("ORION_DB_PASSWORD");
        return dbopen::unlock_with_password(&db, &senha).map_err(|e| anyhow::anyhow!("{e}"));
    }
    if !std::io::stdin().is_terminal() {
        anyhow::bail!("o banco é cifrado: rode num terminal ou defina ORION_DB_PASSWORD");
    }
    for _ in 0..3 {
        let senha = rpassword::prompt_password("Senha do banco do Orion: ")?;
        match dbopen::unlock_with_password(&db, &senha) {
            Ok(()) => return Ok(()),
            Err(KeyError::Wrong) => eprintln!("senha incorreta"),
            Err(e) => anyhow::bail!("{e}"),
        }
    }
    anyhow::bail!("3 tentativas erradas")
}

/// Log cifrado em arquivo, só quando o banco é cifrado e está destravado.
pub fn encrypted_log(s: &Settings) -> Option<orion_core::logcrypt::EncryptedLog> {
    let master = dbopen::master_key()?;
    orion_core::logcrypt::EncryptedLog::new(&s.logs_dir(), &master, s.log_retention_days).ok()
}

#[cfg(feature = "sqlcipher")]
pub fn cifrar(s: &Settings, stdin: bool) -> anyhow::Result<i32> {
    let db = s.db_path();
    if dbopen::is_encrypted(&db) {
        eprintln!("o banco já está cifrado");
        return Ok(1);
    }
    println!("Pare o Orion antes de cifrar o banco. Um orion.db.plain.bak em claro fica ao lado até você apagá-lo (orion db-apagar-claro).");
    let senha = senha_nova(stdin)?;
    match dbopen::encrypt_database(&db, &senha, Kdf::PLENO) {
        Ok(codigo) => {
            println!("\nBanco cifrado.\n\nCHAVE DE RECUPERAÇÃO (aparece só agora; guarde fora deste PC):\n\n    {}\n\nSem a senha e sem este código, os dados não abrem.", *codigo);
            Ok(0)
        }
        Err(e) => {
            eprintln!("{e}");
            Ok(1)
        }
    }
}

#[cfg(not(feature = "sqlcipher"))]
pub fn cifrar(_: &Settings, _: bool) -> anyhow::Result<i32> {
    eprintln!("este executável foi compilado sem SQLCipher (feature 'sqlcipher')");
    Ok(1)
}

pub fn trocar_senha(s: &Settings, stdin: bool) -> anyhow::Result<i32> {
    let db = s.db_path();
    if !dbopen::is_encrypted(&db) {
        eprintln!("o banco não está cifrado");
        return Ok(1);
    }
    let atual = pedir("Senha atual do banco: ", stdin)?;
    let nova = senha_nova(stdin)?;
    Ok(resultado(dbopen::change_password(&db, &atual, &nova), "senha do banco trocada"))
}

pub fn recuperar(s: &Settings, stdin: bool) -> anyhow::Result<i32> {
    let db = s.db_path();
    if !dbopen::is_encrypted(&db) {
        eprintln!("o banco não está cifrado");
        return Ok(1);
    }
    let codigo = pedir("Chave de recuperação: ", stdin)?;
    let nova = senha_nova(stdin)?;
    Ok(resultado(dbopen::reset_password_with_recovery(&db, &codigo, &nova), "senha do banco redefinida"))
}

pub fn nova_recuperacao(s: &Settings, stdin: bool) -> anyhow::Result<i32> {
    let db = s.db_path();
    if !dbopen::is_encrypted(&db) {
        eprintln!("o banco não está cifrado");
        return Ok(1);
    }
    let senha = pedir("Senha do banco: ", stdin)?;
    match dbopen::new_recovery_code(&db, &senha) {
        Ok(c) => {
            println!("NOVA CHAVE DE RECUPERAÇÃO (a anterior deixou de valer):\n\n    {}\n", *c);
            Ok(0)
        }
        Err(e) => {
            eprintln!("{e}");
            Ok(1)
        }
    }
}

pub fn apagar_claro(s: &Settings) -> anyhow::Result<i32> {
    let db = s.db_path();
    if !dbopen::is_encrypted(&db) {
        eprintln!("o banco não está cifrado: o backup em claro só existe depois de cifrar");
        return Ok(1);
    }
    match dbopen::delete_plain_backup(&db) {
        Ok(true) => println!("backup em claro sobrescrito e apagado"),
        Ok(false) => println!("não há backup em claro"),
        Err(e) => {
            eprintln!("{e}");
            return Ok(1);
        }
    }
    Ok(0)
}

fn resultado(r: Result<(), MigrateError>, ok: &str) -> i32 {
    match r {
        Ok(()) => {
            println!("{ok}");
            0
        }
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

/// `orion logs [--dia AAAA-MM-DD]`: lê os logs cifrados (precisa do banco destravado).
pub fn ler_logs(s: &Settings, dia: Option<&str>) -> anyhow::Result<i32> {
    let Some(master) = dbopen::master_key() else {
        eprintln!("logs cifrados só existem com o banco cifrado");
        return Ok(1);
    };
    let alvo = match dia {
        Some(d) => vec![s.logs_dir().join(format!("orion-{d}.log.enc"))],
        None => orion_core::logcrypt::files(&s.logs_dir()).into_iter().rev().take(1).collect(),
    };
    for arq in alvo {
        for linha in orion_core::logcrypt::read_file(&arq, &master)? {
            println!("{linha}");
        }
    }
    Ok(0)
}

/// Quando o Windows bloqueia (Win+L): encerra as sessões, apaga a chave da memória e, se
/// `lock_exit`, encerra o processo (o SQLCipher só solta a chave ao fechar as conexões).
#[cfg(windows)]
pub fn watch_screen_lock(s: &Settings, auth: std::sync::Arc<orion_core::auth::AuthService>) {
    if !s.lock_on_screen_lock || (!dbopen::is_encrypted(&s.db_path()) && orion_core::onboarding::Profile::load(&s.data_dir).ok().flatten().is_none()) {
        return;
    }
    let sair = s.lock_exit;
    std::thread::spawn(move || {
        let mut travado = false;
        let mut seguidas = 0u8;
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
            // Duas leituras seguidas: uma só pode ser um instante de troca de área de trabalho.
            let lido = orion_platform::session_locked();
            seguidas = if lido { seguidas.saturating_add(1) } else { 0 };
            let agora = seguidas >= 2;
            if agora && !travado {
                let n = auth.revoke_all();
                dbopen::lock();
                *crate::onboarding::BACKUP_KEY.lock()=None;
                tracing::warn!("Windows bloqueado: {n} sessão(ões) encerrada(s) e chave apagada da memória");
                if sair {
                    std::process::exit(0);
                }
            }
            travado = agora;
        }
    });
}
