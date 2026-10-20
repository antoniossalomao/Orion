//! Modelos locais (GGUF e voz): o que o catálogo oferece, o que já está no disco, baixar e remover.
//! Tudo é pedido seu na tela; o download confere tamanho e SHA-256 (`orion_local::download`).

use crate::estado::Estado;
use crate::http::*;
use axum::extract::{Path, State};
use axum::Json;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{LazyLock, Mutex};

/// id → (baixados, total, erro) enquanto baixa ou depois de falhar.
static ANDAMENTO: LazyLock<Mutex<HashMap<String, (u64, u64, Option<String>)>>> = LazyLock::new(Default::default);

/// Pasta dos modelos: a do modelo ativo, a do manifesto do instalador ou `<dados>/modelos`.
fn pasta(st: &Estado) -> PathBuf {
    if let Some(d) = st.settings.local_model_path.as_ref().and_then(|p| p.parent()) {
        return d.to_path_buf();
    }
    let manifesto = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.join("orion-install.json")));
    if let Some(d) = manifesto.and_then(|m| std::fs::read(m).ok()).and_then(|b| serde_json::from_slice::<Value>(&b).ok()).and_then(|v| v["models_dir"].as_str().map(PathBuf::from)) {
        return d;
    }
    st.settings.data_dir.join("modelos")
}

pub async fn listar(_: Admin, State(st): State<Estado>) -> Json<Value> {
    let dir = pasta(&st);
    let andamento = ANDAMENTO.lock().map(|g| g.clone()).unwrap_or_default();
    let itens: Vec<Value> = orion_local::catalog::all()
        .into_iter()
        .map(|a| {
            let caminho = dir.join(&a.filename);
            let instalado = caminho.metadata().is_ok_and(|m| m.len() == a.bytes);
            let ativo = st.settings.local_model_path.as_ref().is_some_and(|p| p == &caminho);
            let (baixados, erro) = andamento.get(&a.id).map(|(d, _, e)| (*d, e.clone())).unwrap_or((0, None));
            let baixando = andamento.get(&a.id).is_some_and(|(_, _, e)| e.is_none()) && !instalado;
            json!({"id": a.id, "nome": a.name, "tipo": a.kind, "bytes": a.bytes, "instalado": instalado, "ativo": ativo, "baixando": baixando, "baixados": baixados, "erro": erro})
        })
        .collect();
    Json(json!({"pasta": dir.display().to_string(), "modelos": itens}))
}

pub async fn baixar(_: Admin, State(st): State<Estado>, Path(id): Path<String>) -> ApiResult<(axum::http::StatusCode, Json<Value>)> {
    let Some(a) = orion_local::catalog::find(&id) else { return erro(404, "modelo desconhecido") };
    let dir = pasta(&st);
    if dir.join(&a.filename).metadata().is_ok_and(|m| m.len() == a.bytes) {
        return Ok((axum::http::StatusCode::OK, Json(json!({"id": id, "estado": "instalado"}))));
    }
    {
        let mut g = ANDAMENTO.lock().map_err(|_| ApiError::new(500, "estado dos downloads indisponível"))?;
        if g.values().any(|(_, _, e)| e.is_none()) {
            return erro(409, "já há um download em andamento");
        }
        g.insert(id.clone(), (0, a.bytes, None));
    }
    let id2 = id.clone();
    tokio::task::spawn_blocking(move || {
        let cancel = AtomicBool::new(false);
        let r = orion_local::download::install(&id2, &dir, &cancel, &|feito, total| {
            if let Ok(mut g) = ANDAMENTO.lock() {
                g.insert(id2.clone(), (feito, total, None));
            }
        });
        if let Ok(mut g) = ANDAMENTO.lock() {
            match r {
                Ok(_) => {
                    g.remove(&id2);
                }
                Err(e) => {
                    g.insert(id2.clone(), (0, 0, Some(e.to_string())));
                }
            }
        }
    });
    Ok((axum::http::StatusCode::ACCEPTED, Json(json!({"id": id, "estado": "baixando"}))))
}

pub async fn remover(_: Admin, State(st): State<Estado>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let Some(a) = orion_local::catalog::find(&id) else { return erro(404, "modelo desconhecido") };
    let caminho = pasta(&st).join(&a.filename);
    if st.settings.local_model_path.as_ref().is_some_and(|p| p == &caminho) {
        return erro(409, "esse modelo está em uso: troque o modelo antes de remover");
    }
    if let Ok(mut g) = ANDAMENTO.lock() {
        g.remove(&id);
    }
    let removido = std::fs::remove_file(&caminho).is_ok();
    Ok(Json(json!({"ok": true, "removido": removido})))
}
