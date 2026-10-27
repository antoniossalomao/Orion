//! Consentimento local do primeiro uso e das Configurações autenticadas.
//! Fica fora do perfil/backup (REGRAS.md: autorização única, local e revogável).
use serde::{Deserialize, Serialize};
use std::{io::Write, path::Path};

pub const FILE: &str = "tool-permissions.json";

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolPermissions {
    pub desktop_tools: bool,
    pub web_tools: bool,
    pub without_confirmation: bool,
}

impl ToolPermissions {
    pub fn all() -> Self {
        Self { desktop_tools: true, web_tools: true, without_confirmation: true }
    }

    pub fn load(data: &Path) -> anyhow::Result<Option<Self>> {
        match std::fs::read(data.join(FILE)) {
            Ok(bytes) => {
                anyhow::ensure!(bytes.len() <= 4096, "arquivo de permissões inválido");
                Ok(Some(serde_json::from_slice(&bytes)?))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Publica o arquivo inteiro antes de atualizar a política em memória.
    pub fn save(&self, data: &Path) -> anyhow::Result<()> {
        let mut staged = tempfile::NamedTempFile::new_in(data)?;
        staged.write_all(&serde_json::to_vec_pretty(self)?)?;
        staged.as_file().sync_all()?;
        staged.persist(data.join(FILE)).map_err(|e| e.error)?;
        Ok(())
    }
}
