//! Downloads só após escolha no instalador; nunca substituem arquivo sem verificar o hash.
use crate::catalog::{self, Artifact};
use anyhow::{ensure, Context};
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub fn verify(path: &Path, artifact: &Artifact) -> anyhow::Result<()> {
    verify_digest(path, &artifact.sha256, artifact.bytes)
}

fn verify_digest(path: &Path, sha256: &str, bytes: u64) -> anyhow::Result<()> {
    let mut f = File::open(path)?;
    ensure!(f.metadata()?.len() == bytes, "tamanho de modelo incorreto");
    let mut digest = Sha256::new();
    let mut chunk = [0u8; 65536];
    loop {
        let n = f.read(&mut chunk)?;
        if n == 0 {
            break;
        }
        digest.update(&chunk[..n]);
    }
    ensure!(hex::encode(digest.finalize()) == sha256, "SHA-256 do modelo não confere");
    Ok(())
}

/// Retorna o caminho e se foi criado nesta instalação. `.part` permite retomar após queda.
pub fn install(id: &str, directory: &Path, cancel: &AtomicBool, progress: &dyn Fn(u64, u64)) -> anyhow::Result<(PathBuf, bool)> {
    let a = catalog::find(id).context("modelo fora do catálogo fixado")?;
    std::fs::create_dir_all(directory)?;
    let target = directory.join(&a.filename);
    if target.exists() {
        let (sha, bytes) = a.final_digest();
        verify_digest(&target, sha, bytes).context("modelo existente inválido; remova-o explicitamente antes de reinstalar")?;
        return Ok((target, false));
    }
    let part = directory.join(format!("{}.part", a.filename));
    ensure!(!part.symlink_metadata().is_ok_and(|m| m.file_type().is_symlink()), "arquivo parcial não pode ser symlink");
    let mut offset = part.metadata().map(|m| m.len()).unwrap_or(0);
    if offset > a.bytes {
        std::fs::remove_file(&part)?;
        offset = 0;
    }
    ensure!(!cancel.load(Ordering::SeqCst), "download cancelado");
    if offset < a.bytes {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
        runtime.block_on(async {
            let client = reqwest::Client::builder()
                .https_only(true)
                .connect_timeout(Duration::from_secs(20))
                .read_timeout(Duration::from_secs(20))
                .timeout(Duration::from_secs(7200))
                .build()?;
            let mut request = client.get(a.url());
            if offset > 0 {
                request = request.header("Range", format!("bytes={offset}-"));
            }
            let mut response = cancellable(request.send(), cancel)
                .await
                .context("não foi possível baixar o modelo")?
                .error_for_status()?;
            if offset > 0 && response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
                let range = response.headers().get("content-range").and_then(|v| v.to_str().ok()).unwrap_or("");
                ensure!(range.starts_with(&format!("bytes {offset}-")), "servidor devolveu intervalo incorreto");
            } else {
                ensure!(response.status() == reqwest::StatusCode::OK, "resposta de download inesperada");
                offset = 0;
            }
            let mut file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(offset == 0)
                .append(offset > 0)
                .open(&part)?;
            loop {
                ensure!(!cancel.load(Ordering::SeqCst), "download cancelado");
                let Some(chunk) = cancellable(response.chunk(), cancel).await? else {
                    break;
                };
                let n = chunk.len();
                ensure!(offset + n as u64 <= a.bytes, "download maior que o tamanho publicado");
                file.write_all(&chunk)?;
                offset += n as u64;
                progress(offset, a.bytes);
            }
            file.sync_all()?;
            Ok::<_, anyhow::Error>(())
        })?;
    }
    if let Err(e) = verify(&part, &a) {
        // Arquivo incompleto serve para retomar; arquivo completo com hash errado não serve.
        if part.metadata().is_ok_and(|m| m.len() == a.bytes) {
            let _ = std::fs::remove_file(&part);
        }
        return Err(e);
    }
    ensure!(!cancel.load(Ordering::SeqCst), "download cancelado");
    if let Some(entry) = &a.extract {
        // pacote conferido: extrai só o arquivo pedido para um temporário e confere de novo
        let tmp = directory.join(format!("{}.extract", a.filename));
        {
            let mut zip = zip::ZipArchive::new(File::open(&part)?).context("pacote corrompido")?;
            let mut inner = zip.by_name(entry).context("arquivo ausente no pacote")?;
            let mut out = File::create(&tmp)?;
            std::io::copy(&mut inner, &mut out)?;
            out.sync_all()?;
        }
        let (sha, bytes) = a.final_digest();
        if let Err(e) = verify_digest(&tmp, sha, bytes) {
            let _ = std::fs::remove_file(&tmp);
            return Err(e);
        }
        std::fs::hard_link(&tmp, &target)?;
        let _ = std::fs::remove_file(tmp);
        let _ = std::fs::remove_file(part);
        return Ok((target, true));
    }
    // hard_link é atômico e falha se alguém criou o destino durante o download.
    std::fs::hard_link(&part, &target)?;
    let _ = std::fs::remove_file(part);
    Ok((target, true))
}

async fn cancellable<T>(future: impl std::future::Future<Output = Result<T, reqwest::Error>>, cancel: &AtomicBool) -> anyhow::Result<T> {
    tokio::pin!(future);
    loop {
        ensure!(!cancel.load(Ordering::SeqCst), "download cancelado");
        tokio::select! {
            result = &mut future => return Ok(result?),
            _ = tokio::time::sleep(Duration::from_millis(50)) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tamanho_e_hash_sao_obrigatorios() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("modelo");
        std::fs::write(&p, b"abc").unwrap();
        let mut a = catalog::all().remove(0);
        a.bytes = 3;
        a.sha256 = hex::encode(Sha256::digest(b"abc"));
        assert!(verify(&p, &a).is_ok());
        std::fs::write(&p, b"abd").unwrap();
        assert!(verify(&p, &a).is_err());
        std::fs::write(&p, b"ab").unwrap();
        assert!(verify(&p, &a).is_err());
    }
    #[test]
    fn destino_arbitrario_e_cancelamento_nao_baixam() {
        let d = tempfile::tempdir().unwrap();
        assert!(install("https://example.com/model", d.path(), &AtomicBool::new(false), &|_, _| {}).is_err());
        assert!(install("qwen3-0.6b", d.path(), &AtomicBool::new(true), &|_, _| {}).is_err());
    }
}
