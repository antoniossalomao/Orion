//! M19: imagem colada (Ctrl+V) no compositor vira anexo. A imagem da área de transferência é gravada
//! como PNG temporário e segue o mesmo caminho do arrastar e soltar: limite, tipo e assinatura dos bytes
//! conferidos no app e de novo no núcleo (REGRAS.md: conteúdo externo é dado, não instrução).

/// Prefixo do PNG temporário; o anexo apaga o arquivo depois de ler os bytes.
pub const PASTED_PREFIX: &str = "orion-colado-";

/// Lê a imagem da área de transferência e grava um PNG temporário. `None` quando não há imagem
/// (texto comum colado segue só pelo campo de mensagem) ou quando a leitura falha.
pub fn clipboard_image_to_file() -> Option<String> {
    let img = arboard::Clipboard::new().ok()?.get_image().ok()?;
    let (w, h) = (u32::try_from(img.width).ok()?, u32::try_from(img.height).ok()?);
    let buf = image::RgbaImage::from_raw(w, h, img.bytes.into_owned())?;
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let path = std::env::temp_dir().join(format!("{PASTED_PREFIX}{stamp}.png"));
    buf.save_with_format(&path, image::ImageFormat::Png).ok()?;
    Some(path.to_string_lossy().into_owned())
}

/// Apaga o PNG temporário de uma colagem, se o caminho for um deles; outros arquivos nunca são tocados.
pub fn remove_if_pasted(path: &std::path::Path) {
    let is_pasted = path.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(PASTED_PREFIX) && n.ends_with(".png"));
    if is_pasted && path.parent() == Some(std::env::temp_dir().as_path()) {
        let _ = std::fs::remove_file(path);
    }
}
