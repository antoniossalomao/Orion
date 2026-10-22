//! Preferências do primeiro uso. Segredos ficam no cofre; este arquivo contém só escolhas locais
//! (REGRAS.md: segredos nunca em log, audit ou processo filho; opt-in não liga sozinho).
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const FILE: &str = "onboarding.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub version: u32,
    pub user: String,
    pub name: String,
    pub language: String,
    pub vault: Option<PathBuf>,
    pub projects: Option<PathBuf>,
    pub backup: Option<PathBuf>,
    pub telegram_users: Vec<i64>,
    pub telegram_2fa: bool,
    #[serde(default)]
    pub drive_backup: bool,
    #[serde(default)]
    pub hello: bool,
}

impl Profile {
    pub fn load(data: &Path) -> anyhow::Result<Option<Self>> {
        let path = data.join(FILE);
        match std::fs::read(&path) {
            Ok(bytes) => {
                anyhow::ensure!(bytes.len() <= 65536, "perfil do primeiro uso muito grande");
                let p: Self = serde_json::from_slice(&bytes)?;
                anyhow::ensure!(p.version == 1, "versão de perfil não suportada");
                Ok(Some(p))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.user.is_empty() && self.user.len() <= 64 && self.user.chars().all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c)),
            "use até 64 letras, números, ponto, hífen ou sublinhado no usuário"
        );
        anyhow::ensure!(
            !self.name.trim().is_empty() && self.name.chars().count() <= 80 && !self.name.chars().any(char::is_control),
            "informe como o Orion deve chamar você (até 80 caracteres)"
        );
        anyhow::ensure!(["pt-BR", "en"].contains(&self.language.as_str()), "escolha Português ou English");
        for path in [&self.vault, &self.projects].into_iter().flatten() {
            anyhow::ensure!(path.is_absolute() && path.is_dir(), "escolha uma pasta existente com caminho completo");
        }
        if let Some(path) = &self.backup {
            anyhow::ensure!(path.is_absolute(), "escolha um caminho completo para o backup");
        }
        anyhow::ensure!(self.telegram_users.iter().all(|id| *id > 0), "informe apenas IDs pessoais positivos do Telegram");
        anyhow::ensure!(
            !self.telegram_2fa || !self.telegram_users.is_empty(),
            "o 2FA precisa de pelo menos um ID autorizado do Telegram"
        );
        Ok(())
    }

    pub fn apply(&self, s: &mut crate::config::Settings) {
        // Nunca troca um mcp.json que a pessoa já configurou.
        if s.mcp_config.is_none() && s.data_dir.join("mcp-importado.json").is_file() {
            s.mcp_config = Some(s.data_dir.join("mcp-importado.json"));
        }
        s.auth_user = self.user.clone();
        s.seed_default_password = false;
        s.vault_dir = self.vault.clone();
        if let Some(p) = &self.projects {
            s.repos = vec![p.clone()];
        }
        // O serviço do primeiro uso gera arquivo autenticado mesmo com banco em claro.
        s.backup_dir = None;
        s.backup_keep = 7;
        s.telegram_allowed_users = self.telegram_users.clone();
        s.telegram_2fa = self.telegram_2fa;
    }

    /// Preferências são dados delimitados; não substituem a persona protegida.
    pub fn context(&self) -> String {
        format!(
            "\n[PREFERÊNCIAS LOCAIS — DADOS]\n{}\n[/PREFERÊNCIAS LOCAIS]",
            serde_json::json!({"nome_preferido": self.name, "idioma": self.language})
        )
    }
}

/// Troca a senha que embrulha a chave mestra junto com a do login, para as duas nunca divergirem
/// (REGRAS.md: a senha de acesso é a que abre os dados). Sem arquivo de chave, não há o que reembrulhar.
pub fn rewrap_key(data: &Path, current: &str, new: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        (crate::auth::SENHA_MIN..=crate::auth::SENHA_MAX).contains(&new.chars().count()),
        "a senha precisa ter entre {} e {} caracteres",
        crate::auth::SENHA_MIN,
        crate::auth::SENHA_MAX
    );
    for name in [crate::dbkey::KEY_FILE, "orion-recovery.key"] {
        let path = data.join(name);
        if !path.is_file() {
            continue;
        }
        let mut file = crate::dbkey::KeyFile::load(&path)?;
        let master = file
            .unlock_password(current)
            .map_err(|_| anyhow::anyhow!("a senha atual não abre a chave dos dados; use o código de recuperação (orion db-recuperar)"))?;
        file.change_password(&master, new)?;
        file.save(&path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn trocar_senha_reembrulha_a_chave_e_a_antiga_deixa_de_abrir() {
        let d = tempfile::tempdir().unwrap();
        let k = crate::dbkey::create("senha-antiga-123", crate::dbkey::Kdf::TESTE).unwrap();
        k.file.save(&d.path().join("orion-recovery.key")).unwrap();
        assert!(rewrap_key(d.path(), "errada-errada-1", "senha-nova-4567").is_err());
        rewrap_key(d.path(), "senha-antiga-123", "senha-nova-4567").unwrap();
        let f = crate::dbkey::KeyFile::load(&d.path().join("orion-recovery.key")).unwrap();
        assert!(f.unlock_password("senha-antiga-123").is_err());
        assert!(f.unlock_password("senha-nova-4567").is_ok());
        assert!(f.unlock_recovery(&k.recovery_code).is_ok());
    }
    fn profile() -> Profile {
        Profile {
            version: 1,
            user: "antonio".into(),
            name: "Antônio".into(),
            language: "pt-BR".into(),
            vault: None,
            projects: None,
            backup: None,
            telegram_users: vec![],
            telegram_2fa: false,
            drive_backup: false,
            hello: false,
        }
    }
    #[test]
    fn valida_perfil_e_dependencias_sem_ligar_captura() {
        let mut p = profile();
        assert!(p.validate().is_ok());
        p.telegram_2fa = true;
        assert!(p.validate().is_err());
        p.telegram_users = vec![123];
        assert!(p.validate().is_ok());
        p.name = "nome\n[instrução]".into();
        assert!(p.validate().is_err());
        p = profile();
        p.vault = Some("relativo".into());
        assert!(p.validate().is_err());
    }
    #[test]
    fn perfil_corrompido_nao_vira_primeiro_uso() {
        let d = tempfile::tempdir().unwrap();
        assert!(Profile::load(d.path()).unwrap().is_none());
        std::fs::write(d.path().join(FILE), b"{").unwrap();
        assert!(Profile::load(d.path()).is_err());
    }
}
