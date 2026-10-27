//! Imagens do chat (F05.11): enviadas por bytes, validadas pela assinatura e guardadas só na memória
//! até o próximo turno. Nunca vão para o banco nem para o disco (REGRAS.md: o histórico guarda só o
//! aviso de que houve imagem). A imagem é dado do usuário para o modelo, não instrução.

use crate::http::*;
use axum::body::Bytes;
use axum::extract::Query;
use axum::Json;
use base64::Engine;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

pub const MAX_IMAGEM: usize = 10 * 1024 * 1024;
const MAX_GUARDADAS: usize = 8;
pub const MAX_POR_TURNO: usize = 4;
const VALIDADE: Duration = Duration::from_secs(30 * 60);

struct Guardada {
    desde: Instant,
    mime: &'static str,
    bytes: Vec<u8>,
}

static GUARDA: LazyLock<Mutex<HashMap<String, Guardada>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Tipo pela assinatura dos primeiros bytes; extensão e `Content-Type` não valem.
pub fn tipo(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        Some("image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

#[derive(Deserialize)]
pub struct ImagemQ {
    name: String,
}

pub async fn enviar_imagem(_: Admin, Query(q): Query<ImagemQ>, corpo: Bytes) -> ApiResult<Json<Value>> {
    let nome = q.name.trim();
    if nome.is_empty() || nome.chars().count() > 160 || nome.chars().any(|c| (c as u32) < 32 || c == '/' || c == '\\') {
        return erro(422, "image_name_invalid");
    }
    if corpo.is_empty() {
        return erro(422, "image_empty");
    }
    if corpo.len() > MAX_IMAGEM {
        return erro(413, "image_size_limit");
    }
    let Some(mime) = tipo(&corpo) else { return erro(422, "image_type_invalid") };
    let id = orion_store::uuid_hex();
    let tamanho = corpo.len();
    let mut g = GUARDA.lock().unwrap_or_else(|e| e.into_inner());
    g.retain(|_, v| v.desde.elapsed() < VALIDADE);
    if g.len() >= MAX_GUARDADAS {
        return erro(429, "image_pending_limit");
    }
    g.insert(id.clone(), Guardada { desde: Instant::now(), mime, bytes: corpo.to_vec() });
    Ok(Json(json!({"id": id, "name": nome, "mime": mime, "size": tamanho})))
}

/// Tira as imagens da guarda (valem para um turno só) e devolve as data URLs para o modelo.
pub fn tomar(ids: &[String]) -> ApiResult<Vec<String>> {
    if ids.len() > MAX_POR_TURNO {
        return erro(422, "imagens: até 4 por mensagem");
    }
    let mut g = GUARDA.lock().unwrap_or_else(|e| e.into_inner());
    g.retain(|_, v| v.desde.elapsed() < VALIDADE);
    // Confere todas antes de consumir qualquer uma: erro não gasta as outras.
    if let Some(falta) = ids.iter().find(|id| !g.contains_key(*id)) {
        return erro(404, format!("image_not_found: {falta}"));
    }
    Ok(ids
        .iter()
        .filter_map(|id| g.remove(id))
        .map(|i| format!("data:{};base64,{}", i.mime, base64::engine::general_purpose::STANDARD.encode(&i.bytes)))
        .collect())
}


#[derive(Deserialize)]
pub struct Liberar {
    ids: Vec<String>,
}

/// Solta imagens que você tirou do compositor, para não ocuparem a guarda até vencerem.
pub async fn liberar_imagens(_: Admin, Json(b): Json<Liberar>) -> ApiResult<Json<Value>> {
    if b.ids.len() > MAX_GUARDADAS {
        return erro(422, "imagens: ids demais");
    }
    let mut g = GUARDA.lock().unwrap_or_else(|e| e.into_inner());
    let soltas = b.ids.iter().filter(|id| g.remove(*id).is_some()).count();
    Ok(Json(json!({"ok": true, "liberadas": soltas})))
}
