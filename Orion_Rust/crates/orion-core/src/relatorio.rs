//! Relatório diário "Você fez" (F09.05): o que aconteceu no dia, em Markdown, só com o que o Orion já guarda.
//!
//! Determinístico (não passa pelo modelo) e local. O job das 23:59 guarda o texto; a rota o devolve e
//! você decide se exporta para o vault (REGRAS.md: resumo da conversa nunca substitui o registro).

use chrono::{Local, NaiveDate, TimeZone};
use orion_store::Store;
use serde_json::Value;

/// Chave do texto guardado pelo job do dia, por escopo.
pub fn chave(project_id: Option<&str>, date: NaiveDate) -> String {
    format!("voce_fez:{}:{date}", project_id.unwrap_or("personal"))
}

fn meia_noite(d: NaiveDate) -> anyhow::Result<f64> {
    let t = d.and_hms_opt(0, 0, 0).ok_or_else(|| anyhow::anyhow!("data inválida"))?;
    Ok(Local.from_local_datetime(&t).earliest().ok_or_else(|| anyhow::anyhow!("data inválida"))?.timestamp() as f64)
}

fn texto(v: &Value, k: &str) -> String {
    v[k].as_str().unwrap_or("").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Monta o relatório do dia para o escopo (`None` = pessoal). Vazio de atividade também gera texto.
pub fn voce_fez(memory: &Store, project_id: Option<&str>, date: NaiveDate) -> anyhow::Result<String> {
    let (ini, fim) = (meia_noite(date)?, meia_noite(date.succ_opt().ok_or_else(|| anyhow::anyhow!("data inválida"))?)?);
    let a = memory.daily_activity(project_id, ini, fim)?;
    let n = |k: &str| a[k].as_i64().unwrap_or(0);
    let mut md = format!("# Você fez · {date}\n\n");
    md += &format!(
        "- {} conversa(s) e {} mensagem(ns) suas\n- {} ação(ões) autorizada(s)\n- {} pedido(s) de aprovação\n- {} tarefa(s) concluída(s)\n- {} gravação(ões) de nota autorizada(s)\n",
        n("conversations"), n("messages"), n("actions"), n("approval_requests"), n("completed_tasks"), n("note_write_authorizations")
    );

    let conversas = memory.query(
        "SELECT s.title AS title, count(*) AS n FROM sessions s JOIN messages m ON m.session_id=s.id \
         WHERE s.project_id IS ? AND m.role='user' AND m.created_at>=? AND m.created_at<? GROUP BY s.id ORDER BY n DESC, s.created_at LIMIT 20",
        &[&project_id, &ini, &fim],
    )?;
    if !conversas.is_empty() {
        md += "\n## Conversas\n\n";
        for c in &conversas {
            let titulo = texto(&Value::Object(c.clone()), "title");
            md += &format!("- {} ({} mensagem(ns))\n", if titulo.is_empty() { "Sem título".into() } else { titulo }, c["n"].as_i64().unwrap_or(0));
        }
    }

    let acoes = memory.query(
        "SELECT a.tool AS tool, count(*) AS n FROM audit a JOIN sessions s ON s.id=a.session_id \
         WHERE s.project_id IS ? AND a.ts>=? AND a.ts<? AND a.action='allow' AND a.risk<>'read' GROUP BY a.tool ORDER BY n DESC, a.tool LIMIT 20",
        &[&project_id, &ini, &fim],
    )?;
    if !acoes.is_empty() {
        md += "\n## Ações autorizadas\n\n";
        for r in &acoes {
            md += &format!("- {}: {}\n", texto(&Value::Object(r.clone()), "tool"), r["n"].as_i64().unwrap_or(0));
        }
    }

    let tarefas = memory.query(
        "SELECT title FROM tasks WHERE project_id IS ? AND updated_at>=? AND updated_at<? AND status='concluida' ORDER BY updated_at LIMIT 30",
        &[&project_id, &ini, &fim],
    )?;
    if !tarefas.is_empty() {
        md += "\n## Tarefas concluídas\n\n";
        for r in &tarefas {
            md += &format!("- {}\n", texto(&Value::Object(r.clone()), "title"));
        }
    }
    Ok(md)
}
