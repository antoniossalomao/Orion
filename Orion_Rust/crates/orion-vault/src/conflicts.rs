//! Comparação e mescla de conflitos do vault (F06.08).
//!
//! Quando a nota do Orion difere da que está no vault, `save_conversation` preserva a original e grava
//! a versão nova como `<nota> - conflito Orion <hash>.md`. Aqui você lê as duas e decide; o Orion nunca
//! escolhe sozinho (REGRAS.md: você é o administrador). Só a pasta da memória do Orion é tocada.

use crate::scanner::SecretScanner;
use crate::sync::VaultManager;
use crate::{safe_path, VaultError};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::{Path, PathBuf};

const MARCA: &str = " - conflito Orion ";
const AVISO: &str = "\n> Conflito de edição. A versão anterior foi preservada. Revise as duas versões antes de publicar.\n";

fn pasta() -> PathBuf {
    Path::new("03 Recursos").join("Orion").join("Memória")
}

fn invalid(m: &str) -> VaultError {
    VaultError::InvalidFormat(m.into())
}

fn sha(t: &str) -> String {
    format!("{:x}", Sha256::digest(t.as_bytes()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictSummary {
    pub original_path: String,
    pub proposed_path: String,
    pub original_exists: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictDetail {
    pub original_path: String,
    pub proposed_path: String,
    pub original: String,
    /// A versão do Orion sem o aviso de conflito: pronta para ser usada como está.
    pub proposal: String,
    /// Prova de qual original foi mostrada; a resolução recusa se ele mudou desde então.
    pub original_sha256: String,
}

/// `<pasta>/<nome> - conflito Orion <20 hex>.md` → (`<pasta>/<nome>.md`). Só a pasta da memória.
fn original_de(proposed: &Path) -> Result<PathBuf, VaultError> {
    let nome = proposed.file_name().and_then(|n| n.to_str()).ok_or_else(|| invalid("nome de proposta inválido"))?;
    let stem = nome.strip_suffix(".md").ok_or_else(|| invalid("a proposta precisa ser uma nota .md"))?;
    let (base, hash) = stem.rsplit_once(MARCA).ok_or_else(|| invalid("o arquivo não é uma proposta de conflito do Orion"))?;
    if base.is_empty() || hash.len() != 20 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(invalid("o arquivo não é uma proposta de conflito do Orion"));
    }
    if proposed.parent() != Some(pasta().as_path()) {
        return Err(invalid("a proposta precisa estar na memória do Orion"));
    }
    Ok(pasta().join(format!("{base}.md")))
}

fn replace_note(path: &Path, content: &str) -> Result<(), std::io::Error> {
    let mut file = tempfile::NamedTempFile::new_in(path.parent().ok_or_else(|| std::io::Error::other("pasta inválida"))?)?;
    file.write_all(content.as_bytes())?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|e| e.error)?;
    Ok(())
}

impl VaultManager {
    /// Propostas de conflito pendentes na memória do Orion.
    pub fn list_conflicts(&self) -> Result<Vec<ConflictSummary>, VaultError> {
        let dir = safe_path::checked(&self.config().path, &pasta(), false)?;
        let mut out = vec![];
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(e.into()),
        };
        for entry in entries {
            let name = entry?.file_name();
            let rel = pasta().join(&name);
            let Ok(original) = original_de(&rel) else { continue };
            let Ok(checked) = safe_path::checked(&self.config().path, &rel, false) else { continue };
            if !checked.is_file() {
                continue;
            }
            let original_exists = safe_path::checked(&self.config().path, &original, false).is_ok_and(|p| p.is_file());
            out.push(ConflictSummary { original_path: to_slash(&original), proposed_path: to_slash(&rel), original_exists });
        }
        out.sort_by(|a, b| a.proposed_path.cmp(&b.proposed_path));
        Ok(out)
    }

    /// As duas versões lado a lado, para você comparar.
    pub fn read_conflict(&self, proposed_path: &str) -> Result<ConflictDetail, VaultError> {
        let proposed_rel = PathBuf::from(proposed_path);
        let original_rel = original_de(&proposed_rel)?;
        let proposed = safe_path::read_note(&safe_path::checked(&self.config().path, &proposed_rel, false)?)?;
        let original = safe_path::read_note(&safe_path::checked(&self.config().path, &original_rel, false)?)?;
        let proposal = proposed.strip_suffix(AVISO).unwrap_or(&proposed).to_string();
        Ok(ConflictDetail {
            original_path: to_slash(&original_rel),
            proposed_path: to_slash(&proposed_rel),
            original_sha256: sha(&original),
            original,
            proposal,
        })
    }

    /// Grava o texto final na nota original e remove a proposta. Recusa se a original mudou depois que
    /// você a viu (`original_sha256`), se o texto tem segredo ou se perde o frontmatter. Sem Git: o
    /// arquivo fica modificado para você revisar e commitar.
    pub fn resolve_conflict(&self, proposed_path: &str, original_sha256: &str, final_text: &str) -> Result<String, VaultError> {
        let proposed_rel = PathBuf::from(proposed_path);
        let original_rel = original_de(&proposed_rel)?;
        let proposed = safe_path::checked(&self.config().path, &proposed_rel, false)?;
        let original = safe_path::checked(&self.config().path, &original_rel, false)?;
        if sha(&safe_path::read_note(&original)?) != original_sha256 {
            return Err(invalid("a nota original mudou depois da comparação; abra o conflito de novo"));
        }
        if final_text.len() as u64 > safe_path::NOTE_LIMIT {
            return Err(invalid("nota maior que 2 MiB"));
        }
        SecretScanner::scan(final_text).map_err(|e| VaultError::SecretDetected(e.to_string()))?;
        if !final_text.starts_with("---\n") || !final_text[4..].contains("\n---\n") {
            return Err(invalid("o texto final precisa manter o frontmatter da nota"));
        }
        let text = if final_text.ends_with('\n') { final_text.to_string() } else { format!("{final_text}\n") };
        replace_note(&original, &text)?;
        // A proposta é arquivo do próprio Orion; some só depois de a original estar gravada.
        std::fs::remove_file(&proposed)?;
        Ok(to_slash(&original_rel))
    }

    /// Descarta a proposta e mantém a original como está.
    pub fn discard_conflict(&self, proposed_path: &str) -> Result<(), VaultError> {
        let proposed_rel = PathBuf::from(proposed_path);
        original_de(&proposed_rel)?;
        let proposed = safe_path::checked(&self.config().path, &proposed_rel, false)?;
        std::fs::remove_file(proposed)?;
        Ok(())
    }
}

fn to_slash(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}
