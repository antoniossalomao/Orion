//! Briefing matinal: o que o Antônio tem para hoje, só com o que o Orion já guarda.
//!
//! Texto determinístico, sem modelo (REGRAS.md: captura rápida e briefing não passam pelo
//! modelo): roda sem gateway, não gasta cota e não tem como ser desviado por conteúdo externo.

use crate::util::local;
use chrono::{Datelike, Duration, Local, TimeZone};
use orion_store::ops::Operations;
use orion_store::Store;
use serde_json::Value;

const DIAS: [&str; 7] = ["segunda-feira", "terça-feira", "quarta-feira", "quinta-feira", "sexta-feira", "sábado", "domingo"];
/// Por seção; o resto vira "e mais N".
pub const MAX_LINHAS: usize = 8;

fn secao(titulo: &str, linhas: &[String]) -> String {
    if linhas.is_empty() {
        return String::new();
    }
    let mut corpo: Vec<String> = linhas.iter().take(MAX_LINHAS).cloned().collect();
    if linhas.len() > MAX_LINHAS {
        corpo.push(format!("… e mais {}", linhas.len() - MAX_LINHAS));
    }
    format!("{titulo}\n{}", corpo.iter().map(|l| format!("• {l}")).collect::<Vec<_>>().join("\n"))
}

fn inicio_do_dia(agora: f64) -> f64 {
    let d = local(agora).date_naive().and_hms_opt(0, 0, 0).unwrap();
    Local.from_local_datetime(&d).earliest().map(|x| x.timestamp() as f64).unwrap_or(agora)
}

fn f(r: &Value, k: &str) -> f64 {
    r[k].as_f64().unwrap_or(0.0)
}

fn s(r: &Value, k: &str) -> String {
    r[k].as_str().unwrap_or("").to_string()
}

pub type Agenda = dyn Fn(f64) -> Option<String> + Send + Sync;

pub fn build_briefing(ops: &Operations, agora: f64, nome: &str, agenda: Option<&Agenda>) -> String {
    let dt = local(agora);
    let inicio = inicio_do_dia(agora);
    let fim = proximo_dia(agora);
    let lembretes: Vec<Value> = ops
        .list_reminders(false)
        .unwrap_or_default()
        .into_iter()
        .map(Value::Object)
        .filter(|r| f(r, "due_at") < fim)
        .collect();
    let atrasados: Vec<String> = lembretes
        .iter()
        .filter(|r| f(r, "due_at") < inicio)
        .map(|r| format!("{} — {}", local(f(r, "due_at")).format("%d/%m %H:%M"), s(r, "title")))
        .collect();
    let de_hoje: Vec<String> = lembretes
        .iter()
        .filter(|r| f(r, "due_at") >= inicio)
        .map(|r| format!("{} — {}", local(f(r, "due_at")).format("%H:%M"), s(r, "title")))
        .collect();
    let agendados: Vec<String> = ops
        .list_schedules()
        .unwrap_or_default()
        .into_iter()
        .map(Value::Object)
        .filter(|x| x["active"].as_i64().unwrap_or(0) != 0 && x["next_run"].as_f64().is_some_and(|n| agora <= n && n < fim))
        .map(|x| format!("{} — {}", local(f(&x, "next_run")).format("%H:%M"), s(&x, "title")))
        .collect();
    let mut tarefas: Vec<Value> = ops.list_tasks(None).unwrap_or_default().into_iter().map(Value::Object).filter(|t| s(t, "status") != "concluida").collect();
    // o que está em andamento vem primeiro (ordenação estável)
    tarefas.sort_by_key(|t| s(t, "status") != "em_andamento");
    let linhas_tarefas: Vec<String> = tarefas
        .iter()
        .map(|t| format!("{}{}", s(t, "title"), if s(t, "status") == "em_andamento" { " (em andamento)" } else { "" }))
        .collect();
    let de_agenda = agenda.and_then(|a| a(agora));
    let partes = [
        format!("☀️ Bom dia, {nome}. Hoje é {}, {}.", DIAS[dt.weekday().num_days_from_monday() as usize], dt.format("%d/%m/%Y")),
        de_agenda.map(|a| format!("📅 Agenda de hoje (Google)\n{a}")).unwrap_or_default(),
        secao("⏰ Atrasados", &atrasados),
        secao("🔔 Lembretes de hoje", &de_hoje),
        secao("🗓️ Agendamentos de hoje", &agendados),
        secao("✅ Tarefas em aberto", &linhas_tarefas),
    ];
    let mut corpo: Vec<String> = partes.into_iter().filter(|p| !p.is_empty()).collect();
    if corpo.len() == 1 {
        corpo.push("Nada pendente por aqui.".into());
    }
    corpo.join("\n\n")
}

/// Resumo dos últimos 7 dias, só com contagens do que o Orion guarda (sem modelo).
pub fn build_weekly(memory: &Store, agora: f64, nome: &str) -> String {
    let desde = agora - 7.0 * 86400.0;
    let q = |sql: &str| -> Vec<String> {
        memory
            .query(sql, &[&desde])
            .unwrap_or_default()
            .iter()
            .filter_map(|r| r.values().next().and_then(|v| v.as_str().map(String::from)))
            .collect()
    };
    let n = |sql: &str, com_desde: bool| -> i64 {
        let p: Vec<&dyn orion_store::rusqlite::ToSql> = if com_desde { vec![&desde] } else { vec![] };
        memory.query(sql, &p).ok().and_then(|r| r.first().and_then(|x| x.values().next().and_then(Value::as_i64))).unwrap_or(0)
    };
    let concluidas = q("SELECT title FROM tasks WHERE status='concluida' AND updated_at>=? ORDER BY updated_at DESC");
    let criadas = n("SELECT COUNT(*) FROM tasks WHERE created_at>=?", true);
    let abertas = n("SELECT COUNT(*) FROM tasks WHERE status!='concluida'", false);
    let fatos = q("SELECT text FROM facts WHERE created_at>=? ORDER BY created_at DESC");
    let pedidos = n("SELECT COUNT(*) FROM messages WHERE role='user' AND created_at>=?", true);
    let conversas = n("SELECT COUNT(DISTINCT session_id) FROM messages WHERE role='user' AND created_at>=?", true);
    let para_ler: Vec<String> = memory
        .query("SELECT title, url FROM read_later WHERE status IN ('pendente','erro') ORDER BY created_at LIMIT 10", &[])
        .unwrap_or_default()
        .iter()
        .map(|r| r["title"].as_str().filter(|t| !t.is_empty()).or(r["url"].as_str()).unwrap_or("").to_string())
        .collect();
    let partes = [
        format!("🗓️ Semana de {} a {}, {nome}.", local(desde).format("%d/%m"), local(agora).format("%d/%m")),
        format!("Você conversou {pedidos} vez(es) com o Orion, em {conversas} conversa(s)."),
        format!("Tarefas: {} concluída(s), {criadas} criada(s), {abertas} em aberto.", concluidas.len()),
        secao("✅ Concluídas", &concluidas),
        secao("🧠 Fatos novos na memória", &fatos),
        secao("📚 Para ler (ler depois)", &para_ler),
    ];
    partes.into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join("\n\n")
}

pub fn proximo_dia(agora: f64) -> f64 {
    let d = local(agora).date_naive() + Duration::days(1);
    Local.from_local_datetime(&d.and_hms_opt(0, 0, 0).unwrap()).earliest().map(|x| x.timestamp() as f64).unwrap_or(agora + 86400.0)
}
