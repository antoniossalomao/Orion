use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// Configuração do Vault local do Obsidian.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultConfig {
    pub path: PathBuf,
    pub remote_url: Option<String>,
    pub branch: String,
    pub auto_pull: bool,
    pub auto_push: bool,
}

impl Default for VaultConfig {
    fn default() -> Self {
        Self {
            path: PathBuf::from(r"C:\Vault"),
            remote_url: None,
            branch: "main".to_string(),
            auto_pull: true,
            auto_push: true,
        }
    }
}

#[derive(Debug, Deserialize)]
struct ObsidianConfig {
    vaults: HashMap<String, ObsidianVaultEntry>,
}

#[derive(Debug, Deserialize)]
struct ObsidianVaultEntry {
    path: String,
    open: Option<bool>,
    ts: Option<u64>,
}

/// Detecta o caminho do Vault ativo do Obsidian no sistema operacional (F06.01).
///
/// 1. Tenta ler `%APPDATA%\obsidian\obsidian.json`.
/// 2. Procura pelo vault com `"open": true` ou com maior timestamp `ts`.
/// 3. Se não encontrar ou o arquivo não existir, checa `C:\Vault` como fallback padrão.
pub fn detect_vault_path() -> Option<PathBuf> {
    // 1. Verificar obsidian.json
    if let Some(appdata) = dirs::config_dir() {
        let obs_json = appdata.join("obsidian").join("obsidian.json");
        if obs_json.is_file() {
            if let Ok(content) = std::fs::read_to_string(&obs_json) {
                if let Ok(cfg) = serde_json::from_str::<ObsidianConfig>(&content) {
                    // Primeiro procura vault aberto
                    for entry in cfg.vaults.values() {
                        if entry.open == Some(true) {
                            let p = PathBuf::from(&entry.path);
                            if p.is_dir() {
                                info!("Vault ativo detectado via obsidian.json: {:?}", p);
                                return Some(p);
                            }
                        }
                    }
                    // Se nenhum estiver marcado explicitamente como open, pega o mais recente
                    let mut entries: Vec<_> = cfg.vaults.values().collect();
                    entries.sort_by_key(|e| e.ts.unwrap_or(0));
                    if let Some(latest) = entries.last() {
                        let p = PathBuf::from(&latest.path);
                        if p.is_dir() {
                            info!("Vault mais recente detectado via obsidian.json: {:?}", p);
                            return Some(p);
                        }
                    }
                }
            }
        }
    }

    // 2. Fallbacks de caminhos conhecidos
    let candidates = [
        Path::new(r"C:\Vault"),
        Path::new(r"D:\Vault"),
    ];

    for c in candidates {
        if c.is_dir() {
            info!("Vault detectado pelo caminho padrão: {:?}", c);
            return Some(c.to_path_buf());
        }
    }

    warn!("Nenhum Vault Obsidian detectado automaticamente.");
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_vault_path_or_default() {
        let path = detect_vault_path();
        if Path::new(r"C:\Vault").is_dir() {
            assert!(path.is_some());
            assert_eq!(path.unwrap(), PathBuf::from(r"C:\Vault"));
        }
    }
}
