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

/// Memória por caminho de origem: vale enquanto a data e o tamanho do arquivo não mudam; uma falha só vale por
/// alguns segundos (a imagem pode aparecer logo depois).
struct Entrada {
    mtime: Option<std::time::SystemTime>,
    len: u64,
    res: Option<PathBuf>,
    quando: std::time::Instant,
}

static MEMO: OnceLock<Mutex<HashMap<PathBuf, Entrada>>> = OnceLock::new();
/// Imagens que vêm do núcleo (`/imagens/<nome>`), já baixadas e guardadas no cache.
static URLS: OnceLock<Mutex<HashMap<String, PathBuf>>> = OnceLock::new();

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
    let meta = std::fs::metadata(origem).ok();
    let (mtime, len) = (meta.as_ref().and_then(|m| m.modified().ok()), meta.as_ref().map_or(0, |m| m.len()));
    if let Some(e) = memo.lock().ok()?.get(origem) {
        let igual = e.mtime == mtime && e.len == len;
        let vale = match &e.res {
            Some(p) => p.exists(),
            None => e.quando.elapsed() < std::time::Duration::from_secs(5),
        };
        if igual && vale {
            return e.res.clone();
        }
    }
    let r = gerar(origem);
    memo.lock().ok()?.insert(origem.to_path_buf(), Entrada { mtime, len, res: r.clone(), quando: std::time::Instant::now() });
    r
}

/// Nome de imagem gerada pelo núcleo, na mesma forma que a rota dele aceita:
/// `/imagens/img-<9 a 12 dígitos>-<8 hex>.<png|jpg|webp>`; qualquer outra coisa não é.
pub fn url_do_nucleo(u: &str) -> bool {
    let Some(n) = u.strip_prefix("/imagens/img-") else { return false };
    let Some((corpo, ext)) = n.rsplit_once('.') else { return false };
    let Some((num, hex)) = corpo.split_once('-') else { return false };
    matches!(ext, "png" | "jpg" | "webp")
        && (9..=12).contains(&num.len())
        && num.chars().all(|c| c.is_ascii_digit())
        && hex.len() == 8
        && hex.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'))
}

/// Guarda no cache os bytes de uma imagem baixada do núcleo e lembra a URL.
pub fn guardar_url(url: &str, bytes: &[u8]) -> Option<PathBuf> {
    if !url_do_nucleo(url) || bytes.len() as u64 > ORIGEM_MAX {
        return None;
    }
    let p = gravar(bytes)?;
    URLS.get_or_init(|| Mutex::new(HashMap::new())).lock().ok()?.insert(url.to_string(), p.clone());
    Some(p)
}

/// Versão em cache de uma imagem do núcleo, se já foi baixada (e o arquivo ainda existe).
pub fn da_url(url: &str) -> Option<PathBuf> {
    let p = URLS.get()?.lock().ok()?.get(url).cloned()?;
    p.exists().then_some(p)
}

fn gerar(origem: &Path) -> Option<PathBuf> {
    let meta = std::fs::metadata(origem).ok()?;
    if !meta.is_file() || meta.len() > ORIGEM_MAX || !eh_imagem(origem) {
        return None;
    }
    gravar(&std::fs::read(origem).ok()?)
}

/// Reduz (≤ 1600 px), converte para WebP e grava com o nome pelo conteúdo; devolve o caminho no cache.
fn gravar(bytes: &[u8]) -> Option<PathBuf> {
    let destino = pasta()?.join(nome_por_conteudo(bytes));
    if destino.exists() {
        tocar(&destino);
        return Some(destino);
    }
    let img = image::load_from_memory(bytes).ok()?;
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
