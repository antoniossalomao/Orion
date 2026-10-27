//! Diário de sessão no vault (F06.07): depois de cada resposta, a conversa vira (ou atualiza) uma nota
//! em `03 Recursos/Orion/Memória/`. Opt-in (ORION_VAULT_DIARIO), determinístico (não passa pelo modelo),
//! sem commit nem push. REGRAS.md: você é o administrador; o Orion nunca sobrescreve o que você editou.

use orion_store::Store;
use orion_vault::{ConversationNote, VaultManager};
use parking_lot::Mutex;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const MAX_FALA: usize = 4_000;
const MAX_TOTAL: usize = 200_000;
/// Valor guardado no lugar do hash quando a nota foi editada por você: o diário para de atualizá-la.
const CONFLITO: &str = "conflito";

fn chave(sid: &str) -> String {
    format!("vault_diario:{sid}")
}

fn uma_linha(t: &str, max: usize) -> String {
    t.split_whitespace().collect::<Vec<_>>().join(" ").chars().filter(|c| !c.is_control()).take(max).collect()
}

/// Monta a nota a partir do que está no banco. `None` se ainda não há conversa para registrar.
fn montar(memory: &Arc<Store>, vault: &Path, sid: &str) -> anyhow::Result<Option<ConversationNote>> {
    let Some(sessao) = memory.get_session(sid)? else { return Ok(None) };
    let falas: Vec<_> = memory
        .history(sid, 1000, 0)?
        .into_iter()
        .filter(|m| (m.role == "user" || m.role == "assistant") && !m.text.starts_with("[CONTEXTO EXTERNO"))
        .collect();
    let Some(primeira) = falas.iter().find(|m| m.role == "user") else { return Ok(None) };
    let inicio = chrono::DateTime::from_timestamp(sessao.created_at as i64, 0).unwrap_or_else(chrono::Utc::now).with_timezone(&chrono::Local);

    let mut dialogo = String::new();
    for m in &falas {
        let quem = if m.role == "user" { "Antônio" } else { "Orion" };
        let texto: String = m.text.trim().chars().take(MAX_FALA).collect();
        let bloco = format!("**{quem}:**\n\n{texto}\n\n");
        if dialogo.len() + bloco.len() > MAX_TOTAL {
            dialogo.push_str("> Conversa truncada neste diário; o registro completo fica no Orion.\n");
            break;
        }
        dialogo.push_str(&bloco);
    }

    let titulo = sessao.title.as_deref().map(|t| uma_linha(t, 100)).filter(|t| !t.is_empty()).unwrap_or_else(|| uma_linha(&primeira.text, 60));
    let titulo = if titulo.is_empty() { "Conversa".to_string() } else { titulo };
    let resumo = uma_linha(&primeira.text, 200);

    // A nota precisa de um Projeto existente no vault; sem ele, cai no projeto Orion.
    let existe = |nome: &str| vault.join("01 Projetos").join(nome).join(format!("{nome}.md")).is_file();
    let projeto = sessao
        .project_id
        .as_deref()
        .and_then(|id| orion_core::projects::Projects::new(memory.clone()).get(id).ok())
        .and_then(|p| p["name"].as_str().map(str::to_owned))
        .filter(|n| existe(n))
        .unwrap_or_else(|| "Orion".into());

    Ok(Some(ConversationNote {
        date_str: inicio.format("%Y-%m-%d").to_string(),
        time_str: inicio.format("%H:%M").to_string(),
        project_name: projeto,
        title: titulo,
        summary: if resumo.is_empty() { "Conversa do Orion".into() } else { resumo },
        tags: vec!["orion".into(), "diario".into(), "conversa".into()],
        dialogue_markdown: dialogo,
        decisions: vec![],
    }))
}

fn salvar(memory: &Arc<Store>, vault: &Path, sid: &str) -> anyhow::Result<()> {
    let anterior = memory.meta_get(&chave(sid));
    if anterior.as_deref() == Some(CONFLITO) {
        return Ok(());
    }
    let Some(nota) = montar(memory, vault, sid)? else { return Ok(()) };
    let mgr = VaultManager::new(Some(vault.to_path_buf()))?.without_git();
    let res = mgr.save_diary(&nota, anterior.as_deref())?;
    if res.conflict.is_some() {
        tracing::warn!("diário do vault: a nota foi editada; versão nova ficou como proposta de conflito e o diário desta conversa parou");
        memory.meta_set(&chave(sid), Some(CONFLITO))?;
    } else {
        memory.meta_set(&chave(sid), Some(&res.content_sha256))?;
    }
    Ok(())
}

/// Gancho do agente: roda em segundo plano, uma conversa de cada vez; chamadas que chegam durante a
/// gravação são descartadas e a próxima resposta atualiza a nota por inteiro.
pub fn gancho(memory: Arc<Store>, vault: PathBuf) -> Arc<dyn Fn(&str) + Send + Sync> {
    let rodando = Arc::new(Mutex::new(HashSet::<String>::new()));
    Arc::new(move |sid: &str| {
        if !rodando.lock().insert(sid.to_string()) {
            return;
        }
        let (memory, vault, rodando, sid) = (memory.clone(), vault.clone(), rodando.clone(), sid.to_string());
        tokio::task::spawn_blocking(move || {
            if let Err(e) = salvar(&memory, &vault, &sid) {
                tracing::warn!("diário do vault adiado: {e}");
            }
            rodando.lock().remove(&sid);
        });
    })
}
