//! Sincronização do vault com o GitHub feita pelo próprio Orion: ao abrir, a cada 15 minutos, antes
//! de gravar uma nota e ao fechar. Pull, commit do que mudou e push, sempre sem `--force` e com o
//! login do Git do computador (REGRAS.md: o vault é sincronizado pelo Orion, sem sobrescrever nada).
//! O resultado da última rodada fica em `vault:sync:last` no banco; falha nunca impede o Orion de subir.

use orion_store::Store;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub const EVERY: Duration = Duration::from_secs(15 * 60);
const LAST: &str = "vault:sync:last";

/// Uma rodada. Devolve a frase para mostrar (sucesso ou motivo da falha).
pub fn run_blocking(vault: PathBuf, memory: &Store) -> Result<String, String> {
    let outcome = orion_vault::VaultManager::new(Some(vault)).map_err(|e| e.to_string()).and_then(|m| match m.sync_all() {
        Ok(Some(r)) => Ok(summary(&r)),
        Ok(None) => Err("o vault não é um repositório Git".to_string()),
        Err(e) => Err(e.to_string()),
    });
    let at = orion_core::util::agora();
    let (ok, message) = match &outcome {
        Ok(m) => (true, m.clone()),
        Err(m) => (false, m.clone()),
    };
    let _ = memory.meta_set(LAST, Some(&serde_json::json!({"at": at, "ok": ok, "message": message}).to_string()));
    outcome
}

pub fn summary(r: &orion_vault::FullSyncReport) -> String {
    let mut parts = vec![];
    if r.committed > 0 {
        parts.push(format!("{} arquivo(s) enviado(s)", r.committed));
    }
    if r.pulled {
        parts.push("novidades do GitHub trazidas".to_string());
    }
    if parts.is_empty() {
        parts.push("já estava em dia".to_string());
    }
    if !r.held_back.is_empty() {
        parts.push(format!("{} arquivo(s) ficaram só locais por conter segredo ou serem grandes demais: {}", r.held_back.len(), r.held_back.iter().take(5).cloned().collect::<Vec<_>>().join(", ")));
    }
    format!("Vault sincronizado: {}", parts.join("; "))
}

/// Ao abrir e depois a cada 15 minutos, enquanto a opção estiver ligada e o pânico desligado.
pub fn spawn_loop(vault: PathBuf, memory: Arc<Store>, enabled: Arc<AtomicBool>, modos: Arc<orion_core::modos::Modos>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            if enabled.load(Ordering::Relaxed) && !modos.panico() {
                let (v, m) = (vault.clone(), memory.clone());
                match tokio::task::spawn_blocking(move || run_blocking(v, &m)).await {
                    Ok(Ok(msg)) => tracing::info!("{msg}"),
                    Ok(Err(e)) => tracing::warn!("vault: sincronização falhou: {e}"),
                    Err(_) => {}
                }
            }
            tokio::time::sleep(EVERY).await;
        }
    })
}
