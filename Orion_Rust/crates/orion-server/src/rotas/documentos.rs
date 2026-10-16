//! Documentos enviados (`/documents`) e ramificações de conversa (`/branches`).

use crate::estado::Estado;
use crate::http::*;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::response::Response;
use axum::Json;
use orion_core::documents::{self, MAX_TOTAL, MAX_UPLOAD};
use orion_store::{with_scope, DataScope};
use serde::Deserialize;
use serde_json::{json, Value};

const ID32: &str = r"^[a-f0-9]{32}$";

#[derive(Deserialize)]
pub struct ProjQ {
    project_id: Option<String>,
}

fn validar(st: &Estado, pid: &Option<String>) -> ApiResult<()> {
    if let Some(p) = pid {
        let r = st.memory.query("SELECT archived FROM projects WHERE id=?", &[p])?;
        if r.first().is_none_or(|r| r["archived"].as_i64().unwrap_or(1) != 0) {
            return erro(409, "project_missing_or_archived");
        }
    }
    Ok(())
}

fn linha_de(st: &Estado, pid: &Option<String>, id: &str) -> ApiResult<Json<Value>> {
    let rows = documents::linhas(&st.memory, pid.as_deref())?;
    rows.into_iter().find(|r| r["id"] == json!(id)).map(|r| Json(Value::Object(r))).ok_or_else(|| ApiError::new(404, "document_not_found"))
}

/// Indexa em segundo plano (E4.3). Espera um pouco para o caso comum (arquivo pequeno) voltar já
/// pronto; o resto o front acompanha pelo `progress`.
async fn rodar(st: &Estado, id: &str, pid: &Option<String>) -> ApiResult<()> {
    if !st.memory.query("SELECT 1 FROM uploads WHERE id=? AND status='processing'", &[&id])?.is_empty() {
        return erro(409, "document_busy");
    }
    let (m, id2, pid2) = (st.memory.clone(), id.to_string(), pid.clone());
    let tarefa = tokio::task::spawn_blocking(move || documents::process(&m, &id2, pid2.as_deref()));
    let _ = tokio::time::timeout(std::time::Duration::from_secs_f64(documents::ESPERA_S), tarefa).await;
    Ok(())
}

pub async fn listar_documentos(_: Admin, State(st): State<Estado>, Query(q): Query<ProjQ>) -> ApiResult<Json<Value>> {
    Ok(Json(json!(documents::linhas(&st.memory, q.project_id.as_deref())?)))
}

#[derive(Deserialize)]
pub struct UploadQ {
    name: String,
    project_id: Option<String>,
}

pub async fn enviar_documento(_: Admin, State(st): State<Estado>, Query(q): Query<UploadQ>, corpo: Bytes) -> ApiResult<Json<Value>> {
    validar(&st, &q.project_id)?;
    let nome = &q.name;
    let kind = std::path::Path::new(nome).extension().map(|e| format!(".{}", e.to_string_lossy().to_lowercase())).unwrap_or_default();
    if nome.trim().is_empty() || nome.chars().count() > 160 || nome.chars().any(|c| (c as u32) < 32 || c == '/' || c == '\\') || ![".pdf", ".txt", ".md"].contains(&kind.as_str()) {
        return erro(422, "document_type_or_name_invalid");
    }
    if corpo.len() > MAX_UPLOAD {
        return erro(413, "document_size_limit");
    }
    if corpo.is_empty() {
        return erro(422, "document_empty");
    }
    let id = orion_store::uuid_hex();
    let agora = st.memory.now();
    let r: ApiResult<()> = st.memory.tx(|c| {
        let total: i64 = c.query_row("SELECT COALESCE(sum(length(raw)),0) FROM uploads", [], |r| r.get(0))?;
        if total + corpo.len() as i64 > MAX_TOTAL {
            return Ok(Err(ApiError::new(413, "document_storage_limit")));
        }
        c.execute(
            "INSERT INTO uploads(id,project_id,name,kind,raw,status,created_at) VALUES (?,?,?,?,?,?,?)",
            orion_store::rusqlite::params![id, q.project_id, nome, kind, corpo.to_vec(), "received", agora],
        )?;
        Ok(Ok(()))
    })?;
    r?;
    rodar(&st, &id, &q.project_id).await?;
    linha_de(&st, &q.project_id, &id)
}

pub async fn tentar_de_novo(_: Admin, State(st): State<Estado>, Path(id): Path<String>, Query(q): Query<ProjQ>) -> ApiResult<Json<Value>> {
    validar(&st, &q.project_id)?;
    rodar(&st, &id, &q.project_id).await?;
    linha_de(&st, &q.project_id, &id)
}

/// Estado de um documento (status e `progress`): o front consulta a cada 1 s enquanto ele indexa.
pub async fn detalhe_documento(_: Admin, State(st): State<Estado>, Path(id): Path<String>, Query(q): Query<ProjQ>) -> ApiResult<Json<Value>> {
    linha_de(&st, &q.project_id, &id)
}

/// Apaga os trechos e os vetores do documento e indexa de novo a partir do original.
pub async fn reindexar(_: Admin, State(st): State<Estado>, Path(id): Path<String>, Query(q): Query<ProjQ>) -> ApiResult<Json<Value>> {
    validar(&st, &q.project_id)?;
    let achados = st.memory.query("SELECT name, status FROM uploads WHERE id=? AND project_id IS ?", &[&id, &q.project_id])?;
    let Some(a) = achados.first() else { return erro(404, "document_not_found") };
    if a["status"] == json!("processing") {
        return erro(409, "document_busy");
    }
    let fonte = format!("upload:{id}/{}", a["name"].as_str().unwrap_or(""));
    with_scope(DataScope::project(q.project_id.clone(), false), || st.memory.remove_document(&fonte))?;
    rodar(&st, &id, &q.project_id).await?;
    linha_de(&st, &q.project_id, &id)
}

#[derive(Deserialize)]
pub struct DocumentoMover {
    project_id: Option<String>,
}

/// "Disponível em": muda o escopo do documento. O texto indexado é refeito a partir do original no
/// escopo novo; o do escopo antigo sai da busca antes (nunca fica nos dois).
pub async fn mover_documento(_: Admin, State(st): State<Estado>, Path(id): Path<String>, Query(q): Query<ProjQ>, Json(c): Json<Value>) -> ApiResult<Json<Value>> {
    let Some(alvo) = c.get("project_id") else { return erro(422, "project_id obrigatório") };
    let alvo = alvo.as_str().map(String::from);
    if let Some(p) = &alvo {
        conferir(p, ID32, "project_id")?;
    }
    let _ = DocumentoMover { project_id: None };
    validar(&st, &alvo)?;
    if alvo == q.project_id {
        return linha_de(&st, &q.project_id, &id);
    }
    let achados = st.memory.query("SELECT name FROM uploads WHERE id=? AND project_id IS ?", &[&id, &q.project_id])?;
    let Some(a) = achados.first() else { return erro(404, "document_not_found") };
    let fonte = format!("upload:{id}/{}", a["name"].as_str().unwrap_or(""));
    with_scope(DataScope::project(q.project_id.clone(), false), || st.memory.remove_document(&fonte))?;
    st.memory.tx(|c| {
        c.execute("UPDATE uploads SET project_id=? WHERE id=?", orion_store::rusqlite::params![alvo, id])?;
        Ok(())
    })?;
    rodar(&st, &id, &alvo).await?;
    linha_de(&st, &alvo, &id)
}

pub async fn baixar_documento(_: Admin, State(st): State<Estado>, Path(id): Path<String>, Query(q): Query<ProjQ>) -> ApiResult<Response> {
    let r = st.memory.query("SELECT name FROM uploads WHERE id=? AND project_id IS ?", &[&id, &q.project_id])?;
    let Some(r) = r.first() else { return erro(404, "document_not_found") };
    let nome = r["name"].as_str().unwrap_or("").to_string();
    let raw: Vec<u8> = st.memory.conn().query_row("SELECT raw FROM uploads WHERE id=?", [&id], |r| r.get(0)).map_err(orion_store::StoreError::from)?;
    let cod: String = nome.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-._~/".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect();
    Ok(Response::builder()
        .header("content-type", "application/octet-stream")
        .header("content-disposition", format!("attachment; filename*=UTF-8''{cod}"))
        .header("x-content-type-options", "nosniff")
        .header("cache-control", "no-store")
        .header("content-security-policy", "default-src 'none'; sandbox")
        .body(axum::body::Body::from(raw))
        .map_err(anyhow::Error::from)?)
}

// ── ramificações ─────────────────────────────────────────────────────────

pub async fn caminhos(_: Admin, State(st): State<Estado>, Path(sid): Path<String>) -> ApiResult<Json<Value>> {
    let Some(sessao) = st.memory.get_session(&sid)?.filter(|s| s.channel == "web") else { return erro(404, "session_not_found") };
    let raiz_row = st.memory.query("SELECT root_session FROM message_branches WHERE session_id=?", &[&sid])?;
    let raiz = raiz_row.first().and_then(|r| r["root_session"].as_str().map(String::from)).unwrap_or_else(|| sid.clone());
    let paths = st.memory.query(
        "SELECT session_id,source_message,edited_text,prefix_count FROM message_branches WHERE root_session=? ORDER BY created_at LIMIT 100",
        &[&raiz],
    )?;
    let mut itens: Vec<(String, Value, Value, i64)> = vec![(raiz.clone(), Value::Null, Value::Null, 0)];
    itens.extend(paths.iter().map(|p| (p["session_id"].as_str().unwrap_or("").to_string(), p["source_message"].clone(), p["edited_text"].clone(), p["prefix_count"].as_i64().unwrap_or(0))));
    let mut out = vec![];
    for (id, mensagem, texto, prefixo) in itens {
        let Some(c) = st.memory.get_session(&id)?.filter(|c| c.project_id == sessao.project_id) else { continue };
        let artefatos = st.memory.query("SELECT count(*) AS n FROM artifacts WHERE session_id=?", &[&id])?[0]["n"].clone();
        let msgs = st.memory.query("SELECT count(*) AS n FROM messages WHERE session_id=?", &[&id])?[0]["n"].as_i64().unwrap_or(0);
        out.push(json!({
            "session_id": id, "title": c.title.unwrap_or_else(|| "Caminho original".into()), "message_id": mensagem,
            "edited_text": if msgs <= prefixo { texto } else { Value::Null }, "artifacts": artefatos,
        }));
    }
    Ok(Json(json!(out)))
}

#[derive(Deserialize)]
pub struct Edit {
    text: String,
    session_id: String,
}

pub async fn ramificar(_: Admin, State(st): State<Estado>, Path(mid): Path<i64>, Json(c): Json<Edit>) -> ApiResult<Json<Value>> {
    if !(1..=32000).contains(&c.text.chars().count()) {
        return erro(422, "text: entre 1 e 32000 caracteres");
    }
    conferir(&c.session_id, ID32, "session_id")?;
    let Some(sessao) = st.memory.get_session(&c.session_id)?.filter(|s| s.channel == "web") else { return erro(404, "session_not_found") };
    if sessao.archived || sessao.read_only || st.agent.as_ref().is_some_and(|a| a.busy(&sessao.id)) {
        return erro(409, "session_busy_or_read_only");
    }
    if let Some(p) = &sessao.project_id {
        let r = st.memory.query("SELECT archived FROM projects WHERE id=?", &[p])?;
        if r.first().is_none_or(|r| r["archived"].as_i64().unwrap_or(1) != 0) {
            return erro(409, "project_archived");
        }
    }
    let piso = st.memory.counter_get(&format!("history_after:{}", sessao.id));
    if st.memory.query("SELECT id FROM messages WHERE id=? AND session_id=? AND role='user' AND id>?", &[&mid, &sessao.id, &piso])?.is_empty() {
        return erro(404, "editable_message_not_found");
    }
    let raiz = st.memory.query("SELECT root_session FROM message_branches WHERE session_id=?", &[&sessao.id])?;
    let raiz_id = raiz.first().and_then(|r| r["root_session"].as_str().map(String::from)).unwrap_or_else(|| sessao.id.clone());
    if st.memory.query("SELECT count(*) AS n FROM message_branches WHERE root_session=?", &[&raiz_id])?[0]["n"].as_i64().unwrap_or(0) >= 99 {
        return erro(409, "branch_limit");
    }
    let prefixo = st.memory.query(
        "SELECT role,text,created_at,provenance FROM messages WHERE session_id=? AND id>? AND id<? AND role IN ('user','assistant') ORDER BY id",
        &[&sessao.id, &piso, &mid],
    )?;
    if prefixo.len() > 200 || prefixo.iter().map(|r| r["text"].as_str().unwrap_or("").chars().count()).sum::<usize>() > 128_000 {
        return erro(413, "branch_context_limit");
    }
    // criação e cópia do prefixo atômicas; ferramentas, instruções de sistema e decisões não vão
    let titulo = format!("Versão · {}", sessao.title.clone().unwrap_or_else(|| "Conversa".into()).chars().take(100).collect::<String>());
    let agora = st.memory.now();
    let alvo = st.memory.tx(|conn| {
        let alvo = st.memory.new_session_in(conn, "web", Some(&titulo), sessao.project_id.as_deref())?;
        for r in &prefixo {
            conn.execute(
                "INSERT INTO messages(session_id,role,text,created_at,provenance) VALUES (?,?,?,?,?)",
                orion_store::rusqlite::params![alvo.id, r["role"].as_str(), r["text"].as_str(), r["created_at"].as_f64(), r["provenance"].as_str()],
            )?;
        }
        conn.execute(
            "INSERT INTO message_branches(session_id,root_session,parent_session,source_message,edited_text,prefix_count,created_at) VALUES (?,?,?,?,?,?,?)",
            orion_store::rusqlite::params![alvo.id, raiz_id, sessao.id, mid, c.text, prefixo.len() as i64, agora],
        )?;
        Ok(alvo)
    })?;
    if st.memory.counter_get(&format!("taint:{}", sessao.id)) != 0 {
        st.memory.counter_set(&format!("taint:{}", alvo.id), 1)?;
    }
    Ok(Json(json!({"session_id": alvo.id, "edited_text": c.text, "requires_send": true})))
}
