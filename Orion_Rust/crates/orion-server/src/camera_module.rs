//! Módulo de câmera da janela nativa: vídeo com HUD, rosto e gestos com as mãos (REGRAS.md: a
//! câmera só liga por pedido seu; gesto só abre e navega; nunca aprova nem executa).
//!
//! A câmera liga ao abrir o módulo e desliga ao fechar. Um fio próprio lê ~15 quadros por segundo,
//! roda o detector de movimento e, acordado, o motor de gestos (GPU quando há DirectML/CoreML).
//! A janela só pede o último resultado; nada vai a disco nem sai do computador.

use orion_app::net::CameraSnapshot;
use orion_core::camera::{CameraStream, FrameSource, GrayFrame};
use orion_core::hand_gestures::{self, GestureEngine, HandModels};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// ~15 quadros por segundo: fluido para gesto, leve para a CPU.
const FRAME_INTERVAL: Duration = Duration::from_millis(66);
const PREVIEW_W: u32 = 320;
const MAX_PENDING: usize = 8;

#[derive(Default)]
struct Shared {
    snapshot: Option<CameraSnapshot>,
    gestures: Vec<String>,
}

struct Worker {
    stop: Arc<AtomicBool>,
    shared: Arc<Mutex<Shared>>,
}

static WORKER: Mutex<Option<Worker>> = Mutex::new(None);

pub fn is_open() -> bool {
    WORKER.lock().is_some()
}

/// Liga a câmera e o fio de análise. Bloqueante (abre o dispositivo): rode em `spawn_blocking`.
pub fn open(device: i32) -> Result<(), String> {
    let mut slot = WORKER.lock();
    if slot.is_some() {
        return Ok(());
    }
    let stream = CameraStream::open_with(device, Duration::from_secs(4), FRAME_INTERVAL, true).map_err(|e| e.to_string())?;
    let stop = Arc::new(AtomicBool::new(false));
    let shared = Arc::new(Mutex::new(Shared::default()));
    let (s, sh) = (stop.clone(), shared.clone());
    std::thread::Builder::new()
        .name("orion-camera-module".into())
        .spawn(move || run(stream, s, sh))
        .map_err(|e| format!("não foi possível iniciar o módulo de câmera: {e}"))?;
    tracing::info!(target: "orion.audit", "camera_modulo_aberto");
    *slot = Some(Worker { stop, shared });
    Ok(())
}

/// Desliga: o fio para no próximo quadro e solta a câmera.
pub fn close() {
    if let Some(w) = WORKER.lock().take() {
        w.stop.store(true, Ordering::Relaxed);
        tracing::info!(target: "orion.audit", "camera_modulo_fechado");
    }
}

/// Último quadro e os gestos acumulados desde a leitura anterior.
pub fn poll(cpu: Option<f32>) -> Option<CameraSnapshot> {
    let guard = WORKER.lock();
    let w = guard.as_ref()?;
    let mut sh = w.shared.lock();
    let mut snap = sh.snapshot.clone()?;
    snap.gestures = std::mem::take(&mut sh.gestures);
    snap.cpu = cpu;
    Some(snap)
}

fn load_engine() -> (Option<GestureEngine>, Option<String>) {
    let Some(dir) = hand_gestures::runtime_dir() else { return (None, Some("pasta do motor de gestos indisponível".into())) };
    if !hand_gestures::available(&dir) {
        return (None, Some("Gestos desligados: instale o motor de IA pelo instalador (opção Gestos com as mãos)".into()));
    }
    match HandModels::load(&dir) {
        Ok(models) => (Some(GestureEngine::new(models)), None),
        Err(e) => {
            tracing::info!("gestos indisponíveis: {e}");
            (None, Some(format!("Gestos indisponíveis: {e}")))
        }
    }
}

fn run(stream: CameraStream, stop: Arc<AtomicBool>, shared: Arc<Mutex<Shared>>) {
    // o motor carrega no próprio fio: a janela abre a câmera na hora, os gestos chegam logo depois
    let (mut engine, note) = load_engine();
    let accel = engine.as_ref().map(|e| e.accel().label().to_string());
    let mut seen = 0u64;
    while !stop.load(Ordering::Relaxed) {
        if let Some(err) = stream.error() {
            let mut sh = shared.lock();
            let snap = sh.snapshot.get_or_insert_with(CameraSnapshot::default);
            snap.note = Some(format!("A câmera parou: {err}"));
            break;
        }
        let Some(frame) = stream.latest(seen) else {
            std::thread::sleep(Duration::from_millis(8));
            continue;
        };
        seen = frame.seq;
        let snap = analyze(&frame, engine.as_mut(), accel.clone(), note.clone(), &shared);
        let mut sh = shared.lock();
        sh.snapshot = Some(snap);
    }
    // soltar o fluxo desliga a câmera
    drop(stream);
}

fn analyze(frame: &GrayFrame, engine: Option<&mut GestureEngine>, accel: Option<String>, note: Option<String>, shared: &Arc<Mutex<Shared>>) -> CameraSnapshot {
    let (w, h) = (frame.w as f32, frame.h as f32);
    let face = orion_core::rosto_lbph::analyze_frame(&frame.luma, frame.w as usize, frame.h as usize).face.map(|(r, _)| {
        let (x, y, rw, rh) = (r.x as f32 / w, r.y as f32 / h, r.w as f32 / w, r.h as f32 / h);
        [1.0 - x - rw, y, rw, rh]
    });
    let mut snap = CameraSnapshot { seq: frame.seq, face, accel, note, ..Default::default() };
    if let (Some(engine), Some(rgb)) = (engine, frame.rgb.as_ref()) {
        match engine.process(rgb, &frame.luma, Instant::now()) {
            Ok(r) => {
                snap.awake = r.awake;
                snap.infer_ms = r.awake.then_some(r.infer_ms);
                snap.hand = r.hand.map(|hand| hand.points.iter().map(|p| [1.0 - p[0] / w, p[1] / h]).collect());
                if let Some(g) = r.gesture {
                    tracing::info!(target: "orion.audit", "gesto={}", g.label());
                    let mut sh = shared.lock();
                    sh.gestures.push(g.label().to_string());
                    let excess = sh.gestures.len().saturating_sub(MAX_PENDING);
                    sh.gestures.drain(..excess);
                }
            }
            Err(e) => snap.note = Some(format!("Gestos pausados: {e}")),
        }
    }
    if let Some(rgb) = frame.rgb.as_ref() {
        let (pw, ph) = (PREVIEW_W.min(rgb.width()), ((rgb.height() as u64 * PREVIEW_W.min(rgb.width()) as u64) / rgb.width().max(1) as u64).max(1) as u32);
        let (sw, sh) = (rgb.width(), rgb.height());
        let mut rgba = Vec::with_capacity((pw * ph * 4) as usize);
        for y in 0..ph {
            let sy = (y as u64 * sh as u64 / ph as u64) as u32;
            for x in 0..pw {
                // espelho, cor levemente apagada para o HUD acender por cima
                let sx = ((pw - 1 - x) as u64 * sw as u64 / pw as u64) as u32;
                let p = rgb.get_pixel(sx, sy).0;
                rgba.extend_from_slice(&[(p[0] as u16 * 80 / 100) as u8, (p[1] as u16 * 80 / 100) as u8, (p[2] as u16 * 84 / 100) as u8, 255]);
            }
        }
        (snap.w, snap.h, snap.rgba) = (pw, ph, rgba);
    }
    snap
}
