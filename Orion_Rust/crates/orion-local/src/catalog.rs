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
}
impl Artifact {
    pub fn url(&self) -> String {
        format!("https://huggingface.co/{}/resolve/{}/{}", self.repository, self.revision, self.filename)
    }
}
pub fn all() -> Vec<Artifact> {
    serde_json::from_str(include_str!("catalog.json")).expect("catálogo fixado válido")
}
pub fn find(id: &str) -> Option<Artifact> {
    all().into_iter().find(|m| m.id == id)
}
