//! Nota raiz do projeto no vault (F08.06): `01 Projetos/<Nome>/<Nome>.md`.
//! REGRAS.md: você é o administrador; o Orion só cria a nota, nunca a sobrescreve.

use crate::VaultError;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ProjectNote {
    pub name: String,
    pub instructions: String,
    /// Pasta local vinculada ao projeto.
    pub root: Option<String>,
    /// URL do repositório remoto, quando o projeto foi clonado.
    pub remote: Option<String>,
    pub date_str: String,
}

impl ProjectNote {
    pub fn rel_path(&self) -> PathBuf {
        let name = self.name.trim();
        PathBuf::from("01 Projetos").join(name).join(format!("{name}.md"))
    }

    /// Nome que cabe em pasta e arquivo do vault e em wikilink (mesmas proibições da validação de conversas).
    fn check_name(&self) -> Result<(), VaultError> {
        let name = self.name.trim();
        if name.is_empty()
            || name.chars().count() > 100
            || name.chars().any(char::is_control)
            || name.contains(['/', '\\', '[', ']', '#', '|', ':', '<', '>', '"', '?', '*'])
            || name.ends_with([' ', '.'])
            || [".", ".."].contains(&name)
        {
            return Err(VaultError::InvalidFormat("nome do projeto inválido para o vault".into()));
        }
        chrono::NaiveDate::parse_from_str(&self.date_str, "%Y-%m-%d")
            .map_err(|_| VaultError::InvalidFormat("data inválida; use AAAA-MM-DD".into()))?;
        Ok(())
    }

    pub(crate) fn to_markdown(&self) -> Result<String, VaultError> {
        self.check_name()?;
        let name = self.name.trim();
        let mut out = format!("---\ntype: projeto\nstatus: ativo\ntags: [orion, projeto]\ndata: {}\n---\n\n# {name}\n", self.date_str);
        if !self.instructions.trim().is_empty() {
            out.push_str(&format!("\n## Instruções\n\n{}\n", self.instructions.trim()));
        }
        let (root, remote) = (self.root.as_deref(), self.remote.as_deref());
        if root.is_some() || remote.is_some() {
            out.push_str("\n## Repositório\n\n");
            if let Some(r) = remote {
                out.push_str(&format!("- Remoto: {r}\n"));
            }
            if let Some(r) = root {
                out.push_str(&format!("- Pasta local: `{r}`\n"));
            }
        }
        Ok(out)
    }
}
