//! Credenciais de clientes MCP de entrada (`/mcp-export/clients`) e o servidor MCP de leitura
//! (`/mcp-export/rpc`, com a credencial do cliente no lugar do login).

use crate::estado::Estado;
use crate::http::*;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use orion_mcp::server::{despachar, Ctx};
use serde::Deserialize;
use serde_json::{json, Value};

pub async fn clientes_listar(_: Admin, State(st): State<Estado>) -> ApiResult<Json<Value>> {
    Ok(Json(json!(st.export_credentials.listar()?)))
}

#[derive(Deserialize)]
pub struct Grant {
    name: String,
    project_id: Option<String>,
    permissions: Vec<String>,
    lifetime_hours: Option<i64>,
}

pub async fn cliente_emitir(_: Admin, State(st): State<Estado>, Json(g): Json<Grant>) -> ApiResult<Response> {
    if let Some(p) = &g.project_id {
        conferir(p, r"^[a-f0-9]{32}$", "project_id")?;
    }
    let r = st.export_credentials.emitir(&g.name, g.project_id.as_deref(), &g.permissions, g.lifetime_hours.unwrap_or(168)).map_err(|(s, m)| ApiError::new(s, m))?;
    let mut resp = Json(r).into_response();
    resp.headers_mut().insert("cache-control", "no-store".parse().unwrap());
    Ok(resp)
}

pub async fn cliente_revogar(_: Admin, State(st): State<Estado>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    if !st.export_credentials.revogar(&id)? {
        return erro(404, "export_client_not_found");
    }
    Ok(Json(json!({"ok": true})))
}

/// MCP Streamable HTTP sem estado, respostas JSON, pedidos de até 16 KB.
pub async fn rpc(State(st): State<Estado>, headers: HeaderMap, corpo: Bytes) -> Response {
    let bearer = headers.get("authorization").and_then(|h| h.to_str().ok()).and_then(|h| h.strip_prefix("Bearer ")).map(String::from);
    if st.export_credentials.verificar(bearer.as_deref().unwrap_or(""), None).is_err() {
        let mut r = (StatusCode::UNAUTHORIZED, Json(json!({"detail": "export_unauthorized"}))).into_response();
        r.headers_mut().insert("cache-control", "no-store".parse().unwrap());
        return r;
    }
    if corpo.len() > 16 * 1024 {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    }
    let Ok(msg) = serde_json::from_slice::<Value>(&corpo) else {
        return (StatusCode::BAD_REQUEST, Json(json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": "JSON inválido"}}))).into_response();
    };
    let mut ctx = Ctx { client_name: String::new(), bearer };
    let r = match msg {
        Value::Array(lote) => {
            let mut saidas = vec![];
            for m in lote {
                if let Some(r) = despachar(&*st.export, &mut ctx, m).await {
                    saidas.push(r);
                }
            }
            (!saidas.is_empty()).then_some(Value::Array(saidas))
        }
        m => despachar(&*st.export, &mut ctx, m).await,
    };
    match r {
        Some(v) => (StatusCode::OK, Json(v)).into_response(),
        None => StatusCode::ACCEPTED.into_response(),
    }
}
