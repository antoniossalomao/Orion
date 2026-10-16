//! Escopo do token da ponte de desktop (REGRAS.md: ponte de desktop só abre e cola o que você
//! aprovou): só estas rotas; o resto responde 403.

use regex::Regex;
use std::sync::LazyLock;

pub const ESCOPO: &str = "ponte";
pub const ROTAS_DA_PONTE: [&str; 5] = ["/captura", "/ponte/imagem", "/ponte/explicar", "/posso-desligar", "/area-transferencia"];
static ROTINA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^/rotinas/[a-z0-9][a-z0-9_-]{0,31}/executar$").unwrap());

pub fn rota_permitida(metodo: &str, caminho: &str) -> bool {
    // cortar é sempre seguro; sair exige a senha
    if metodo == "POST" && caminho == "/modo/panico" {
        return true;
    }
    // tecla de rotina: só concedidas
    if metodo == "POST" && ROTINA.is_match(caminho) {
        return true;
    }
    ROTAS_DA_PONTE.iter().any(|r| caminho == *r || caminho.starts_with(&format!("{r}/")))
}

// ── rotas ─────────────────────────────────────────────────────────────────────

use crate::estado::Estado;
use crate::http::*;
use axum::body::Bytes;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Value};
use std::time::Duration;

const CHAVE_AVISO: &str = "ponte:isso_aviso_aceito";
const VIGIA_S: u64 = 2;

fn em_panico(st: &Estado) -> bool {
    st.modos.panico()
}

pub async fn estado(_: Admin, State(st): State<Estado>) -> Json<Value> {
    Json(st.ponte.estado())
}

/// Manda `abrir` ou `colar` à ponte conectada. Só com login (nunca o token da ponte).
pub async fn comando(_: Admin, State(st): State<Estado>, Json(c): Json<Value>) -> ApiResult<Json<Value>> {
    let o = c.as_object().ok_or_else(|| ApiError::new(422, "corpo inválido"))?;
    if let Some(k) = o.keys().find(|k| !["cmd", "rota", "texto"].contains(&k.as_str())) {
        return erro(422, format!("campo desconhecido: {k}"));
    }
    if em_panico(&st) {
        return erro(409, "modo pânico: a ponte está desligada");
    }
    match st.ponte.enviar(&c) {
        Err(e) => erro(422, e.0),
        Ok(false) => erro(503, "a ponte não está conectada (rode `orion ponte`)"),
        Ok(true) => Ok(Json(json!({"ok": true}))),
    }
}

/// A captura do "O que é isso?": só na memória do servidor, 5 minutos, nunca em disco.
pub async fn guardar_imagem(_: Admin, State(st): State<Estado>, corpo: Bytes) -> ApiResult<Json<Value>> {
    if corpo.len() > orion_ai::vision::MAX_IMAGEM {
        return erro(413, "imagem grande demais");
    }
    match st.imagens_ponte.guardar(&corpo, orion_ai::vision::MAX_IMAGEM) {
        Ok(id) => Ok(Json(json!({"id": id}))),
        Err(m) => erro(422, m),
    }
}

pub fn limite_imagem() -> usize {
    orion_ai::vision::MAX_IMAGEM + 1024
}

pub async fn ver_imagem(_: Admin, State(st): State<Estado>, Path(id): Path<String>) -> ApiResult<Response> {
    let Some((dados, mime)) = st.imagens_ponte.pegar(&id) else { return erro(404, "a captura expirou (5 minutos)") };
    Ok(([("content-type", mime), ("cache-control", "no-store"), ("x-content-type-options", "nosniff")], dados).into_response())
}

/// Pergunta ao modelo de visão sobre a captura. A imagem **sai do computador**: exige
/// `ORION_VISION_TOOLS`, não roda em pânico e, na primeira vez, só devolve
/// `{"aviso": "primeira_vez"}` até vir `aceito: true` (nada sai antes).
pub async fn explicar(_: Admin, State(st): State<Estado>, Json(c): Json<Value>) -> ApiResult<Json<Value>> {
    let o = c.as_object().ok_or_else(|| ApiError::new(422, "corpo inválido"))?;
    if let Some(k) = o.keys().find(|k| !["imagem_id", "pergunta", "aceito"].contains(&k.as_str())) {
        return erro(422, format!("campo desconhecido: {k}"));
    }
    let id = c["imagem_id"].as_str().unwrap_or("");
    if id.len() < 8 || id.len() > 40 {
        return erro(422, "imagem_id: valor inválido");
    }
    conferir(id, r"^[A-Za-z0-9_-]+$", "imagem_id")?;
    let pergunta = c.get("pergunta").and_then(Value::as_str).unwrap_or("").to_string();
    if pergunta.chars().count() > 500 {
        return erro(422, "pergunta: no máximo 500 caracteres");
    }
    if em_panico(&st) {
        return erro(409, "modo pânico: nada sai do computador");
    }
    let Some(vision) = crate::estado::vision_from_settings(&st.settings) else { return erro(409, "ligue ORION_VISION_TOOLS para perguntar sobre imagens") };
    let Some((dados, mime)) = st.imagens_ponte.pegar(id) else { return erro(404, "a captura expirou (5 minutos): aperte a tecla de novo") };
    if st.memory.meta_get(CHAVE_AVISO).is_none() {
        if c.get("aceito").and_then(Value::as_bool) != Some(true) {
            return Ok(Json(json!({"aviso": "primeira_vez"})));
        }
        st.memory.meta_set(CHAVE_AVISO, Some("1"))?;
    }
    let n = dados.len();
    let texto = tokio::task::spawn_blocking(move || vision.describe(&dados, mime, &pergunta)).await.map_err(|e| anyhow::anyhow!(e))?.map_err(|e| ApiError::new(502, e.to_string()))?;
    let _ = st.ops.audit_add(&json!({"tool": "ponte_explicar", "action": "allow", "risk": "egress", "reason": format!("imagem de {n} bytes ao provedor de visão")}));
    Ok(Json(json!({"texto": texto})))
}

fn fechar(codigo: u16) -> Message {
    Message::Close(Some(CloseFrame { code: codigo, reason: "".into() }))
}

pub async fn ws_ponte(State(st): State<Estado>, headers: HeaderMap, ws: WebSocketUpgrade) -> Response {
    let token = headers.get("authorization").and_then(|h| h.to_str().ok()).unwrap_or("").trim_start_matches("Bearer ").trim().to_string();
    ws.on_upgrade(move |socket| ponte_socket(st, token, socket))
}

async fn ponte_socket(st: Estado, token: String, mut socket: WebSocket) {
    if st.auth.device_scope(Some(&token)).as_deref() != Some(ESCOPO) || em_panico(&st) {
        let _ = socket.send(fechar(1008)).await;
        return;
    }
    let (id, mut fila) = st.ponte.anexar();
    loop {
        tokio::select! {
            // o cliente só avisa que está vivo; desconectar encerra
            m = socket.recv() => match m {
                None | Some(Err(_)) | Some(Ok(Message::Close(_))) => break,
                Some(Ok(_)) => {}
            },
            c = tokio::time::timeout(Duration::from_secs(VIGIA_S), fila.recv()) => match c {
                Err(_) => {
                    // pânico ou pareamento desfeito: derruba a ponte
                    if em_panico(&st) || st.auth.device_scope(Some(&token)).as_deref() != Some(ESCOPO) {
                        break;
                    }
                }
                Ok(None) | Ok(Some(None)) => break, // outra ponte assumiu
                Ok(Some(Some(cmd))) => {
                    if em_panico(&st) || socket.send(Message::Text(cmd.to_string())).await.is_err() {
                        break;
                    }
                }
            }
        }
    }
    st.ponte.soltar(id);
    let _ = socket.send(fechar(1000)).await;
}
