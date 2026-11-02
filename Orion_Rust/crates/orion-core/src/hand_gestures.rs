//! Gestos com as mãos no módulo de câmera (REGRAS.md: gesto só abre e navega; nunca aprova nem
//! executa).
//!
//! Pipeline do MediaPipe Hands (modelos ONNX do OpenCV Zoo, Apache 2.0): detector de palma 192×192
//! → 21 pontos da mão num recorte girado 224×224 → regras geométricas para o gesto. A inferência
//! roda no ONNX Runtime carregado em tempo de execução (`ort` com `load-dynamic`): DirectML no
//! Windows, CoreML no Mac, CPU no resto. As DLLs e os modelos ficam em `runtime_dir()`, baixados
//! e conferidos pelo instalador; sem eles, os gestos ficam indisponíveis e nada mais muda.
//!
//! Para não gastar GPU à toa, um detector de movimento barato (CPU, 80×60, quadro inteiro)
//! quadro) só acorda o detector de mãos quando algo se mexe; com a mão rastreada, o detector de
//! palma nem roda: o recorte do quadro seguinte vem dos pontos do quadro atual.

use image::RgbImage;
use std::collections::VecDeque;
use std::path::Path;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

pub const PALM_MODEL: &str = "palm_detection_mediapipe_2023feb.onnx";
pub const LANDMARK_MODEL: &str = "handpose_estimation_mediapipe_2023feb.onnx";
pub use crate::vision_runtime::{runtime_dir, Accel, ORT_LIB};

const PALM_SIZE: u32 = 192;
const HAND_SIZE: u32 = 224;
const PALM_SCORE: f32 = 0.5;
const HAND_SCORE: f32 = 0.75;
/// Depois de movimento, o detector fica acordado por este tempo.
pub const ACTIVE_WINDOW: Duration = Duration::from_millis(2500);
/// Diferença média (0..255) entre quadros reduzidos que conta como movimento.
const MOTION_THRESHOLD: f32 = 4.0;
/// Quadros seguidos com o mesmo gesto antes de valer.
const STABLE_FRAMES: u32 = 4;
const GESTURE_COOLDOWN: Duration = Duration::from_millis(1000);
const SWIPE_WINDOW: Duration = Duration::from_millis(650);
const SWIPE_DX: f32 = 0.22;

/// Tudo o que os gestos precisam está na pasta? (barato: só confere arquivos)
pub fn available(dir: &Path) -> bool {
    [ORT_LIB, PALM_MODEL, LANDMARK_MODEL].iter().all(|f| dir.join(f).is_file())
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
    /// Coordenadas relativas estimadas pelo modelo; não são medições de um sensor de profundidade.
    pub world: Option<[[f32; 3]; 21]>,
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
        if !scores[i].is_finite() || !score.is_finite() || score < PALM_SCORE {
            continue;
        }
        let d = &boxes[i * 18..i * 18 + 18];
        if d.iter().any(|v| !v.is_finite()) || d[2] <= 0.0 || d[3] <= 0.0 { continue; }
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
    let (s, c) = (-rot).sin_cos();
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for p in p.keypoints {
        let (x, y) = (p[0] * c - p[1] * s, p[0] * s + p[1] * c);
        x0 = x0.min(x); y0 = y0.min(y); x1 = x1.max(x); y1 = y1.max(y);
    }
    let (s, c) = rot.sin_cos();
    let (x, y) = ((x0 + x1) * 0.5, (y0 + y1) * 0.5);
    transform_rect(x * c - y * s, x * s + y * c, x1 - x0, y1 - y0, rot, -0.4, 3.0)
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
    transform_rect(cx, cy, x1 - x0, y1 - y0, rot, -0.1, 1.65)
}

// ── classificação ───────────────────────────────────────────────────────────

fn finger_extended(h: &Hand, mcp: usize, pip: usize, tip: usize) -> bool {
    if let Some(p) = &h.world {
        let delta = |a: usize, b: usize| [p[b][0]-p[a][0], p[b][1]-p[a][1], p[b][2]-p[a][2]];
        let cosine = |a: [f32; 3], b: [f32; 3]| {
            let norm = |v: [f32; 3]| v.iter().map(|x| x*x).sum::<f32>().sqrt();
            a.iter().zip(b).map(|(x,y)| x*y).sum::<f32>() / (norm(a)*norm(b)).max(1e-9)
        };
        return cosine(delta(mcp,pip), delta(pip,tip-1)) > 0.65
            && cosine(delta(pip,tip-1), delta(tip-1,tip)) > 0.65;
    }
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
        [true, true, true, true] => Some(Gesture::OpenPalm),
        [false, false, false, false] if thumb && up => Some(Gesture::ThumbsUp),
        [false, false, false, false] if !thumb => Some(Gesture::Fist),
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
    since: Option<Instant>,
    neutral_since: Option<Instant>,
}

impl Stabilizer {
    pub fn push(&mut self, g: Option<Gesture>, now: Instant) -> Option<Gesture> {
        if g != self.candidate {
            self.candidate = g;
            self.count = 0;
            self.since = Some(now);
        }
        if g.is_none() {
            let neutral = self.neutral_since.get_or_insert(now);
            if now.saturating_duration_since(*neutral) >= Duration::from_millis(200) { self.fired = None; }
            return None;
        }
        self.neutral_since = None;
        let g = g?;
        self.count += 1;
        let cooled = self.last_fire.is_none_or(|t| now.duration_since(t) >= GESTURE_COOLDOWN);
        let stable = self.since.is_some_and(|t| now.saturating_duration_since(t) >= Duration::from_millis(180));
        if self.count >= STABLE_FRAMES && stable && self.fired != Some(g) && cooled {
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
        let &(t0, x0, y0) = self.track.front()?;
        let (dx, dy) = (x - x0, y - y0);
        let path: f32 = self.track.iter().zip(self.track.iter().skip(1)).map(|(a,b)| (b.1-a.1).abs()).sum();
        if now.saturating_duration_since(t0) >= Duration::from_millis(150) && dx.abs() > SWIPE_DX && dy.abs() < SWIPE_DX * 0.6 && dx.abs() >= path * 0.8 {
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

/// Movimento em todo o quadro, comparando blocos de uma miniatura 80×60 em CPU.
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
        let moved = self.previous.len() != small.len() || (0..gh).step_by(10).any(|by| {
            (0..gw).step_by(10).any(|bx| {
                let mut sum = 0u32;
                for y in by..(by+10).min(gh) { for x in bx..(bx+10).min(gw) {
                    sum += small[y*gw+x].abs_diff(self.previous[y*gw+x]) as u32;
                }}
                sum as f32 / 100.0 > MOTION_THRESHOLD
            })
        });
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

/// Os dois modelos carregados.
pub struct HandModels {
    palm: ort::session::Session,
    landmarks: ort::session::Session,
    pub accel: Accel,
}

impl HandModels {
    pub fn load(dir: &Path) -> Result<Self, GestureError> {
        let (palm, a1) = crate::vision_runtime::session(&dir.join(PALM_MODEL)).map_err(GestureError::Runtime)?;
        let (landmarks, a2) = crate::vision_runtime::session(&dir.join(LANDMARK_MODEL)).map_err(GestureError::Runtime)?;
        Ok(Self { palm, landmarks, accel: if a1 == a2 { a1 } else { Accel::Hybrid } })
    }

    fn cpu_fallback(&mut self) -> Result<(), GestureError> {
        let dir = runtime_dir().ok_or_else(|| GestureError::Runtime("pasta do motor de visão indisponível".into()))?;
        self.palm = crate::vision_runtime::cpu_session(&dir.join(PALM_MODEL)).map_err(GestureError::Runtime)?;
        self.landmarks = crate::vision_runtime::cpu_session(&dir.join(LANDMARK_MODEL)).map_err(GestureError::Runtime)?;
        self.accel = Accel::Cpu;
        Ok(())
    }

    pub fn detect_palms(&mut self, img: &RgbImage) -> Result<Vec<Palm>, GestureError> {
        let result = self.detect_palms_inner(img);
        if result.is_err() && self.accel != Accel::Cpu { self.cpu_fallback()?; return self.detect_palms_inner(img); }
        result
    }

    fn detect_palms_inner(&mut self, img: &RgbImage) -> Result<Vec<Palm>, GestureError> {
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
        let result = self.landmarks_inner(img, roi);
        if result.is_err() && self.accel != Accel::Cpu { self.cpu_fallback()?; return self.landmarks_inner(img,roi); }
        result
    }

    fn landmarks_inner(&mut self, img: &RgbImage, roi: &Roi) -> Result<Option<Hand>, GestureError> {
        let input = crop_tensor(img, roi, HAND_SIZE);
        let err = |e: ort::Error| GestureError::Runtime(e.to_string());
        let tensor = ort::value::Tensor::from_array(([1usize, HAND_SIZE as usize, HAND_SIZE as usize, 3], input)).map_err(err)?;
        let out = self.landmarks.run(ort::inputs![tensor]).map_err(err)?;
        // saídas: pontos na tela (63), confiança (1), mão (1), pontos no mundo (63) — nessa ordem
        let screen = out.get("Identity").ok_or_else(|| GestureError::Runtime("pontos de mão ausentes".into()))?.try_extract_tensor::<f32>().map_err(err)?.1;
        let confidence = out.get("Identity_1").ok_or_else(|| GestureError::Runtime("confiança da mão ausente".into()))?.try_extract_tensor::<f32>().map_err(err)?.1;
        let world_values = out.get("Identity_3").ok_or_else(|| GestureError::Runtime("pontos 3D da mão ausentes".into()))?.try_extract_tensor::<f32>().map_err(err)?.1;
        let Some(&score) = confidence.first() else { return Ok(None) };
        if !score.is_finite() || !(HAND_SCORE..=1.0).contains(&score) || screen.len() != 63 || screen.iter().any(|v| !v.is_finite()) {
            return Ok(None);
        }
        if screen.chunks(3).filter(|p| p[0] < -22.4 || p[0] > 246.4 || p[1] < -22.4 || p[1] > 246.4).count() > 2 { return Ok(None); }
        let mut points = [[0.0f32; 2]; 21];
        for (i, p) in points.iter_mut().enumerate() {
            *p = roi.to_frame(screen[i * 3] / HAND_SIZE as f32, screen[i * 3 + 1] / HAND_SIZE as f32);
        }
        let world = if world_values.len() == 63 && world_values.iter().all(|v| v.is_finite()) {
            let mut world = [[0.0; 3]; 21];
            for (i,p) in world.iter_mut().enumerate() { p.copy_from_slice(&world_values[i*3..i*3+3]); }
            Some(world)
        } else { None };
        let hand = Hand { points, score, world };
        if hand.size() < 8.0 { return Ok(None); }
        Ok(Some(hand))
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
    last_detection: Option<Instant>,
}

impl GestureEngine {
    pub fn new(models: HandModels) -> Self {
        Self { models, gate: MotionGate::default(), stabilizer: Stabilizer::default(), swipe: SwipeDetector::default(), tracked: None, last_detection: None }
    }

    pub fn accel(&self) -> Accel {
        self.models.accel
    }

    pub fn process(&mut self, img: &RgbImage, luma: &[u8], now: Instant) -> Result<FrameResult, GestureError> {
        let (w, h) = (img.width() as usize, img.height() as usize);
        let discovery = self.last_detection.is_none_or(|t| now.saturating_duration_since(t) >= Duration::from_millis(700));
        let awake = self.gate.push(luma, w, h, now) || self.tracked.is_some() || discovery;
        if !awake {
            self.stabilizer.push(None, now);
            self.swipe.reset();
            return Ok(FrameResult::default());
        }
        let started = Instant::now();
        let roi = if discovery || self.tracked.is_none() {
            self.last_detection = Some(now);
            let palms = self.models.detect_palms(img)?;
            let selected = if let Some(previous) = self.tracked {
                palms.iter().min_by(|a,b| {
                    let distance = |p: &Palm| ((p.bbox[0]+p.bbox[2])*0.5-previous.cx).hypot((p.bbox[1]+p.bbox[3])*0.5-previous.cy);
                    distance(a).total_cmp(&distance(b))
                })
            } else { palms.first() };
            selected.map(roi_from_palm).or(self.tracked)
        } else { self.tracked };
        let mut hand = match roi {
            Some(r) => self.models.landmarks(img, &r)?,
            None => None,
        };
        if hand.is_none() && self.tracked.is_some() && !discovery {
            self.last_detection = Some(now);
            if let Some(palm) = self.models.detect_palms(img)?.first() {
                hand = self.models.landmarks(img, &roi_from_palm(palm))?;
            }
        }
        let infer_ms = started.elapsed().as_secs_f32() * 1000.0;
        let Some(hand) = hand else {
            self.tracked = None;
            self.stabilizer.push(None, now);
            self.swipe.reset();
            return Ok(FrameResult { awake: true, infer_ms, ..Default::default() });
        };
        if let Some(old) = self.tracked {
            let fresh = roi_from_hand(&hand);
            if (fresh.cx-old.cx).hypot(fresh.cy-old.cy) > old.size * 0.6 {
                self.stabilizer.push(None, now);
                self.swipe.reset();
            }
        }
        self.tracked = Some(roi_from_hand(&hand));
        self.gate.keep_awake(now);
        // centro da palma em coordenadas espelhadas (como a pessoa se vê na tela)
        let c = hand.points[9];
        let swipe = self.swipe.push(1.0 - c[0] / w as f32, c[1] / h as f32, now);
        let pose = self.stabilizer.push(classify(&hand), now);
        Ok(FrameResult { gesture: swipe.or(pose), hand: Some(hand), awake: true, infer_ms })
    }
}
