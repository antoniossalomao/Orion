//! Face ID local: entrar e confirmar pedidos que exigem senha olhando para a câmera (REGRAS.md: o
//! Face ID é opt-in, só vale na janela nativa e a senha continua sempre como alternativa).
//!
//! Cadastros novos usam YuNet, cinco pontos alinhados e SFace, com DirectML/CoreML ou CPU, sem
//! rede. Perfis antigos mantêm a leitura LBPH. O modelo fica em `faceid.bin`, cifrado com a
//! chave do rosto no cofre do sistema: precisa existir antes do banco cifrado abrir, então não pode
//! morar dentro dele. A senha que o Face ID libera fica no cofre do sistema (`ORION_FACEID_LOGIN`),
//! como no Windows Hello.
//!
//! O que isto **não** é: prova de vida. Uma câmera comum não distingue sempre o rosto de uma boa foto
//! dele. Por isso: três quadros seguidos precisam bater, a imagem precisa variar entre eles (quadro
//! congelado de câmera virtual não passa), cinco sessões falhas travam o Face ID até a próxima
//! entrada com senha, e aprovar ferramenta do agente continua sendo só por botão.

use crate::camera::FrameSource;
use crate::rosto::{cosseno, decrypt_bytes_with, encrypt_bytes_with, CofreRosto, RostoError};
use crate::rosto_lbph::{analyze_frame, measure_light, recortar_e_redimensionar, Analysis, RetanguloFace, PATCH_SIDE, UNIFORM_DIM};
use parking_lot::Mutex;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

pub const FILE: &str = "faceid.bin";
/// Credencial `{user, password}` liberada depois de o rosto bater (mesmo formato do Hello).
/// Chave própria do modelo do Face ID no cofre do sistema: apagar o rosto de presença
/// (`ORION_FACE_KEY`) não pode invalidar o Face ID.
pub const KEY: &str = "ORION_FACEID_KEY";
pub const CREDENTIAL: &str = "ORION_FACEID_LOGIN";
/// Quadros aceitos no cadastro.
pub const SAMPLES: usize = 40;
/// Aguarda a exposição e o foco automáticos antes de aceitar qualquer amostra.
pub const ENROLL_WARMUP: Duration = Duration::from_secs(2);
/// Amostras espaçadas evitam preencher o cadastro com uma única pose em poucos instantes.
pub const ENROLL_INTERVAL: Duration = Duration::from_millis(500);
pub const ENROLL_MIN_DURATION: Duration = Duration::from_secs(25);
pub const ENROLL_DEADLINE: Duration = Duration::from_secs(60);
/// Intervalo entre verificações: o suficiente para parecer instantâneo sem ocupar a CPU.
pub const INTERVAL: Duration = Duration::from_millis(100);
/// Tempo máximo de uma leitura antes de cair para a senha.
pub const DEADLINE: Duration = Duration::from_secs(7);
/// Quadros seguidos que precisam bater.
pub const REQUIRED_MATCHES: u32 = 3;
/// Quadros com rosto que não bate até desistir (~2 s).
pub const MAX_MISMATCHES: u32 = 20;
/// Quadros escuros seguidos até desistir por falta de luz (~2 s).
pub const MAX_DARK_FRAMES: u32 = 20;
/// Sessões falhas seguidas que travam o Face ID até entrar com a senha.
pub const MAX_FAILURES: u32 = 5;
/// Limites do limiar adaptativo (cosseno sobre LBPH uniforme com raiz de Hellinger).
pub const THRESHOLD_MIN: f64 = 0.86;
pub const THRESHOLD_MAX: f64 = 0.985;
/// Validade de um tíquete de aprovação.
pub const TICKET_TTL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FaceIdError {
    #[error("Face ID não está ativado")]
    Inactive,
    #[error("Face ID travado depois de várias tentativas; entre com a senha")]
    Locked,
    #[error("câmera indisponível: {0}")]
    NoCamera(String),
    #[error("pouca luz para ler o rosto")]
    LowLight,
    #[error("nenhum rosto encontrado")]
    NoFace,
    #[error("não foi possível obter um cadastro consistente; mantenha o rosto iluminado e estável e tente novamente")]
    PoorEnrollment,
    #[error("{0}")]
    Vault(String),
}

impl From<RostoError> for FaceIdError {
    fn from(e: RostoError) -> Self {
        FaceIdError::Vault(e.0)
    }
}

/// O modelo do rosto: vetor médio das amostras e o limiar calibrado no próprio cadastro.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Model {
    pub version: u32,
    pub user: String,
    pub mean: Vec<f32>,
    pub threshold: f64,
    pub samples: usize,
    #[serde(default)]
    pub templates: Vec<Vec<f32>>,
}

impl Model {
    pub fn score(&self, vector: &[f64]) -> f64 {
        let mean: Vec<f64> = self.mean.iter().map(|&x| x as f64).collect();
        let score = cosseno(&mean, vector);
        if self.version == 3 && !self.templates.is_empty() {
            let best = self.templates.iter().map(|v| cosseno(&v.iter().map(|&x| x as f64).collect::<Vec<_>>(), vector)).fold(-1.0f64, f64::max);
            if score < crate::face_neural::MIN_SIMILARITY { return score; }
            return 0.65 * best + 0.35 * score;
        }
        score
    }
}

/// Calibra cada amostra contra a média das outras, sem a própria amostra inflar a semelhança.
/// Cadastro inconsistente é recusado antes de gravar (REGRAS.md: o Face ID é opt-in, só vale na
/// janela nativa e a senha continua sempre como alternativa).
pub fn model_from_samples(user: &str, samples: &[Vec<f64>]) -> Result<Model, FaceIdError> {
    let neural = samples.first().is_some_and(|v| v.len() == crate::face_neural::DIM);
    let (min, max) = if neural { (crate::face_neural::MIN_SIMILARITY, crate::face_neural::MAX_SIMILARITY) } else { (THRESHOLD_MIN, THRESHOLD_MAX) };
    if samples.len() < SAMPLES || (neural && samples.len() != SAMPLES) || samples.iter().any(|a| !valid_vector(a, neural)) {
        return Err(FaceIdError::PoorEnrollment);
    }
    let mean = crate::rosto::media(samples)?;
    let mut sum = vec![0.0; mean.len()];
    for sample in samples {
        let norm = sample.iter().map(|x| x * x).sum::<f64>().sqrt();
        for (total, value) in sum.iter_mut().zip(sample) {
            *total += value / norm;
        }
    }
    let mut similarities: Vec<f64> = samples.iter().map(|a| {
        let norm = a.iter().map(|x| x * x).sum::<f64>().sqrt();
        let others: Vec<f64> = sum.iter().zip(a).map(|(total, value)| total - value / norm).collect();
        cosseno(&others, a)
    }).collect();
    similarities.sort_by(|a, b| a.total_cmp(b));
    if similarities[0] < min {
        return Err(FaceIdError::PoorEnrollment);
    }
    // percentil 25: uma amostra ruim não derruba o limiar de todo mundo
    let p25 = similarities[similarities.len() / 4];
    let threshold = if neural { (p25 - 0.25).clamp(min, max) } else { (p25 - (1.0 - p25) * 0.6).clamp(min, max) };
    let templates = if neural {
        samples.chunks(SAMPLES / 5).map(|pose| crate::rosto::media(pose).map(|v| v.into_iter().map(|x| x as f32).collect())).collect::<Result<Vec<_>, _>>()?
    } else { vec![] };
    Ok(Model { version: if neural { 3 } else { 2 }, user: user.to_string(), mean: mean.iter().map(|&x| x as f32).collect(), threshold, samples: samples.len(), templates })
}

fn valid_vector(vector: &[f64], neural: bool) -> bool {
    let norm_squared = vector.iter().map(|x| x * x).sum::<f64>();
    vector.len() == (if neural { crate::face_neural::DIM } else { UNIFORM_DIM })
        && vector.iter().all(|x| x.is_finite() && (neural || *x >= 0.0))
        && norm_squared.is_finite() && norm_squared > 1e-9
}

pub fn path(data_dir: &Path) -> PathBuf {
    data_dir.join(FILE)
}

/// Barato o bastante para a tela de entrada consultar a cada desenho.
pub fn is_enabled(data_dir: &Path) -> bool {
    path(data_dir).is_file()
}

pub fn save(data_dir: &Path, vault: &dyn CofreRosto, model: &Model) -> Result<(), FaceIdError> {
    let json = zeroize::Zeroizing::new(serde_json::to_vec(model).map_err(|e| FaceIdError::Vault(e.to_string()))?);
    let blob = encrypt_bytes_with(vault, KEY, &json)?;
    let tmp = data_dir.join(format!("{FILE}.tmp"));
    std::fs::write(&tmp, blob).and_then(|_| std::fs::rename(&tmp, path(data_dir))).map_err(|e| FaceIdError::Vault(format!("não foi possível gravar o Face ID: {e}")))
}

pub fn load(data_dir: &Path, vault: &dyn CofreRosto) -> Result<Model, FaceIdError> {
    let blob = std::fs::read(path(data_dir)).map_err(|_| FaceIdError::Inactive)?;
    let json = zeroize::Zeroizing::new(decrypt_bytes_with(vault, KEY, &blob)?);
    let m: Model = serde_json::from_slice(&json).map_err(|_| FaceIdError::Vault("modelo do Face ID corrompido; cadastre de novo".into()))?;
    let mean: Vec<f64> = m.mean.iter().map(|&x| x as f64).collect();
    let neural = m.version == 3;
    let (min, max) = if neural { (crate::face_neural::MIN_SIMILARITY, crate::face_neural::MAX_SIMILARITY) } else { (THRESHOLD_MIN, THRESHOLD_MAX) };
    if !(1..=3).contains(&m.version) || !valid_vector(&mean, neural) || !m.threshold.is_finite()
        || !(min..=max).contains(&m.threshold)
        || (neural && (m.samples < SAMPLES || m.templates.len() != 5 || m.templates.iter().any(|t| !valid_vector(&t.iter().map(|&v| v as f64).collect::<Vec<_>>(), true))))
    {
        return Err(FaceIdError::Vault("modelo do Face ID de outra versão; cadastre de novo".into()));
    }
    Ok(m)
}

/// Desliga o Face ID: apaga o modelo e a credencial. A chave do rosto fica (a presença usa).
pub fn remove(data_dir: &Path, vault: &dyn CofreRosto) {
    let _ = std::fs::remove_file(path(data_dir));
    vault.del(CREDENTIAL);
    vault.del(KEY);
    reset_failures();
}

// ── trava por tentativas ────────────────────────────────────────────────────

static FAILURES: AtomicU32 = AtomicU32::new(0);

pub fn is_locked() -> bool {
    FAILURES.load(Ordering::SeqCst) >= MAX_FAILURES
}
pub fn record_failure() {
    FAILURES.fetch_add(1, Ordering::SeqCst);
}
/// Entrar com a senha (ou o rosto bater) destrava.
pub fn reset_failures() {
    FAILURES.store(0, Ordering::SeqCst);
}

// ── verificação ─────────────────────────────────────────────────────────────

/// Dica para a interface enquanto procura.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hint {
    Starting,
    NoFace,
    LowLight,
    Reading,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    NoCamera,
    ModelUnavailable,
    LowLight,
    NotRecognized,
    NoFace,
    Frozen,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    Searching(Hint),
    Success { score: f64 },
    Failed(Failure),
}

/// Máquina de estados de uma leitura. Pura (o tempo entra por parâmetro) para ser testável.
pub struct Verifier {
    model: Model,
    matches: u32,
    mismatches: u32,
    dark_frames: u32,
    saw_face: bool,
    saw_light: bool,
    varied: bool,
    previous: Option<(RetanguloFace, Vec<f64>)>,
    hint: Hint,
    last_score: Option<f64>,
}

impl Verifier {
    pub fn new(model: Model) -> Self {
        Self { model, matches: 0, mismatches: 0, dark_frames: 0, saw_face: false, saw_light: false, varied: false, previous: None, hint: Hint::Starting, last_score: None }
    }

    /// Confiança exibida no HUD (0..=1), relativa ao limiar do modelo: bater no limiar vale
    /// 95%, acima dele sobe até 99,9%. Não é probabilidade; é só a régua visual da semelhança.
    pub fn confidence(&self) -> Option<f64> {
        let (score, t) = (self.last_score?, self.model.threshold);
        let floor = if self.model.version == 3 { 0.0 } else { 0.6 };
        Some(if score >= t { 0.95 + 0.049 * ((score - t) / (1.0 - t).max(1e-6)).min(1.0) } else { 0.95 * ((score - floor) / (t - floor).max(1e-6)).clamp(0.0, 1.0) })
    }

    pub fn hint(&self) -> Hint {
        self.hint
    }

    /// Prazo esgotado sem decisão.
    pub fn expire(&self) -> Step {
        Step::Failed(if !self.saw_light && self.dark_frames > 0 {
            Failure::LowLight
        } else if self.matches >= REQUIRED_MATCHES && !self.varied {
            Failure::Frozen
        } else if self.saw_face {
            Failure::NotRecognized
        } else {
            Failure::NoFace
        })
    }

    pub fn evaluate(&mut self, analysis: &Analysis, elapsed: Duration) -> Step {
        self.evaluate_frame(analysis, elapsed, None)
    }

    fn evaluate_frame(&mut self, analysis: &Analysis, elapsed: Duration, changed: Option<bool>) -> Step {
        if elapsed >= DEADLINE {
            return self.expire();
        }
        if !analysis.light.is_enough() {
            self.dark_frames += 1;
            self.matches = 0;
            self.varied = false;
            self.previous = None;
            self.last_score = None;
            self.hint = Hint::LowLight;
            return if self.dark_frames >= MAX_DARK_FRAMES { Step::Failed(Failure::LowLight) } else { Step::Searching(Hint::LowLight) };
        }
        self.dark_frames = 0;
        self.saw_light = true;
        let Some((rect, vector)) = &analysis.face else {
            self.matches = 0;
            self.varied = false;
            self.previous = None;
            self.last_score = None;
            self.hint = Hint::NoFace;
            return Step::Searching(Hint::NoFace);
        };
        self.saw_face = true;
        self.hint = Hint::Reading;
        let score = self.model.score(vector);
        self.last_score = Some(score);
        if let Some((r0, v0)) = &self.previous {
            // micro-movimento natural: o recorte ou a textura mudam um pouco entre quadros
            if changed.unwrap_or_else(|| r0 != rect || cosseno(v0, vector) < 0.99995) {
                self.varied = true;
            }
        }
        self.previous = Some((*rect, vector.clone()));
        if score >= self.model.threshold {
            self.matches += 1;
            self.mismatches = 0;
            if self.matches >= REQUIRED_MATCHES && self.varied {
                return Step::Success { score };
            }
        } else {
            self.matches = 0;
            self.varied = false;
            self.previous = None;
            self.mismatches += 1;
            if self.mismatches >= MAX_MISMATCHES {
                return Step::Failed(Failure::NotRecognized);
            }
        }
        Step::Searching(Hint::Reading)
    }
}

/// Uma leitura em andamento: a fonte de quadros (webcam) e o verificador. Cada `passo` analisa só
/// o quadro mais novo; quem chama decide o ritmo (a tela de entrada chama a cada `INTERVAL`).
pub struct Session {
    source: Box<dyn FrameSource>,
    verifier: Verifier,
    started: Instant,
    seen: u64,
    last: Option<(std::sync::Arc<crate::camera::GrayFrame>, Option<RetanguloFace>)>,
    pub purpose: String,
    pub user: String,
    engine: Option<std::sync::Arc<Mutex<crate::face_neural::FaceEngine>>>,
    landmarks: Option<crate::face_neural::Face>,
    infer_ms: f32,
    fingerprint: Option<[u8; 32]>,
}

impl Session {
    pub fn new(source: Box<dyn FrameSource>, model: Model, purpose: &str) -> Result<Self, FaceIdError> {
        let user = model.user.clone();
        let engine = if model.version == 3 { Some(crate::face_neural::shared().map_err(FaceIdError::Vault)?) } else { None };
        Ok(Self { source, verifier: Verifier::new(model), started: Instant::now(), seen: 0, last: None, purpose: purpose.into(), user, engine, landmarks: None, infer_ms: 0.0, fingerprint: None })
    }

    /// Chamada bloqueante e curta (alguns ms): rode fora da thread da interface.
    pub fn step(&mut self) -> Step {
        if self.source.error().is_some() {
            return Step::Failed(Failure::NoCamera);
        }
        let elapsed = self.started.elapsed();
        let Some(q) = self.source.latest(self.seen) else {
            return if elapsed >= DEADLINE { self.verifier.expire() } else { Step::Searching(self.verifier.hint()) };
        };
        self.seen = q.seq;
        let started = Instant::now();
        let analysis = if let Some(engine) = &self.engine {
            match engine.lock().analyze(&q) {
                Ok((a, face)) => { self.landmarks = face; a }
                Err(e) => { tracing::warn!("Face ID: {e}"); return Step::Failed(Failure::ModelUnavailable); }
            }
        } else { analyze_frame(&q.luma, q.w as usize, q.h as usize) };
        self.infer_ms = started.elapsed().as_secs_f32() * 1000.0;
        let fingerprint = frame_fingerprint(&q);
        let changed = self.fingerprint.is_some_and(|old| old != fingerprint);
        self.fingerprint = Some(fingerprint);
        self.last = Some((q, analysis.face.as_ref().map(|(r, _)| *r)));
        self.verifier.evaluate_frame(&analysis, self.started.elapsed(), Some(changed))
    }

    /// Prévia do último quadro analisado para o HUD (só em memória, nunca em disco).
    pub fn preview(&self) -> Option<Preview> {
        let (frame, face) = self.last.as_ref()?;
        let mut preview = Preview::new(frame, *face, self.verifier.confidence());
        if let Some(engine) = &self.engine {
            preview.neural(frame, self.landmarks.as_ref(), engine.lock().accel, self.infer_ms);
        }
        Some(preview)
    }
}

/// Compara a imagem de origem, sem confundir a invariância do SFace com câmera congelada
/// (REGRAS.md: o Face ID é opt-in; a senha continua sempre como alternativa).
fn frame_fingerprint(frame: &crate::camera::GrayFrame) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(frame.w.to_le_bytes());
    digest.update(frame.h.to_le_bytes());
    digest.update(&frame.luma);
    digest.finalize().into()
}

/// Largura da miniatura do HUD.
pub const PREVIEW_W: u32 = 320;

/// Miniatura espelhada (como um espelho, que é o que a pessoa espera ver), escurecida, em RGBA,
/// com o rosto em coordenadas normalizadas (0..1). Vive só na memória da janela.
#[derive(Debug, Clone)]
pub struct Preview {
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
    /// x, y, largura, altura normalizados, já espelhados.
    pub face: Option<[f32; 4]>,
    pub confidence: Option<f64>,
    pub enrollment: Option<EnrollmentProgress>,
    pub landmarks: Option<Vec<[f32; 2]>>,
    pub accel: Option<String>,
    pub infer_ms: Option<f32>,
}

impl Preview {
    pub fn new(frame: &crate::camera::GrayFrame, face: Option<RetanguloFace>, confidence: Option<f64>) -> Self {
        let (fw, fh) = (frame.w.max(1), frame.h.max(1));
        let w = PREVIEW_W.min(fw);
        let h = ((fh as u64 * w as u64) / fw as u64).max(1) as u32;
        let mut rgba = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            let sy = (y as u64 * fh as u64 / h as u64) as usize;
            for x in 0..w {
                // espelho: a coluna da direita do sensor vira a da esquerda
                let sx = ((w - 1 - x) as u64 * fw as u64 / w as u64) as usize;
                let v = frame.luma[sy * fw as usize + sx] as u32;
                // cinza escurecido com leve tom frio do tema (o HUD acende por cima)
                let g = (v * 62 / 100) as u8;
                rgba.extend_from_slice(&[g, g.saturating_add(2), g.saturating_add(8), 255]);
            }
        }
        let face = face.map(|r| {
            let (x, y, rw, rh) = (r.x as f32 / fw as f32, r.y as f32 / fh as f32, r.w as f32 / fw as f32, r.h as f32 / fh as f32);
            [1.0 - x - rw, y, rw, rh]
        });
        Self { w, h, rgba, face, confidence, enrollment: None, landmarks: None, accel: None, infer_ms: None }
    }

    fn neural(&mut self, frame: &crate::camera::GrayFrame, face: Option<&crate::face_neural::Face>, accel: crate::vision_runtime::Accel, ms: f32) {
        self.landmarks = face.map(|f| f.points.iter().map(|p| [1.0 - p[0] / frame.w.max(1) as f32, p[1] / frame.h.max(1) as f32]).collect());
        self.accel = Some(accel.label().into());
        self.infer_ms = Some(ms);
    }
}

#[derive(Debug, Clone, Copy)]
pub enum EnrollmentHint {
    WarmingUp,
    NoFace,
    Lighting,
    MoveCloser,
    CenterFace,
    HoldStill,
    VaryPose,
    Capturing,
    Checking,
    Front,
    TurnLeft,
    TurnRight,
    LookUp,
    LookDown,
}

#[derive(Debug, Clone, Copy)]
pub struct EnrollmentProgress {
    pub accepted: usize,
    pub total: usize,
    pub hint: EnrollmentHint,
}

/// Confere o rosto original antes da equalização: o fundo claro não pode esconder um rosto
/// escuro, estourado ou borrado. Não guarda o recorte (REGRAS.md: rosto é local).
fn enrollment_quality(frame: &crate::camera::GrayFrame, rect: RetanguloFace) -> Result<(), EnrollmentHint> {
    let (w, h) = (frame.w as usize, frame.h as usize);
    if rect.w < 96 || rect.h < 96 {
        return Err(EnrollmentHint::MoveCloser);
    }
    let margin = w.min(h) / 40;
    if rect.x < margin || rect.y < margin || rect.x + rect.w + margin > w || rect.y + rect.h + margin > h {
        return Err(EnrollmentHint::CenterFace);
    }
    let patch = recortar_e_redimensionar(&frame.luma, w, h, &rect, PATCH_SIDE, PATCH_SIDE);
    let clipped = patch.iter().filter(|&&v| v <= 8 || v >= 247).count();
    if !measure_light(&patch).is_enough() || clipped * 4 > patch.len() {
        return Err(EnrollmentHint::Lighting);
    }
    let mut sum = 0.0;
    let mut squares = 0.0;
    for y in 1..PATCH_SIDE - 1 {
        for x in 1..PATCH_SIDE - 1 {
            let i = y * PATCH_SIDE + x;
            let laplacian = 4.0 * patch[i] as f64 - patch[i - 1] as f64 - patch[i + 1] as f64
                - patch[i - PATCH_SIDE] as f64 - patch[i + PATCH_SIDE] as f64;
            sum += laplacian;
            squares += laplacian * laplacian;
        }
    }
    let n = ((PATCH_SIDE - 2) * (PATCH_SIDE - 2)) as f64;
    if squares / n - (sum / n).powi(2) < 35.0 {
        return Err(EnrollmentHint::HoldStill);
    }
    Ok(())
}

fn stable_rect(a: RetanguloFace, b: RetanguloFace) -> bool {
    let width = (a.x + a.w).min(b.x + b.w).saturating_sub(a.x.max(b.x));
    let height = (a.y + a.h).min(b.y + b.h).saturating_sub(a.y.max(b.y));
    let intersection = (width * height) as f64;
    let union = (a.w * a.h + b.w * b.h) as f64 - intersection;
    union > 0.0 && intersection / union >= 0.75
}

fn enrollment_pose(count: usize, pose: [f32; 2], baseline: [f32; 2]) -> (bool, EnrollmentHint) {
    let (yaw, pitch) = (pose[0] - baseline[0], pose[1] - baseline[1]);
    match (count / (SAMPLES / 5)).min(4) {
        0 => (pose[0].abs() < 0.18 && (0.2..0.9).contains(&pose[1]), EnrollmentHint::Front),
        1 => (yaw > 0.10 && yaw < 0.45, EnrollmentHint::TurnLeft),
        2 => (yaw < -0.10 && yaw > -0.45, EnrollmentHint::TurnRight),
        3 => (yaw.abs() < 0.18 && pitch < -0.055, EnrollmentHint::LookUp),
        _ => (yaw.abs() < 0.18 && pitch > 0.055, EnrollmentHint::LookDown),
    }
}

/// Cadastro com aquecimento, qualidade e estabilidade, seguido de conferência em quadros novos.
/// Bloqueante; rode em `spawn_blocking` (REGRAS.md: rosto é local).
pub fn enroll(source: &dyn FrameSource, user: &str, deadline: Duration, cancel: &AtomicBool, on_preview: &dyn Fn(Preview)) -> Result<Model, FaceIdError> {
    let engine = crate::face_neural::shared().map_err(FaceIdError::Vault)?;
    let started = Instant::now();
    let (mut samples, mut seen, mut dark_frames, mut no_face) = (Vec::<Vec<f64>>::with_capacity(SAMPLES), 0u64, 0u32, 0u32);
    let mut previous: Option<(RetanguloFace, Vec<f64>)> = None;
    let mut stable_frames = 0;
    let mut last_sample = None;
    let mut model: Option<Model> = None;
    let mut verifier: Option<Verifier> = None;
    let mut checking_started = None;
    let mut saw_face = false;
    let mut baseline = [0.0f32; 2];
    let mut previous_fingerprint = None;
    let mut accepted_fingerprint = None;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(FaceIdError::Vault("cadastro cancelado".into()));
        }
        if let Some(e) = source.error() {
            return Err(FaceIdError::NoCamera(e));
        }
        if started.elapsed() >= deadline {
            return Err(if saw_face { FaceIdError::PoorEnrollment } else if dark_frames > no_face { FaceIdError::LowLight } else { FaceIdError::NoFace });
        }
        if let Some(q) = source.latest(seen) {
            seen = q.seq;
            let infer_started = Instant::now();
            let (a, face, accel) = {
                let mut engine = engine.lock();
                let (a, face) = engine.analyze(&q).map_err(FaceIdError::Vault)?;
                (a, face, engine.accel)
            };
            let infer_ms = infer_started.elapsed().as_secs_f32() * 1000.0;
            let elapsed = started.elapsed();
            if cancel.load(Ordering::Relaxed) { return Err(FaceIdError::Vault("cadastro cancelado".into())); }
            if elapsed >= deadline { return Err(FaceIdError::PoorEnrollment); }
            let fingerprint = frame_fingerprint(&q);
            let changed = previous_fingerprint.is_some_and(|old| old != fingerprint);
            previous_fingerprint = Some(fingerprint);
            let mut hint = EnrollmentHint::Capturing;
            let quality = if !a.light.is_enough() {
                dark_frames += 1;
                Err(EnrollmentHint::Lighting)
            } else if let Some((rect, _)) = &a.face {
                saw_face = true;
                enrollment_quality(&q, *rect)
            } else {
                no_face += 1;
                Err(EnrollmentHint::NoFace)
            };
            if elapsed < ENROLL_WARMUP {
                hint = EnrollmentHint::WarmingUp;
                previous = None;
                stable_frames = 0;
            } else if let Err(reason) = quality {
                hint = reason;
                previous = None;
                stable_frames = 0;
                // Uma interrupção também quebra a sequência da conferência final.
                if model.is_some() {
                    verifier = model.clone().map(Verifier::new);
                }
            } else if let Some((rect, vector)) = &a.face {
                if let Some(checker) = verifier.as_mut() {
                    hint = EnrollmentHint::Checking;
                    match checker.evaluate_frame(&a, elapsed.saturating_sub(checking_started.unwrap_or(elapsed)), Some(changed)) {
                        Step::Success { .. } if elapsed >= ENROLL_MIN_DURATION => return model.ok_or(FaceIdError::PoorEnrollment),
                        Step::Failed(_) => return Err(FaceIdError::PoorEnrollment),
                        _ => {}
                    }
                } else {
                    let pose = face.as_ref().map(|f| f.pose()).unwrap_or([0.0; 2]);
                    let (pose_ok, direction) = enrollment_pose(samples.len(), pose, baseline);
                    let stable = previous.as_ref().is_some_and(|(r, v)| stable_rect(*r, *rect) && cosseno(v, vector) >= crate::face_neural::MIN_SIMILARITY);
                    stable_frames = if stable { stable_frames + 1 } else { 1 };
                    previous = Some((*rect, vector.clone()));
                    if !pose_ok {
                        hint = direction;
                        previous = None;
                        stable_frames = 0;
                    } else if stable_frames < 3 {
                        hint = EnrollmentHint::HoldStill;
                    } else if accepted_fingerprint == Some(fingerprint) {
                        hint = EnrollmentHint::VaryPose;
                    } else if last_sample.is_none_or(|last| elapsed.saturating_sub(last) >= ENROLL_INTERVAL) {
                        if !samples.is_empty() && cosseno(&crate::rosto::media(&samples)?, vector) < crate::face_neural::MIN_SIMILARITY {
                            hint = EnrollmentHint::CenterFace;
                        } else {
                            if samples.len() < SAMPLES / 5 {
                                let count = samples.len() as f32;
                                baseline = [(baseline[0] * count + pose[0]) / (count + 1.0), (baseline[1] * count + pose[1]) / (count + 1.0)];
                            }
                            samples.push(vector.clone());
                            accepted_fingerprint = Some(fingerprint);
                            last_sample = Some(elapsed);
                            if samples.len() % (SAMPLES / 5) == 0 {
                                previous = None;
                                stable_frames = 0;
                            }
                            hint = enrollment_pose(samples.len(), pose, baseline).1;
                            if samples.len() == SAMPLES {
                                let candidate = model_from_samples(user, &samples)?;
                                verifier = Some(Verifier::new(candidate.clone()));
                                model = Some(candidate);
                                checking_started = Some(elapsed);
                                hint = EnrollmentHint::Checking;
                            }
                        }
                    }
                }
            }
            let mut preview = Preview::new(&q, a.face.as_ref().map(|(r, _)| *r), None);
            preview.neural(&q, face.as_ref(), accel, infer_ms);
            preview.enrollment = Some(EnrollmentProgress { accepted: samples.len(), total: SAMPLES, hint });
            on_preview(preview);
        }
        std::thread::sleep(INTERVAL);
    }
}

// ── tíquetes de aprovação ───────────────────────────────────────────────────

/// Prova de curta duração de que o rosto bateu, para um único pedido (por exemplo `autonomo`,
/// `panico`). Só a ligação nativa emite; as rotas consomem uma vez. 256 bits aleatórios.
#[derive(Default)]
pub struct Tickets(Mutex<HashMap<String, (String, Instant)>>);

impl Tickets {
    pub fn issue(&self, purpose: &str) -> String {
        let mut b = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut b);
        let t = hex::encode(b);
        let mut m = self.0.lock();
        m.retain(|_, (_, at)| at.elapsed() < TICKET_TTL);
        m.insert(t.clone(), (purpose.to_string(), Instant::now()));
        t
    }

    /// Consome o tíquete: vale uma vez, só para o propósito emitido e dentro do prazo.
    pub fn consume(&self, ticket: &str, purpose: &str) -> bool {
        if ticket.len() != 64 {
            return false;
        }
        match self.0.lock().remove(ticket) {
            Some((p, at)) => p == purpose && at.elapsed() < TICKET_TTL,
            None => false,
        }
    }
}

pub static TICKETS: LazyLock<Tickets> = LazyLock::new(Default::default);
