//! Caminhos do vault são dados; nunca seguem links para fora da pasta escolhida.
//! REGRAS.md: ler segredo ou chave pede confirmação; captura rápida grava sem sobrescrever.
use crate::VaultError;
use std::path::{Component, Path, PathBuf};

pub(crate) const NOTE_LIMIT: u64 = 2 * 1024 * 1024;

pub(crate) fn relative(path: &Path) -> Result<(), VaultError> {
    if path.as_os_str().is_empty()
        || path.components().any(|c| match c {
            Component::Normal(name) => name.to_string_lossy().contains('\\'),
            _ => true,
        })
    {
        return Err(VaultError::InvalidFormat(
            "caminho relativo de nota inválido".into(),
        ));
    }
    Ok(())
}

pub(crate) fn checked(root: &Path, rel: &Path, create_dirs: bool) -> Result<PathBuf, VaultError> {
    relative(rel)?;
    let mut current = root.to_path_buf();
    let count = rel.components().count();
    for (i, part) in rel.components().enumerate() {
        current.push(part);
        match std::fs::symlink_metadata(&current) {
            Ok(meta) => {
                if meta.file_type().is_symlink() || (i + 1 < count && !meta.is_dir()) {
                    return Err(VaultError::InvalidFormat(
                        "nota ou pasta usa link simbólico ou caminho inválido".into(),
                    ));
                }
                // Reparse points também incluem junctions do Windows.
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    if meta.file_attributes() & 0x400 != 0 {
                        return Err(VaultError::InvalidFormat(
                            "junction ou reparse point não permitido no caminho da nota".into(),
                        ));
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if create_dirs && i + 1 < count {
                    match std::fs::create_dir(&current) {
                        Ok(()) => {}
                        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                            return checked(root, rel, create_dirs);
                        }
                        Err(e) => return Err(e.into()),
                    }
                }
            }
            Err(e) => return Err(e.into()),
        }
    }
    Ok(current)
}

pub(crate) fn read_note(path: &Path) -> Result<String, VaultError> {
    use std::io::Read;
    let meta = std::fs::symlink_metadata(path)?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > NOTE_LIMIT {
        return Err(VaultError::InvalidFormat(
            "nota inválida ou maior que 2 MiB".into(),
        ));
    }
    let mut content = String::new();
    std::fs::File::open(path)?
        .take(NOTE_LIMIT + 1)
        .read_to_string(&mut content)?;
    if content.len() as u64 > NOTE_LIMIT {
        return Err(VaultError::InvalidFormat("nota maior que 2 MiB".into()));
    }
    Ok(content)
}
