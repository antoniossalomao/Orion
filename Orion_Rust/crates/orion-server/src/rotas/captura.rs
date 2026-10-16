//! Captura rápida, ler depois. O token da ponte alcança só `/captura` (REGRAS.md: ponte de desktop
//! só abre e cola o que você aprovou).

use crate::estado::Estado;
use crate::http::*;
use axum::extract::{Path, Query, State};
use axum::Json;
use orion_core::capture::Capturer;
use orion_core::captura_rapida::{classificar, sem_acento, MAX_TEXTO};
use orion_core::ler_depois::so_um_link;
use orion_core::vida::{capsula, categoria_de, dias_da_capsula, gasto_por_modelo, parse_gasto, VERBO_GASTO};
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
    if VERBO_GASTO.is_match(&sem_acento(&texto)) {
        gateway = None;
    }
    let (mut c, mut origem) = classificar(gateway, &texto, chrono::Local::now()).await;
    if c.tipo == "gasto" && c.valor.is_none() && parse_gasto(&texto).is_none() {
        if let Some(g) = gasto_por_modelo(&texto, st.gateway_local.as_ref()).await {
            c = orion_core::captura_rapida::Classificacao { tipo: "gasto".into(), titulo: g["description"].as_str().unwrap_or("").into(), quando: None, valor: Some(g["cents"].as_f64().unwrap_or(0.0) / 100.0) };
            origem = "modelo local";
        }
    }
    let mut tipo = c.tipo.clone();
    let mut aviso = Value::Null;
    let dias = if regex::Regex::new(r"\bme (cobra|lembra|avisa)").unwrap().is_match(&sem_acento(&texto)) { dias_da_capsula(&texto) } else { None };
    if let Some(d) = dias {
        // E10.6: "daqui 6 meses me cobra isso"
        let cap = capsula(&st.memory, &texto.chars().take(200).collect::<String>(), d, &texto).map_err(|e| ApiError::new(422, e.0))?;
        return Ok(Json(json!({
            "id": st.desfazer.registrar("capsula", cap["id"].clone()), "tipo": "capsula", "titulo": cap["titulo"], "quando": cap["quando"],
            "valor": null, "origem": origem, "aviso": "cápsula guardada",
        })));
    }
    let g = if tipo == "gasto" { parse_gasto(&texto) } else { None }; // "gastei 30 no mercado" sem R$
    let cents = g.as_ref().and_then(|g| g["cents"].as_i64()).or(c.valor.map(|v| (v * 100.0).round() as i64));
    if tipo == "gasto" && cents.is_some_and(|x| x > 0) {
        // E10.3 (REGRAS.md: finanças são locais): o gasto vai ao banco local, nunca a provedor de fora
        let cents = cents.unwrap_or(0);
        let (cat, desc) = match &g {
            Some(g) => (g["category"].as_str().unwrap_or("outros").to_string(), g["description"].as_str().unwrap_or("").to_string()),
            None => (categoria_de(&c.titulo), c.titulo.clone()),
        };
        let l = st.financas.registrar_gasto(cents, &cat, &desc, "captura").map_err(|e| ApiError::new(422, e.0))?;
        return Ok(Json(json!({
            "id": st.desfazer.registrar("gasto", l["id"].clone()), "tipo": "gasto", "titulo": desc, "quando": null,
            "valor": cents as f64 / 100.0, "origem": origem, "aviso": format!("gasto lançado em {cat}"),
        })));
    }
    if tipo == "gasto" {
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
        "gasto" => st.financas.apagar_gasto(n).map_err(|e| ApiError::new(409, e.0))?,
        "capsula" => {
            st.memory.tx(|c| Ok(c.execute("DELETE FROM reminders WHERE id=? AND kind='capsula'", [n]).map(|_| ())?))?;
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
