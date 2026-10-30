//! Captura pontual da webcam (E11; REGRAS.md: a câmera só liga por pedido seu, e cada uso fica
//! registrado).
//!
//! Abre o dispositivo, lê um quadro e fecha na hora. A imagem crua nunca vai para o disco nem para o
//! banco; tudo fica em memória e morre ao fim da chamada.

use crate::util::{env_limpo, rodar, which, ErroProcesso};
use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{CameraFormat, CameraIndex, FrameFormat, RequestedFormat, RequestedFormatType};
use nokhwa::Camera;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub const MAX_FOTO: usize = 8 * 1024 * 1024;
/// Resolução de trabalho: todo quadro maior é reduzido a isto antes de qualquer análise.
pub const ANALYSIS_W: u32 = 640;
pub const ANALYSIS_H: u32 = 480;
/// O fluxo decodifica no máximo um quadro a cada 200 ms; os demais só são drenados do driver.
pub const DECODE_INTERVAL: Duration = Duration::from_millis(200);

#[derive(Debug, Clone, thiserror::Error)]
#[error("{0}")]
pub struct CameraError(pub String);

/// Lista os dispositivos de câmera disponíveis no sistema via nokhwa.
pub fn listar_cameras() -> Vec<String> {
    nokhwa::query(nokhwa::utils::ApiBackend::Auto)
        .map(|list| list.into_iter().map(|info| info.human_name()).collect())
        .unwrap_or_default()
}

/// Abre a câmera pedindo ~640x480 em MJPEG (o driver escolhe o formato mais próximo); se o
/// formato não existir, aceita o que o driver oferecer e o quadro é reduzido depois.
fn open_camera(device: i32) -> Result<Camera, CameraError> {
    let index = CameraIndex::Index(device as u32);
    let preferred = RequestedFormat::new::<RgbFormat>(RequestedFormatType::Closest(CameraFormat::new_from(ANALYSIS_W, ANALYSIS_H, FrameFormat::MJPEG, 30)));
    Camera::new(index.clone(), preferred)
        .or_else(|_| Camera::new(index, RequestedFormat::new::<RgbFormat>(RequestedFormatType::None)))
        .map_err(|e| CameraError(format!("falha ao inicializar webcam: {e}")))
}

/// Reduz (nunca amplia) para caber em 640x480 mantendo a proporção.
pub fn downscale_rgb(img: image::RgbImage) -> image::RgbImage {
    if img.width() <= ANALYSIS_W && img.height() <= ANALYSIS_H {
        return img;
    }
    let scale = (ANALYSIS_W as f64 / img.width() as f64).min(ANALYSIS_H as f64 / img.height() as f64);
    let (w, h) = (((img.width() as f64 * scale).round() as u32).max(1), ((img.height() as f64 * scale).round() as u32).max(1));
    image::imageops::resize(&img, w, h, image::imageops::FilterType::Triangle)
}

/// Um quadro já reduzido e em escala de cinza, pronto para `rosto_lbph::analisar_quadro`.
#[derive(Debug, Clone)]
pub struct GrayFrame {
    pub w: u32,
    pub h: u32,
    pub luma: Vec<u8>,
    /// Número de série: cresce a cada quadro novo (quem lê sabe se já viu este).
    pub seq: u64,
    /// Cópia colorida (mesma resolução), só quando quem abriu o fluxo precisa dela (gestos).
    pub rgb: Option<Arc<image::RgbImage>>,
}

impl GrayFrame {
    pub fn from_rgb(rgb: image::RgbImage, seq: u64) -> Self {
        Self::from_rgb_keep(rgb, seq, false)
    }

    pub fn from_rgb_keep(rgb: image::RgbImage, seq: u64, keep_rgb: bool) -> Self {
        let small = downscale_rgb(rgb);
        let luma = image::imageops::grayscale(&small);
        Self { w: luma.width(), h: luma.height(), luma: luma.into_raw(), seq, rgb: keep_rgb.then(|| Arc::new(small)) }
    }
}

/// Fonte de quadros: a webcam real (`CameraStream`) ou uma fonte de teste.
pub trait FrameSource: Send {
    /// O quadro mais recente, se houver um novo desde `depois_de`.
    fn latest(&self, after: u64) -> Option<Arc<GrayFrame>>;
    /// Motivo da falha, se a câmera caiu.
    fn error(&self) -> Option<String>;
}

/// Câmera aberta num fio próprio, só enquanto o Face ID procura um rosto (REGRAS.md: a câmera
/// só liga por pedido seu). Os quadros ficam em memória, apenas o último; nada vai a disco.
/// Fechar (ou soltar) o `CameraStream` para o fio e desliga a câmera.
pub struct CameraStream {
    latest: Arc<Mutex<Option<Arc<GrayFrame>>>>,
    error: Arc<Mutex<Option<String>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl CameraStream {
    /// Abre a câmera e espera até `espera` pela confirmação; sem câmera, falha logo (o chamador
    /// cai para a senha).
    pub fn open(device: i32, wait: Duration) -> Result<Self, CameraError> {
        Self::open_with(device, wait, DECODE_INTERVAL, false)
    }

    /// Como `open`, com intervalo de decodificação próprio e, se pedido, o quadro colorido junto
    /// (o módulo de câmera decodifica ~15 vezes por segundo para os gestos ficarem fluidos).
    pub fn open_with(device: i32, wait: Duration, interval: Duration, keep_rgb: bool) -> Result<Self, CameraError> {
        if !(0..=16).contains(&device) {
            return Err(CameraError("índice de câmera inválido".into()));
        }
        let latest = Arc::new(Mutex::new(None));
        let error = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), CameraError>>();
        let (u, e, p) = (latest.clone(), error.clone(), stop.clone());
        let thread = std::thread::Builder::new()
            .name("orion-camera".into())
            .spawn(move || {
                let mut camera = match open_camera(device).and_then(|mut c| {
                    c.open_stream().map_err(|e| CameraError(format!("falha ao abrir stream da webcam: {e}")))?;
                    Ok(c)
                }) {
                    Ok(c) => c,
                    Err(err) => {
                        let _ = ready_tx.send(Err(err));
                        return;
                    }
                };
                let _ = ready_tx.send(Ok(()));
                let mut seq = 0u64;
                let mut last_decode: Option<Instant> = None;
                let mut failures = 0u32;
                while !p.load(Ordering::Relaxed) {
                    // `frame()` bloqueia até o próximo quadro do driver (~33 ms a 30 fps)
                    let raw_frame = match camera.frame() {
                        Ok(f) => {
                            failures = 0;
                            f
                        }
                        Err(err) => {
                            failures += 1;
                            if failures >= 15 {
                                *e.lock() = Some(format!("a webcam parou de responder: {err}"));
                                break;
                            }
                            std::thread::sleep(Duration::from_millis(30));
                            continue;
                        }
                    };
                    if last_decode.is_some_and(|t| t.elapsed() < interval) {
                        continue;
                    }
                    last_decode = Some(Instant::now());
                    if let Ok(rgb) = raw_frame.decode_image::<RgbFormat>() {
                        seq += 1;
                        *u.lock() = Some(Arc::new(GrayFrame::from_rgb_keep(rgb, seq, keep_rgb)));
                    }
                }
                let _ = camera.stop_stream();
            })
            .map_err(|e| CameraError(format!("não foi possível iniciar a câmera: {e}")))?;
        let mut stream = Self { latest, error, stop, thread: Some(thread) };
        match ready_rx.recv_timeout(wait) {
            Ok(Ok(())) => Ok(stream),
            Ok(Err(e)) => Err(e),
            Err(_) => {
                stream.stop.store(true, Ordering::Relaxed);
                stream.thread.take();
                Err(CameraError("a webcam não respondeu a tempo".into()))
            }
        }
    }

    pub fn close(mut self) {
        self.shutdown();
    }

    /// Pede para o fio parar e não espera por ele: `camera.frame()` pode ficar preso numa webcam
    /// travada, e esperar aqui congelaria quem soltou o fluxo (inclusive a janela, ao cancelar a
    /// leitura para usar a senha). O fio desliga a câmera sozinho quando a leitura pendente volta.
    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        drop(self.thread.take());
        self.latest.lock().take();
    }
}

impl FrameSource for CameraStream {
    fn latest(&self, after: u64) -> Option<Arc<GrayFrame>> {
        self.latest.lock().as_ref().filter(|q| q.seq > after).cloned()
    }
    fn error(&self) -> Option<String> {
        self.error.lock().clone()
    }
}

impl Drop for CameraStream {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Captura um único quadro usando `nokhwa` (Media Foundation no Windows / nativo) e devolve em JPEG em memória.
pub fn capturar_nokhwa(dispositivo: i32) -> Result<Vec<u8>, CameraError> {
    if dispositivo < 0 || dispositivo > 16 {
        return Err(CameraError("índice de câmera inválido".into()));
    }
    let mut camera = open_camera(dispositivo)?;

    camera
        .open_stream()
        .map_err(|e| CameraError(format!("falha ao abrir stream da webcam: {e}")))?;

    let frame = camera
        .frame()
        .map_err(|e| CameraError(format!("falha ao capturar quadro da webcam: {e}")))?;

    let _ = camera.stop_stream();

    let rgb = downscale_rgb(
        frame
            .decode_image::<RgbFormat>()
            .map_err(|e| CameraError(format!("falha ao decodificar quadro RGB: {e}")))?,
    );

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
