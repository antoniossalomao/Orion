//! Módulo de câmera da janela nativa: vídeo com HUD, rosto e gestos com as mãos (REGRAS.md: a
//! câmera só liga por pedido seu; gesto só abre e navega; nunca aprova nem executa).
//!
//! A câmera liga ao abrir o módulo e desliga ao fechar. Um fio próprio lê ~15 quadros por segundo,
//! roda o detector de movimento e, acordado, o motor de gestos (GPU quando há DirectML/CoreML).
//! A janela só pede o último resultado; nada vai a disco nem sai do computador.

use orion_app::net::{CameraSnapshot, DetectedObject};
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
    object_input: Option<Arc<GrayFrame>>,
    objects: Option<(u64, Instant, Vec<DetectedObject>, String, f32)>,
    object_note: Option<String>,
    /// Rosto roda em fio próprio, como os objetos: a inferência nunca trava a leitura dos quadros.
    face_input: Option<Arc<GrayFrame>>,
    face: Option<(Instant, Option<orion_core::face_neural::Face>)>,
    face_accel: Option<String>,
    face_note: Option<String>,
}

struct Worker {
    stop: Arc<AtomicBool>,
    shared: Arc<Mutex<Shared>>,
    stream: Option<Arc<CameraStream>>,
}

static WORKER: Mutex<Option<Worker>> = Mutex::new(None);

pub fn is_open() -> bool {
    WORKER.lock().is_some()
}

/// Liga a câmera e o fio de análise. Bloqueante (abre o dispositivo): rode em `spawn_blocking`.
pub fn open(device: i32, object_model: Option<std::path::PathBuf>) -> Result<(), String> {
    let stop = Arc::new(AtomicBool::new(false));
    let shared = Arc::new(Mutex::new(Shared::default()));
    {
        let mut slot = WORKER.lock();
        if slot.is_some() { return Ok(()); }
        // Reserva antes de abrir o driver; fechar nunca espera pela inicialização do dispositivo.
        *slot = Some(Worker { stop: stop.clone(), shared: shared.clone(), stream: None });
    }
    let result = (|| {
        let stream = Arc::new(CameraStream::open_with(device, Duration::from_secs(4), FRAME_INTERVAL, true).map_err(|e| e.to_string())?);
        {
            let mut slot = WORKER.lock();
            match slot.as_mut() {
                Some(worker) if Arc::ptr_eq(&worker.stop, &stop) && !stop.load(Ordering::Relaxed) => {
                    worker.stream = Some(stream.clone());
                }
                _ => {
                    stream.cancel();
                    return Err("leitura encerrada".into());
                }
            }
        }
        let (s, sh) = (stop.clone(), shared.clone());
        std::thread::Builder::new()
            .name("orion-camera-module".into())
            .spawn(move || run(stream, s, sh, object_model))
            .map_err(|e| format!("não foi possível iniciar o módulo de câmera: {e}"))?;
        tracing::info!(target: "orion.audit", "camera_modulo_aberto");
        Ok(())
    })();
    if result.is_err() {
        let mut slot = WORKER.lock();
        if slot.as_ref().is_some_and(|w| Arc::ptr_eq(&w.stop, &stop)) {
            if let Some(w) = slot.take() {
                w.stop.store(true, Ordering::Relaxed);
                if let Some(stream) = w.stream { stream.cancel(); }
            }
        }
    }
    result
}

/// Desliga: o fio para no próximo quadro e solta a câmera.
pub fn close() {
    if let Some(w) = WORKER.lock().take() {
        w.stop.store(true, Ordering::Relaxed);
        if let Some(stream) = w.stream { stream.cancel(); }
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
        return (None, Some("Gestos indisponíveis: instale os recursos de visão no instalador".into()));
    }
    match HandModels::load(&dir) {
        Ok(models) => (Some(GestureEngine::new(models)), None),
        Err(e) => {
            tracing::info!("gestos indisponíveis: {e}");
            (None, Some("Gestos indisponíveis: confira a instalação dos recursos de visão".into()))
        }
    }
}

fn run(stream: Arc<CameraStream>, stop: Arc<AtomicBool>, shared: Arc<Mutex<Shared>>, object_model: Option<std::path::PathBuf>) {
    let (object_stop, object_shared) = (stop.clone(), shared.clone());
    let objects = std::thread::Builder::new().name("orion-camera-objects".into())
        .spawn(move || object_worker(object_model, object_stop, object_shared));
    let (face_stop, face_shared) = (stop.clone(), shared.clone());
    let faces = std::thread::Builder::new().name("orion-camera-face".into())
        .spawn(move || face_worker(face_stop, face_shared));
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
        if stop.load(Ordering::Relaxed) { break; }
        let (last_face, face_accel) = {
            let sh = shared.lock();
            let face = sh.face.as_ref().filter(|(at, _)| at.elapsed() < Duration::from_millis(600)).and_then(|(_, f)| f.clone());
            (face, sh.face_accel.clone())
        };
        let mut snap = analyze(&frame, engine.as_mut(), accel.clone(), note.clone(), last_face.as_ref(), &shared);
        snap.face_accel = face_accel;
        let mut sh = shared.lock();
        sh.object_input = Some(frame.clone());
        sh.face_input = Some(frame.clone());
        if let Some((seq, at, detected, backend, ms)) = &sh.objects {
            if frame.seq.saturating_sub(*seq) <= 8 && at.elapsed() < Duration::from_millis(700) {
                snap.objects = detected.clone();
            }
            snap.object_accel = Some(backend.clone());
            snap.object_ms = Some(*ms);
        }
        if let Some(message) = &sh.face_note {
            snap.note = Some(match snap.note { Some(note) => format!("{note} · {message}"), None => message.clone() });
        }
        if let Some(message) = &sh.object_note {
            snap.note = Some(match snap.note { Some(note) => format!("{note} · {message}"), None => message.clone() });
        }
        sh.snapshot = Some(snap);
    }
    // soltar o fluxo desliga a câmera
    drop(stream);
    stop.store(true, Ordering::Relaxed);
    if let Ok(thread) = objects { let _ = thread.join(); }
    if let Ok(thread) = faces { let _ = thread.join(); }
}

/// Detecta o rosto a ~8 por segundo num fio separado (GPU quando há DirectML); o laço dos quadros só lê o último resultado.
fn face_worker(stop: Arc<AtomicBool>, shared: Arc<Mutex<Shared>>) {
    let engine = match orion_core::face_neural::shared() {
        Ok(engine) => engine,
        Err(e) => {
            tracing::info!("rosto indisponível: {e}");
            shared.lock().face_note = Some("Rosto indisponível: confira a instalação dos recursos de visão".into());
            return;
        }
    };
    shared.lock().face_accel = Some(engine.lock().accel.label().to_string());
    while !stop.load(Ordering::Relaxed) {
        let frame = shared.lock().face_input.take();
        let Some(frame) = frame else { std::thread::sleep(Duration::from_millis(16)); continue; };
        let Some(rgb) = &frame.rgb else { continue; };
        let found = engine.lock().detect(rgb).ok().and_then(|v| v.into_iter().next());
        shared.lock().face = Some((Instant::now(), found));
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn object_worker(model: Option<std::path::PathBuf>, stop: Arc<AtomicBool>, shared: Arc<Mutex<Shared>>) {
    let detector = orion_core::visao_local::Detector::new(model, 0.4);
    if let Err(e) = detector.warm() {
        tracing::info!("objetos indisponíveis: {e}");
        shared.lock().object_note = Some("Objetos indisponíveis: confira a instalação dos recursos de visão".into());
        return;
    }
    let mut last = Instant::now() - Duration::from_millis(500);
    while !stop.load(Ordering::Relaxed) {
        let interval = if detector.accel() == orion_core::vision_runtime::Accel::Cpu { Duration::from_millis(500) } else { Duration::from_millis(200) };
        if last.elapsed() < interval { std::thread::sleep(Duration::from_millis(16)); continue; }
        let frame = shared.lock().object_input.take();
        let Some(frame) = frame else { std::thread::sleep(Duration::from_millis(16)); continue; };
        let Some(rgb) = &frame.rgb else { continue; };
        last = Instant::now();
        match detector.detect_rgb(rgb) {
            Ok(found) => {
                let (w, h) = (frame.w as f64, frame.h as f64);
                let found = found.into_iter().take(24).map(|d| DetectedObject {
                    label: d.label, confidence: d.confidence as f32,
                    bbox: [(1.0-d.bbox.2/w) as f32, (d.bbox.1/h) as f32, ((d.bbox.2-d.bbox.0)/w) as f32, ((d.bbox.3-d.bbox.1)/h) as f32],
                }).collect();
                let mut sh = shared.lock();
                sh.object_note = None;
                sh.objects = Some((frame.seq, Instant::now(), found, detector.accel().label().into(), last.elapsed().as_secs_f32()*1000.0));
            }
            Err(e) => {
                let mut sh = shared.lock();
                if sh.object_note.is_none() { tracing::info!("objetos indisponíveis: {e}"); }
                sh.object_note = Some("Objetos indisponíveis: confira a instalação dos recursos de visão".into());
            }
        }
    }
}

fn analyze(frame: &GrayFrame, engine: Option<&mut GestureEngine>, accel: Option<String>, note: Option<String>, detected_face: Option<&orion_core::face_neural::Face>, shared: &Arc<Mutex<Shared>>) -> CameraSnapshot {
    let (w, h) = (frame.w as f32, frame.h as f32);
    let face = detected_face.map(|f| f.rect).map(|r| {
        let (x, y, rw, rh) = (r.x as f32 / w, r.y as f32 / h, r.w as f32 / w, r.h as f32 / h);
        [1.0 - x - rw, y, rw, rh]
    });
    let mut snap = CameraSnapshot { seq: frame.seq, face, accel, note, ..Default::default() };
    snap.face_landmarks = detected_face.map(|f| f.points.iter().map(|p| [1.0-p[0]/w,p[1]/h]).collect());
    if let (Some(engine), Some(rgb)) = (engine, frame.rgb.as_ref()) {
        match engine.process(rgb, &frame.luma, Instant::now()) {
            Ok(r) => {
                snap.accel = Some(engine.accel().label().into());
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
            Err(e) => {
                tracing::debug!("gestos pausados: {e}");
                snap.note = Some("Gestos indisponíveis: confira a instalação dos recursos de visão".into());
            }
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
