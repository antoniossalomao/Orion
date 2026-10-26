//! Escala e densidade da interface, guardadas ao lado dos dados do app para valerem na próxima abertura (F01.07).
//! Só o app real lê este arquivo; capturas e testes começam sempre no padrão.

use std::path::PathBuf;

pub const SCALES: [u32; 5] = [90, 100, 115, 130, 150];

fn file() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA").or_else(|| std::env::var_os("HOME"))?;
    Some(PathBuf::from(base).join("Orion").join("interface.txt"))
}

/// Lê `(escala, compacta)`; valores ausentes, inválidos ou fora dos degraus ficam de fora.
pub fn load() -> (Option<u32>, Option<bool>) {
    let Some(path) = file() else { return (None, None) };
    let Ok(text) = std::fs::read_to_string(path) else { return (None, None) };
    let (mut scale, mut compact) = (None, None);
    for line in text.lines() {
        match line.split_once('=').map(|(k, v)| (k.trim(), v.trim())) {
            Some(("scale", v)) => scale = v.parse::<u32>().ok().filter(|s| SCALES.contains(s)),
            Some(("compact", v)) => compact = v.parse::<bool>().ok(),
            _ => {}
        }
    }
    (scale, compact)
}

/// Grava em arquivo temporário e renomeia, para que uma falha no meio não deixe o arquivo truncado.
pub fn write_atomic(path: &std::path::Path, body: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, body)?;
    std::fs::rename(&tmp, path)
}

pub fn save(scale: u32, compact: bool) -> std::io::Result<()> {
    let Some(path) = file() else { return Ok(()) };
    write_atomic(&path, &format!("scale={scale}\ncompact={compact}\n"))
}
