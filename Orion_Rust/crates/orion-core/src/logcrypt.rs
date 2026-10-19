//! Log cifrado em disco com retenção (F02).
//!
//! Um arquivo por dia (`orion-AAAA-MM-DD.log.enc`, data UTC). Cada linha é um registro
//! `[u32 tamanho][nonce de 12 bytes][texto cifrado e autenticado]` (ChaCha20-Poly1305) com uma
//! subchave da chave mestra. Sem a senha do Orion o arquivo não se lê; registro adulterado ou
//! cortado por queda de energia é ignorado sem perder os anteriores. Arquivos mais velhos que a
//! retenção são apagados pelo nome (a data está no nome, não no `mtime`).

use crate::dbkey::{subkey, MasterKey};
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use parking_lot::Mutex;
use rand::RngCore;
use std::io::Write;
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

const PREFIXO: &str = "orion-";
const SUFIXO: &str = ".log.enc";
const MAX_LINHA: usize = 64 * 1024;

pub struct EncryptedLog {
    dir: PathBuf,
    key: Zeroizing<[u8; 32]>,
    retention_days: u32,
    ultimo_dia: Mutex<String>,
}

fn dia(now: f64) -> String {
    chrono::DateTime::from_timestamp(now as i64, 0).unwrap_or_default().format("%Y-%m-%d").to_string()
}

impl EncryptedLog {
    pub fn new(dir: &Path, master: &MasterKey, retention_days: u32) -> std::io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        Ok(EncryptedLog { dir: dir.to_path_buf(), key: subkey(master, "logs"), retention_days, ultimo_dia: Mutex::new(String::new()) })
    }

    fn file(&self, d: &str) -> PathBuf {
        self.dir.join(format!("{PREFIXO}{d}{SUFIXO}"))
    }

    /// Grava uma linha no arquivo do dia e, na primeira escrita de cada dia, poda os antigos.
    pub fn write_line(&self, line: &str, now: f64) -> std::io::Result<()> {
        let d = dia(now);
        {
            let mut ultimo = self.ultimo_dia.lock();
            if *ultimo != d {
                *ultimo = d.clone();
                prune(&self.dir, self.retention_days, now);
            }
        }
        let texto: String = line.chars().take(MAX_LINHA).collect();
        let mut nonce = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce);
        let ct = ChaCha20Poly1305::new(Key::from_slice(self.key.as_ref()))
            .encrypt(Nonce::from_slice(&nonce), texto.as_bytes())
            .map_err(|_| std::io::Error::other("falha ao cifrar o log"))?;
        let mut registro = Vec::with_capacity(4 + 12 + ct.len());
        registro.extend_from_slice(&((12 + ct.len()) as u32).to_le_bytes());
        registro.extend_from_slice(&nonce);
        registro.extend_from_slice(&ct);
        // uma única escrita por registro: dois processos não misturam pedaços de linhas
        std::fs::OpenOptions::new().create(true).append(true).open(self.file(&d))?.write_all(&registro)
    }
}

/// Lê um arquivo de log cifrado. Para no primeiro registro que não autentica.
pub fn read_file(path: &Path, master: &MasterKey) -> std::io::Result<Vec<String>> {
    let key = subkey(master, "logs");
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key.as_ref()));
    let bytes = std::fs::read(path)?;
    let (mut i, mut linhas) = (0usize, Vec::new());
    while i + 4 <= bytes.len() {
        let n = u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap()) as usize;
        if n < 12 + 16 || i + 4 + n > bytes.len() {
            break;
        }
        let (nonce, ct) = bytes[i + 4..i + 4 + n].split_at(12);
        match cipher.decrypt(Nonce::from_slice(nonce), ct) {
            Ok(pt) => linhas.push(String::from_utf8_lossy(&pt).into_owned()),
            Err(_) => break,
        }
        i += 4 + n;
    }
    Ok(linhas)
}

/// Arquivos de log do diretório, do mais antigo para o mais novo.
pub fn files(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| data_do_nome(n).is_some()))
        .collect();
    v.sort();
    v
}

fn data_do_nome(nome: &str) -> Option<chrono::NaiveDate> {
    let d = nome.strip_prefix(PREFIXO)?.strip_suffix(SUFIXO)?;
    chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()
}

/// Apaga os arquivos com mais de `retention_days` dias. Devolve quantos saíram. Só mexe em
/// arquivos com o nome exato dos logs; qualquer outro fica.
pub fn prune(dir: &Path, retention_days: u32, now: f64) -> usize {
    let Some(hoje) = chrono::DateTime::from_timestamp(now as i64, 0).map(|d| d.date_naive()) else { return 0 };
    let limite = hoje - chrono::Duration::days(retention_days as i64);
    let mut n = 0;
    for p in files(dir) {
        let velho = p.file_name().and_then(|n| n.to_str()).and_then(data_do_nome).is_some_and(|d| d < limite);
        if velho && std::fs::remove_file(&p).is_ok() {
            n += 1;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dbkey::{create, Kdf};

    const T0: f64 = 1_760_000_000.0; // 2025-10-09
    const DIA: f64 = 86_400.0;

    #[test]
    fn grava_cifrado_e_le_de_volta_com_a_chave() {
        let tmp = tempfile::tempdir().unwrap();
        let m = create("senha-forte-123", Kdf::TESTE).unwrap().master;
        let log = EncryptedLog::new(tmp.path(), &m, 30).unwrap();
        log.write_line("{\"msg\":\"ola segredo-do-log\"}", T0).unwrap();
        log.write_line("segunda linha", T0 + 1.0).unwrap();
        let arq = files(tmp.path());
        assert_eq!(arq.len(), 1);
        assert!(!std::fs::read(&arq[0]).unwrap().windows(11).any(|w| w == b"segredo-do-"));
        assert_eq!(read_file(&arq[0], &m).unwrap(), ["{\"msg\":\"ola segredo-do-log\"}", "segunda linha"]);
        let outra = create("outra-senha-123", Kdf::TESTE).unwrap().master;
        assert!(read_file(&arq[0], &outra).unwrap().is_empty(), "chave errada não lê nada");
    }

    #[test]
    fn registro_cortado_ou_adulterado_nao_estraga_os_anteriores() {
        let tmp = tempfile::tempdir().unwrap();
        let m = create("senha-forte-123", Kdf::TESTE).unwrap().master;
        let log = EncryptedLog::new(tmp.path(), &m, 30).unwrap();
        for i in 0..3 {
            log.write_line(&format!("linha {i}"), T0).unwrap();
        }
        let p = files(tmp.path()).remove(0);
        let mut b = std::fs::read(&p).unwrap();
        b.truncate(b.len() - 5); // queda de energia no meio do último registro
        std::fs::write(&p, &b).unwrap();
        assert_eq!(read_file(&p, &m).unwrap(), ["linha 0", "linha 1"]);
        let meio = 4 + 12 + 5;
        b[meio] ^= 1; // adulterou o primeiro
        std::fs::write(&p, &b).unwrap();
        assert!(read_file(&p, &m).unwrap().is_empty());
    }

    #[test]
    fn retencao_apaga_so_log_velho_pelo_nome() {
        let tmp = tempfile::tempdir().unwrap();
        let m = create("senha-forte-123", Kdf::TESTE).unwrap().master;
        let _ = &m;
        for k in [20.0, 8.0, 7.0, 3.0, 0.0] {
            std::fs::write(tmp.path().join(format!("{PREFIXO}{}{SUFIXO}", dia(T0 - k * DIA))), "x").unwrap();
        }
        std::fs::write(tmp.path().join("anotacao.txt"), "meu").unwrap();
        std::fs::write(tmp.path().join("orion-nao-e-data.log.enc"), "x").unwrap();
        assert_eq!(files(tmp.path()).len(), 5);
        assert_eq!(prune(tmp.path(), 7, T0), 2, "20 e 8 dias atrás");
        assert_eq!(files(tmp.path()).len(), 3);
        assert!(tmp.path().join("anotacao.txt").exists() && tmp.path().join("orion-nao-e-data.log.enc").exists());
    }

    #[test]
    fn primeira_escrita_do_dia_poda_sozinha() {
        let tmp = tempfile::tempdir().unwrap();
        let m = create("senha-forte-123", Kdf::TESTE).unwrap().master;
        std::fs::write(tmp.path().join("orion-2020-01-01.log.enc"), "velho").unwrap();
        EncryptedLog::new(tmp.path(), &m, 30).unwrap().write_line("hoje", T0).unwrap();
        assert_eq!(files(tmp.path()).len(), 1);
        assert!(!tmp.path().join("orion-2020-01-01.log.enc").exists());
    }
}
