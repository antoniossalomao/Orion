//! API de contexto explícito e preferências locais. Todas as operações exigem login
//! (REGRAS.md: toda rota da API exige login; o que é de um projeto fica no projeto).

use crate::estado::Estado;
use crate::http::*;
use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::{Local, NaiveDate, TimeZone};
use orion_core::projects::Projects;
use orion_store::workspace::ProjectPreferences;
use serde::Deserialize;
use serde_json::{json, Value};

fn project(st: &Estado, id: &str, writable: bool) -> ApiResult<()> {
    conferir(id, r"^[a-f0-9]{32}$", "project_id")?;
    if writable {
        st.memory.writable_project(id)?;
    } else {
        Projects::new(st.memory.clone())
            .get(id)
            .map_err(|_| ApiError::new(404, "project_not_found"))?;
    }
    Ok(())
}

fn idle_project(st: &Estado, id: &str) -> ApiResult<()> {
    project(st, id, true)?;
    let sessions = st.memory.list_sessions(None, 100000, None, Some(id))?;
    if sessions.iter().any(|s| {
        st.agent.as_ref().is_some_and(|a| a.busy(&s.id)) || st.policy.approvals.unresolved(&s.id)
    }) {
        return erro(409, "project_busy_or_approval_pending");
    }
    Ok(())
}

pub async fn project_settings(
    _: Admin,
    State(st): State<Estado>,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    project(&st, &id, false)?;
    let p = st.memory.project_preferences(&id)?;
    Ok(Json(
        json!({"model":p.model,"max_steps":p.max_steps.unwrap_or(25),"inherited":{"model":p.model.is_none(),"max_steps":p.max_steps.is_none()},"overrides":p}),
    ))
}

pub async fn project_settings_save(
    _: Admin,
    State(st): State<Estado>,
    Path(id): Path<String>,
    Json(p): Json<ProjectPreferences>,
) -> ApiResult<Json<Value>> {
    idle_project(&st, &id)?;
    if let Some(model) = &p.model {
        let configured = st.gateway.as_ref().is_some_and(|g| {
            g.stats().iter().any(|e| {
                e["nome"].as_str() == Some(model.as_str()) && e["camada"].as_str() != Some("local")
            })
        });
        if !configured {
            return erro(422, "project_model_not_configured");
        }
    }
    st.memory.set_project_preferences(&id, &p)?;
    st.policy.approvals.invalidate_binding(&id);
    Ok(Json(json!({"ok":true,"overrides":p})))
}

pub async fn project_references(
    _: Admin,
    State(st): State<Estado>,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    project(&st, &id, false)?;
    Ok(Json(json!(st.memory.project_references(&id)?)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct References {
    document_ids: Vec<i64>,
}

pub async fn project_references_save(
    _: Admin,
    State(st): State<Estado>,
    Path(id): Path<String>,
    Json(b): Json<References>,
) -> ApiResult<Json<Value>> {
    idle_project(&st, &id)?;
    st.memory.set_project_references(&id, &b.document_ids)?;
    st.policy.approvals.invalidate_binding(&id);
    Ok(Json(json!({"ok":true})))
}

#[derive(Deserialize)]
pub struct Scope {
    project_id: Option<String>,
}

fn check_scope(st: &Estado, scope: &Scope, writable: bool) -> ApiResult<()> {
    if let Some(id) = &scope.project_id {
        project(st, id, writable)?;
    }
    Ok(())
}

pub async fn prompt_commands(
    _: Admin,
    State(st): State<Estado>,
    Query(q): Query<Scope>,
) -> ApiResult<Json<Value>> {
    check_scope(&st, &q, false)?;
    Ok(Json(json!(st
        .memory
        .prompt_commands(q.project_id.as_deref())?)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Prompt {
    prompt: String,
}

pub async fn prompt_command_save(
    _: Admin,
    State(st): State<Estado>,
    Path(name): Path<String>,
    Query(q): Query<Scope>,
    Json(b): Json<Prompt>,
) -> ApiResult<Json<Value>> {
    check_scope(&st, &q, true)?;
    st.memory
        .save_prompt_command(q.project_id.as_deref(), &name, &b.prompt)?;
    Ok(Json(json!({"ok":true})))
}

pub async fn prompt_command_delete(
    _: Admin,
    State(st): State<Estado>,
    Path(name): Path<String>,
    Query(q): Query<Scope>,
) -> ApiResult<Json<Value>> {
    check_scope(&st, &q, true)?;
    Ok(Json(
        json!({"deleted":st.memory.delete_prompt_command(q.project_id.as_deref(),&name)?}),
    ))
}

#[derive(Deserialize)]
pub struct Conversation {
    #[serde(default = "web")]
    channel: String,
}
fn web() -> String {
    "web".into()
}

fn session(st: &Estado, id: &str, channel: &str) -> ApiResult<orion_store::sessions::Session> {
    conferir(id, r"^[a-f0-9]{32}$", "session_id")?;
    conferir(channel, CANAL, "channel")?;
    st.memory
        .get_session(id)?
        .filter(|s| s.channel == channel)
        .ok_or_else(|| ApiError::new(404, "session_not_found"))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Feedback {
    rating: Option<i8>,
}

pub async fn message_feedback(
    _: Admin,
    State(st): State<Estado>,
    Path((sid, mid)): Path<(String, i64)>,
    Query(q): Query<Conversation>,
    Json(b): Json<Feedback>,
) -> ApiResult<Json<Value>> {
    session(&st, &sid, &q.channel)?;
    st.memory
        .set_message_feedback(&q.channel, &sid, mid, b.rating)?;
    Ok(Json(json!({"ok":true,"rating":b.rating})))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pin {
    pinned: bool,
}

pub async fn message_pin(
    _: Admin,
    State(st): State<Estado>,
    Path((sid, mid)): Path<(String, i64)>,
    Query(q): Query<Conversation>,
    Json(b): Json<Pin>,
) -> ApiResult<Json<Value>> {
    session(&st, &sid, &q.channel)?;
    if st.agent.as_ref().is_some_and(|a| a.busy(&sid)) || st.policy.approvals.unresolved(&sid) {
        return erro(409, "session_busy_or_approval_pending");
    }
    st.memory
        .set_message_pinned(&q.channel, &sid, mid, b.pinned)?;
    Ok(Json(json!({"ok":true,"pinned":b.pinned})))
}

pub async fn conversation_context(
    _: Admin,
    State(st): State<Estado>,
    Path(sid): Path<String>,
    Query(q): Query<Conversation>,
) -> ApiResult<Json<Value>> {
    let session = session(&st, &sid, &q.channel)?;
    let pins = st.memory.pinned_context(&sid)?;
    let summary = st.memory.get_summary(&sid)?;
    let references = match &session.project_id {
        Some(pid) => st.memory.project_references(pid)?,
        None => vec![],
    };
    let preferences = match &session.project_id {
        Some(pid) => st.memory.project_preferences(pid)?,
        None => Default::default(),
    };
    Ok(Json(
        json!({"session_id":sid,"project_id":session.project_id,"pinned_messages":pins,"annotations":st.memory.message_annotations(&sid)?,"summary":summary,"compacted":summary.is_some(),"references":references,"preferences":preferences,"pinned_limit_chars":8000,"reference_limit_chars":24000,"external_context":!pins.is_empty() || !references.is_empty()}),
    ))
}

#[derive(Deserialize)]
pub struct Day {
    date: Option<String>,
    project_id: Option<String>,
}

pub async fn daily_summary(
    _: Admin,
    State(st): State<Estado>,
    Query(q): Query<Day>,
) -> ApiResult<Json<Value>> {
    if let Some(pid) = &q.project_id {
        project(&st, pid, false)?;
    }
    let date = match q.date {
        Some(text) => NaiveDate::parse_from_str(&text, "%Y-%m-%d")
            .map_err(|_| ApiError::new(422, "date_invalid"))?,
        None => Local::now().date_naive(),
    };
    let next = date
        .succ_opt()
        .ok_or_else(|| ApiError::new(422, "date_invalid"))?;
    let start = Local
        .from_local_datetime(
            &date
                .and_hms_opt(0, 0, 0)
                .ok_or_else(|| ApiError::new(422, "date_invalid"))?,
        )
        .earliest()
        .ok_or_else(|| ApiError::new(422, "date_invalid"))?;
    let end = Local
        .from_local_datetime(
            &next
                .and_hms_opt(0, 0, 0)
                .ok_or_else(|| ApiError::new(422, "date_invalid"))?,
        )
        .latest()
        .ok_or_else(|| ApiError::new(422, "date_invalid"))?;
    let mut result = st.memory.daily_activity(
        q.project_id.as_deref(),
        start.timestamp() as f64,
        end.timestamp() as f64,
    )?;
    result["date"] = json!(date.to_string());
    Ok(Json(result))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Maintenance {
    operation: String,
}

pub async fn maintain_database(
    _: Admin,
    State(st): State<Estado>,
    Json(b): Json<Maintenance>,
) -> ApiResult<Json<Value>> {
    if st
        .memory
        .list_sessions(None, 100000, None, None)?
        .iter()
        .any(|s| st.agent.as_ref().is_some_and(|a| a.busy(&s.id)))
    {
        return erro(409, "agent_busy");
    }
    let store = st.memory.clone();
    tokio::task::spawn_blocking(move || match b.operation.as_str() {
        "vacuum" => store.vacuum(),
        "rebuild_search" => store.rebuild_search_indexes(),
        _ => orion_store::invalid("maintenance_operation_invalid"),
    })
    .await
    .map_err(|_| ApiError::new(500, "maintenance_failed"))??;
    Ok(Json(json!({"ok":true})))
}
