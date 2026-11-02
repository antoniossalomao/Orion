//! YuNet + cinco pontos + alinhamento por similaridade + SFace (REGRAS.md: rosto é local,
//! cadastrado só pelo dono, e nunca sai do computador). Não mede profundidade nem prova vida.

use crate::rosto_lbph::{Analysis, RetanguloFace, measure_light};
use crate::vision_runtime::{self, Accel};
use image::RgbImage;
use parking_lot::Mutex;
use std::sync::Arc;

pub const DETECTOR: &str = "face_detection_yunet_2023mar.onnx";
pub const RECOGNIZER: &str = "face_recognition_sface_2021dec.onnx";
pub const DIM: usize = 128;
const DETECT_SIZE: u32 = 640;
const ALIGN_SIZE: usize = 112;
pub const MIN_SIMILARITY: f64 = 0.55;
/// Faixa mais exigente que 0,363 da referência SFace; precisa de calibração biométrica real.
pub const MAX_SIMILARITY: f64 = 0.65;

#[derive(Debug, Clone)]
pub struct Face {
    pub rect: RetanguloFace,
    pub points: [[f32; 2]; 5],
    pub score: f32,
}

impl Face {
    /// Deslocamentos 2D normalizados pelo espaço entre os olhos; só orientam a captura.
    pub fn pose(&self) -> [f32; 2] {
        let (a, b, n) = (self.points[0], self.points[1], self.points[2]);
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let d = (dx * dx + dy * dy).max(1.0);
        let (nx, ny) = (n[0] - (a[0] + b[0]) * 0.5, n[1] - (a[1] + b[1]) * 0.5);
        [(nx * dx + ny * dy) / d, (ny * dx - nx * dy) / d]
    }
}

pub struct FaceEngine {
    detector: ort::session::Session,
    recognizer: ort::session::Session,
    pub accel: Accel,
}

static ENGINE: Mutex<Option<Arc<Mutex<FaceEngine>>>> = Mutex::new(None);

/// Carga única; um erro não fica memorizado, permitindo instalar os pesos e tentar de novo.
pub fn shared() -> Result<Arc<Mutex<FaceEngine>>, String> {
    let mut engine = ENGINE.lock();
    if let Some(e) = engine.as_ref() { return Ok(e.clone()); }
    let dir = vision_runtime::runtime_dir().ok_or("pasta dos recursos de visão indisponível")?;
    let (detector, a1) = vision_runtime::session(&dir.join(DETECTOR))?;
    let (recognizer, a2) = vision_runtime::session(&dir.join(RECOGNIZER))?;
    let loaded = Arc::new(Mutex::new(FaceEngine { detector, recognizer, accel: if a1 == a2 { a1 } else { Accel::Hybrid } }));
    *engine = Some(loaded.clone());
    Ok(loaded)
}

impl FaceEngine {
    fn cpu_fallback(&mut self) -> Result<(), String> {
        let dir = vision_runtime::runtime_dir().ok_or("pasta do motor de visão indisponível")?;
        self.detector = vision_runtime::cpu_session(&dir.join(DETECTOR))?;
        self.recognizer = vision_runtime::cpu_session(&dir.join(RECOGNIZER))?;
        self.accel = Accel::Cpu;
        Ok(())
    }

    /// Detecção em escala proporcional, sem deformar o rosto e sem incluir padding no resultado.
    pub fn detect(&mut self, img: &RgbImage) -> Result<Vec<Face>, String> {
        let found = self.detect_inner(img);
        if found.is_err() && self.accel != Accel::Cpu {
            tracing::info!("rosto: falha de execução na GPU; tentando CPU");
            self.cpu_fallback()?;
            return self.detect_inner(img);
        }
        found
    }

    fn detect_inner(&mut self, img: &RgbImage) -> Result<Vec<Face>, String> {
        if img.width() == 0 || img.height() == 0 { return Ok(vec![]); }
        let scale = (DETECT_SIZE as f32 / img.width() as f32).min(DETECT_SIZE as f32 / img.height() as f32);
        let (w, h) = (((img.width() as f32 * scale).round() as u32).max(1), ((img.height() as f32 * scale).round() as u32).max(1));
        let small = image::imageops::resize(img, w, h, image::imageops::FilterType::Triangle);
        let mut input = vec![0.0; 3 * DETECT_SIZE as usize * DETECT_SIZE as usize];
        for (x, y, pixel) in small.enumerate_pixels() {
            for c in 0..3 { input[(c * DETECT_SIZE as usize + y as usize) * DETECT_SIZE as usize + x as usize] = pixel[2 - c] as f32; }
        }
        let tensor = ort::value::Tensor::from_array(([1usize, 3, DETECT_SIZE as usize, DETECT_SIZE as usize], input)).map_err(|e| e.to_string())?;
        let out = self.detector.run(ort::inputs![tensor]).map_err(|e| e.to_string())?;
        let values = |name: String| -> Result<&[f32], String> {
            out.get(&name).ok_or_else(|| format!("saída YuNet ausente: {name}"))?
                .try_extract_tensor::<f32>().map(|(_, v)| v).map_err(|e| e.to_string())
        };
        let mut faces = vec![];
        for stride in [8usize, 16, 32] {
            let cls = values(format!("cls_{stride}"))?;
            let obj = values(format!("obj_{stride}"))?;
            let boxes = values(format!("bbox_{stride}"))?;
            let kps = values(format!("kps_{stride}"))?;
            let cols = DETECT_SIZE as usize / stride;
            let count = cols * cols;
            if cls.len() != count || obj.len() != count || boxes.len() != count * 4 || kps.len() != count * 10 {
                return Err("dimensões da saída YuNet inválidas".into());
            }
            for i in 0..count {
                let score = (cls[i].clamp(0.0, 1.0) * obj[i].clamp(0.0, 1.0)).sqrt();
                if !score.is_finite() || score < 0.85 { continue; }
                let (x, y, step) = ((i % cols) as f32, (i / cols) as f32, stride as f32 / scale);
                let (cx, cy) = ((x + boxes[i * 4]) * step, (y + boxes[i * 4 + 1]) * step);
                let (rw, rh) = (boxes[i * 4 + 2].exp() * step, boxes[i * 4 + 3].exp() * step);
                if ![cx, cy, rw, rh].iter().all(|v| v.is_finite()) { continue; }
                let (x0, y0) = ((cx - rw * 0.5).max(0.0), (cy - rh * 0.5).max(0.0));
                let (x1, y1) = ((cx + rw * 0.5).min(img.width() as f32), (cy + rh * 0.5).min(img.height() as f32));
                if x1 - x0 < 32.0 || y1 - y0 < 32.0 { continue; }
                let mut points = [[0.0; 2]; 5];
                for (j, p) in points.iter_mut().enumerate() {
                    *p = [(x + kps[i * 10 + j * 2]) * step, (y + kps[i * 10 + j * 2 + 1]) * step];
                }
                if points.iter().flatten().any(|v| !v.is_finite()) { continue; }
                let eye_distance = (points[1][0] - points[0][0]).hypot(points[1][1] - points[0][1]);
                if eye_distance < rw * 0.15 { continue; }
                faces.push(Face { rect: RetanguloFace { x: x0 as usize, y: y0 as usize, w: (x1 - x0) as usize, h: (y1 - y0) as usize }, points, score });
            }
        }
        faces.sort_by(|a, b| b.score.total_cmp(&a.score));
        let mut kept: Vec<Face> = vec![];
        for face in faces {
            let r = face.rect;
            let bbox = (r.x as f64, r.y as f64, (r.x + r.w) as f64, (r.y + r.h) as f64);
            if kept.iter().all(|f| {
                let q = f.rect;
                crate::visao_local::iou(bbox, (q.x as f64, q.y as f64, (q.x + q.w) as f64, (q.y + q.h) as f64)) < 0.3
            }) { kept.push(face); }
            if kept.len() == 8 { break; }
        }
        Ok(kept)
    }

    pub fn embedding(&mut self, img: &RgbImage, face: &Face) -> Result<Vec<f64>, String> {
        let result = self.embedding_inner(img,face);
        if result.is_err() && self.accel != Accel::Cpu {
            tracing::info!("rosto: falha de execução na GPU; tentando CPU");
            self.cpu_fallback()?;
            return self.embedding_inner(img,face);
        }
        result
    }

    fn embedding_inner(&mut self, img: &RgbImage, face: &Face) -> Result<Vec<f64>, String> {
        let input = aligned_tensor(img, &face.points)?;
        let tensor = ort::value::Tensor::from_array(([1usize, 3, ALIGN_SIZE, ALIGN_SIZE], input)).map_err(|e| e.to_string())?;
        let out = self.recognizer.run(ort::inputs![tensor]).map_err(|e| e.to_string())?;
        let (_, data) = out[0].try_extract_tensor::<f32>().map_err(|e| e.to_string())?;
        if data.len() != DIM || data.iter().any(|v| !v.is_finite()) { return Err("descritor SFace inválido".into()); }
        let norm = data.iter().map(|v| (*v as f64).powi(2)).sum::<f64>().sqrt();
        if norm < 1e-9 { return Err("descritor SFace nulo".into()); }
        Ok(data.iter().map(|v| *v as f64 / norm).collect())
    }

    pub fn analyze(&mut self, frame: &crate::camera::GrayFrame) -> Result<(Analysis, Option<Face>), String> {
        let light = measure_light(&frame.luma);
        if !light.is_enough() { return Ok((Analysis { light, face: None }, None)); }
        let img = frame.rgb.as_ref().ok_or("Face ID neural exige captura colorida")?;
        let faces = self.detect(img)?;
        // Nunca escolhe entre pessoas diferentes durante cadastro ou autenticação.
        if faces.len() != 1 { return Ok((Analysis { light, face: None }, None)); }
        let face = faces.into_iter().next().ok_or("rosto ausente")?;
        let embedding = self.embedding(img, &face)?;
        Ok((Analysis { light, face: Some((face.rect, embedding)) }, Some(face)))
    }
}

/// Ajuste dos cinco pontos para o molde SFace, com recorte bilinear RGB em 112×112.
fn aligned_tensor(img: &RgbImage, points: &[[f32; 2]; 5]) -> Result<Vec<f32>, String> {
    let target = [[38.2946f32, 51.6963], [73.5318, 51.5014], [56.0252, 71.7366], [41.5493, 92.3655], [70.7299, 92.2041]];
    let center = |p: &[[f32; 2]; 5]| p.iter().fold([0.0; 2], |a, b| [a[0] + b[0] / 5.0, a[1] + b[1] / 5.0]);
    let (src, dst) = (center(points), center(&target));
    let (mut a, mut b, mut den) = (0.0f32, 0.0f32, 0.0f32);
    for (s, t) in points.iter().zip(target) {
        let (x, y, u, v) = (s[0] - src[0], s[1] - src[1], t[0] - dst[0], t[1] - dst[1]);
        a += x * u + y * v;
        b += x * v - y * u;
        den += x * x + y * y;
    }
    if den < 1e-6 { return Err("pontos faciais degenerados".into()); }
    let (a, b) = (a / den, b / den);
    let (tx, ty) = (dst[0] - a * src[0] + b * src[1], dst[1] - b * src[0] - a * src[1]);
    let det = a * a + b * b;
    if !det.is_finite() || det < 1e-9 { return Err("alinhamento facial inválido".into()); }
    let px = |x: i32, y: i32, c: usize| -> f32 {
        if x < 0 || y < 0 || x >= img.width() as i32 || y >= img.height() as i32 { 0.0 } else { img.get_pixel(x as u32, y as u32)[c] as f32 }
    };
    let mut out = vec![0.0; 3 * ALIGN_SIZE * ALIGN_SIZE];
    for y in 0..ALIGN_SIZE {
        for x in 0..ALIGN_SIZE {
            let (u, v) = (x as f32 - tx, y as f32 - ty);
            let (sx, sy) = ((a * u + b * v) / det, (-b * u + a * v) / det);
            let (ix, iy) = (sx.floor() as i32, sy.floor() as i32);
            let (fx, fy) = (sx - ix as f32, sy - iy as f32);
            for c in 0..3 {
                let top = px(ix, iy, c) * (1.0 - fx) + px(ix + 1, iy, c) * fx;
                let bottom = px(ix, iy + 1, c) * (1.0 - fx) + px(ix + 1, iy + 1, c) * fx;
                out[(c * ALIGN_SIZE + y) * ALIGN_SIZE + x] = top * (1.0 - fy) + bottom * fy;
            }
        }
    }
    Ok(out)
}
