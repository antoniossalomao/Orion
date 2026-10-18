//! Captura pontual da webcam (E11; REGRAS.md: a câmera só liga por pedido seu, e cada uso fica
//! registrado).
//!
//! Abre o dispositivo, lê um quadro e fecha na hora. A imagem crua nunca vai para o disco nem para o
//! banco; tudo fica em memória e morre ao fim da chamada.

use crate::util::{env_limpo, rodar, which, ErroProcesso};
use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{CameraIndex, RequestedFormat, RequestedFormatType};
use nokhwa::Camera;
use std::time::Duration;

pub const MAX_FOTO: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, thiserror::Error)]
#[error("{0}")]
pub struct CameraError(pub String);

/// Lista os dispositivos de câmera disponíveis no sistema via nokhwa.
pub fn listar_cameras() -> Vec<String> {
    nokhwa::query(nokhwa::utils::ApiBackend::Auto)
        .map(|list| list.into_iter().map(|info| info.human_name()).collect())
        .unwrap_or_default()
}

/// Captura um único quadro usando `nokhwa` (Media Foundation no Windows / nativo) e devolve em JPEG em memória.
pub fn capturar_nokhwa(dispositivo: i32) -> Result<Vec<u8>, CameraError> {
    if dispositivo < 0 || dispositivo > 16 {
        return Err(CameraError("índice de câmera inválido".into()));
    }
    let index = CameraIndex::Index(dispositivo as u32);
    let requested = RequestedFormat::new::<RgbFormat>(RequestedFormatType::None);
    let mut camera = Camera::new(index, requested)
        .map_err(|e| CameraError(format!("falha ao inicializar webcam: {e}")))?;

    camera
        .open_stream()
        .map_err(|e| CameraError(format!("falha ao abrir stream da webcam: {e}")))?;

    let frame = camera
        .frame()
        .map_err(|e| CameraError(format!("falha ao capturar quadro da webcam: {e}")))?;

    let _ = camera.stop_stream();

    let rgb = frame
        .decode_image::<RgbFormat>()
        .map_err(|e| CameraError(format!("falha ao decodificar quadro RGB: {e}")))?;

    let mut jpeg = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 85);
    encoder
        .encode(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|e| CameraError(format!("falha ao codificar JPEG: {e}")))?;

    if jpeg.len() > MAX_FOTO {
        return Err(CameraError("a webcam devolveu mais de 8 MB".into()));
    }
    Ok(jpeg)
}

fn argv_ffmpeg(dispositivo: i32) -> Option<Vec<String>> {
    let exe = which("ffmpeg")?.to_string_lossy().into_owned();
    let base = vec![exe, "-nostdin".into(), "-hide_banner".into(), "-loglevel".into(), "error".into()];
    let entrada = if cfg!(windows) {
        vec!["-f".into(), "dshow".into(), "-i".into(), format!("video={dispositivo}")]
    } else if cfg!(target_os = "macos") {
        vec!["-f".into(), "avfoundation".into(), "-framerate".into(), "30".into(), "-i".into(), format!("{dispositivo}:none")]
    } else {
        vec!["-f".into(), "v4l2".into(), "-i".into(), format!("/dev/video{dispositivo}")]
    };
    let saida = vec!["-frames:v".into(), "1".into(), "-f".into(), "image2".into(), "-vcodec".into(), "mjpeg".into(), "-q:v".into(), "3".into(), "pipe:1".into()];
    Some([base, entrada, saida].concat())
}

/// Tira UMA foto da webcam em JPEG (bytes em memória) e fecha o dispositivo na hora.
/// Tenta a captura nativa via `nokhwa` (Media Foundation) primeiro; se falhar (ex: sem driver físico ou mock),
/// recorre ao `ffmpeg` como fallback.
pub fn foto_jpeg(dispositivo: i32, timeout_s: f64) -> Result<Vec<u8>, CameraError> {
    if dispositivo < 0 || dispositivo > 16 {
        return Err(CameraError("índice de câmera inválido".into()));
    }
    // Tenta primeiro captura nativa com nokhwa
    match capturar_nokhwa(dispositivo) {
        Ok(bytes) => return Ok(bytes),
        Err(e) => {
            tracing::debug!("Captura nativa nokhwa indisponível ({e}); tentando ffmpeg");
        }
    }

    let argv = argv_ffmpeg(dispositivo).ok_or_else(|| CameraError("sem captura de câmera neste sistema".into()))?;
    let r = rodar(&argv, None, Duration::from_secs_f64(timeout_s), Some(env_limpo())).map_err(|e| match e {
        ErroProcesso::Tempo => CameraError("a webcam não respondeu a tempo".into()),
        ErroProcesso::Io(_) => CameraError("sem captura de câmera neste sistema".into()),
    })?;
    if r.codigo != Some(0) || !r.stdout.starts_with(b"\xff\xd8") {
        let motivo: String = String::from_utf8_lossy(&r.stderr).trim().chars().take(160).collect();
        return Err(CameraError(format!(
            "webcam {dispositivo} indisponível{}",
            if motivo.is_empty() { String::new() } else { format!(": {motivo}") }
        )));
    }
    if r.stdout.len() > MAX_FOTO {
        return Err(CameraError("a webcam devolveu mais de 8 MB".into()));
    }
    Ok(r.stdout)
}

/// JPEG em memória -> `(largura, altura, rgb_bytes)`.
pub fn decodificar(jpeg: &[u8]) -> Result<(u32, u32, Vec<u8>), CameraError> {
    let img = image::load_from_memory(jpeg).map_err(|_| CameraError("a imagem não pôde ser lida".into()))?.to_rgb8();
    Ok((img.width(), img.height(), img.into_raw()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indice_invalido_recusa_sem_abrir() {
        assert!(foto_jpeg(-1, 1.0).is_err());
        assert!(foto_jpeg(99, 1.0).is_err());
    }

    #[test]
    fn decodificar_bytes_invalidos_falha() {
        assert!(decodificar(b"nao e jpeg").is_err());
    }
}
