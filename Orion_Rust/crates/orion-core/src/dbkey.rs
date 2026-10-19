//! Chave mestra do banco (F02.01, F02.10).
//!
//! A chave de 256 bits do banco é **aleatória** e nunca é a senha. Ela fica no arquivo `orion.key`
//! embrulhada duas vezes (ChaCha20-Poly1305): uma pela chave derivada da senha (Argon2id) e outra
//! pela derivada da **chave de recuperação**, mostrada uma única vez. Trocar a senha só reembrulha a
//! chave mestra, sem recifrar o banco. O arquivo não contém nada que abra o banco sem a senha ou o
//! código de recuperação.

use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine as _;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::path::Path;
use zeroize::Zeroizing;

pub type MasterKey = Zeroizing<[u8; 32]>;

pub const KEY_FILE: &str = "orion.key";
const VERSAO: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    #[error("senha ou chave de recuperação incorreta")]
    Wrong,
    #[error("arquivo de chave inválido: {0}")]
    Invalid(String),
    #[error("disco: {0}")]
    Io(String),
}

type Result<T> = std::result::Result<T, KeyError>;

/// Custo do Argon2id. `Teste` só existe para os testes não gastarem 19 MiB por derivação.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Kdf {
    pub m_kib: u32,
    pub t: u32,
}

impl Kdf {
    /// OWASP 2023: 19 MiB, 2 passes.
    pub const PLENO: Kdf = Kdf { m_kib: 19 * 1024, t: 2 };
    pub const TESTE: Kdf = Kdf { m_kib: 8, t: 1 };
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Wrapped {
    salt: String,
    nonce: String,
    ct: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyFile {
    version: u32,
    kdf: Kdf,
    password: Wrapped,
    recovery: Wrapped,
}

fn b64(b: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD_NO_PAD.encode(b)
}

fn unb64(s: &str) -> Result<Vec<u8>> {
    base64::engine::general_purpose::STANDARD_NO_PAD.decode(s).map_err(|e| KeyError::Invalid(e.to_string()))
}

fn kek(segredo: &[u8], salt: &[u8], kdf: Kdf) -> Result<Zeroizing<[u8; 32]>> {
    let params = Params::new(kdf.m_kib, kdf.t, 1, None).map_err(|e| KeyError::Invalid(e.to_string()))?;
    let mut out = Zeroizing::new([0u8; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params).hash_password_into(segredo, salt, out.as_mut()).map_err(|e| KeyError::Invalid(e.to_string()))?;
    Ok(out)
}

fn wrap(master: &[u8; 32], segredo: &[u8], kdf: Kdf) -> Result<Wrapped> {
    let (mut salt, mut nonce) = ([0u8; 16], [0u8; 12]);
    rand::thread_rng().fill_bytes(&mut salt);
    rand::thread_rng().fill_bytes(&mut nonce);
    let k = kek(segredo, &salt, kdf)?;
    let ct = ChaCha20Poly1305::new(Key::from_slice(k.as_ref())).encrypt(Nonce::from_slice(&nonce), master.as_ref()).map_err(|_| KeyError::Invalid("falha ao embrulhar a chave".into()))?;
    Ok(Wrapped { salt: b64(&salt), nonce: b64(&nonce), ct: b64(&ct) })
}

fn unwrap_key(w: &Wrapped, segredo: &[u8], kdf: Kdf) -> Result<MasterKey> {
    let (salt, nonce, ct) = (unb64(&w.salt)?, unb64(&w.nonce)?, unb64(&w.ct)?);
    if salt.len() < 16 || nonce.len() != 12 {
        return Err(KeyError::Invalid("sal ou nonce com tamanho errado".into()));
    }
    let k = kek(segredo, &salt, kdf)?;
    // AEAD: senha errada falha na autenticação, nunca devolve chave lixo
    let pt = ChaCha20Poly1305::new(Key::from_slice(k.as_ref())).decrypt(Nonce::from_slice(&nonce), ct.as_ref()).map_err(|_| KeyError::Wrong)?;
    let pt = Zeroizing::new(pt);
    let mut out = Zeroizing::new([0u8; 32]);
    if pt.len() != 32 {
        return Err(KeyError::Invalid("chave mestra com tamanho errado".into()));
    }
    out.copy_from_slice(&pt);
    Ok(out)
}

const ALFABETO: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// 20 bytes aleatórios em base32, em grupos de 4: `ABCD-EFGH-...` (32 letras).
fn novo_codigo() -> Zeroizing<String> {
    let mut raw = Zeroizing::new([0u8; 20]);
    rand::thread_rng().fill_bytes(raw.as_mut());
    let mut s = String::new();
    let (mut acc, mut bits) = (0u32, 0u32);
    for b in raw.iter() {
        acc = (acc << 8) | *b as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            s.push(ALFABETO[((acc >> bits) & 31) as usize] as char);
        }
    }
    Zeroizing::new(s.as_bytes().chunks(4).map(|c| std::str::from_utf8(c).unwrap()).collect::<Vec<_>>().join("-"))
}

/// Tira hífens e espaços e põe em maiúsculas: o usuário pode digitar do jeito que anotou.
fn normalizar(codigo: &str) -> Zeroizing<String> {
    Zeroizing::new(codigo.chars().filter(|c| !c.is_whitespace() && *c != '-').collect::<String>().to_uppercase())
}

/// Resultado de criar o cofre: o arquivo, a chave mestra e o código de recuperação (mostrar uma vez).
pub struct Created {
    pub file: KeyFile,
    pub master: MasterKey,
    pub recovery_code: Zeroizing<String>,
}

pub fn create(password: &str, kdf: Kdf) -> Result<Created> {
    let mut master = Zeroizing::new([0u8; 32]);
    rand::thread_rng().fill_bytes(master.as_mut());
    let code = novo_codigo();
    let file = KeyFile { version: VERSAO, kdf, password: wrap(&master, password.as_bytes(), kdf)?, recovery: wrap(&master, normalizar(&code).as_bytes(), kdf)? };
    Ok(Created { file, master, recovery_code: code })
}

impl KeyFile {
    pub fn unlock_password(&self, password: &str) -> Result<MasterKey> {
        unwrap_key(&self.password, password.as_bytes(), self.kdf)
    }

    pub fn unlock_recovery(&self, code: &str) -> Result<MasterKey> {
        unwrap_key(&self.recovery, normalizar(code).as_bytes(), self.kdf)
    }

    /// Reembrulha a chave mestra com outra senha. O banco não muda.
    pub fn change_password(&mut self, master: &MasterKey, new_password: &str) -> Result<()> {
        self.password = wrap(master, new_password.as_bytes(), self.kdf)?;
        Ok(())
    }

    /// Gera outro código de recuperação e **invalida o anterior** (F02.10).
    pub fn new_recovery(&mut self, master: &MasterKey) -> Result<Zeroizing<String>> {
        let code = novo_codigo();
        self.recovery = wrap(master, normalizar(&code).as_bytes(), self.kdf)?;
        Ok(code)
    }

    pub fn load(path: &Path) -> Result<Self> {
        let f: KeyFile = serde_json::from_slice(&std::fs::read(path).map_err(|e| KeyError::Io(e.to_string()))?).map_err(|e| KeyError::Invalid(e.to_string()))?;
        if f.version != VERSAO {
            return Err(KeyError::Invalid(format!("versão {} desconhecida", f.version)));
        }
        Ok(f)
    }

    /// Grava por arquivo temporário e renomeação: uma queda no meio nunca deixa o cofre pela metade.
    pub fn save(&self, path: &Path) -> Result<()> {
        let tmp = path.with_extension("key.tmp");
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| KeyError::Invalid(e.to_string()))?;
        std::fs::write(&tmp, bytes).map_err(|e| KeyError::Io(e.to_string()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
        }
        std::fs::rename(&tmp, path).map_err(|e| KeyError::Io(e.to_string()))
    }
}

/// Subchave de uso específico (logs, backups) derivada da chave mestra; a mestra nunca cifra dois
/// usos diferentes.
pub fn subkey(master: &MasterKey, uso: &str) -> Zeroizing<[u8; 32]> {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(b"orion-subkey-v1\0");
    h.update(uso.as_bytes());
    h.update([0]);
    h.update(master.as_ref());
    Zeroizing::new(h.finalize().into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn senha_abre_e_senha_errada_nao() {
        let c = create("senha-forte-123", Kdf::TESTE).unwrap();
        assert_eq!(*c.file.unlock_password("senha-forte-123").unwrap(), *c.master);
        assert!(matches!(c.file.unlock_password("senha-errada-123"), Err(KeyError::Wrong)));
        assert!(matches!(c.file.unlock_password(""), Err(KeyError::Wrong)));
    }

    #[test]
    fn recuperacao_abre_do_jeito_que_o_usuario_digitar() {
        let c = create("senha-forte-123", Kdf::TESTE).unwrap();
        let code = c.recovery_code.to_string();
        assert_eq!(code.len(), 32 + 7, "{code}");
        assert!(code.split('-').all(|g| g.len() == 4));
        for variante in [code.clone(), code.to_lowercase(), code.replace('-', " "), format!("  {code}  ")] {
            assert_eq!(*c.file.unlock_recovery(&variante).unwrap(), *c.master, "{variante:?}");
        }
        assert!(matches!(c.file.unlock_recovery("AAAA-AAAA-AAAA-AAAA-AAAA-AAAA-AAAA-AAAA"), Err(KeyError::Wrong)));
    }

    #[test]
    fn trocar_a_senha_mantem_a_chave_e_invalida_a_antiga() {
        let mut c = create("senha-forte-123", Kdf::TESTE).unwrap();
        c.file.change_password(&c.master, "outra-senha-456").unwrap();
        assert_eq!(*c.file.unlock_password("outra-senha-456").unwrap(), *c.master);
        assert!(c.file.unlock_password("senha-forte-123").is_err());
        assert!(c.file.unlock_recovery(&c.recovery_code).is_ok(), "recuperação continua valendo");
    }

    #[test]
    fn novo_codigo_de_recuperacao_invalida_o_anterior() {
        let mut c = create("senha-forte-123", Kdf::TESTE).unwrap();
        let antigo = c.recovery_code.to_string();
        let novo = c.file.new_recovery(&c.master).unwrap();
        assert_ne!(*novo, antigo);
        assert!(c.file.unlock_recovery(&antigo).is_err());
        assert_eq!(*c.file.unlock_recovery(&novo).unwrap(), *c.master);
    }

    #[test]
    fn arquivo_adulterado_nao_abre_e_nada_vaza_em_claro() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(KEY_FILE);
        let c = create("senha-forte-123", Kdf::TESTE).unwrap();
        c.file.save(&path).unwrap();
        let texto = std::fs::read_to_string(&path).unwrap();
        assert!(!texto.contains(&b64(c.master.as_ref())) && !texto.contains("senha-forte"));
        let mut f = KeyFile::load(&path).unwrap();
        assert!(f.unlock_password("senha-forte-123").is_ok());
        f.password.ct = b64(&[0u8; 48]);
        assert!(f.unlock_password("senha-forte-123").is_err());
        std::fs::write(&path, b"{ nao e json").unwrap();
        assert!(KeyFile::load(&path).is_err());
        assert!(!tmp.path().join("orion.key.tmp").exists());
    }

    #[test]
    fn subchaves_sao_diferentes_por_uso() {
        let c = create("senha-forte-123", Kdf::TESTE).unwrap();
        assert_ne!(*subkey(&c.master, "logs"), *subkey(&c.master, "backup"));
        assert_ne!(*subkey(&c.master, "logs"), *c.master);
        assert_eq!(*subkey(&c.master, "logs"), *subkey(&c.master, "logs"));
    }
}
