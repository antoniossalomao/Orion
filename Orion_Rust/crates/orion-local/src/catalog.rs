//! Fontes fixadas por revisão e SHA-256; não recebem URLs do modelo (REGRAS.md: conteúdo externo é dado).
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub repository: String,
    pub revision: String,
    pub filename: String,
    pub sha256: String,
    pub bytes: u64,
    /// Fonte fora do Hugging Face (pacote NuGet oficial da Microsoft), também fixada por versão.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Arquivo de dentro do pacote baixado que vira `filename` (o pacote é conferido por `sha256`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extract: Option<String>,
    /// SHA-256 e tamanho do arquivo extraído.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extracted_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extracted_bytes: Option<u64>,
}
impl Artifact {
    pub fn url(&self) -> String {
        self.url.clone().unwrap_or_else(|| format!("https://huggingface.co/{}/resolve/{}/{}", self.repository, self.revision, self.filename))
    }
    /// Hash e tamanho que o arquivo final (já extraído, se for pacote) precisa ter.
    pub fn final_digest(&self) -> (&str, u64) {
        match (&self.extracted_sha256, self.extracted_bytes) {
            (Some(sha), Some(bytes)) if self.extract.is_some() => (sha, bytes),
            _ => (&self.sha256, self.bytes),
        }
    }
}
/// Ids do motor de gestos (ONNX Runtime, DirectML e os dois modelos de mão), nesta ordem.
pub const GESTURE_RUNTIME: [&str; 4] = ["onnxruntime-directml-1.24.4", "directml-1.15.4", "mediapipe-palm-detection", "mediapipe-hand-landmarks"];
pub fn all() -> Vec<Artifact> {
    serde_json::from_str(include_str!("catalog.json")).expect("catálogo fixado válido")
}
pub fn find(id: &str) -> Option<Artifact> {
    all().into_iter().find(|m| m.id == id)
}
