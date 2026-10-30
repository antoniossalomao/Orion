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

pub fn shortcut()->bool{file().and_then(|p|std::fs::read_to_string(p.with_file_name("shortcut.txt")).ok()).is_none_or(|s|s.trim()!="false")}
pub fn save_shortcut(on:bool)->std::io::Result<()>{if let Some(path)=file(){write_atomic(&path.with_file_name("shortcut.txt"),if on{"true"}else{"false"})}else{Ok(())}}

/// Tamanho e posição da janela principal (F01.11). Posição em coordenadas da área de trabalho
/// inteira, o que também guarda o monitor. Valores fora de faixa ficam de fora.
pub fn load_geometry() -> Option<(iced::Size, Option<iced::Point>)> {
    let text = std::fs::read_to_string(file()?.with_file_name("janela.txt")).ok()?;
    let mut v = std::collections::HashMap::new();
    for line in text.lines() {
        if let Some((k, x)) = line.split_once('=') {
            if let Ok(n) = x.trim().parse::<f32>() {
                v.insert(k.trim().to_string(), n);
            }
        }
    }
    let (w, h) = (*v.get("w")?, *v.get("h")?);
    if !(760.0..=10_000.0).contains(&w) || !(520.0..=10_000.0).contains(&h) {
        return None;
    }
    // monitor que sumiu deixaria a janela fora da tela: posição absurda volta ao centro
    let pos = match (v.get("x"), v.get("y")) {
        // a barra de título (canto de cima, 40 px para dentro) precisa cair num monitor conectado agora
        (Some(&x), Some(&y)) if (-10_000.0..=20_000.0).contains(&x) && (-200.0..=20_000.0).contains(&y) && orion_platform::point_on_monitor((x + 40.0) as i32, (y + 20.0) as i32) => Some(iced::Point::new(x, y)),
        _ => None,
    };
    Some((iced::Size::new(w, h), pos))
}

pub fn save_geometry(size: iced::Size, pos: Option<iced::Point>) -> std::io::Result<()> {
    let Some(path) = file() else { return Ok(()) };
    let mut body = format!("w={}\nh={}\n", size.width.round(), size.height.round());
    if let Some(p) = pos {
        body += &format!("x={}\ny={}\n", p.x.round(), p.y.round());
    }
    write_atomic(&path.with_file_name("janela.txt"), &body)
}
