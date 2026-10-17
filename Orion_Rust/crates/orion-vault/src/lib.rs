//! Crate `orion-vault`: Sincronização nativa, parser e memória no Vault do Obsidian (Fase F06).
//!
//! Este crate implementa todas as exigências do marco F06:
//! - Detecção automática do caminho do Vault via `obsidian.json` ou caminho explícito.
//! - Parser completo de notas Markdown (YAML frontmatter, `#tags`, `[[wikilinks]]`, `type`, `projeto`).
//! - Verificação de segurança (segredos e PII) antes de gravar/commitar notas (R06/R07).
//! - Gravação estruturada de conversas em `03 Recursos/Orion/Memória/` com frontmatter canônico (`claude.md`).
//! - Detecção e tratamento de notas privadas (`#privado`).
//! - Sincronização Git (`git pull`, `git add`, `git commit`, `git push`) direto na branch `main`.
//! - Resolução de conflitos de edição externa (mantém ambas as versões para decisão no chat).
//! - Geração de relatórios de mudanças em `03 Recursos/Relatórios de Mudanças/`.

pub mod detector;
pub mod git;
pub mod note;
pub mod parser;
pub mod scanner;
pub mod sync;

pub use detector::{detect_vault_path, VaultConfig};
pub use git::{GitError, GitSync};
pub use note::{ConversationNote, NoteFrontmatter, NoteMetadata};
pub use parser::{ParsedNote, VaultParser};
pub use scanner::{ScanError, SecretScanner};
pub use sync::{SyncResult, VaultManager, VaultStats};

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
