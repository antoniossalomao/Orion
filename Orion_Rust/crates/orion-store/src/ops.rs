//! Operação do dia a dia sobre o mesmo banco da memória: lembretes, agendamentos, tarefas,
//! "números", prompts salvos, arestas do grafo, fila de avisos, vigilância de pastas e audit.
//!
//! Tudo parametrizado; datas em segundos desde a época (o texto ISO, quando vem de fora, é lido
//! como hora local).

use crate::scope::current;
use crate::{invalid, query_maps, Result, Store, StoreError};
use chrono::{Local, NaiveDate, NaiveDateTime, TimeZone};
use regex::Regex;
use rusqlite::types::Value as Sql;
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

pub const TIPOS_AGENDAMENTO: [&str; 3] = ["unico", "diario", "intervalo_min"];
pub const STATUS_TAREFA: [&str; 3] = ["pendente", "em_andamento", "concluida"];
pub const MAX_VIGILANCIAS: usize = 20;
pub const MAX_NOMES_VIGIADOS: usize = 5000;
/// Download ainda em curso.
const PARCIAL: [&str; 5] = [".crdownload", ".part", ".tmp", ".download", ".partial"];

pub type Row = Map<String, Value>;

static ORDER_LIMIT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(ORDER BY|LIMIT)\b").unwrap());

/// Valor de data vindo de fora: ISO ou número.
pub enum When {
    Epoch(f64),
    Text(String),
}

impl From<f64> for When {
    fn from(v: f64) -> Self {
        When::Epoch(v)
    }
}
impl From<&str> for When {
    fn from(v: &str) -> Self {
        When::Text(v.into())
    }
}

/// ISO (`2026-06-25T14:30:00`) ou epoch → epoch. ISO sem fuso é hora local.
pub fn parse_when(valor: &When) -> Result<f64> {
    let t = match valor {
        When::Epoch(v) => return Ok(*v),
        When::Text(t) => t.trim(),
    };
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(t) {
        return Ok(dt.timestamp() as f64 + dt.timestamp_subsec_micros() as f64 / 1e6);
    }
    for f in ["%Y-%m-%dT%H:%M:%S%.f%:z", "%Y-%m-%d %H:%M:%S%.f%:z", "%Y-%m-%dT%H:%M%:z", "%Y-%m-%d %H:%M%:z"] {
        if let Ok(dt) = chrono::DateTime::parse_from_str(t, f) {
            return Ok(dt.timestamp() as f64);
        }
    }
    for f in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%dT%H:%M", "%Y-%m-%d %H:%M", "%Y-%m-%dT%H"] {
        if let Ok(n) = NaiveDateTime::parse_from_str(t, f) {
            return local_ts(n);
        }
    }
    if let Ok(d) = NaiveDate::parse_from_str(t, "%Y-%m-%d") {
        return local_ts(d.and_hms_opt(0, 0, 0).unwrap());
    }
    invalid(format!("data/hora inválida: '{t}' (use ISO, ex.: 2026-06-25T14:30:00)"))
}

fn local_ts(n: NaiveDateTime) -> Result<f64> {
    Local
        .from_local_datetime(&n)
        .earliest()
        .map(|d| d.timestamp() as f64 + d.timestamp_subsec_micros() as f64 / 1e6)
        .ok_or_else(|| StoreError::Invalid("hora local inexistente".into()))
}

/// Próximo disparo de um agendamento, a partir de `base` (mesma regra do legado).
pub fn next_run(kind: &str, base: f64, when: Option<f64>, time_of_day: &str, interval_min: i64) -> Result<f64> {
    match kind {
        "unico" => when.ok_or_else(|| StoreError::Invalid("tipo 'unico' exige 'quando' (ISO)".into())),
        "diario" => {
            let erro = || StoreError::Invalid("tipo 'diario' exige 'horario' no formato HH:MM".into());
            let (h, m) = time_of_day.split_once(':').ok_or_else(erro)?;
            let (h, m): (u32, u32) = (h.trim().parse().map_err(|_| erro())?, m.trim().parse().map_err(|_| erro())?);
            let dia = chrono::DateTime::from_timestamp(base as i64, 0).ok_or_else(erro)?.with_timezone(&Local).date_naive();
            let prox = dia.and_hms_opt(h, m, 0).ok_or_else(erro)?;
            let mut ts = local_ts(prox)?;
            if ts <= base {
                ts = local_ts(prox + chrono::Duration::days(1))?;
            }
            Ok(ts)
        }
        "intervalo_min" => {
            if interval_min <= 0 {
                return invalid("tipo 'intervalo_min' exige 'intervalo_min' > 0");
            }
            Ok(base + (interval_min * 60) as f64)
        }
        _ => invalid(format!("tipo inválido: {kind}. Use: {}", TIPOS_AGENDAMENTO.join(", "))),
    }
}

/// Prévia das próximas `n` execuções de um agendamento, em `DD/MM HH:MM` local (F12.01).
pub fn schedule_preview(sc: &Row, n: usize) -> Vec<String> {
    let fmt = |ts: f64| chrono::DateTime::from_timestamp(ts as i64, 0).map(|d| d.with_timezone(&Local).format("%d/%m %H:%M").to_string()).unwrap_or_default();
    let Some(mut prox) = sc.get("next_run").and_then(Value::as_f64) else { return vec![] };
    let kind = s(sc, "kind");
    let mut v = vec![fmt(prox)];
    while kind != "unico" && v.len() < n {
        let Ok(seguinte) = next_run(kind, prox, None, s(sc, "time_of_day"), i(sc, "interval_min")) else { break };
        prox = seguinte;
        v.push(fmt(prox));
    }
    v
}

fn t(x: &str) -> Sql {
    Sql::Text(x.to_string())
}

fn nome(tabela: &str) -> &str {
    match tabela {
        "reminders" => "lembrete",
        "schedules" => "agendamento",
        "tasks" => "tarefa",
        "numbers" => "número",
        "prompts" => "prompt",
        t => t,
    }
}

/// Nomes dos arquivos da pasta (sem os parciais de download), até um teto.
fn nomes_da_pasta(pasta: &Path) -> Vec<String> {
    let mut nomes = vec![];
    if let Ok(d) = std::fs::read_dir(pasta) {
        for e in d.flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            if !PARCIAL.iter().any(|p| n.to_lowercase().ends_with(p)) {
                nomes.push(n);
                if nomes.len() >= MAX_NOMES_VIGIADOS {
                    break;
                }
            }
        }
    }
    nomes.sort();
    nomes
}

fn resolve(p: &str) -> PathBuf {
    orion_policy::paths::realpath(&PathBuf::from(orion_policy::paths::norm(p)))
}

fn i(r: &Row, k: &str) -> i64 {
    r.get(k).and_then(Value::as_i64).unwrap_or(0)
}
fn s<'a>(r: &'a Row, k: &str) -> &'a str {
    r.get(k).and_then(Value::as_str).unwrap_or("")
}

pub struct Operations {
    s: Arc<Store>,
    /// Não perturbe (REGRAS.md: pânico é corte; não perturbe só segura): segura os não urgentes.
    pub segurar: parking_lot::RwLock<Arc<dyn Fn() -> bool + Send + Sync>>,
}

impl Operations {
    pub fn new(store: Arc<Store>) -> Self {
        Operations { s: store, segurar: parking_lot::RwLock::new(Arc::new(|| false)) }
    }

    pub fn store(&self) -> &Arc<Store> {
        &self.s
    }

    pub fn now(&self) -> f64 {
        self.s.now()
    }

    /// Consulta restrita ao projeto atual (o que é de um projeto fica no projeto).
    fn query(&self, sql: &str, params: Vec<Sql>) -> Result<Vec<Row>> {
        let c = self.s.conn();
        if sql.contains(" FROM edges") {
            let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
            return query_maps(&c, sql, &refs);
        }
        let cut = ORDER_LIMIT.find(sql).map(|m| m.start()).unwrap_or(sql.len());
        let (prefixo, sufixo) = sql.split_at(cut);
        let junta = if prefixo.contains(" WHERE ") { " AND " } else { " WHERE " };
        let antes = prefixo.matches('?').count();
        let mut p = params;
        p.insert(antes, current().project_id.map(Sql::Text).unwrap_or(Sql::Null));
        let refs: Vec<&dyn rusqlite::ToSql> = p.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
        query_maps(&c, &format!("{prefixo}{junta}project_id IS ? {sufixo}"), &refs)
    }

    fn one(&self, tabela: &str, rid: i64) -> Result<Row> {
        self.query(&format!("SELECT * FROM {tabela} WHERE id=?"), vec![rid.into()])?
            .into_iter()
            .next()
            .ok_or_else(|| StoreError::NotFound(format!("{} {rid} não existe", nome(tabela))))
    }

    fn update(&self, tabela: &str, rid: i64, sets: &str, mut params: Vec<Sql>) -> Result<()> {
        params.push(rid.into());
        params.push(current().project_id.map(Sql::Text).unwrap_or(Sql::Null));
        let n = self.s.tx(|c| {
            Ok(c.execute(
                &format!("UPDATE {tabela} SET {sets} WHERE id=? AND project_id IS ?"),
                rusqlite::params_from_iter(params),
            )?)
        })?;
        if n == 0 {
            return Err(StoreError::NotFound(format!("{} {rid} não existe", nome(tabela))));
        }
        Ok(())
    }

    fn delete(&self, tabela: &str, rid: i64) -> Result<bool> {
        let p = current().project_id;
        self.s.tx(|c| {
            Ok(c.execute(&format!("DELETE FROM {tabela} WHERE id=? AND project_id IS ?"), rusqlite::params![rid, p])? > 0)
        })
    }

    fn insert(&self, sql: &str, params: Vec<Sql>) -> Result<i64> {
        self.s.tx(|c| {
            c.execute(sql, rusqlite::params_from_iter(params))?;
            Ok(c.last_insert_rowid())
        })
    }

    fn projeto() -> Sql {
        current().project_id.map(Sql::Text).unwrap_or(Sql::Null)
    }

    // ── lembretes ─────────────────────────────────────────────────────────
    /// `insist_min` (E9.3): 0 avisa uma vez; de 5 a 1440, reavisa a cada N minutos até ficar
    /// pronto.
    pub fn add_reminder(&self, title: &str, due_at: &When, note: &str, insist_min: i64) -> Result<Row> {
        let titulo = title.trim();
        if titulo.is_empty() {
            return invalid("informe o título do lembrete");
        }
        if insist_min != 0 && !(5..=1440).contains(&insist_min) {
            return invalid("insistir de 5 a 1440 minutos (0 avisa uma vez só)");
        }
        let id = self.insert(
            "INSERT INTO reminders(title, due_at, note, created_at, project_id, insist_min) VALUES (?,?,?,?,?,?)",
            vec![t(titulo), parse_when(due_at)?.into(), t(note), self.now().into(), Self::projeto(), insist_min.into()],
        )?;
        self.one("reminders", id)
    }

    /// Lembrete com tipo (E10: cápsula do tempo usa `kind='capsula'`).
    pub fn add_reminder_kind(&self, title: &str, due_at: f64, note: &str, kind: &str) -> Result<Row> {
        let id = self.insert(
            "INSERT INTO reminders(title, due_at, note, created_at, project_id, kind) VALUES (?,?,?,?,?,?)",
            vec![t(title.trim()), due_at.into(), t(note), self.now().into(), Self::projeto(), t(kind)],
        )?;
        self.one("reminders", id)
    }

    pub fn list_reminders(&self, include_done: bool) -> Result<Vec<Row>> {
        self.query("SELECT * FROM reminders WHERE (? OR done=0) ORDER BY due_at, id", vec![(include_done as i64).into()])
    }

    /// Vencidos, não concluídos e ainda não avisados.
    pub fn due_reminders(&self, now: Option<f64>) -> Result<Vec<Row>> {
        self.query(
            "SELECT * FROM reminders WHERE done=0 AND notified=0 AND due_at<=? ORDER BY due_at",
            vec![now.unwrap_or_else(|| self.now()).into()],
        )
    }

    /// Insistentes já avisados, ainda não feitos, cujo intervalo passou desde o último aviso.
    pub fn due_nudges(&self, now: Option<f64>) -> Result<Vec<Row>> {
        self.query(
            "SELECT * FROM reminders WHERE done=0 AND notified=1 AND insist_min>0
             AND COALESCE(last_nudge, due_at) + insist_min*60 <= ? ORDER BY due_at",
            vec![now.unwrap_or_else(|| self.now()).into()],
        )
    }

    pub fn mark_reminder_nudged(&self, id: i64, now: Option<f64>) -> Result<()> {
        self.update("reminders", id, "last_nudge=?", vec![now.unwrap_or_else(|| self.now()).into()])
    }

    /// Adiar (E9.3): volta a ser um lembrete novo daqui a `minutes`.
    pub fn snooze_reminder(&self, id: i64, minutes: i64) -> Result<Row> {
        if !(1..=1440).contains(&minutes) {
            return invalid("adiar de 1 a 1440 minutos");
        }
        self.update(
            "reminders",
            id,
            "due_at=?, notified=0, last_nudge=NULL",
            vec![(self.now() + (minutes * 60) as f64).into()],
        )?;
        self.one("reminders", id)
    }

    pub fn complete_reminder(&self, id: i64) -> Result<Row> {
        self.update("reminders", id, "done=1", vec![])?;
        self.one("reminders", id)
    }

    pub fn mark_reminder_notified(&self, id: i64) -> Result<()> {
        self.update("reminders", id, "notified=1", vec![])
    }

    pub fn remove_reminder(&self, id: i64) -> Result<bool> {
        self.delete("reminders", id)
    }

    // ── agendamentos recorrentes ──────────────────────────────────────────
    #[allow(clippy::too_many_arguments)]
    pub fn add_schedule(
        &self,
        title: &str,
        kind: &str,
        when: Option<&When>,
        time_of_day: &str,
        interval_min: i64,
        tool: &str,
        params: Option<&Value>,
    ) -> Result<Row> {
        let titulo = title.trim();
        if titulo.is_empty() {
            return invalid("informe o título do agendamento");
        }
        let agora = self.now();
        let quando = match when {
            Some(When::Text(t)) if t.is_empty() => None,
            Some(w) => Some(parse_when(w)?),
            None => None,
        };
        let prox = next_run(kind, agora, quando, time_of_day, interval_min)?;
        let params = orion_policy::pyjson::dumps(params.unwrap_or(&json!({})), false);
        let id = self.insert(
            "INSERT INTO schedules(title, kind, time_of_day, interval_min, tool, params, next_run, created_at, project_id)
             VALUES (?,?,?,?,?,?,?,?,?)",
            vec![
                t(titulo),
                t(kind),
                t(time_of_day),
                interval_min.into(),
                t(tool),
                params.into(),
                prox.into(),
                agora.into(),
                Self::projeto(),
            ],
        )?;
        self.one("schedules", id)
    }

    pub fn list_schedules(&self) -> Result<Vec<Row>> {
        self.query("SELECT * FROM schedules ORDER BY next_run IS NULL, next_run", vec![])
    }

    pub fn due_schedules(&self, now: Option<f64>) -> Result<Vec<Row>> {
        self.query(
            "SELECT * FROM schedules WHERE active=1 AND next_run IS NOT NULL AND next_run<=? ORDER BY next_run",
            vec![now.unwrap_or_else(|| self.now()).into()],
        )
    }

    /// F12.04: próximo horário em que algo precisa rodar (agendamento ativo ou lembrete pendente),
    /// depois de `depois`, em todos os projetos. Serve para acordar o PC suspenso.
    pub fn proximo_despertar(&self, depois: f64) -> Result<Option<f64>> {
        let c = self.s.conn();
        let rows = query_maps(
            &c,
            "SELECT MIN(t) AS t FROM (SELECT next_run AS t FROM schedules WHERE active=1 AND next_run>?1
             UNION ALL SELECT due_at AS t FROM reminders WHERE done=0 AND notified=0 AND due_at>?1)",
            &[&depois as &dyn rusqlite::ToSql],
        )?;
        Ok(rows.first().and_then(|r| r.get("t")).and_then(Value::as_f64))
    }

    /// Registra o disparo e calcula o próximo (um atraso longo dispara uma vez só).
    pub fn mark_schedule_fired(&self, id: i64, now: Option<f64>) -> Result<Row> {
        let agora = now.unwrap_or_else(|| self.now());
        let sc = self.one("schedules", id)?;
        if s(&sc, "kind") == "unico" {
            self.update("schedules", id, "last_run=?, next_run=NULL, active=0", vec![agora.into()])?;
        } else {
            let prox = next_run(s(&sc, "kind"), agora, None, s(&sc, "time_of_day"), i(&sc, "interval_min"))?;
            self.update("schedules", id, "last_run=?, next_run=?", vec![agora.into(), prox.into()])?;
        }
        self.one("schedules", id)
    }

    /// Pausar não perde o horário; retomar recalcula o próximo a partir de agora.
    pub fn set_schedule_active(&self, id: i64, active: bool) -> Result<Row> {
        let sc = self.one("schedules", id)?;
        if active && s(&sc, "kind") != "unico" {
            let prox = next_run(s(&sc, "kind"), self.now(), None, s(&sc, "time_of_day"), i(&sc, "interval_min"))?;
            self.update("schedules", id, "active=1, next_run=?", vec![prox.into()])?;
        } else {
            self.update("schedules", id, "active=?", vec![(active as i64).into()])?;
        }
        self.one("schedules", id)
    }

    pub fn remove_schedule(&self, id: i64) -> Result<bool> {
        self.delete("schedules", id)
    }

    // ── tarefas ───────────────────────────────────────────────────────────
    pub fn add_task(&self, title: &str) -> Result<Row> {
        let titulo = title.trim();
        if titulo.is_empty() {
            return invalid("informe o título da tarefa");
        }
        let agora = self.now();
        let id = self.insert(
            "INSERT INTO tasks(title, created_at, updated_at, project_id) VALUES (?,?,?,?)",
            vec![t(titulo), agora.into(), agora.into(), Self::projeto()],
        )?;
        self.one("tasks", id)
    }

    pub fn list_tasks(&self, status: Option<&str>) -> Result<Vec<Row>> {
        let st: Sql = status.map(|x| Sql::Text(x.into())).unwrap_or(Sql::Null);
        self.query("SELECT * FROM tasks WHERE (? IS NULL OR status=?) ORDER BY created_at, id", vec![st.clone(), st])
    }

    pub fn set_task_status(&self, id: i64, status: &str) -> Result<Row> {
        if !STATUS_TAREFA.contains(&status) {
            return invalid(format!("status inválido. Use: {}", STATUS_TAREFA.join(", ")));
        }
        self.update("tasks", id, "status=?, updated_at=?", vec![t(status), self.now().into()])?;
        self.one("tasks", id)
    }

    pub fn remove_task(&self, id: i64) -> Result<bool> {
        self.delete("tasks", id)
    }

    /// Objetivos em aberto para o briefing: tarefas não concluídas e lembretes pendentes, os
    /// mais antigos primeiro.
    pub fn open_goals(&self, limit: usize) -> Result<Vec<String>> {
        let tarefas = self.query("SELECT title, status FROM tasks WHERE status<>'concluida' ORDER BY created_at, id", vec![])?;
        let lembretes = self.query("SELECT title, due_at FROM reminders WHERE done=0 ORDER BY due_at, id", vec![])?;
        let mut linhas: Vec<String> =
            tarefas.iter().map(|t| format!("tarefa ({}): {}", s(t, "status"), s(t, "title"))).collect();
        for r in &lembretes {
            let quando = chrono::DateTime::from_timestamp(r["due_at"].as_f64().unwrap_or(0.0) as i64, 0)
                .unwrap_or_default()
                .with_timezone(&Local)
                .format("%d/%m %H:%M");
            linhas.push(format!("lembrete para {quando}: {}", s(r, "title")));
        }
        linhas.truncate(limit);
        Ok(linhas)
    }

    // ── "números": observações com relevância ─────────────────────────────
    pub fn add_number(&self, target: &str, score: f64, reason: &str, source: &str) -> Result<Row> {
        let alvo = target.trim();
        if alvo.is_empty() {
            return invalid("informe o alvo da observação");
        }
        let id = self.insert(
            "INSERT INTO numbers(target, score, reason, source, created_at, project_id) VALUES (?,?,?,?,?,?)",
            vec![t(alvo), score.clamp(0.0, 1.0).into(), t(reason), t(source), self.now().into(), Self::projeto()],
        )?;
        self.one("numbers", id)
    }

    pub fn list_numbers(&self, only_pending: bool, min_score: f64) -> Result<Vec<Row>> {
        if only_pending {
            return self.query(
                "SELECT * FROM numbers WHERE notified=0 AND score>=? ORDER BY score DESC, id",
                vec![min_score.into()],
            );
        }
        self.query("SELECT * FROM numbers ORDER BY created_at DESC, id DESC LIMIT 50", vec![])
    }

    pub fn mark_number_notified(&self, id: i64) -> Result<()> {
        self.update("numbers", id, "notified=1", vec![])
    }

    // ── prompts salvos ────────────────────────────────────────────────────
    pub fn add_prompt(&self, title: &str, content: &str, command: &str) -> Result<Row> {
        if title.trim().is_empty() || content.trim().is_empty() {
            return invalid("informe título e conteúdo do prompt");
        }
        let id = self.insert(
            "INSERT INTO prompts(title, command, content, created_at, project_id) VALUES (?,?,?,?,?)",
            vec![t(title.trim()), t(command.trim()), t(content), self.now().into(), Self::projeto()],
        )?;
        self.one("prompts", id)
    }

    pub fn list_prompts(&self) -> Result<Vec<Row>> {
        self.query("SELECT * FROM prompts ORDER BY created_at DESC, id DESC", vec![])
    }

    pub fn remove_prompt(&self, id: i64) -> Result<bool> {
        self.delete("prompts", id)
    }

    // ── grafo ─────────────────────────────────────────────────────────────
    /// Cria a aresta; repetida (mesmos origem, relação e destino) não duplica.
    pub fn add_edge(&self, src: &str, rel: &str, dst: &str, weight: f64, kind: &str) -> Result<bool> {
        let agora = self.now();
        self.s.tx(|c| {
            Ok(c.execute(
                "INSERT OR IGNORE INTO edges(src, rel, dst, weight, kind, created_at) VALUES (?,?,?,?,?,?)",
                rusqlite::params![src, rel, dst, weight, kind, agora],
            )? > 0)
        })
    }

    /// Arestas que saem ou chegam em `node` (direção em `direction`).
    pub fn neighbors(&self, node: &str, rel: Option<&str>, limit: i64) -> Result<Vec<Row>> {
        let r: Sql = rel.map(|x| Sql::Text(x.into())).unwrap_or(Sql::Null);
        self.query(
            "SELECT src, rel, dst, weight, kind, 'out' AS direction FROM edges WHERE src=? AND (? IS NULL OR rel=?)
             UNION ALL SELECT src, rel, dst, weight, kind, 'in' AS direction FROM edges
             WHERE dst=? AND (? IS NULL OR rel=?) LIMIT ?",
            vec![t(node), r.clone(), r.clone(), t(node), r.clone(), r, limit.into()],
        )
    }

    /// Arestas do grafo (todas, ou só de um `kind`: `sono`, `vault`...), das mais novas.
    pub fn list_edges(&self, kind: Option<&str>, limit: i64) -> Result<Vec<Row>> {
        let k: Sql = kind.map(|x| Sql::Text(x.into())).unwrap_or(Sql::Null);
        self.query(
            "SELECT src, rel, dst, weight, kind, created_at FROM edges WHERE (? IS NULL OR kind=?)
             ORDER BY created_at DESC, src, rel, dst LIMIT ?",
            vec![k.clone(), k, limit.clamp(1, 20_000).into()],
        )
    }

    /// Apaga uma aresta (com `kind`, só se for daquele tipo).
    pub fn remove_edge(&self, src: &str, rel: &str, dst: &str, kind: Option<&str>) -> Result<bool> {
        self.s.tx(|c| {
            Ok(c.execute(
                "DELETE FROM edges WHERE src=?1 AND rel=?2 AND dst=?3 AND (?4 IS NULL OR kind=?4)",
                rusqlite::params![src, rel, dst, kind],
            )? > 0)
        })
    }

    pub fn edge_count(&self) -> i64 {
        self.s.conn().query_row("SELECT COUNT(*) FROM edges", [], |r| r.get(0)).unwrap_or(0)
    }

    // ── fila de avisos ────────────────────────────────────────────────────
    /// `urgente`: sai mesmo com o não perturbe ligado; o resto espera na fila.
    pub fn notify(&self, kind: &str, text: &str, refe: Option<&str>, urgente: bool) -> Result<i64> {
        let p = current().project_id;
        let agora = self.now();
        self.s.tx(|c| {
            if let Some(r) = refe {
                let old: Option<i64> = rusqlite::OptionalExtension::optional(c.query_row(
                    "SELECT id FROM notifications WHERE project_id IS ? AND kind=? AND ref=?",
                    rusqlite::params![p, kind, r],
                    |r| r.get(0),
                ))?;
                if let Some(o) = old {
                    return Ok(o);
                }
            }
            c.execute(
                "INSERT INTO notifications(kind, ref, text, created_at, project_id, urgent) VALUES (?,?,?,?,?,?)",
                rusqlite::params![kind, refe, text, agora, p, urgente as i64],
            )?;
            Ok(c.last_insert_rowid())
        })
    }

    /// O que os canais entregam. Com o não perturbe ligado, só os urgentes. `todos` conta tudo.
    pub fn pending_notifications(&self, limit: i64, todos: bool) -> Result<Vec<Row>> {
        let so_urgentes = !todos && (self.segurar.read())();
        self.query(
            "SELECT * FROM notifications WHERE delivered_at IS NULL AND (urgent=1 OR ?=0) ORDER BY id LIMIT ?",
            vec![(so_urgentes as i64).into(), limit.into()],
        )
    }

    /// Pendentes só dos `tipos` dados (vazio = todos), com o filtro do não perturbe; tudo antes do
    /// limite, para o que fica de fora não ocupar a janela de um canal que só entrega alguns tipos.
    pub fn pending_notifications_of(&self, tipos: &[String], limit: i64) -> Result<Vec<Row>> {
        let so_urgentes = (self.segurar.read())();
        let marcas = vec!["?"; tipos.len()].join(",");
        let filtro = if tipos.is_empty() { String::new() } else { format!(" AND kind IN ({marcas})") };
        let mut params: Vec<Sql> = vec![(so_urgentes as i64).into()];
        params.extend(tipos.iter().map(|t| Sql::from(t.clone())));
        params.push(limit.into());
        self.query(&format!("SELECT * FROM notifications WHERE delivered_at IS NULL AND (urgent=1 OR ?=0){filtro} ORDER BY id LIMIT ?"), params)
    }

    /// Avisos mais novos primeiro, entregues ou não (a caixa de atividade da interface).
    pub fn recent_notifications(&self, limit: i64) -> Result<Vec<Row>> {
        self.query("SELECT * FROM notifications ORDER BY id DESC LIMIT ?", vec![limit.clamp(1, 200).into()])
    }

    pub fn ack_notification(&self, id: i64) -> Result<bool> {
        let p = current().project_id;
        let agora = self.now();
        self.s.tx(|c| {
            Ok(c.execute(
                "UPDATE notifications SET delivered_at=? WHERE id=? AND delivered_at IS NULL AND project_id IS ?",
                rusqlite::params![agora, id, p],
            )? > 0)
        })
    }

    // ── vigilância de pastas (lista no `meta`) ────────────────────────────
    /// Avisa de arquivos novos em `pasta` (a foto de agora é o ponto de partida).
    pub fn watch_add(&self, pasta: &str) -> Result<Value> {
        let alvo = resolve(pasta);
        if !alvo.is_dir() {
            return invalid(format!("não é uma pasta: {}", alvo.display()));
        }
        let chave = format!("watch:{}", alvo.display());
        let existe = !self.s.query("SELECT 1 FROM meta WHERE key=?", &[&chave])?.is_empty();
        if self.watch_list()?.len() >= MAX_VIGILANCIAS && !existe {
            return invalid(format!("limite de {MAX_VIGILANCIAS} pastas vigiadas"));
        }
        let nomes = nomes_da_pasta(&alvo);
        let corpo = orion_policy::pyjson::dumps_ascii(&json!({"nomes": nomes, "desde": self.now()}), false);
        self.s.tx(|c| {
            c.execute(
                "INSERT INTO meta(key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                [&chave, &corpo],
            )?;
            Ok(())
        })?;
        Ok(json!({"pasta": alvo.display().to_string(), "arquivos_agora": nomes.len()}))
    }

    pub fn watch_list(&self) -> Result<Vec<Value>> {
        let rows = self.s.query("SELECT key, value FROM meta WHERE key LIKE 'watch:%' ORDER BY key", &[])?;
        Ok(rows
            .iter()
            .map(|r| {
                let estado: Value = serde_json::from_str(s(r, "value")).unwrap_or(Value::Null);
                json!({"pasta": &s(r, "key")["watch:".len()..], "desde": estado.get("desde")})
            })
            .collect())
    }

    pub fn watch_remove(&self, pasta: &str) -> Result<bool> {
        let chave = format!("watch:{}", resolve(pasta).display());
        self.s.tx(|c| Ok(c.execute("DELETE FROM meta WHERE key=?", [chave])? > 0))
    }

    /// Compara cada pasta vigiada com a última foto e avisa dos nomes novos.
    pub fn watch_poll(&self) -> Result<usize> {
        let mut avisos = 0;
        for r in self.s.query("SELECT key, value FROM meta WHERE key LIKE 'watch:%'", &[])? {
            let chave = s(&r, "key").to_string();
            let pasta = PathBuf::from(&chave["watch:".len()..]);
            if !pasta.is_dir() {
                self.notify("vigilancia", &format!("A pasta vigiada {} sumiu; parei de vigiar.", pasta.display()), None, false)?;
                self.s.tx(|c| Ok(c.execute("DELETE FROM meta WHERE key=?", [&chave])?))?;
                avisos += 1;
                continue;
            }
            let estado: Value = serde_json::from_str(s(&r, "value")).unwrap_or(json!({}));
            let antes: std::collections::HashSet<String> = estado
                .get("nomes")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                .unwrap_or_default();
            let agora = nomes_da_pasta(&pasta);
            let novos: Vec<&String> = agora.iter().filter(|n| !antes.contains(*n)).collect();
            if novos.is_empty() {
                if agora.iter().cloned().collect::<std::collections::HashSet<_>>() != antes {
                    // só saíram arquivos: atualiza a foto sem avisar
                    self.watch_salvar(&chave, &agora, &estado)?;
                }
                continue;
            }
            let mut lista = novos.iter().take(5).map(|s| s.as_str()).collect::<Vec<_>>().join(", ");
            if novos.len() > 5 {
                lista.push_str(&format!(" (+{})", novos.len() - 5));
            }
            self.notify("vigilancia", &format!("Novos arquivos em {}: {lista}", pasta.display()), Some(&chave), false)?;
            self.watch_salvar(&chave, &agora, &estado)?;
            avisos += 1;
        }
        Ok(avisos)
    }

    fn watch_salvar(&self, chave: &str, nomes: &[String], estado: &Value) -> Result<()> {
        let corpo = orion_policy::pyjson::dumps_ascii(&json!({"nomes": nomes, "desde": estado.get("desde")}), false);
        self.s.tx(|c| {
            c.execute("UPDATE meta SET value=? WHERE key=?", [&corpo, chave])?;
            Ok(())
        })
    }

    // ── trilha de auditoria (decisões da política; args já redigidos) ──────
    /// Grava uma decisão da política. Erro propaga: a política nega o que não for leitura
    /// (REGRAS.md: falha de auditoria nega a ação).
    pub fn audit_add(&self, evento: &Value) -> Result<i64> {
        let g = |k: &str| evento.get(k).cloned().unwrap_or(Value::Null);
        let txt = |k: &str| match g(k) {
            Value::String(s) => s,
            Value::Null => String::new(),
            v => v.to_string(),
        };
        let args = orion_policy::pyjson::dumps(&evento.get("args").cloned().unwrap_or(json!({})), false);
        let agora = self.now();
        self.s.tx(|c| {
            c.execute(
                "INSERT INTO audit(ts, session_id, tool, action, risk, reason, tainted, args) VALUES (?,?,?,?,?,?,?,?)",
                rusqlite::params![
                    agora,
                    g("session_id").as_str(),
                    txt("tool"),
                    txt("action"),
                    g("risk").as_str(),
                    txt("reason"),
                    g("tainted").as_bool().unwrap_or(false) as i64,
                    args
                ],
            )?;
            Ok(c.last_insert_rowid())
        })
    }

    /// Decisões mais recentes primeiro; `args` volta como objeto.
    pub fn audit_recent(&self, limite: i64, ferramenta: Option<&str>, acao: Option<&str>, desde: Option<f64>) -> Result<Vec<Row>> {
        let mut filtros = vec![];
        let mut params: Vec<Sql> = vec![];
        for (coluna, valor) in [("tool", ferramenta), ("action", acao)] {
            if let Some(v) = valor.filter(|v| !v.is_empty()) {
                filtros.push(format!("{coluna}=?"));
                params.push(t(v));
            }
        }
        if let Some(d) = desde {
            filtros.push("ts>=?".into());
            params.push(d.into());
        }
        params.push(limite.clamp(1, 500).into());
        let onde = if filtros.is_empty() { String::new() } else { format!(" WHERE {}", filtros.join(" AND ")) };
        let c = self.s.conn();
        let refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
        let mut rows = query_maps(&c, &format!("SELECT * FROM audit{onde} ORDER BY id DESC LIMIT ?"), &refs)?;
        for r in &mut rows {
            let a = serde_json::from_str(s(r, "args")).unwrap_or(json!({}));
            r.insert("args".into(), a);
        }
        Ok(rows)
    }

    /// Apaga decisões mais velhas que `dias`; devolve quantas.
    pub fn audit_prune(&self, dias: i64) -> Result<usize> {
        let limite = self.now() - (dias * 86400) as f64;
        self.s.tx(|c| Ok(c.execute("DELETE FROM audit WHERE ts < ?", [limite])?))
    }
}
