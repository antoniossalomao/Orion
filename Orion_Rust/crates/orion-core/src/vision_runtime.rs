//! Inferência local compartilhada por rosto, objetos e gestos (REGRAS.md: rosto é local;
//! a câmera só liga por pedido seu). Modelos são instalados por escolha explícita, sem rede aqui.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[cfg(windows)]
pub const ORT_LIB: &str = "onnxruntime.dll";
#[cfg(target_os = "macos")]
pub const ORT_LIB: &str = "libonnxruntime.dylib";
#[cfg(all(unix, not(target_os = "macos")))]
pub const ORT_LIB: &str = "libonnxruntime.so";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accel { DirectMl, CoreMl, Cpu, Hybrid }

impl Accel {
    pub fn label(self) -> &'static str {
        match self { Self::DirectMl => "DirectML", Self::CoreMl => "CoreML", Self::Cpu => "CPU", Self::Hybrid => "GPU + CPU" }
    }
}

pub fn runtime_dir() -> Option<PathBuf> {
    std::env::var_os("ORION_RUNTIME_DIR").map(PathBuf::from)
        .or_else(|| {
            let values = crate::config::local_defaults(None)?;
            let path = values.get("ORION_RUNTIME_DIR").or_else(|| values.get("ORION_GESTURES_RUNTIME"))?.as_str()?.trim();
            (!path.is_empty()).then(|| PathBuf::from(path))
        })
        .or_else(|| dirs::data_local_dir().map(|d| d.join("Orion").join("runtime")))
}

static ENV: OnceLock<ort::environment::Environment> = OnceLock::new();
static INIT: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

fn environment(dir: &Path) -> Result<&'static ort::environment::Environment, String> {
    let _guard = INIT.lock();
    if let Some(env) = ENV.get() { return Ok(env); }
    let lib = dir.join(ORT_LIB);
    if !lib.is_file() { return Err(format!("motor de visão não instalado: falta {ORT_LIB}")); }
    verify_model(&lib)?;
    #[cfg(windows)]
    if dir.join("DirectML.dll").is_file() {
        verify_model(&dir.join("DirectML.dll"))?;
        ort::util::preload_dylib(dir.join("DirectML.dll")).map_err(|e| e.to_string())?;
    }
    let env = ort::init_from(&lib).map_err(|e| e.to_string())?.with_name("orion-visao").build().map_err(|e| e.to_string())?;
    let _ = ENV.set(env);
    ENV.get().ok_or_else(|| "ambiente de visão indisponível".into())
}

/// Confere os pesos conhecidos antes da primeira carga, incluindo os usados na autenticação.
pub fn verify_model(path: &Path) -> Result<(), String> {
    if !path.is_file() { return Err(format!("modelo de visão não instalado: {}", path.file_name().unwrap_or_default().to_string_lossy())); }
    if let Some(mut artifact) = orion_local::catalog::all().into_iter().find(|a| Some(a.filename.as_str()) == path.file_name().and_then(|n| n.to_str())) {
        let (sha, bytes) = artifact.final_digest();
        let sha = sha.to_string();
        artifact.sha256 = sha;
        artifact.bytes = bytes;
        orion_local::download::verify(path, &artifact).map_err(|_| "modelo de visão alterado; reinstale os recursos de visão".to_string())?;
    }
    Ok(())
}

/// DirectML/CoreML quando a sessão aceita o modelo; CPU como alternativa. Reusa o ambiente e
/// mantém a sessão pronta entre quadros (REGRAS.md: portabilidade; rosto é local).
pub fn session(path: &Path) -> Result<(ort::session::Session, Accel), String> {
    load_session(path, false)
}

pub fn cpu_session(path: &Path) -> Result<ort::session::Session, String> {
    load_session(path, true).map(|(s, _)| s)
}

fn warm(session: &mut ort::session::Session, path: &Path) -> Result<(), String> {
    let shape = match path.file_name().and_then(|n| n.to_str()).unwrap_or("") {
        "face_detection_yunet_2023mar.onnx" | "object_detection_yolox_2022nov.onnx" => [1usize,3,640,640],
        "face_recognition_sface_2021dec.onnx" => [1,3,112,112],
        "palm_detection_mediapipe_2023feb.onnx" => [1,192,192,3],
        "handpose_estimation_mediapipe_2023feb.onnx" => [1,224,224,3],
        _ => return Ok(()),
    };
    // Compila kernels/aloca buffers antes de usar a câmera. Não usa nem guarda um rosto real.
    let tensor = ort::value::Tensor::from_array((shape,vec![0.0f32;shape.iter().product()])).map_err(|e|e.to_string())?;
    session.run(ort::inputs![tensor]).map_err(|e|e.to_string())?;
    Ok(())
}

fn load_session(path: &Path, cpu_only: bool) -> Result<(ort::session::Session, Accel), String> {
    verify_model(path)?;
    let dir = path.parent().filter(|d| d.join(ORT_LIB).is_file()).map(Path::to_path_buf)
        .or_else(runtime_dir).ok_or("pasta do motor de visão indisponível")?;
    let env = environment(&dir)?;
    // O SFace original declara pesos também como entradas. Removê-los dessa lista mantém os
    // pesos e operações idênticos e permite fusão de convoluções/BN pelo runtime, inclusive GPU.
    let optimized = if path.file_name().is_some_and(|n| n == "face_recognition_sface_2021dec.onnx") {
        use prost::Message;
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        let mut model = tract_onnx::pb::ModelProto::decode(bytes.as_slice()).map_err(|e| e.to_string())?;
        if let Some(graph) = model.graph.as_mut() {
            let constants: std::collections::HashSet<_> = graph.initializer.iter().map(|v| v.name.clone()).collect();
            graph.input.retain(|v| !constants.contains(&v.name));
        }
        model.ir_version = model.ir_version.max(4);
        Some(model.encode_to_vec())
    } else { None };
    let commit = |mut builder: ort::session::builder::SessionBuilder| {
        if let Some(bytes) = &optimized { builder.commit_from_memory(bytes) } else { builder.commit_from_file(path) }
    };
    let ready = |builder| -> Result<ort::session::Session,String> {
        let mut session = commit(builder).map_err(|e|e.to_string())?;
        warm(&mut session,path)?;
        Ok(session)
    };
    let builder = || -> Result<ort::session::builder::SessionBuilder, String> {
        ort::session::Session::builder(env).map_err(|e| e.to_string())?
            .with_intra_threads(2).map_err(|e| e.to_string())
    };
    #[cfg(windows)]
    if !cpu_only {
        let accelerated = builder()?.with_memory_pattern(false)
            .and_then(|b| b.with_parallel_execution(false))
            .and_then(|b| b.with_execution_providers([ort::ep::DirectML::default().with_performance_preference(ort::ep::directml::PerformancePreference::HighPerformance).build().error_on_failure()]))
            .map_err(|e| e.to_string()).and_then(ready);
        match accelerated {
            Ok(s) => return Ok((s, Accel::DirectMl)),
            Err(e) => tracing::info!("visão: DirectML indisponível ({e}); usando CPU"),
        }
    }
    #[cfg(target_os = "macos")]
    if !cpu_only {
        if let Ok(b) = builder()?.with_execution_providers([ort::ep::CoreML::default().build().error_on_failure()]) {
            if let Ok(s) = ready(b) { return Ok((s, Accel::CoreMl)); }
        }
    }
    let _ = cpu_only;
    ready(builder()?).map(|s| (s, Accel::Cpu))
}
