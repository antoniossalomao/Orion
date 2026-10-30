//! Gestos com as mãos no módulo de câmera (REGRAS.md: gesto só abre e navega; nunca aprova nem
//! executa).
//!
//! Pipeline do MediaPipe Hands (modelos ONNX do OpenCV Zoo, Apache 2.0): detector de palma 192×192
//! → 21 pontos da mão num recorte girado 224×224 → regras geométricas para o gesto. A inferência
//! roda no ONNX Runtime carregado em tempo de execução (`ort` com `load-dynamic`): DirectML no
//! Windows, CoreML no Mac, CPU no resto. As DLLs e os modelos ficam em `runtime_dir()`, baixados
//! e conferidos pelo instalador; sem eles, os gestos ficam indisponíveis e nada mais muda.
//!
//! Para não gastar GPU à toa, um detector de movimento barato (CPU, 80×60, metade de baixo do
//! quadro) só acorda o detector de mãos quando algo se mexe; com a mão rastreada, o detector de
//! palma nem roda: o recorte do quadro seguinte vem dos pontos do quadro atual.

use image::RgbImage;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

pub const PALM_MODEL: &str = "palm_detection_mediapipe_2023feb.onnx";
pub const LANDMARK_MODEL: &str = "handpose_estimation_mediapipe_2023feb.onnx";
#[cfg(windows)]
pub const ORT_LIB: &str = "onnxruntime.dll";
#[cfg(target_os = "macos")]
pub const ORT_LIB: &str = "libonnxruntime.dylib";
#[cfg(all(unix, not(target_os = "macos")))]
pub const ORT_LIB: &str = "libonnxruntime.so";

const PALM_SIZE: u32 = 192;
const HAND_SIZE: u32 = 224;
const PALM_SCORE: f32 = 0.5;
const HAND_SCORE: f32 = 0.6;
/// Depois de movimento, o detector fica acordado por este tempo.
pub const ACTIVE_WINDOW: Duration = Duration::from_millis(2500);
/// Diferença média (0..255) entre quadros reduzidos que conta como movimento.
const MOTION_THRESHOLD: f32 = 4.0;
/// Quadros seguidos com o mesmo gesto antes de valer.
const STABLE_FRAMES: u32 = 4;
const GESTURE_COOLDOWN: Duration = Duration::from_millis(1000);
const SWIPE_WINDOW: Duration = Duration::from_millis(650);
const SWIPE_DX: f32 = 0.22;

/// Pasta das DLLs e dos modelos: `ORION_RUNTIME_DIR` ou `%LOCALAPPDATA%\Orion\runtime`.
pub fn runtime_dir() -> Option<PathBuf> {
    std::env::var_os("ORION_RUNTIME_DIR").map(PathBuf::from).or_else(|| dirs::data_local_dir().map(|d| d.join("Orion").join("runtime")))
}

/// Tudo o que os gestos precisam está na pasta? (barato: só confere arquivos)
pub fn available(dir: &Path) -> bool {
    [ORT_LIB, PALM_MODEL, LANDMARK_MODEL].iter().all(|f| dir.join(f).is_file())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accel {
    DirectMl,
    CoreMl,
    Cpu,
}

impl Accel {
    pub fn label(self) -> &'static str {
        match self {
            Accel::DirectMl => "DirectML",
            Accel::CoreMl => "CoreML",
            Accel::Cpu => "CPU",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GestureError {
    #[error("motor de gestos não instalado: falta {0}")]
    Missing(String),
    #[error("motor de gestos indisponível: {0}")]
    Runtime(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Gesture {
    OpenPalm,
    Fist,
    ThumbsUp,
    Peace,
    Point,
    SwipeLeft,
    SwipeRight,
}

impl Gesture {
    /// Rótulo do HUD (em inglês, como a telemetria).
    pub fn label(self) -> &'static str {
        match self {
            Gesture::OpenPalm => "OPEN PALM",
            Gesture::Fist => "FIST",
            Gesture::ThumbsUp => "THUMBS UP",
            Gesture::Peace => "PEACE",
            Gesture::Point => "POINT",
            Gesture::SwipeLeft => "SWIPE LEFT",
            Gesture::SwipeRight => "SWIPE RIGHT",
        }
    }
}

/// Uma mão rastreada: 21 pontos em pixels do quadro e a confiança do modelo.
#[derive(Debug, Clone)]
pub struct Hand {
    pub points: [[f32; 2]; 21],
    pub score: f32,
}

impl Hand {
    fn size(&self) -> f32 {
        dist(self.points[0], self.points[9]).max(1.0)
    }
}

fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

// ── geometria do recorte girado ─────────────────────────────────────────────

/// Retângulo girado (centro, lado do quadrado, rotação em radianos), em pixels do quadro.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Roi {
    pub cx: f32,
    pub cy: f32,
    pub size: f32,
    pub rotation: f32,
}

impl Roi {
    /// Ponto do recorte (u, v em 0..1) → ponto do quadro.
    pub fn to_frame(&self, u: f32, v: f32) -> [f32; 2] {
        let (ox, oy) = ((u - 0.5) * self.size, (v - 0.5) * self.size);
        let (s, c) = self.rotation.sin_cos();
        [self.cx + ox * c - oy * s, self.cy + ox * s + oy * c]
    }
}

fn normalize_angle(a: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    a - TAU * ((a + PI) / TAU).floor()
}

/// Rotação que deixa a mão "em pé" (de `from` para `to` apontando para cima), como no MediaPipe.
fn rotation(from: [f32; 2], to: [f32; 2]) -> f32 {
    normalize_angle(std::f32::consts::FRAC_PI_2 - (-(to[1] - from[1])).atan2(to[0] - from[0]))
}

/// Retângulo girado a partir de uma caixa alinhada (centro e tamanho) com deslocamento e escala no
/// referencial da mão (regra `RectTransformation` do MediaPipe, lado longo, quadrado).
fn transform_rect(cx: f32, cy: f32, w: f32, h: f32, rotation: f32, shift_y: f32, scale: f32) -> Roi {
    let (s, c) = rotation.sin_cos();
    let (dx, dy) = (-h * shift_y * s, h * shift_y * c);
    Roi { cx: cx + dx, cy: cy + dy, size: w.max(h) * scale, rotation }
}

/// Recorte girado e redimensionado (bilinear) para o tensor NHWC normalizado 0..1.
fn crop_tensor(img: &RgbImage, roi: &Roi, out: u32) -> Vec<f32> {
    let (w, h) = (img.width() as i32, img.height() as i32);
    let raw = img.as_raw();
    let mut t = Vec::with_capacity((out * out * 3) as usize);
    let px = |x: i32, y: i32, ch: usize| -> f32 {
        if x < 0 || y < 0 || x >= w || y >= h {
            0.0
        } else {
            raw[((y * w + x) * 3) as usize + ch] as f32
        }
    };
    for v in 0..out {
        for u in 0..out {
            let [sx, sy] = roi.to_frame((u as f32 + 0.5) / out as f32, (v as f32 + 0.5) / out as f32);
            let (x0, y0) = ((sx - 0.5).floor(), (sy - 0.5).floor());
            let (fx, fy) = (sx - 0.5 - x0, sy - 0.5 - y0);
            let (x0, y0) = (x0 as i32, y0 as i32);
            for ch in 0..3 {
                let a = px(x0, y0, ch) * (1.0 - fx) + px(x0 + 1, y0, ch) * fx;
                let b = px(x0, y0 + 1, ch) * (1.0 - fx) + px(x0 + 1, y0 + 1, ch) * fx;
                t.push((a * (1.0 - fy) + b * fy) / 255.0);
            }
        }
    }
    t
}

/// Âncoras SSD do detector de palma 192×192: passos 8 (2 por célula) e 16 (6 por célula) = 2016.
fn palm_anchors() -> &'static [[f32; 2]] {
    static ANCHORS: OnceLock<Vec<[f32; 2]>> = OnceLock::new();
    ANCHORS.get_or_init(|| {
        let mut a = Vec::with_capacity(2016);
        for (stride, per_cell) in [(8u32, 2usize), (16, 6)] {
            let grid = PALM_SIZE / stride;
            for y in 0..grid {
                for x in 0..grid {
                    for _ in 0..per_cell {
                        a.push([(x as f32 + 0.5) / grid as f32, (y as f32 + 0.5) / grid as f32]);
                    }
                }
            }
        }
        a
    })
}

/// Palma detectada: caixa (x1, y1, x2, y2), 7 pontos e pontuação, em pixels do quadro.
#[derive(Debug, Clone)]
pub struct Palm {
    pub bbox: [f32; 4],
    pub keypoints: [[f32; 2]; 7],
    pub score: f32,
}

fn iou(a: &[f32; 4], b: &[f32; 4]) -> f32 {
    let (x1, y1, x2, y2) = (a[0].max(b[0]), a[1].max(b[1]), a[2].min(b[2]), a[3].min(b[3]));
    let inter = (x2 - x1).max(0.0) * (y2 - y1).max(0.0);
    let area = |r: &[f32; 4]| (r[2] - r[0]).max(0.0) * (r[3] - r[1]).max(0.0);
    inter / (area(a) + area(b) - inter).max(1e-6)
}

/// Decodifica a saída do detector (`boxes`: 2016×18, `scores`: 2016) já em pixels do quadro.
pub fn decode_palms(boxes: &[f32], scores: &[f32], frame_w: f32, frame_h: f32) -> Vec<Palm> {
    let anchors = palm_anchors();
    let scale = frame_w.max(frame_h);
    // letterbox centrado: o lado curto ganha borda preta
    let (pad_x, pad_y) = ((scale - frame_w) / 2.0, (scale - frame_h) / 2.0);
    let mut found: Vec<Palm> = Vec::new();
    for (i, anchor) in anchors.iter().enumerate().take(scores.len().min(boxes.len() / 18)) {
        let score = 1.0 / (1.0 + (-scores[i]).exp());
        if score < PALM_SCORE {
            continue;
        }
        let d = &boxes[i * 18..i * 18 + 18];
        let (cx, cy) = (d[0] / PALM_SIZE as f32 + anchor[0], d[1] / PALM_SIZE as f32 + anchor[1]);
        let (w, h) = (d[2] / PALM_SIZE as f32, d[3] / PALM_SIZE as f32);
        let bbox = [(cx - w / 2.0) * scale - pad_x, (cy - h / 2.0) * scale - pad_y, (cx + w / 2.0) * scale - pad_x, (cy + h / 2.0) * scale - pad_y];
        let mut keypoints = [[0.0f32; 2]; 7];
        for (k, kp) in keypoints.iter_mut().enumerate() {
            *kp = [(d[4 + k * 2] / PALM_SIZE as f32 + anchor[0]) * scale - pad_x, (d[5 + k * 2] / PALM_SIZE as f32 + anchor[1]) * scale - pad_y];
        }
        found.push(Palm { bbox, keypoints, score });
    }
    found.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut kept: Vec<Palm> = Vec::new();
    for p in found {
        if kept.iter().all(|k| iou(&k.bbox, &p.bbox) < 0.3) {
            kept.push(p);
        }
        if kept.len() == 2 {
            break;
        }
    }
    kept
}

/// Recorte da mão a partir da palma (MediaPipe: punho → base do dedo médio, desloca −0,5, escala 2,6).
pub fn roi_from_palm(p: &Palm) -> Roi {
    let rot = rotation(p.keypoints[0], p.keypoints[2]);
    let (cx, cy) = ((p.bbox[0] + p.bbox[2]) / 2.0, (p.bbox[1] + p.bbox[3]) / 2.0);
    transform_rect(cx, cy, p.bbox[2] - p.bbox[0], p.bbox[3] - p.bbox[1], rot, -0.5, 2.6)
}

/// Recorte do quadro seguinte a partir dos pontos atuais (rastreamento sem detector de palma).
pub fn roi_from_hand(hand: &Hand) -> Roi {
    let rot = rotation(hand.points[0], hand.points[9]);
    let (s, c) = (-rot).sin_cos();
    let (mx, my) = hand.points.iter().fold((0.0, 0.0), |a, p| (a.0 + p[0] / 21.0, a.1 + p[1] / 21.0));
    // caixa dos pontos no referencial da mão
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in &hand.points {
        let (dx, dy) = (p[0] - mx, p[1] - my);
        let (rx, ry) = (dx * c - dy * s, dx * s + dy * c);
        x0 = x0.min(rx);
        y0 = y0.min(ry);
        x1 = x1.max(rx);
        y1 = y1.max(ry);
    }
    let (ccx, ccy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let (s2, c2) = rot.sin_cos();
    let (cx, cy) = (mx + ccx * c2 - ccy * s2, my + ccx * s2 + ccy * c2);
    transform_rect(cx, cy, x1 - x0, y1 - y0, rot, -0.1, 2.0)
}

// ── classificação ───────────────────────────────────────────────────────────

fn finger_extended(h: &Hand, mcp: usize, pip: usize, tip: usize) -> bool {
    let p = &h.points;
    let reach = dist(p[0], p[tip]) > dist(p[0], p[pip]) * 1.12;
    let (a, b) = ([p[pip][0] - p[mcp][0], p[pip][1] - p[mcp][1]], [p[tip][0] - p[pip][0], p[tip][1] - p[pip][1]]);
    let cos = (a[0] * b[0] + a[1] * b[1]) / ((a[0].hypot(a[1]) * b[0].hypot(b[1])).max(1e-6));
    reach && cos > 0.5
}

fn thumb_extended(h: &Hand) -> bool {
    dist(h.points[4], h.points[5]) / h.size() > 0.6 && dist(h.points[4], h.points[17]) > dist(h.points[3], h.points[17])
}

/// Gesto estático de uma mão (sem tempo): regras sobre os 21 pontos.
pub fn classify(h: &Hand) -> Option<Gesture> {
    let ext = [finger_extended(h, 5, 6, 8), finger_extended(h, 9, 10, 12), finger_extended(h, 13, 14, 16), finger_extended(h, 17, 18, 20)];
    let thumb = thumb_extended(h);
    let up = h.points[0][1] - h.points[4][1] > 0.6 * h.size() && h.points[4][1] < h.points[5][1];
    match ext {
        [true, true, true, true] if thumb => Some(Gesture::OpenPalm),
        [false, false, false, false] if thumb && up => Some(Gesture::ThumbsUp),
        [false, false, false, false] if !thumb || !up => Some(Gesture::Fist),
        [true, true, false, false] => Some(Gesture::Peace),
        [true, false, false, false] => Some(Gesture::Point),
        _ => None,
    }
}

// ── tempo: estabilidade, deslize e movimento ────────────────────────────────

/// Só deixa passar um gesto estático depois de `STABLE_FRAMES` quadros iguais, uma vez por pose,
/// com intervalo mínimo entre disparos.
#[derive(Default)]
pub struct Stabilizer {
    candidate: Option<Gesture>,
    count: u32,
    fired: Option<Gesture>,
    last_fire: Option<Instant>,
}

impl Stabilizer {
    pub fn push(&mut self, g: Option<Gesture>, now: Instant) -> Option<Gesture> {
        if g != self.candidate {
            self.candidate = g;
            self.count = 0;
            if g.is_none() {
                self.fired = None;
            }
        }
        let g = g?;
        self.count += 1;
        let cooled = self.last_fire.is_none_or(|t| now.duration_since(t) >= GESTURE_COOLDOWN);
        if self.count >= STABLE_FRAMES && self.fired != Some(g) && cooled {
            self.fired = Some(g);
            self.last_fire = Some(now);
            return Some(g);
        }
        None
    }
}

/// Deslize horizontal da mão (coordenadas já espelhadas, 0..1, como a pessoa vê a tela).
#[derive(Default)]
pub struct SwipeDetector {
    track: VecDeque<(Instant, f32, f32)>,
    last: Option<Instant>,
}

impl SwipeDetector {
    pub fn push(&mut self, x: f32, y: f32, now: Instant) -> Option<Gesture> {
        self.track.push_back((now, x, y));
        while self.track.front().is_some_and(|(t, _, _)| now.duration_since(*t) > SWIPE_WINDOW) {
            self.track.pop_front();
        }
        if self.last.is_some_and(|t| now.duration_since(t) < GESTURE_COOLDOWN) {
            return None;
        }
        let &(_, x0, y0) = self.track.front()?;
        let (dx, dy) = (x - x0, y - y0);
        if dx.abs() > SWIPE_DX && dy.abs() < SWIPE_DX * 0.6 {
            self.track.clear();
            self.last = Some(now);
            return Some(if dx > 0.0 { Gesture::SwipeRight } else { Gesture::SwipeLeft });
        }
        None
    }

    pub fn reset(&mut self) {
        self.track.clear();
    }
}

/// Movimento na metade de baixo do quadro, comparando miniaturas 80×60 em CPU.
#[derive(Default)]
pub struct MotionGate {
    previous: Vec<u8>,
    awake_until: Option<Instant>,
}

impl MotionGate {
    pub fn push(&mut self, luma: &[u8], w: usize, h: usize, now: Instant) -> bool {
        let (gw, gh) = (80usize, 60usize);
        let mut small = vec![0u8; gw * gh];
        if w >= gw && h >= gh && luma.len() >= w * h {
            for gy in 0..gh {
                let y = gy * h / gh;
                for gx in 0..gw {
                    small[gy * gw + gx] = luma[y * w + gx * w / gw];
                }
            }
        }
        let start = gh * 2 / 5;
        let moved = self.previous.len() == small.len() && {
            let n = ((gh - start) * gw) as f32;
            let sum: u32 = small[start * gw..].iter().zip(&self.previous[start * gw..]).map(|(a, b)| a.abs_diff(*b) as u32).sum();
            sum as f32 / n > MOTION_THRESHOLD
        };
        self.previous = small;
        if moved {
            self.awake_until = Some(now + ACTIVE_WINDOW);
        }
        self.awake_until.is_some_and(|t| now < t)
    }

    /// A mão continua na imagem: mantém acordado.
    pub fn keep_awake(&mut self, now: Instant) {
        self.awake_until = Some(now + ACTIVE_WINDOW);
    }
}

// ── inferência ──────────────────────────────────────────────────────────────

static ENV: OnceLock<Result<(ort::environment::Environment, Accel), String>> = OnceLock::new();

/// Carrega o ONNX Runtime da pasta (uma vez por processo) e escolhe a aceleração disponível.
fn environment(dir: &Path) -> Result<Accel, GestureError> {
    let r = ENV.get_or_init(|| {
        let lib = dir.join(ORT_LIB);
        if !lib.is_file() {
            return Err(format!("{}", lib.display()));
        }
        #[cfg(windows)]
        {
            // carrega a DirectML.dll baixada antes do runtime, para o Windows não pegar a do sistema
            let dml = dir.join("DirectML.dll");
            if dml.is_file() {
                let _ = ort::util::preload_dylib(&dml);
            }
        }
        let builder = ort::init_from(&lib).map_err(|e| e.to_string())?.with_name("orion-gestos");
        let env = builder.build().map_err(|e| e.to_string())?;
        let accel = if cfg!(windows) {
            Accel::DirectMl
        } else if cfg!(target_os = "macos") {
            Accel::CoreMl
        } else {
            Accel::Cpu
        };
        Ok((env, accel))
    });
    match r {
        Ok((_, accel)) => Ok(*accel),
        Err(e) if e.ends_with(ORT_LIB) => Err(GestureError::Missing(ORT_LIB.into())),
        Err(e) => Err(GestureError::Runtime(e.clone())),
    }
}

fn session(dir: &Path, model: &str, accel: Accel) -> Result<(ort::session::Session, Accel), GestureError> {
    let path = dir.join(model);
    if !path.is_file() {
        return Err(GestureError::Missing(model.into()));
    }
    let Some(Ok((env, _))) = ENV.get() else { return Err(GestureError::Runtime("ambiente não iniciado".into())) };
    let err = |e: ort::Error| GestureError::Runtime(e.to_string());
    let builder = || -> Result<ort::session::builder::SessionBuilder, GestureError> {
        ort::session::Session::builder(env).map_err(err)?.with_intra_threads(1).map_err(|e| err(e.into()))
    };
    // tenta a GPU com erro explícito; se a placa/driver recusar, cai para CPU
    let accelerated = match accel {
        #[cfg(windows)]
        Accel::DirectMl => builder()?
            .with_memory_pattern(false)
            .and_then(|b| b.with_parallel_execution(false))
            .and_then(|b| b.with_execution_providers([ort::ep::DirectML::default().build().error_on_failure()]))
            .map_err(|e| err(e.into()))
            .and_then(|mut b| b.commit_from_file(&path).map_err(err)),
        #[cfg(target_os = "macos")]
        Accel::CoreMl => builder()?
            .with_execution_providers([ort::ep::CoreML::default().build().error_on_failure()])
            .map_err(|e| err(e.into()))
            .and_then(|mut b| b.commit_from_file(&path).map_err(err)),
        _ => Err(GestureError::Runtime("sem aceleração".into())),
    };
    match accelerated {
        Ok(s) => Ok((s, accel)),
        Err(e) => {
            if accel != Accel::Cpu {
                tracing::info!("gestos: aceleração {} indisponível ({e}); usando CPU", accel.label());
            }
            Ok((builder()?.commit_from_file(&path).map_err(err)?, Accel::Cpu))
        }
    }
}

/// Os dois modelos carregados.
pub struct HandModels {
    palm: ort::session::Session,
    landmarks: ort::session::Session,
    pub accel: Accel,
}

impl HandModels {
    pub fn load(dir: &Path) -> Result<Self, GestureError> {
        let want = environment(dir)?;
        let (palm, a1) = session(dir, PALM_MODEL, want)?;
        let (landmarks, a2) = session(dir, LANDMARK_MODEL, want)?;
        Ok(Self { palm, landmarks, accel: if a1 == a2 { a1 } else { Accel::Cpu } })
    }

    pub fn detect_palms(&mut self, img: &RgbImage) -> Result<Vec<Palm>, GestureError> {
        let (w, h) = (img.width() as f32, img.height() as f32);
        let scale = w.max(h);
        let roi = Roi { cx: w / 2.0, cy: h / 2.0, size: scale, rotation: 0.0 };
        let input = crop_tensor(img, &roi, PALM_SIZE);
        let err = |e: ort::Error| GestureError::Runtime(e.to_string());
        let tensor = ort::value::Tensor::from_array(([1usize, PALM_SIZE as usize, PALM_SIZE as usize, 3], input)).map_err(err)?;
        let out = self.palm.run(ort::inputs![tensor]).map_err(err)?;
        let (mut boxes, mut scores): (Vec<f32>, Vec<f32>) = (vec![], vec![]);
        for (_, v) in out.iter() {
            let (shape, data) = v.try_extract_tensor::<f32>().map_err(err)?;
            match shape.last() {
                Some(18) => boxes = data.to_vec(),
                Some(1) => scores = data.to_vec(),
                _ => {}
            }
        }
        let (boxes, scores) = (&boxes[..], &scores[..]);
        Ok(decode_palms(boxes, scores, w, h))
    }

    /// 21 pontos dentro do recorte; `None` se o modelo não vê mão ali.
    pub fn landmarks(&mut self, img: &RgbImage, roi: &Roi) -> Result<Option<Hand>, GestureError> {
        let input = crop_tensor(img, roi, HAND_SIZE);
        let err = |e: ort::Error| GestureError::Runtime(e.to_string());
        let tensor = ort::value::Tensor::from_array(([1usize, HAND_SIZE as usize, HAND_SIZE as usize, 3], input)).map_err(err)?;
        let out = self.landmarks.run(ort::inputs![tensor]).map_err(err)?;
        // saídas: pontos na tela (63), confiança (1), mão (1), pontos no mundo (63) — nessa ordem
        let mut screen: Option<Vec<f32>> = None;
        let mut singles: Vec<f32> = vec![];
        for (_, v) in out.iter() {
            let (_, data) = v.try_extract_tensor::<f32>().map_err(err)?;
            if data.len() == 63 && screen.is_none() {
                screen = Some(data.to_vec());
            } else if data.len() == 1 {
                singles.push(data[0]);
            }
        }
        let (Some(screen), Some(&score)) = (screen, singles.first()) else { return Ok(None) };
        if score < HAND_SCORE {
            return Ok(None);
        }
        let mut points = [[0.0f32; 2]; 21];
        for (i, p) in points.iter_mut().enumerate() {
            *p = roi.to_frame(screen[i * 3] / HAND_SIZE as f32, screen[i * 3 + 1] / HAND_SIZE as f32);
        }
        Ok(Some(Hand { points, score }))
    }
}

/// O que sai de cada quadro analisado.
#[derive(Debug, Clone, Default)]
pub struct FrameResult {
    pub hand: Option<Hand>,
    pub gesture: Option<Gesture>,
    /// Detector acordado (movimento recente ou mão rastreada).
    pub awake: bool,
    /// Tempo da inferência neste quadro.
    pub infer_ms: f32,
}

/// Motor completo: movimento → palma (só sem rastreio) → pontos → gesto.
pub struct GestureEngine {
    models: HandModels,
    gate: MotionGate,
    stabilizer: Stabilizer,
    swipe: SwipeDetector,
    tracked: Option<Roi>,
}

impl GestureEngine {
    pub fn new(models: HandModels) -> Self {
        Self { models, gate: MotionGate::default(), stabilizer: Stabilizer::default(), swipe: SwipeDetector::default(), tracked: None }
    }

    pub fn accel(&self) -> Accel {
        self.models.accel
    }

    pub fn process(&mut self, img: &RgbImage, luma: &[u8], now: Instant) -> Result<FrameResult, GestureError> {
        let (w, h) = (img.width() as usize, img.height() as usize);
        let awake = self.gate.push(luma, w, h, now) || self.tracked.is_some();
        if !awake {
            self.stabilizer.push(None, now);
            self.swipe.reset();
            return Ok(FrameResult::default());
        }
        let started = Instant::now();
        let roi = match self.tracked {
            Some(r) => Some(r),
            None => self.models.detect_palms(img)?.first().map(roi_from_palm),
        };
        let hand = match roi {
            Some(r) => self.models.landmarks(img, &r)?,
            None => None,
        };
        let infer_ms = started.elapsed().as_secs_f32() * 1000.0;
        let Some(hand) = hand else {
            self.tracked = None;
            self.stabilizer.push(None, now);
            self.swipe.reset();
            return Ok(FrameResult { awake: true, infer_ms, ..Default::default() });
        };
        self.tracked = Some(roi_from_hand(&hand));
        self.gate.keep_awake(now);
        // centro da palma em coordenadas espelhadas (como a pessoa se vê na tela)
        let c = hand.points[9];
        let swipe = self.swipe.push(1.0 - c[0] / w as f32, c[1] / h as f32, now);
        let pose = self.stabilizer.push(classify(&hand), now);
        Ok(FrameResult { gesture: swipe.or(pose), hand: Some(hand), awake: true, infer_ms })
    }
}
