//! Vault do Obsidian: parser, gravação explícita validada e sincronização Git isolada.
//!
//! Notas diferentes nunca são substituídas: a proposta fica separada e sinalizada como conflito.
//! A F06 continua parcial: gravação automática, comparação no chat, relatórios e índice incremental
//! completo ainda não estão implementados aqui. REGRAS.md: você é o administrador;
//! segredos nunca em log, audit ou processo filho.

pub mod conflicts;
pub mod detector;
pub mod git;
pub mod note;
pub mod parser;
pub mod project;
pub mod scanner;
pub mod sync;
mod safe_path;
mod validate;

pub use conflicts::{ConflictDetail, ConflictSummary};
pub use detector::{detect_vault_path, VaultConfig};
pub use git::{clone_public_repo, GitError, GitSync, GithubRepo};
pub use note::{ConversationNote, NoteFrontmatter, NoteMetadata};
pub use parser::{ParsedNote, VaultParser};
pub use project::ProjectNote;
pub use scanner::{ScanError, SecretScanner};
pub use sync::{NoteConflict, SyncResult, VaultManager, VaultStats};

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("Caminho do Vault não encontrado ou inválido: {0}")]
    NotFound(String),

    #[error("Erro de E/S no Vault: {0}")]
    Io(#[from] std::io::Error),

    #[error("Erro de Git no Vault: {0}")]
    Git(#[from] GitError),

    #[error("Segredo ou dado sensível detectado ao gravar nota (R06): {0}")]
    SecretDetected(String),

    #[error("Frontmatter ou formato inválido: {0}")]
    InvalidFormat(String),

    #[error("Erro de serialização YAML: {0}")]
    Yaml(#[from] serde_yaml::Error),

    #[error("Erro interno do Vault: {0}")]
    Internal(String),
}
