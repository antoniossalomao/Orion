//! Bundle treinado: nome do arquivo não prova a palavra nem a qualidade (REGRAS.md: detecção local).
use anyhow::{ensure, Context};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize)]
pub struct Metrics {
    pub recall: f64,
    pub false_alarms_per_hour: f64,
    pub positive_clips: usize,
    pub negative_seconds: f64,
    pub real_audio: bool,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct WakeManifest {
    pub keyword: String,
    pub status: String,
    pub sha256: String,
    pub bytes: u64,
    pub threshold: f32,
    pub input_shape: Vec<usize>,
    pub metrics: Option<Metrics>,
}

pub fn load(directory: &Path, calibration: bool) -> anyhow::Result<(PathBuf, WakeManifest)> {
    let bytes = std::fs::read(directory.join("orion.json"))?;
    ensure!(bytes.len() <= 65536, "manifesto de ativação grande demais");
    let manifest: WakeManifest = serde_json::from_slice(&bytes)?;
    ensure!(
        manifest.keyword == "Orion" && manifest.input_shape == [1, 16, 96],
        "modelo não declara o contrato Orion/openWakeWord"
    );
    ensure!(
        manifest.threshold.is_finite() && (0.05..=0.95).contains(&manifest.threshold),
        "limiar inválido"
    );
    if !calibration {
        ensure!(
            manifest.status == "validated",
            "modelo Orion candidato; valide com áudio real antes de ativar a escuta"
        );
        let metrics = manifest.metrics.as_ref().context("modelo sem relatório de avaliação")?;
        ensure!(
            metrics.real_audio
                && metrics.positive_clips >= 50
                && metrics.negative_seconds >= 3600.0
                && metrics.recall.is_finite()
                && metrics.recall >= 0.95
                && metrics.recall <= 1.0
                && metrics.false_alarms_per_hour.is_finite()
                && (0.0..=0.5).contains(&metrics.false_alarms_per_hour),
            "modelo Orion não atingiu os critérios de validação"
        );
    }
    let path = directory.join("orion.onnx");
    let artifact = crate::catalog::Artifact {
        id: "Orion".into(),
        name: "Orion".into(),
        kind: "wake".into(),
        repository: String::new(),
        revision: String::new(),
        filename: "orion.onnx".into(),
        sha256: manifest.sha256.clone(),
        bytes: manifest.bytes,
    };
    crate::download::verify(&path, &artifact)?;
    Ok((path, manifest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    #[test]
    fn candidato_nao_vira_modelo_final_por_renomear_arquivo() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("orion.onnx"), b"candidate").unwrap();
        let mut m = WakeManifest {
            keyword: "Orion".into(),
            status: "candidate".into(),
            sha256: hex::encode(Sha256::digest(b"candidate")),
            bytes: 9,
            threshold: 0.5,
            input_shape: vec![1, 16, 96],
            metrics: None,
        };
        let save = |m: &WakeManifest| std::fs::write(d.path().join("orion.json"), serde_json::to_vec(m).unwrap()).unwrap();
        save(&m);
        assert!(load(d.path(), true).is_ok());
        assert!(load(d.path(), false).is_err());
        m.status = "validated".into();
        m.metrics = Some(Metrics {
            recall: 1.0,
            false_alarms_per_hour: 0.0,
            positive_clips: 100,
            negative_seconds: 7200.0,
            real_audio: false,
        });
        save(&m);
        assert!(load(d.path(), false).is_err());
        m.metrics.as_mut().unwrap().real_audio = true;
        save(&m);
        assert!(load(d.path(), false).is_ok());
        std::fs::write(d.path().join("orion.onnx"), b"different").unwrap();
        assert!(load(d.path(), false).is_err());
    }
}
