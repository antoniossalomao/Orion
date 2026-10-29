//! Face ID local: entrar e confirmar pedidos que exigem senha olhando para a câmera (REGRAS.md: o
//! Face ID é opt-in, só vale na janela nativa e a senha continua sempre como alternativa).
//!
//! Tudo roda em CPU com o pipeline clássico de `rosto_lbph` (luz → Haar → LBPH uniforme), sem rede
//! e sem modelo neural. O modelo do rosto fica em `faceid.bin` na pasta de dados, cifrado com a
//! chave do rosto no cofre do sistema: precisa existir antes do banco cifrado abrir, então não pode
//! morar dentro dele. A senha que o Face ID libera fica no cofre do sistema (`ORION_FACEID_LOGIN`),
//! como no Windows Hello.
//!
//! O que isto **não** é: prova de vida. LBPH em câmera comum não distingue o rosto de uma boa foto
//! dele. Por isso: três quadros seguidos precisam bater, a imagem precisa variar entre eles (quadro
//! congelado de câmera virtual não passa), cinco sessões falhas travam o Face ID até a próxima
//! entrada com senha, e aprovar ferramenta do agente continua sendo só por botão.

use crate::camera::FrameSource;
use crate::rosto::{cosseno, decrypt_bytes_with, encrypt_bytes_with, CofreRosto, RostoError};
use crate::rosto_lbph::{analyze_frame, Analysis, RetanguloFace, UNIFORM_DIM};
use parking_lot::Mutex;
use rand::RngCore;
use serde::{Deserialize, Serialize};
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
pub const SAMPLES: usize = 8;
/// Intervalo entre verificações: o suficiente para parecer instantâneo sem ocupar a CPU.
pub const INTERVAL: Duration = Duration::from_millis(250);
/// Tempo máximo de uma leitura antes de cair para a senha.
pub const DEADLINE: Duration = Duration::from_secs(7);
/// Quadros seguidos que precisam bater.
pub const REQUIRED_MATCHES: u32 = 3;
/// Quadros com rosto que não bate até desistir (~2 s).
pub const MAX_MISMATCHES: u32 = 8;
/// Quadros escuros seguidos até desistir por falta de luz (~2 s).
pub const MAX_DARK_FRAMES: u32 = 8;
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
}

impl Model {
    pub fn score(&self, vector: &[f64]) -> f64 {
        let mean: Vec<f64> = self.mean.iter().map(|&x| x as f64).collect();
        cosseno(&mean, vector)
    }
}

/// Monta o modelo: média das amostras e limiar = semelhança típica entre cada amostra e a média,
/// com folga, dentro de `LIMIAR_MIN..=LIMIAR_MAX`. Quem cadastra com amostras parecidas entre si
/// ganha um limiar mais exigente.
pub fn model_from_samples(user: &str, samples: &[Vec<f64>]) -> Result<Model, FaceIdError> {
    if samples.len() < SAMPLES.min(5) || samples.iter().any(|a| a.len() != UNIFORM_DIM) {
        return Err(FaceIdError::NoFace);
    }
    let mean = crate::rosto::media(samples)?;
    let mut similarities: Vec<f64> = samples.iter().map(|a| cosseno(&mean, a)).collect();
    similarities.sort_by(|a, b| a.total_cmp(b));
    // percentil 25: uma amostra ruim não derruba o limiar de todo mundo
    let p25 = similarities[similarities.len() / 4];
    let threshold = (p25 - (1.0 - p25) * 0.6).clamp(THRESHOLD_MIN, THRESHOLD_MAX);
    Ok(Model { version: 1, user: user.to_string(), mean: mean.iter().map(|&x| x as f32).collect(), threshold, samples: samples.len() })
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
    if m.mean.len() != UNIFORM_DIM {
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
        Some(if score >= t { 0.95 + 0.049 * ((score - t) / (1.0 - t).max(1e-6)).min(1.0) } else { 0.95 * ((score - 0.6) / (t - 0.6)).clamp(0.0, 1.0) })
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
        if elapsed >= DEADLINE {
            return self.expire();
        }
        if !analysis.light.is_enough() {
            self.dark_frames += 1;
            self.matches = 0;
            self.hint = Hint::LowLight;
            return if self.dark_frames >= MAX_DARK_FRAMES { Step::Failed(Failure::LowLight) } else { Step::Searching(Hint::LowLight) };
        }
        self.dark_frames = 0;
        self.saw_light = true;
        let Some((rect, vector)) = &analysis.face else {
            self.matches = 0;
            self.hint = Hint::NoFace;
            return Step::Searching(Hint::NoFace);
        };
        self.saw_face = true;
        self.hint = Hint::Reading;
        let score = self.model.score(vector);
        self.last_score = Some(score);
        if let Some((r0, v0)) = &self.previous {
            // micro-movimento natural: o recorte ou a textura mudam um pouco entre quadros
            if r0 != rect || cosseno(v0, vector) < 0.99995 {
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
            self.mismatches += 1;
            if self.mismatches >= MAX_MISMATCHES {
                return Step::Failed(Failure::NotRecognized);
            }
        }
        Step::Searching(Hint::Reading)
    }
}

/// Uma leitura em andamento: a fonte de quadros (webcam) e o verificador. Cada `passo` analisa só
/// o quadro mais novo; quem chama decide o ritmo (a tela de entrada chama a cada `INTERVALO`).
pub struct Session {
    source: Box<dyn FrameSource>,
    verifier: Verifier,
    started: Instant,
    seen: u64,
    last: Option<(std::sync::Arc<crate::camera::GrayFrame>, Option<RetanguloFace>)>,
    pub purpose: String,
    pub user: String,
}

impl Session {
    pub fn new(source: Box<dyn FrameSource>, model: Model, purpose: &str) -> Self {
        let user = model.user.clone();
        Self { source, verifier: Verifier::new(model), started: Instant::now(), seen: 0, last: None, purpose: purpose.into(), user }
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
        let analysis = analyze_frame(&q.luma, q.w as usize, q.h as usize);
        self.last = Some((q, analysis.face.as_ref().map(|(r, _)| *r)));
        self.verifier.evaluate(&analysis, elapsed)
    }

    /// Prévia do último quadro analisado para o HUD (só em memória, nunca em disco).
    pub fn preview(&self) -> Option<Preview> {
        let (frame, face) = self.last.as_ref()?;
        Some(Preview::new(frame, *face, self.verifier.confidence()))
    }
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
        Self { w, h, rgba, face, confidence }
    }
}

/// Cadastro: junta `AMOSTRAS` quadros com luz e rosto, um a cada `INTERVALO`, até `prazo`.
/// Bloqueante; rode em `spawn_blocking`.
pub fn enroll(source: &dyn FrameSource, user: &str, deadline: Duration, cancel: &AtomicBool, on_preview: &dyn Fn(Preview)) -> Result<Model, FaceIdError> {
    let started = Instant::now();
    let (mut samples, mut seen, mut dark_frames, mut no_face) = (Vec::with_capacity(SAMPLES), 0u64, 0u32, 0u32);
    while samples.len() < SAMPLES {
        if cancel.load(Ordering::Relaxed) {
            return Err(FaceIdError::Vault("cadastro cancelado".into()));
        }
        if let Some(e) = source.error() {
            return Err(FaceIdError::NoCamera(e));
        }
        if started.elapsed() >= deadline {
            return Err(if dark_frames > no_face { FaceIdError::LowLight } else { FaceIdError::NoFace });
        }
        if let Some(q) = source.latest(seen) {
            seen = q.seq;
            let a = analyze_frame(&q.luma, q.w as usize, q.h as usize);
            on_preview(Preview::new(&q, a.face.as_ref().map(|(r, _)| *r), None));
            match (a.light.is_enough(), a.face) {
                (false, _) => dark_frames += 1,
                (true, None) => no_face += 1,
                (true, Some((_, v))) => samples.push(v),
            }
        }
        std::thread::sleep(INTERVAL);
    }
    model_from_samples(user, &samples)
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
