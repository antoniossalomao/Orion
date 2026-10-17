use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::{info, warn};

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("Repositório git não encontrado em: {0}")]
    NotAGitRepo(String),

    #[error("Falha ao executar comando git '{0}': {1}")]
    CommandFailed(String, String),

    #[error("Erro de E/S ao invocar git: {0}")]
    Io(#[from] std::io::Error),
}

/// Gerenciador de sincronização Git do Vault (F06.02, F06.13).
pub struct GitSync {
    vault_path: PathBuf,
}

impl GitSync {
    pub fn new(vault_path: impl Into<PathBuf>) -> Result<Self, GitError> {
        let p = vault_path.into();
        let dot_git = p.join(".git");
        if !dot_git.exists() {
            return Err(GitError::NotAGitRepo(p.display().to_string()));
        }
        Ok(Self { vault_path: p })
    }

    /// Executa um comando `git` no diretório do vault.
    fn run_git(&self, args: &[&str]) -> Result<String, GitError> {
        let mut cmd = Command::new("git");
        cmd.current_dir(&self.vault_path);
        cmd.args(args);

        // Desativa prompts interativos
        cmd.env("GIT_TERMINAL_PROMPT", "0");

        let output = cmd.output()?;
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let cmd_str = format!("git {}", args.join(" "));
            return Err(GitError::CommandFailed(cmd_str, err));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Executa `git pull --rebase origin main` (F06.02).
    /// Se falhar por rede, registra aviso sem derrubar o app.
    pub fn pull(&self, branch: &str) -> Result<String, GitError> {
        info!("Executando git pull no vault em {:?}", self.vault_path);
        match self.run_git(&["pull", "--rebase", "origin", branch]) {
            Ok(out) => {
                info!("Git pull no vault concluído com sucesso: {}", out);
                Ok(out)
            }
            Err(e) => {
                warn!("Aviso: git pull falhou no vault (pode estar offline): {}", e);
                Err(e)
            }
        }
    }

    /// Adiciona um arquivo ao índice e faz commit direto na main (F06.13).
    pub fn commit_file(&self, rel_path: &Path, message: &str) -> Result<String, GitError> {
        let path_str = rel_path.to_string_lossy().to_string();
        info!("Commitando nota no vault: {} com mensagem: {}", path_str, message);

        self.run_git(&["add", &path_str])?;
        let commit_out = self.run_git(&["commit", "-m", message])?;
        Ok(commit_out)
    }

    /// Faz push para origin main (F06.13).
    pub fn push(&self, branch: &str) -> Result<String, GitError> {
        info!("Executando git push no vault para branch {}", branch);
        self.run_git(&["push", "origin", branch])
    }

    /// Retorna o hash do último commit (HEAD) do vault.
    pub fn head_commit_hash(&self) -> Result<String, GitError> {
        self.run_git(&["rev-parse", "HEAD"])
    }

    /// Verifica se há arquivos modificados ou untracked.
    pub fn status_short(&self) -> Result<String, GitError> {
        self.run_git(&["status", "-s"])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_sync_on_current_vault() {
        let p = Path::new(r"C:\Vault");
        if p.join(".git").exists() {
            let sync = GitSync::new(p).unwrap();
            let hash = sync.head_commit_hash();
            assert!(hash.is_ok());
            assert_eq!(hash.unwrap().len(), 40);
        }
    }
}
