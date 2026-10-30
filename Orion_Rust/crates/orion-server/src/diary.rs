//! Diário de sessão no vault (F06.07): depois de cada resposta, a conversa vira (ou atualiza) uma note
//! em `03 Recursos/Orion/Memória/`. Opt-in (ORION_VAULT_DIARIO), determinístico (não passa pelo modelo),
//! sem commit nem push. REGRAS.md: você é o administrador; o Orion nunca sobrescreve o que você editou.

use orion_store::Store;
use orion_vault::{ConversationNote, VaultManager};
use parking_lot::Mutex;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const MAX_TURN_CHARS: usize = 4_000;
const MAX_TOTAL_CHARS: usize = 200_000;
/// Valor guardado no lugar do hash quando a note foi editada por você: o diário para de atualizá-la.
const CONFLICT: &str = "conflito";

fn key(sid: &str) -> String {
    format!("vault_diario:{sid}")
}

fn one_line(t: &str, max: usize) -> String {
    t.split_whitespace().collect::<Vec<_>>().join(" ").chars().filter(|c| !c.is_control()).take(max).collect()
}

/// Monta a note a partir do que está no banco. `None` se ainda não há conversa para registrar.
fn build(memory: &Arc<Store>, vault: &Path, sid: &str) -> anyhow::Result<Option<ConversationNote>> {
    let Some(session) = memory.get_session(sid)? else { return Ok(None) };
    let turns: Vec<_> = memory
        .history(sid, 1000, 0)?
        .into_iter()
        .filter(|m| (m.role == "user" || m.role == "assistant") && !m.text.starts_with("[CONTEXTO EXTERNO"))
        .collect();
    let Some(first) = turns.iter().find(|m| m.role == "user") else { return Ok(None) };
    let started = chrono::DateTime::from_timestamp(session.created_at as i64, 0).unwrap_or_else(chrono::Utc::now).with_timezone(&chrono::Local);

    let mut dialogue = String::new();
    for m in &turns {
        let who = if m.role == "user" { "Antônio" } else { "Orion" };
        let text: String = m.text.trim().chars().take(MAX_TURN_CHARS).collect();
        let block = format!("**{who}:**\n\n{text}\n\n");
        if dialogue.len() + block.len() > MAX_TOTAL_CHARS {
            dialogue.push_str("> Conversa truncada neste diário; o registro completo fica no Orion.\n");
            break;
        }
        dialogue.push_str(&block);
    }

    let real_title = session.title.as_deref().map(|t| one_line(t, 100)).filter(|t| !t.is_empty()).unwrap_or_else(|| one_line(&first.text, 60));
    // O nome do arquivo sai da data e do título, então o título da nota é fixo e leva o começo do id da
    // sessão: renomear a conversa não cria outro arquivo, e duas conversas do mesmo dia não disputam um só.
    // O título editável vai no texto da nota.
    let title = format!("Conversa {}", sid.chars().take(8).collect::<String>());
    if !real_title.is_empty() {
        dialogue.insert_str(0, &format!("**Título da conversa:** {real_title}\n\n"));
    }
    let summary = one_line(&first.text, 200);

    // A note precisa de um Projeto existente no vault; sem ele, cai no project Orion.
    let exists = |nome: &str| vault.join("01 Projetos").join(nome).join(format!("{nome}.md")).is_file();
    let project = session
        .project_id
        .as_deref()
        .and_then(|id| orion_core::projects::Projects::new(memory.clone()).get(id).ok())
        .and_then(|p| p["name"].as_str().map(str::to_owned))
        .filter(|n| exists(n))
        .unwrap_or_else(|| "Orion".into());

    Ok(Some(ConversationNote {
        date_str: started.format("%Y-%m-%d").to_string(),
        time_str: started.format("%H:%M").to_string(),
        project_name: project,
        title: title,
        summary: if summary.is_empty() { "Conversa do Orion".into() } else { summary },
        tags: vec!["orion".into(), "diario".into(), "conversa".into()],
        dialogue_markdown: dialogue,
        decisions: vec![],
    }))
}

fn save(memory: &Arc<Store>, vault: &Path, sid: &str) -> anyhow::Result<()> {
    let previous = memory.meta_get(&key(sid));
    if previous.as_deref() == Some(CONFLICT) {
        return Ok(());
    }
    let Some(note) = build(memory, vault, sid)? else { return Ok(()) };
    let mgr = VaultManager::new(Some(vault.to_path_buf()))?.without_git();
    let result = mgr.save_diary(&note, previous.as_deref())?;
    if result.conflict.is_some() {
        tracing::warn!("diário do vault: a note foi editada; versão nova ficou como proposta de conflito e o diário desta conversa parou");
        memory.meta_set(&key(sid), Some(CONFLICT))?;
    } else {
        memory.meta_set(&key(sid), Some(&result.content_sha256))?;
        if !result.unchanged {
            record_change(&mgr, sid, &result.file_path);
        }
    }
    Ok(())
}

/// F06.18: `/nota` grava (ou atualiza) agora a nota desta conversa, por comando seu, mesmo com o
/// diário automático desligado. Mesmas proteções do diário: sem sobrescrever o que você editou.
/// Devolve o caminho da nota, ou `None` se ainda não há conversa para registrar.
pub fn save_now(memory: &Arc<Store>, vault: &Path, sid: &str) -> anyhow::Result<Option<String>> {
    anyhow::ensure!(memory.meta_get(&key(sid)).as_deref() != Some(CONFLICT), "a nota desta conversa foi editada no vault; resolva o conflito antes de gravar de novo");
    let Some(note) = build(memory, vault, sid)? else { return Ok(None) };
    let previous = memory.meta_get(&key(sid));
    let mgr = VaultManager::new(Some(vault.to_path_buf()))?.without_git();
    let result = mgr.save_diary(&note, previous.as_deref())?;
    if result.conflict.is_some() {
        memory.meta_set(&key(sid), Some(CONFLICT))?;
        anyhow::bail!("a nota foi editada no vault; a versão nova ficou como proposta de conflito");
    }
    memory.meta_set(&key(sid), Some(&result.content_sha256))?;
    if !result.unchanged {
        record_change(&mgr, sid, &result.file_path);
    }
    Ok(Some(result.file_path))
}

/// Mudanças feitas pelo Orion nesta execução: (conversa, hora, caminho da nota). Vira o relatório ao fechar.
static CHANGES: Mutex<Vec<(String, String, String)>> = Mutex::new(Vec::new());

/// F06.15: anota a mudança no relatório da sessão da conversa, em `03 Recursos/Relatórios de Mudanças`.
fn record_change(mgr: &VaultManager, sid: &str, path: &str) {
    let now = chrono::Local::now();
    let (date, time) = (now.format("%Y-%m-%d").to_string(), now.format("%H:%M").to_string());
    let short: String = sid.chars().filter(|c| c.is_alphanumeric()).take(8).collect();
    let name = format!("{date} - Orion - Mudanças {short}");
    let header = format!("---\ntype: relatorio\ndate: {date}\nhora: \"{time}\"\nagente: orion\nprojeto: \"[[Orion]]\"\ntags: [orion, relatorio, mudancas]\nresumo: Mudanças do Orion nesta conversa\n---\n\n# Mudanças do Orion — {date}\n\n");
    CHANGES.lock().push((sid.to_string(), time.clone(), path.to_string()));
    if let Err(e) = mgr.append_report(&name, &header, &format!("- {time} · `{path}`")) {
        tracing::warn!("relatório de mudanças do vault adiado: {e}");
    }
}

/// F06.15: ao fechar o Orion, grava o relatório da sessão (só se houve mudanças nesta execução).
pub fn session_report(vault: &Path) {
    let changes = std::mem::take(&mut *CHANGES.lock());
    if changes.is_empty() {
        return;
    }
    let now = chrono::Local::now();
    let (date, time) = (now.format("%Y-%m-%d").to_string(), now.format("%H:%M").to_string());
    let mut md = format!("---\ntype: relatorio\ndate: {date}\nhora: \"{time}\"\nagente: orion\nprojeto: \"[[Orion]]\"\ntags: [orion, relatorio, sessao]\nresumo: Sessão do Orion com {} nota(s) alterada(s)\n---\n\n# Sessão do Orion — {date} {time}\n\n## Notas alteradas\n", changes.len());
    for (sid, hour, path) in &changes {
        let short: String = sid.chars().filter(|c| c.is_alphanumeric()).take(8).collect();
        md.push_str(&format!("- {hour} · `{path}` (conversa {short})\n"));
    }
    match VaultManager::new(Some(vault.to_path_buf()))
        .and_then(|m| m.publish_report(&format!("{date} - Orion - Sessão {}", time.replace(':', "-")), &md))
    {
        Ok(path) => tracing::info!("relatório da sessão gravado em {path}"),
        Err(e) => tracing::warn!("relatório da sessão não gravado: {e}"),
    }
}

/// Gancho do agente: roda em segundo plano, uma conversa de cada vez; chamadas que chegam durante a
/// gravação são descartadas e a próxima resposta atualiza a note por inteiro.
pub fn hook(memory: Arc<Store>, vault: PathBuf, enabled: Arc<std::sync::atomic::AtomicBool>) -> Arc<dyn Fn(&str) + Send + Sync> {
    let running = Arc::new(Mutex::new(HashSet::<String>::new()));
    Arc::new(move |sid: &str| {
        if !enabled.load(std::sync::atomic::Ordering::Relaxed) {
            return;
        }
        if !running.lock().insert(sid.to_string()) {
            return;
        }
        let (memory, vault, running, sid) = (memory.clone(), vault.clone(), running.clone(), sid.to_string());
        tokio::task::spawn_blocking(move || {
            if let Err(e) = save(&memory, &vault, &sid) {
                tracing::warn!("diário do vault adiado: {e}");
            }
            running.lock().remove(&sid);
        });
    })
}
