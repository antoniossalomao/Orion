//! Resumo automático de toda conversa (F06.11): quando uma conversa pára por um tempo, o modelo local resume
//! em segundo plano e a nota da conversa em `03 Recursos/Orion/Memória/` ganha o resumo. REGRAS.md: o resumo só
//! nasce no computador (modelo local) e nunca substitui o registro. Conversa privada fica de fora; sem modelo
//! local ou com o diário desligado, nada roda e a nota segue com a primeira frase como resumo.

use orion_core::memory::compact::Compactor;
use orion_store::Store;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Quanto tempo a conversa precisa estar parada para ser resumida.
const OCIOSA_S: f64 = 20.0 * 60.0;
const A_CADA: Duration = Duration::from_secs(5 * 60);

/// Uma só rodada de resumos por abertura do núcleo: recriar o núcleo (reconfigurar) derruba a anterior.
static TAREFA: Mutex<Option<tokio::task::JoinHandle<()>>> = Mutex::new(None);

pub fn spawn_loop(memory: Arc<Store>, compactor: Arc<Compactor>, vault: PathBuf, imagens: PathBuf, diario_ligado: Arc<AtomicBool>, modos: Arc<orion_core::modos::Modos>) {
    let h = tokio::spawn(async move {
        loop {
            tokio::time::sleep(A_CADA).await;
            if !diario_ligado.load(Ordering::Relaxed) || modos.panico() {
                continue;
            }
            // uma conversa por vez: o modelo local é um processo pesado
            let Some(sid) = compactor.pendentes_finais(OCIOSA_S).into_iter().next() else { continue };
            match compactor.resumir_conversa(&sid).await {
                Ok(true) => {
                    let (m, v, i, s) = (memory.clone(), vault.clone(), imagens.clone(), sid.clone());
                    let r = tokio::task::spawn_blocking(move || crate::diary::atualizar(&m, &v, &i, &s)).await;
                    if let Ok(Err(e)) = r {
                        tracing::warn!("resumo da conversa pronto, nota adiada: {e}");
                    }
                }
                Ok(false) => {
                    // nada a resumir de verdade (resumo inválido): não tenta de novo até chegar fala nova
                    tracing::debug!("resumo final não gravado para uma conversa");
                }
                Err(e) => tracing::warn!("resumo final adiado: {e}"),
            }
        }
    });
    if let Ok(mut g) = TAREFA.lock() {
        if let Some(antiga) = g.replace(h) {
            antiga.abort();
        }
    }
}
