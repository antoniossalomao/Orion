//! Scripts Python revisados: snapshot, argv sem shell, ambiente mínimo e prazo.
//!
//! Sem sandbox de SO: código não confiável nunca chega ao runner. POSIX comprovado; o resto
//! permanece indisponível até validação real do controle de processos.

use super::skills::{safe_file, serr, Skill, SkillError};
use parking_lot::Mutex;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

pub const MAX_FILES: usize = 256;
pub const MAX_BUNDLE: usize = 4_000_000;
pub const MAX_OUTPUT: usize = 32000;

type R<T> = Result<T, SkillError>;

pub fn snapshot(skill: &Skill) -> R<(String, BTreeMap<String, Vec<u8>>)> {
    let mut files = BTreeMap::new();
    let mut total = 0usize;
    let mut paths = vec![];
    let mut pilha = vec![skill.root.clone()];
    while let Some(d) = pilha.pop() {
        for e in std::fs::read_dir(&d).map_err(|_| serr("script_bundle_too_large"))?.flatten() {
            if paths.len() >= MAX_FILES * 2 {
                return Err(serr("script_bundle_too_many_entries"));
            }
            let p = e.path();
            if p.is_dir() && !std::fs::symlink_metadata(&p).is_ok_and(|m| m.file_type().is_symlink()) {
                pilha.push(p.clone());
            }
            paths.push(p);
        }
    }
    paths.sort();
    for p in paths {
        if std::fs::symlink_metadata(&p).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(serr("symlink_forbidden"));
        }
        if p.is_dir() {
            continue;
        }
        let rel = p.strip_prefix(&skill.root).map_err(|_| serr("reference_out_of_scope"))?.to_string_lossy().replace('\\', "/");
        safe_file(&skill.root, &rel, 1_000_000)?;
        let data = std::fs::read(&p).map_err(|_| serr("file_missing_or_too_large"))?;
        total += data.len();
        if data.len() > 1_000_000 || total > MAX_BUNDLE || files.len() >= MAX_FILES {
            return Err(serr("script_bundle_too_large"));
        }
        files.insert(rel, data);
    }
    let mut h = Sha256::new();
    for (rel, data) in &files {
        h.update(rel.as_bytes());
        h.update([0u8]);
        h.update(Sha256::digest(data));
    }
    Ok((hex::encode(h.finalize()), files))
}

fn preparar(files: &BTreeMap<String, Vec<u8>>) -> std::io::Result<tempfile::TempDir> {
    let dir = tempfile::Builder::new().prefix("orion-reviewed-script-").tempdir()?;
    for (rel, data) in files {
        let p = dir.path().join(rel);
        if let Some(pai) = p.parent() {
            std::fs::create_dir_all(pai)?;
        }
        std::fs::write(p, data)?;
    }
    Ok(dir)
}

#[derive(Default)]
pub struct ScriptRunner {
    pids: Mutex<Vec<u32>>,
    closed: std::sync::atomic::AtomicBool,
}

#[cfg(unix)]
fn matar_grupo(pid: u32) {
    // SAFETY: kill(2) com o grupo de um processo que nós mesmos criamos em nova sessão.
    unsafe {
        libc::kill(-(pid as i32), libc::SIGKILL);
    }
}

#[cfg(not(unix))]
fn matar_grupo(_pid: u32) {}

impl ScriptRunner {
    pub fn close(&self) {
        self.closed.store(true, std::sync::atomic::Ordering::SeqCst);
        for pid in self.pids.lock().drain(..) {
            matar_grupo(pid);
        }
    }

    /// Reabre o runner depois de `close` (a desativação de um plugin cancela os scripts em curso,
    /// mas as próximas execuções continuam permitidas).
    pub fn reabrir(&self) {
        self.closed.store(false, std::sync::atomic::Ordering::SeqCst);
    }

    pub async fn run(&self, skill: &Skill, relative: &str, argv: &[String], reviewed_digest: &str, timeout_s: f64) -> R<Value> {
        if self.closed.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(serr("scripts_disabled"));
        }
        if !cfg!(unix) {
            return Err(serr("scripts_platform_unverified"));
        }
        if !relative.starts_with("scripts/") || !relative.ends_with(".py") || argv.len() > 32 || argv.iter().any(|a| a.len() > 4096 || a.contains('\0')) {
            return Err(serr("script_arguments_invalid"));
        }
        let s2 = skill.clone();
        let body = tokio::task::spawn_blocking(move || s2.load()).await.map_err(|_| serr("script_not_declared"))??;
        if !body.references.iter().any(|r| r == relative) {
            return Err(serr("script_not_declared"));
        }
        let s3 = skill.clone();
        let (digest, files) = tokio::task::spawn_blocking(move || snapshot(&s3)).await.map_err(|_| serr("script_bundle_too_large"))??;
        if digest != reviewed_digest {
            return Err(serr("script_revision_changed"));
        }
        if !files.contains_key(relative) {
            return Err(serr("script_not_found"));
        }
        // executa os bytes revisados, não o arquivo mutável original
        let dir = tokio::task::spawn_blocking(move || preparar(&files)).await.map_err(|_| serr("script_not_found"))?.map_err(|_| serr("script_not_found"))?;
        let mut cmd = Command::new("python3");
        cmd.arg("-I").arg(Path::new(dir.path()).join(relative)).args(argv).current_dir(dir.path()).env_clear().env("PATH", "/usr/bin:/bin").env("PYTHONUNBUFFERED", "1").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
        #[cfg(unix)]
        // SAFETY: setsid() não aloca nem toca estado do processo pai; roda entre fork e exec.
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
        let mut filho = cmd.spawn().map_err(|_| serr("script_not_found"))?;
        let pid = filho.id().unwrap_or(0);
        self.pids.lock().push(pid);
        let (mut so, mut se) = (filho.stdout.take().unwrap(), filho.stderr.take().unwrap());
        let prazo = Duration::from_secs_f64(timeout_s.clamp(0.1, 30.0));
        let leitura = async {
            let (mut a, mut b) = (vec![], vec![]);
            let (mut ba, mut bb) = ([0u8; 4096], [0u8; 4096]);
            let (mut fa, mut fb) = (false, false);
            while !(fa && fb) {
                tokio::select! {
                    n = so.read(&mut ba), if !fa => { let n = n.unwrap_or(0); if n == 0 { fa = true } else { a.extend_from_slice(&ba[..n]) } }
                    n = se.read(&mut bb), if !fb => { let n = n.unwrap_or(0); if n == 0 { fb = true } else { b.extend_from_slice(&bb[..n]) } }
                }
                if a.len() + b.len() > MAX_OUTPUT {
                    return Err(serr("script_output_limit"));
                }
            }
            let status = filho.wait().await.map_err(|_| serr("script_not_found"))?;
            Ok((a, b, status.code()))
        };
        let r = tokio::time::timeout(prazo, leitura).await;
        // encerra o grupo também quando o script deixa filhos/pipe aberto
        matar_grupo(pid);
        self.pids.lock().retain(|p| *p != pid);
        drop(dir);
        match r {
            Err(_) => Ok(json!({"ok": false, "codigo": "script_timeout"})),
            Ok(Err(e)) => Err(e),
            Ok(Ok((a, b, code))) => {
                let red = |v: &[u8]| orion_policy::redact(&Value::String(String::from_utf8_lossy(v).to_string()), MAX_OUTPUT);
                Ok(json!({"ok": code == Some(0), "exit_code": code, "stdout": red(&a), "stderr": red(&b), "revision": digest}))
            }
        }
    }
}

