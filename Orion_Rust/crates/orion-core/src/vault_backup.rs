//! Backup manual do vault em zip (F06.17). É comando seu: só lê o vault e grava um arquivo novo
//! na pasta de destino, com nome gerado pelo código e sem sobrescrever nada.
//! Fica de fora o que não é nota nem anexo: `.git`, `.trash` e o estado de janelas do Obsidian.

use std::path::{Path, PathBuf};

const PULAR: [&str; 2] = [".git", ".trash"];

fn pular(rel: &Path) -> bool {
    let mut partes = rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned());
    let Some(primeira) = partes.next() else { return false };
    if PULAR.contains(&primeira.as_str()) {
        return true;
    }
    // workspace*.json muda a cada clique no Obsidian e não é conteúdo
    primeira == ".obsidian" && rel.file_name().is_some_and(|n| n.to_string_lossy().starts_with("workspace"))
}

fn arquivos(raiz: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for e in std::fs::read_dir(dir)? {
        let e = e?;
        let p = e.path();
        let rel = p.strip_prefix(raiz).unwrap_or(&p);
        if pular(rel) {
            continue;
        }
        // symlink não é seguido: o backup não sai do vault
        let t = e.file_type()?;
        if t.is_dir() {
            arquivos(raiz, &p, out)?;
        } else if t.is_file() {
            out.push(p);
        }
    }
    Ok(())
}

/// Compacta `vault` em `destino/vault-AAAAMMDD-HHMMSS.zip` e devolve o caminho e quantos arquivos entraram.
/// O destino não pode ficar dentro do vault (o zip entraria nele mesmo).
pub fn zip_vault(vault: &Path, destino: &Path, agora: f64) -> anyhow::Result<(PathBuf, usize)> {
    let vault = vault.canonicalize().map_err(|e| anyhow::anyhow!("vault não encontrado ({}): {e}", vault.display()))?;
    std::fs::create_dir_all(destino)?;
    let destino = destino.canonicalize()?;
    anyhow::ensure!(!destino.starts_with(&vault), "o destino do backup não pode ficar dentro do vault");
    let carimbo = crate::util::local(agora).format("%Y%m%d-%H%M%S");
    let alvo = destino.join(format!("vault-{carimbo}.zip"));
    anyhow::ensure!(!alvo.exists(), "já existe {}", alvo.display());
    let mut lista = vec![];
    arquivos(&vault, &vault, &mut lista)?;
    lista.sort();
    // temporário exclusivo por pedido: dois cliques no mesmo segundo não escrevem no mesmo arquivo
    let tmp = destino.join(format!(".vault-{carimbo}-{}.zip.tmp", orion_store::uuid_hex()));
    let r = (|| -> anyhow::Result<()> {
        let mut z = zip::ZipWriter::new(std::fs::OpenOptions::new().write(true).create_new(true).open(&tmp)?);
        let op = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).large_file(true);
        for p in &lista {
            let nome = p.strip_prefix(&vault)?.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/");
            z.start_file(nome, op)?;
            // copia em fluxo: anexo de vários GB não precisa caber na memória
            std::io::copy(&mut std::fs::File::open(p)?, &mut z)?;
        }
        z.finish()?;
        Ok(())
    })();
    if let Err(e) = r {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    // publica sem sobrescrever: o link falha se outro pedido já publicou esse nome. Disco sem
    // link (FAT/exFAT de HD externo) cai para renomear, depois de conferir que o nome está livre.
    match std::fs::hard_link(&tmp, &alvo) {
        Ok(()) => {
            let _ = std::fs::remove_file(&tmp);
        }
        Err(_) if !alvo.exists() => std::fs::rename(&tmp, &alvo)?,
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            anyhow::bail!("não foi possível publicar {}: {e}", alvo.display());
        }
    }
    Ok((alvo, lista.len()))
}
