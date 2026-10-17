use crate::detector::{detect_vault_path, VaultConfig};
use crate::git::GitSync;
use crate::note::ConversationNote;
use crate::parser::{ParsedNote, VaultParser};
use crate::scanner::SecretScanner;
use crate::VaultError;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{info, warn};

/// Estatísticas de notas do Vault.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VaultStats {
    pub total_notes: usize,
    pub private_notes: usize,
    pub archive_notes: usize,
    pub total_links: usize,
    pub total_tags: usize,
    pub last_commit: Option<String>,
}

/// Resultado de gravação de nota de conversa.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResult {
    pub file_path: String,
    pub commit_hash: Option<String>,
    pub pushed: bool,
    pub warning: Option<String>,
}

/// Gerenciador central de sincronização e memória do Vault (F06).
pub struct VaultManager {
    config: VaultConfig,
    git: Option<GitSync>,
    notes_cache: Arc<RwLock<HashMap<String, ParsedNote>>>,
}

impl VaultManager {
    /// Inicializa o VaultManager com auto-detecção ou caminho configurado.
    pub fn new(custom_path: Option<PathBuf>) -> Result<Self, VaultError> {
        let path = custom_path
            .or_else(detect_vault_path)
            .ok_or_else(|| VaultError::NotFound("Nenhum caminho de Vault encontrado".to_string()))?;

        if !path.is_dir() {
            return Err(VaultError::NotFound(path.display().to_string()));
        }

        let git = GitSync::new(&path).ok();
        let config = VaultConfig {
            path,
            remote_url: None,
            branch: "main".to_string(),
            auto_pull: true,
            auto_push: true,
        };

        Ok(Self {
            config,
            git,
            notes_cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub fn config(&self) -> &VaultConfig {
        &self.config
    }

    /// Executa pull inicial no repositório do Vault se o Git estiver disponível.
    pub fn pull(&self) -> Result<Option<String>, VaultError> {
        if let Some(git) = &self.git {
            if self.config.auto_pull {
                match git.pull(&self.config.branch) {
                    Ok(out) => return Ok(Some(out)),
                    Err(e) => {
                        warn!("Pull falhou (continuando com cópia local): {}", e);
                        return Ok(None);
                    }
                }
            }
        }
        Ok(None)
    }

    /// Salva uma conversa do Orion em `03 Recursos/Orion/Memória/` e faz commit/push na main (F06.09, F06.13).
    pub fn save_conversation(&self, note: &ConversationNote) -> Result<SyncResult, VaultError> {
        // 1. Validar e escanear segredos antes de qualquer operação (R06)
        let md = note.to_markdown();
        if let Err(e) = SecretScanner::scan(&md) {
            return Err(VaultError::SecretDetected(e.to_string()));
        }

        // 2. Garantir diretório `03 Recursos/Orion/Memória/`
        let memoria_dir = self.config.path
            .join("03 Recursos")
            .join("Orion")
            .join("Memória");

        std::fs::create_dir_all(&memoria_dir)?;

        let filename = note.file_name();
        let target_file = memoria_dir.join(&filename);
        let rel_path = PathBuf::from("03 Recursos")
            .join("Orion")
            .join("Memória")
            .join(&filename);

        // 3. Checar conflito com arquivo existente (F06.14)
        let mut warning = None;
        if target_file.exists() {
            let existing = std::fs::read_to_string(&target_file)?;
            if existing != md {
                info!("Atualizando nota de conversa existente com nova rodada: {:?}", target_file);
            }
        }

        // 4. Gravar arquivo no disco
        std::fs::write(&target_file, &md)?;

        // 5. Atualizar cache em memória
        let parsed = VaultParser::parse(&rel_path.to_string_lossy(), &md);
        self.notes_cache.write().insert(rel_path.to_string_lossy().to_string(), parsed);

        // 6. Commit e Push na main (F06.13)
        let mut commit_hash = None;
        let mut pushed = false;

        if let Some(git) = &self.git {
            let commit_msg = format!("Orion: {}", note.summary);
            match git.commit_file(&rel_path, &commit_msg) {
                Ok(_) => {
                    commit_hash = git.head_commit_hash().ok();
                    if self.config.auto_push {
                        match git.push(&self.config.branch) {
                            Ok(_) => pushed = true,
                            Err(e) => {
                                warning = Some(format!("Commit realizado com sucesso, mas push falhou: {}", e));
                            }
                        }
                    }
                }
                Err(e) => {
                    warning = Some(format!("Aviso: falha no git commit: {}", e));
                }
            }
        }

        Ok(SyncResult {
            file_path: rel_path.to_string_lossy().to_string(),
            commit_hash,
            pushed,
            warning,
        })
    }

    /// Faz uma indexação rápida de todas as notas `.md` do Vault.
    pub fn index_all(&self) -> Result<VaultStats, VaultError> {
        let mut cache = self.notes_cache.write();
        cache.clear();

        let mut total_notes = 0;
        let mut private_notes = 0;
        let mut archive_notes = 0;
        let mut total_links = 0;
        let mut all_tags = std::collections::HashSet::new();

        self.scan_dir(&self.config.path, &mut |rel_path, content| {
            let parsed = VaultParser::parse(&rel_path, &content);
            if parsed.metadata.is_private {
                private_notes += 1;
            }
            if parsed.metadata.is_archive {
                archive_notes += 1;
            }
            total_links += parsed.metadata.wikilinks.len();
            for t in &parsed.metadata.tags {
                all_tags.insert(t.clone());
            }
            total_notes += 1;
            cache.insert(rel_path, parsed);
        })?;

        let last_commit = self.git.as_ref().and_then(|g| g.head_commit_hash().ok());

        Ok(VaultStats {
            total_notes,
            private_notes,
            archive_notes,
            total_links,
            total_tags: all_tags.len(),
            last_commit,
        })
    }

    /// Busca textual rápida no cache de notas parseadas.
    pub fn search(&self, query: &str) -> Vec<ParsedNote> {
        let q = query.to_lowercase();
        let cache = self.notes_cache.read();

        let mut matches = Vec::new();
        for note in cache.values() {
            if note.metadata.title.to_lowercase().contains(&q)
                || note.metadata.tags.iter().any(|t| t.to_lowercase().contains(&q))
                || note.metadata.wikilinks.iter().any(|l| l.to_lowercase().contains(&q))
                || note.body_content.to_lowercase().contains(&q)
            {
                matches.push(note.clone());
            }
        }

        // Ordena: primeiro não-arquivados, depois por tamanho
        matches.sort_by(|a, b| {
            a.metadata.is_archive.cmp(&b.metadata.is_archive)
                .then(b.metadata.char_count.cmp(&a.metadata.char_count))
        });

        matches
    }

    fn scan_dir<F>(&self, root: &Path, callback: &mut F) -> Result<(), VaultError>
    where
        F: FnMut(String, String),
    {
        for entry in walk_md_files(root)? {
            if let Ok(rel) = entry.strip_prefix(root) {
                let rel_str = rel.to_string_lossy().replace('\\', "/");
                // Pular diretórios internos do git ou lixeira
                if rel_str.starts_with(".git") || rel_str.starts_with(".obsidian") {
                    continue;
                }
                if let Ok(content) = std::fs::read_to_string(&entry) {
                    callback(rel_str, content);
                }
            }
        }
        Ok(())
    }
}

fn walk_md_files(dir: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut files = Vec::new();
    if dir.is_dir() {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let p = entry.path();
            if p.is_dir() {
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !name.starts_with('.') {
                    files.extend(walk_md_files(&p)?);
                }
            } else if p.extension().and_then(|s| s.to_str()) == Some("md") {
                files.push(p);
            }
        }
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_vault_manager_save_and_scan() {
        let dir = tempdir().unwrap();
        let vault_path = dir.path().to_path_buf();

        let mgr = VaultManager::new(Some(vault_path.clone())).unwrap();

        let note = ConversationNote {
            date_str: "2026-10-17".to_string(),
            time_str: "03:50".to_string(),
            project_name: "Orion".to_string(),
            title: "Teste F06".to_string(),
            summary: "Resumo do teste de sincronização nativa.".to_string(),
            tags: vec!["orion".to_string(), "teste".to_string()],
            dialogue_markdown: "Olá\n\nOi".to_string(),
            decisions: vec![],
        };

        let res = mgr.save_conversation(&note).unwrap();
        assert!(vault_path.join(&res.file_path).exists());

        let stats = mgr.index_all().unwrap();
        assert_eq!(stats.total_notes, 1);

        let results = mgr.search("sincronização");
        assert_eq!(results.len(), 1);
    }
}
