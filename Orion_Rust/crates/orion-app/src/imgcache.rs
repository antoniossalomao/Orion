//! Cache de imagens do chat (F05.17): cada imagem é reduzida a 1600 px no maior lado e guardada em WebP (sem perdas),
//! com o nome pelo conteúdo (duas cópias da mesma imagem ocupam um arquivo só). A pasta nunca passa de 1 GB: ao
//! passar, os arquivos menos usados saem primeiro. A tela só segura a imagem enquanto a mensagem está montada
//! (janela virtual do chat); ao sair da janela o renderizador a descarta e ela volta do WebP ao reabrir.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

pub const LIMITE: u64 = 1 << 30;
pub const LADO_MAX: u32 = 1600;
/// Imagens maiores que isso não são abertas (proteção contra arquivo gigante ou malicioso).
const ORIGEM_MAX: u64 = 25 * 1024 * 1024;

static MEMO: OnceLock<Mutex<HashMap<PathBuf, Option<PathBuf>>>> = OnceLock::new();

pub fn eh_imagem(p: &Path) -> bool {
    matches!(p.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref(), Some("png" | "jpg" | "jpeg" | "gif" | "webp"))
}

fn pasta() -> Option<PathBuf> {
    let base = if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
    }?;
    let d = base.join("Orion").join("imagens");
    std::fs::create_dir_all(&d).ok()?;
    Some(d)
}

fn nome_por_conteudo(bytes: &[u8]) -> String {
    // dois hashes de 64 bits com sementes diferentes: colisão acidental é desprezível
    let mut a = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut a);
    let mut b = std::collections::hash_map::DefaultHasher::new();
    (bytes.len(), 0x9e37_79b9u32).hash(&mut b);
    bytes.iter().rev().take(4096).for_each(|x| x.hash(&mut b));
    format!("{:016x}{:016x}.webp", a.finish(), b.finish())
}

/// Garante a versão em cache (≤ 1600 px, WebP) de uma imagem do disco e devolve o caminho dela.
/// `None` se não for imagem legível. O resultado fica na memória para não refazer o trabalho a cada quadro.
pub fn preparar(origem: &Path) -> Option<PathBuf> {
    let memo = MEMO.get_or_init(|| {
        limitar(LIMITE); // na primeira vez do processo, a pasta volta ao limite
        Mutex::new(HashMap::new())
    });
    if let Some(r) = memo.lock().ok()?.get(origem) {
        if r.as_ref().is_none_or(|p| p.exists()) {
            return r.clone();
        }
    }
    let r = gerar(origem);
    memo.lock().ok()?.insert(origem.to_path_buf(), r.clone());
    r
}

fn gerar(origem: &Path) -> Option<PathBuf> {
    let meta = std::fs::metadata(origem).ok()?;
    if !meta.is_file() || meta.len() > ORIGEM_MAX || !eh_imagem(origem) {
        return None;
    }
    let bytes = std::fs::read(origem).ok()?;
    let destino = pasta()?.join(nome_por_conteudo(&bytes));
    if destino.exists() {
        tocar(&destino);
        return Some(destino);
    }
    let img = image::load_from_memory(&bytes).ok()?;
    let img = if img.width().max(img.height()) > LADO_MAX { img.resize(LADO_MAX, LADO_MAX, image::imageops::FilterType::Triangle) } else { img };
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::WebP).ok()?;
    std::fs::write(&destino, out.into_inner()).ok()?;
    limitar(LIMITE);
    Some(destino)
}

fn tocar(p: &Path) {
    if let Ok(f) = std::fs::OpenOptions::new().write(true).open(p) {
        let _ = f.set_modified(std::time::SystemTime::now());
    }
}

/// Apaga os arquivos menos usados até a pasta caber em `limite` bytes. Devolve quantos saíram.
pub fn limitar(limite: u64) -> usize {
    let Some(dir) = pasta() else { return 0 };
    limitar_em(&dir, limite)
}

fn limitar_em(dir: &Path, limite: u64) -> usize {
    let Ok(rd) = std::fs::read_dir(dir) else { return 0 };
    let mut arqs: Vec<(std::time::SystemTime, u64, PathBuf)> = rd
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            (m.is_file() && e.path().extension().is_some_and(|x| x == "webp")).then(|| (m.modified().unwrap_or(std::time::UNIX_EPOCH), m.len(), e.path()))
        })
        .collect();
    let mut total: u64 = arqs.iter().map(|a| a.1).sum();
    arqs.sort();
    let mut apagados = 0;
    for (_, tam, p) in arqs {
        if total <= limite {
            break;
        }
        if std::fs::remove_file(&p).is_ok() {
            total -= tam;
            apagados += 1;
        }
    }
    apagados
}
