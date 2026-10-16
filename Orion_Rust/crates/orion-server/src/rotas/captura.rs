//! Captura rápida, ler depois. O token da ponte alcança só `/captura` (REGRAS.md: ponte de desktop
//! só abre e cola o que você aprovou).

use crate::estado::Estado;
use crate::http::*;
use axum::extract::{Path, Query, State};
use axum::Json;
use orion_core::capture::Capturer;
use orion_core::captura_rapida::{classificar, sem_acento, MAX_TEXTO};
use orion_core::ler_depois::so_um_link;
use serde::Deserialize;
use serde_json::{json, Value};

fn capturer(st: &Estado) -> ApiResult<Capturer> {
    match &st.settings.vault_dir {
        Some(v) => Ok(Capturer::new(v.clone(), &st.settings.capture_folder)),
        None => erro(409, "para guardar notas, defina ORION_VAULT_DIR"),
    }
}

#[derive(Deserialize)]
pub struct PedidoCaptura {
    texto: String,
}

pub async fn capturar(_: Admin, State(st): State<Estado>, Json(c): Json<PedidoCaptura>) -> ApiResult<Json<Value>> {
    let n = c.texto.chars().count();
    if !(1..=MAX_TEXTO).contains(&n) {
        return erro(422, "texto: entre 1 e 2000 caracteres");
    }
    let texto = c.texto.trim().to_string();
    if texto.is_empty() {
        return erro(422, "texto vazio");
    }
    // E9.8: só um link → ler depois
    if let Some(link) = so_um_link(&texto) {
        let item = st.ler_depois.adicionar(&link).map_err(|e| ApiError::new(422, e.0))?;
        if item["status"] == "pendente" {
            st.ler_depois.agendar(item["id"].as_i64().unwrap_or(0));
        }
        return Ok(Json(json!({
            "id": st.desfazer.registrar("leitura", item["id"].clone()), "tipo": "leitura", "titulo": link,
            "quando": null, "valor": null, "origem": "regra", "aviso": "guardei para ler depois",
        })));
    }
    // em pânico (REGRAS.md: pânico é corte; não perturbe só segura) nada vai ao modelo
    let mut gateway = st.gateway.as_deref();
    if st.modos.panico() {
        gateway = None;
    }
    // REGRAS.md: texto de gasto não vai à nuvem; a regra decide
    if sem_acento(&texto).split_whitespace().any(|p| ["gastei", "paguei", "comprei", "custou"].contains(&p)) {
        gateway = None;
    }
    let (c, origem) = classificar(gateway, &texto, chrono::Local::now()).await;
    let mut tipo = c.tipo.clone();
    let mut aviso = Value::Null;
    if tipo == "gasto" {
        // finanças locais (E10.3) ainda não existem neste lado: guarda como nota
        tipo = "nota".into();
        aviso = json!("diga o valor para lançar o gasto; guardei como nota");
    }
    let refe: Value = match tipo.as_str() {
        "tarefa" => st.ops.add_task(&c.titulo).map_err(|e| ApiError::new(422, e.to_string()))?["id"].clone(),
        "lembrete" => st.ops.add_reminder(&c.titulo, &c.quando.clone().unwrap_or_default().as_str().into(), "", 0).map_err(|e| ApiError::new(422, e.to_string()))?["id"].clone(),
        _ => {
            let cap = capturer(&st)?;
            let p = cap.save_text(&texto, "texto", "orion", MAX_TEXTO).map_err(|e| ApiError::new(409, e.0))?;
            json!(cap.relativo(&p))
        }
    };
    st.ops.audit_add(&json!({"tool": "captura_rapida", "action": "allow", "risk": "write", "reason": tipo}))?;
    Ok(Json(json!({
        "id": st.desfazer.registrar(&tipo, refe), "tipo": tipo, "titulo": c.titulo,
        "quando": if tipo == "lembrete" { json!(c.quando) } else { Value::Null }, "valor": c.valor, "origem": origem, "aviso": aviso,
    })))
}

pub async fn desfazer_captura(_: Admin, State(st): State<Estado>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let Some((tipo, refe)) = st.desfazer.tirar(&id) else { return erro(404, "não há mais o que desfazer (passaram 10 minutos?)") };
    let n = refe.as_i64().unwrap_or(0);
    match tipo.as_str() {
        "leitura" => {
            st.ler_depois.descartar(n).map_err(|e| ApiError::new(404, e.0))?;
        }
        "tarefa" => {
            st.ops.remove_task(n)?;
        }
        "lembrete" => {
            st.ops.remove_reminder(n)?;
        }
        _ => {
            capturer(&st)?.apagar_nota(refe.as_str().unwrap_or("")).map_err(|e| ApiError::new(409, e.0))?;
        }
    }
    st.ops.audit_add(&json!({"tool": "captura_rapida", "action": "allow", "risk": "write", "reason": format!("desfez {tipo}")}))?;
    Ok(Json(json!({"ok": true})))
}

#[derive(Deserialize)]
pub struct LinkLerDepois {
    url: String,
}

pub async fn ler_depois_adicionar(_: Admin, State(st): State<Estado>, Json(c): Json<LinkLerDepois>) -> ApiResult<Json<Value>> {
    let item = st.ler_depois.adicionar(&c.url).map_err(|e| ApiError::new(422, e.0))?;
    if item["status"] == "pendente" {
        st.ler_depois.agendar(item["id"].as_i64().unwrap_or(0));
    }
    Ok(Json(item))
}

#[derive(Deserialize)]
pub struct StatusQ {
    status: Option<String>,
}

pub async fn ler_depois_listar(_: Admin, State(st): State<Estado>, Query(q): Query<StatusQ>) -> ApiResult<Json<Value>> {
    if q.status.as_ref().is_some_and(|s| s.chars().count() > 20) {
        return erro(422, "status grande demais");
    }
    Ok(Json(json!({"itens": st.ler_depois.listar(q.status.as_deref(), 50).map_err(|e| ApiError::new(409, e.0))?})))
}

pub async fn ler_depois_descartar(_: Admin, State(st): State<Estado>, Path(rid): Path<i64>) -> ApiResult<Json<Value>> {
    Ok(Json(st.ler_depois.descartar(rid).map_err(|e| ApiError::new(404, e.0))?))
}
