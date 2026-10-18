//! Utilitários comuns às ferramentas: caminho do usuário, processo filho sem segredo e cortes.

use regex::Regex;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::LazyLock;
use std::time::Duration;
use wait_timeout::ChildExt;

static NOME_DE_SEGREDO: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)(KEY|TOKEN|SECRET|PASSWORD|PASSWD|CREDENTIAL)").unwrap());

pub fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// `Path(p).expanduser()`.
pub fn expanduser(p: &str) -> PathBuf {
    if p == "~" {
        return home();
    }
    if let Some(r) = p.strip_prefix("~/").or_else(|| p.strip_prefix("~\\")) {
        return home().join(r);
    }
    PathBuf::from(p)
}

/// Ambiente do filho: sem `ORION_*` e sem nada que pareça chave, token ou senha
/// (REGRAS.md: segredos nunca em log, audit ou processo filho).
pub fn env_limpo() -> HashMap<String, String> {
    std::env::vars().filter(|(k, _)| !k.to_uppercase().starts_with("ORION_") && !NOME_DE_SEGREDO.is_match(k)).collect()
}

pub fn cortar(texto: &str, limite: usize) -> String {
    if texto.chars().count() <= limite {
        texto.to_string()
    } else {
        texto.chars().take(limite).collect::<String>() + "\n…[truncado]"
    }
}

pub struct Saida {
    pub codigo: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[derive(Debug)]
pub enum ErroProcesso {
    Tempo,
    Io(std::io::Error),
}

/// Roda o argv sem shell, sem entrada padrão, com ambiente limpo e tempo-limite.
pub fn rodar(argv: &[String], cwd: Option<&Path>, timeout: Duration, env: Option<HashMap<String, String>>) -> Result<Saida, ErroProcesso> {
    let mut cmd = Command::new(&argv[0]);
    cmd.args(&argv[1..]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    if let Some(c) = cwd {
        cmd.current_dir(c);
    }
    cmd.env_clear().envs(env.unwrap_or_else(env_limpo));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut filho = cmd.spawn().map_err(ErroProcesso::Io)?;
    let mut out = filho.stdout.take();
    let mut err = filho.stderr.take();
    let t1 = std::thread::spawn(move || {
        let mut b = vec![];
        if let Some(o) = out.as_mut() {
            let _ = o.read_to_end(&mut b);
        }
        b
    });
    let t2 = std::thread::spawn(move || {
        let mut b = vec![];
        if let Some(o) = err.as_mut() {
            let _ = o.read_to_end(&mut b);
        }
        b
    });
    match filho.wait_timeout(timeout).map_err(ErroProcesso::Io)? {
        Some(st) => Ok(Saida { codigo: st.code(), stdout: t1.join().unwrap_or_default(), stderr: t2.join().unwrap_or_default() }),
        None => {
            let _ = filho.kill();
            let _ = filho.wait();
            Err(ErroProcesso::Tempo)
        }
    }
}

pub fn which(nome: &str) -> Option<PathBuf> {
    orion_ai::delegate::which(nome)
}

pub fn agora() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

pub fn local(ts: f64) -> chrono::DateTime<chrono::Local> {
    chrono::DateTime::from_timestamp(ts as i64, ((ts.fract()) * 1e9) as u32).unwrap_or_default().with_timezone(&chrono::Local)
}
