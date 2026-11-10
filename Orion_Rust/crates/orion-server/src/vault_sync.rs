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

/// Pastas bloqueadas (já normalizadas), definidas na abertura do núcleo. Os três caminhos que sincronizam
/// (rodada periódica, "Sincronizar agora" e fechamento) passam por `run_blocking`.
static BLOQUEADAS: std::sync::RwLock<Vec<PathBuf>> = std::sync::RwLock::new(Vec::new());

pub fn definir_bloqueadas(pastas: Vec<PathBuf>) {
    if let Ok(mut g) = BLOQUEADAS.write() {
        *g = pastas;
    }
}

/// F02.23: com pasta bloqueada dentro do vault, não sincroniza nada. O Git lê e envia os arquivos que
/// mudaram, e o bloqueio promete que o Orion nunca lê nem escreve naquela pasta.
fn bloqueada_no_vault(vault: &std::path::Path) -> bool {
    let raiz = std::fs::canonicalize(vault).unwrap_or_else(|_| vault.to_path_buf());
    // a pasta bloqueada dentro do vault, ou o próprio vault dentro de uma pasta bloqueada
    BLOQUEADAS
        .read()
        .map(|g| g.iter().any(|b| {
            let b = std::fs::canonicalize(b).unwrap_or_else(|_| b.clone());
            b.starts_with(&raiz) || raiz.starts_with(&b)
        }))
        .unwrap_or(false)
}

/// Uma rodada. Devolve a frase para mostrar (sucesso ou motivo da falha).
pub fn run_blocking(vault: PathBuf, memory: &std::sync::Arc<Store>) -> Result<String, String> {
    if bloqueada_no_vault(&vault) {
        // não conta como falha do Git: é uma escolha sua. A mensagem diz o que fazer.
        let message = "Sincronização do vault suspensa: há uma pasta bloqueada dentro dele (ou ele está numa pasta bloqueada). Tire o bloqueio para sincronizar.".to_string();
        let _ = memory.meta_set(LAST, Some(&serde_json::json!({"at": orion_core::util::agora(), "ok": false, "message": message}).to_string()));
        return Err(message);
    }
    let pasta = vault.clone();
    let outcome = orion_vault::VaultManager::new(Some(vault)).map_err(|e| e.to_string()).and_then(|m| match m.sync_all() {
        Ok(Some(r)) => Ok(summary(&r)),
        Ok(None) => Err("o vault não é um repositório Git".to_string()),
        Err(e) => Err(e.to_string()),
    });
    // F06.16: confere o que agentes externos escreveram (e o que veio do GitHub) desde a rodada anterior
    let outcome = outcome.map(|msg| {
        let r = crate::vault_auditoria::auditar(&pasta, memory);
        if r.conferidas > 0 {
            format!("{msg}; {} nota(s) de agentes externos conferida(s), {} com problema", r.conferidas, r.com_problema)
        } else {
            msg
        }
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
