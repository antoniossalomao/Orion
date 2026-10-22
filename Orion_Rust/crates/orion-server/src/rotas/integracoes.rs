//! Contas OAuth, Agenda (leitura), propostas de evento e planos de organização de arquivos.

use crate::estado::Estado;
use crate::http::*;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use orion_core::calendar::Binding;
use orion_core::calendar_events::Event;
use serde::Deserialize;
use serde_json::{json, Value};

type Corpo = Json<Value>;

fn projeto_disponivel(st: &Estado, scope: &str) -> ApiResult<()> {
    if let Some(p) = scope.strip_prefix("project:") {
        let r = st.memory.query("SELECT archived FROM projects WHERE id=?", &[&p])?;
        if r.first().map_or(true, |x| x["archived"].as_i64().unwrap_or(0) != 0) {
            return erro(409, "project_unavailable");
        }
    }
    Ok(())
}

fn ocupada(st: &Estado, sessao: &str) -> ApiResult<()> {
    if st.agent.as_ref().is_some_and(|a| a.busy(sessao)) {
        return erro(409, "session_busy");
    }
    Ok(())
}

fn unico<T>(o: &Option<T>) -> ApiResult<&T> {
    o.as_ref().ok_or_else(|| ApiError::new(409, "integração indisponível neste servidor"))
}

// ── contas ───────────────────────────────────────────────────────────────────

pub async fn contas_listar(_: Admin, State(st): State<Estado>) -> ApiResult<Corpo> {
    Ok(Json(json!(unico(&st.accounts)?.listar()?)))
}

pub async fn conta_criar(_: Admin, State(st): State<Estado>, Json(b): Corpo) -> ApiResult<Corpo> {
    if let Some(s) = b.get("scope").and_then(Value::as_str) {
        projeto_disponivel(&st, s)?;
    }
    Ok(Json(unico(&st.accounts)?.criar(&b)?))
}

pub async fn conta_autorizar(_: Admin, State(st): State<Estado>, headers: HeaderMap, Path(id): Path<String>) -> ApiResult<Corpo> {
    let host = headers.get("host").and_then(|h| h.to_str().ok()).unwrap_or("127.0.0.1");
    let base = format!("http://{host}");
    Ok(Json(unico(&st.accounts)?.autorizar(&id, &format!("{base}/accounts/callback/{id}")).await?))
}

pub async fn conta_revogar(_: Admin, State(st): State<Estado>, Path(id): Path<String>) -> ApiResult<Corpo> {
    let r = unico(&st.accounts)?.revogar(&id, false).await?;
    if let Some(c) = &st.catalog {
        let _ = c.refresh().await;
    }
    Ok(Json(r))
}

#[derive(Deserialize)]
pub struct Callback {
    #[serde(default)]
    code: String,
    #[serde(default)]
    state: String,
    iss: Option<String>,
}

pub async fn conta_callback(State(st): State<Estado>, Path(id): Path<String>, Query(q): Query<Callback>) -> ApiResult<Response> {
    unico(&st.accounts)?.callback(&id, &q.code, &q.state, q.iss.as_deref())?;
    Ok((StatusCode::OK, [("content-type", "text/plain; charset=utf-8"), ("cache-control", "no-store"), ("referrer-policy", "no-referrer")], "Autorização recebida. Volte ao Orion.").into_response())
}

// ── agenda ───────────────────────────────────────────────────────────────────

pub async fn agenda_vinculos(_: Admin, State(st): State<Estado>) -> ApiResult<Corpo> {
    Ok(Json(json!(st.memory.query("SELECT * FROM calendar_bindings", &[])?)))
}

pub async fn agenda_vincular(_: Admin, State(st): State<Estado>, Json(b): Corpo) -> ApiResult<Corpo> {
    let bind = Binding::parse(&b).map_err(|m| ApiError::new(422, m))?;
    projeto_disponivel(&st, &bind.scope)?;
    Ok(Json(unico(&st.calendar)?.bind(&bind).await?))
}

#[derive(Deserialize)]
pub struct ProjQ {
    project_id: Option<String>,
}

fn proj(q: &ProjQ) -> ApiResult<Option<&str>> {
    if let Some(p) = &q.project_id {
        conferir(p, r"^[a-f0-9]{32}$", "project_id")?;
    }
    Ok(q.project_id.as_deref())
}

fn texto<'a>(b: &'a Value, k: &str) -> ApiResult<&'a str> {
    b.get(k).and_then(Value::as_str).ok_or_else(|| ApiError::new(422, format!("{k}: obrigatório")))
}

fn so_chaves(b: &Value, ok: &[&str]) -> ApiResult<()> {
    match b.as_object().and_then(|o| o.keys().find(|k| !ok.contains(&k.as_str()))) {
        Some(k) => erro(422, format!("campo desconhecido: {k}")),
        None => Ok(()),
    }
}

pub async fn eventos_listar(_: Admin, State(st): State<Estado>, Query(q): Query<ProjQ>) -> ApiResult<Corpo> {
    Ok(Json(json!(unico(&st.events)?.listar(proj(&q)?)?)))
}

pub async fn evento_criar(_: Admin, State(st): State<Estado>, Query(q): Query<ProjQ>, Json(b): Corpo) -> ApiResult<Corpo> {
    let p = proj(&q)?;
    let ev = Event::parse(&b, true).map_err(|m| ApiError::new(422, m))?;
    let sessao = texto(&b, "session_id")?;
    conferir(sessao, r"^[a-f0-9]{32}$", "session_id")?;
    Ok(Json(unico(&st.events)?.criar(sessao, &ev, p)?))
}

pub async fn evento_revisar(_: Admin, State(st): State<Estado>, Path(id): Path<String>, Query(q): Query<ProjQ>, Json(b): Corpo) -> ApiResult<Corpo> {
    let p = proj(&q)?;
    so_chaves(&b, &["reviewed_digest"])?;
    let d = texto(&b, "reviewed_digest")?;
    conferir(d, r"^[a-f0-9]{64}$", "reviewed_digest")?;
    let e = unico(&st.events)?;
    ocupada(&st, e.get(&id, p)?["session_id"].as_str().unwrap_or(""))?;
    Ok(Json(e.revisar(&id, p, d).await?))
}

pub async fn evento_retomar(_: Admin, State(st): State<Estado>, Path(id): Path<String>, Query(q): Query<ProjQ>, Json(b): Corpo) -> ApiResult<Corpo> {
    let p = proj(&q)?;
    so_chaves(&b, &["approval_id"])?;
    let a = texto(&b, "approval_id")?;
    if a.is_empty() || a.len() > 128 {
        return erro(422, "approval_id: valor inválido");
    }
    let e = unico(&st.events)?;
    ocupada(&st, e.get(&id, p)?["session_id"].as_str().unwrap_or(""))?;
    Ok(Json(e.retomar(&id, p, a).await?))
}

// ── planos de arquivos ───────────────────────────────────────────────────────

pub async fn planos_listar(_: Admin, State(st): State<Estado>, Query(q): Query<ProjQ>) -> ApiResult<Corpo> {
    Ok(Json(json!(unico(&st.file_plans)?.listar(proj(&q)?)?)))
}

pub async fn plano_criar(_: Admin, State(st): State<Estado>, Query(q): Query<ProjQ>, Json(b): Corpo) -> ApiResult<Corpo> {
    let p = proj(&q)?;
    so_chaves(&b, &["root", "session_id"])?;
    let (root, sessao) = (texto(&b, "root")?, texto(&b, "session_id")?);
    conferir(sessao, r"^[a-f0-9]{32}$", "session_id")?;
    if root.is_empty() || root.len() > 4096 {
        return erro(422, "root: valor inválido");
    }
    ocupada(&st, sessao)?;
    let fp = unico(&st.file_plans)?.clone();
    let (root, sessao, p) = (root.to_string(), sessao.to_string(), p.map(String::from));
    let escopo = orion_store::current_scope();
    let r = tokio::task::spawn_blocking(move || orion_store::with_scope(escopo, || fp.criar(&root, &sessao, p.as_deref()))).await.map_err(|e| anyhow::anyhow!(e))??;
    Ok(Json(r))
}

pub async fn plano_revisar(_: Admin, State(st): State<Estado>, Path(id): Path<String>, Query(q): Query<ProjQ>, Json(b): Corpo) -> ApiResult<Corpo> {
    let p = proj(&q)?;
    so_chaves(&b, &["reviewed_digest"])?;
    let d = texto(&b, "reviewed_digest")?;
    conferir(d, r"^[a-f0-9]{64}$", "reviewed_digest")?;
    let fp = unico(&st.file_plans)?;
    ocupada(&st, fp.get(&id, p)?["session_id"].as_str().unwrap_or(""))?;
    Ok(Json(fp.revisar(&id, p, d)?))
}

pub async fn plano_aplicar(_: Admin, State(st): State<Estado>, Path(id): Path<String>, Query(q): Query<ProjQ>, Json(b): Corpo) -> ApiResult<Corpo> {
    let p = proj(&q)?;
    so_chaves(&b, &["approval_id"])?;
    let a = texto(&b, "approval_id")?;
    if a.is_empty() || a.len() > 128 {
        return erro(422, "approval_id: valor inválido");
    }
    let fp = unico(&st.file_plans)?;
    ocupada(&st, fp.get(&id, p)?["session_id"].as_str().unwrap_or(""))?;
    Ok(Json(fp.aplicar(&id, p, a)?))
}
