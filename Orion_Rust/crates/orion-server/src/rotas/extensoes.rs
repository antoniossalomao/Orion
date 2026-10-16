//! API de administração de plugins e conexões MCP, fora do catálogo de ferramentas do modelo.

use crate::estado::Estado;
use crate::http::*;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use orion_core::extensions::archives::{archive_files, archive_snapshot, MAX_ARCHIVE};
use orion_core::extensions::catalogo::{versao_maior, LOCAL};
use orion_core::extensions::host::parse_config;
use orion_core::extensions::importar::converter;
use orion_core::extensions::manager::{public_plugin, Classificacoes, PluginManager};
use orion_core::extensions::plugins::{perr, PluginError};
use orion_core::extensions::profiles::{profile, profiles};
use orion_policy::Risk;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

type Corpo = Json<Value>;

fn gerente(st: &Estado) -> ApiResult<Arc<PluginManager>> {
    st.extensions.clone().ok_or_else(|| ApiError::new(503, "extensions_unavailable"))
}

fn validar_escopo(st: &Estado, scope: &str) -> ApiResult<()> {
    if let Some(p) = scope.strip_prefix("project:") {
        let proj = orion_core::projects::Projects::new(st.memory.clone()).get(p).map_err(|e| ApiError::new(404, e.to_string()))?;
        if proj["archived"] == Value::Bool(true) {
            return erro(409, "project_archived");
        }
    }
    Ok(())
}

fn em_thread<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> impl std::future::Future<Output = ApiResult<T>> {
    async move { tokio::task::spawn_blocking(f).await.map_err(|e| ApiError::from(anyhow::anyhow!(e))) }
}

fn zip_corpo(headers: &HeaderMap, corpo: &Bytes) -> ApiResult<()> {
    let tipo = headers.get("content-type").and_then(|h| h.to_str().ok()).unwrap_or("").split(';').next().unwrap_or("").trim().to_string();
    if tipo != "application/zip" {
        return erro(415, "plugin_zip_required");
    }
    if corpo.len() > MAX_ARCHIVE {
        return erro(413, "archive_size_limit");
    }
    Ok(())
}

pub fn limite_zip() -> usize {
    MAX_ARCHIVE + 1024
}

pub async fn plugins_listar(_: Admin, State(st): State<Estado>) -> ApiResult<Corpo> {
    Ok(Json(json!(gerente(&st)?.list()?)))
}

pub async fn plugins_disponiveis(_: Admin) -> ApiResult<Corpo> {
    Ok(Json(json!(em_thread(profiles).await??)))
}

pub async fn instalar_perfil(_: Admin, State(st): State<Estado>, Path(id): Path<String>) -> ApiResult<Corpo> {
    let pacote = em_thread(move || profile(&id)).await??;
    let m = gerente(&st)?;
    let _g = m.lock.lock().await;
    let m2 = m.clone();
    let row = em_thread(move || m2.installer.install_snapshot(&pacote, "orion", true)).await??;
    Ok(Json(public_plugin(&row)))
}

#[derive(Deserialize)]
pub struct Instalar {
    folder: String,
    #[serde(default)]
    update: bool,
}

pub async fn instalar_pasta(_: Admin, State(st): State<Estado>, Json(b): Json<Instalar>) -> ApiResult<Corpo> {
    if b.folder.is_empty() || b.folder.len() > 4096 {
        return erro(422, "folder: valor inválido");
    }
    if !PathBuf::from(&b.folder).is_absolute() {
        return Err(perr("bundle_source_invalid").into());
    }
    let m = gerente(&st)?;
    let _g = m.lock.lock().await;
    let (m2, pasta) = (m.clone(), PathBuf::from(b.folder));
    let row = em_thread(move || m2.installer.install_folder(&pasta, "local", b.update)).await??;
    Ok(Json(public_plugin(&row)))
}

#[derive(Deserialize, Default)]
pub struct UpdateQ {
    #[serde(default)]
    update: bool,
}

pub async fn importar_zip(_: Admin, State(st): State<Estado>, Query(q): Query<UpdateQ>, headers: HeaderMap, corpo: Bytes) -> ApiResult<Corpo> {
    zip_corpo(&headers, &corpo)?;
    let pacote = em_thread(move || archive_snapshot(&corpo, "plugin.zip")).await??;
    let m = gerente(&st)?;
    let _g = m.lock.lock().await;
    let m2 = m.clone();
    let row = em_thread(move || m2.installer.install_snapshot(&pacote, "zip", q.update)).await??;
    Ok(Json(public_plugin(&row)))
}

/// Lê um ZIP no formato do Claude Code, do Codex ou do Orion e diz o que viraria um plugin e o que
/// ficaria de fora. Não instala nada.
pub async fn importar_previa(_: Admin, headers: HeaderMap, corpo: Bytes) -> ApiResult<Corpo> {
    zip_corpo(&headers, &corpo)?;
    let r = em_thread(move || archive_files(&corpo, "plugin.zip").and_then(converter).map(|c| c.resumo())).await??;
    Ok(Json(r))
}

/// Converte (skills apenas) e instala como qualquer pacote: inativo e sem concessão.
pub async fn importar_externo(_: Admin, State(st): State<Estado>, Query(q): Query<UpdateQ>, headers: HeaderMap, corpo: Bytes) -> ApiResult<Corpo> {
    zip_corpo(&headers, &corpo)?;
    let conv = em_thread(move || archive_files(&corpo, "plugin.zip").and_then(converter)).await??;
    let m = gerente(&st)?;
    let _g = m.lock.lock().await;
    let (m2, ignorado, origem) = (m.clone(), conv.ignorado.clone(), format!("import:{}", conv.formato));
    let pacote = conv.package;
    let row = em_thread(move || m2.installer.install_snapshot(&pacote, &origem, q.update)).await??;
    let mut p = public_plugin(&row);
    p["ignorado"] = json!(ignorado);
    Ok(Json(p))
}

fn catalogos(m: &PluginManager) -> ApiResult<Arc<orion_core::extensions::catalogo::Catalogos>> {
    m.catalogos.read().clone().ok_or_else(|| ApiError::new(503, "catalog_unavailable"))
}

/// Plugins dos catálogos (local e cadastrados). Versão nova é só aviso: nada atualiza.
pub async fn catalogo_listar(_: Admin, State(st): State<Estado>) -> ApiResult<Corpo> {
    let m = gerente(&st)?;
    let (entradas, fontes) = catalogos(&m)?.listar().await;
    let instalados: HashMap<String, String> = m.store.list()?.iter().map(|r| (r["id"].as_str().unwrap_or("").to_string(), r["version"].as_str().unwrap_or("").to_string())).collect();
    let plugins: Vec<Value> = entradas
        .iter()
        .map(|e| {
            let mut p = e.publico();
            p["instalado"] = json!(instalados.get(&e.id));
            p["atualizacao"] = json!(instalados.get(&e.id).is_some_and(|v| versao_maior(&e.version, v)));
            p["local"] = json!(e.fonte == LOCAL);
            p
        })
        .collect();
    Ok(Json(json!({"fontes": fontes, "plugins": plugins})))
}

/// Baixa um pacote do catálogo, confere o SHA-256 do índice e instala INATIVO e sem concessão.
pub async fn catalogo_instalar(_: Admin, State(st): State<Estado>, Json(b): Corpo) -> ApiResult<Corpo> {
    if let Some(k) = b.as_object().and_then(|o| o.keys().find(|k| !["fonte", "id", "version"].contains(&k.as_str()))) {
        return erro(422, format!("campo desconhecido: {k}"));
    }
    let fonte = b["fonte"].as_str().filter(|f| !f.is_empty() && f.len() <= 2048).ok_or_else(|| ApiError::new(422, "fonte: valor inválido"))?;
    let id = b["id"].as_str().ok_or_else(|| ApiError::new(422, "id: obrigatório"))?;
    conferir(id, r"^[a-z0-9][a-z0-9_-]{0,47}$", "id")?;
    let versao = b.get("version").and_then(Value::as_str);
    let m = gerente(&st)?;
    let (entrada, dados) = catalogos(&m)?.baixar(fonte, id, versao).await?;
    let nome = format!("{}.zip", entrada.id);
    let pacote = em_thread(move || archive_snapshot(&dados, &nome)).await??;
    if pacote.manifest.id != entrada.id || pacote.manifest.version != entrada.version {
        return Err(perr("catalog_manifest_mismatch").into());
    }
    let _g = m.lock.lock().await;
    let ja = m.store.list()?.iter().any(|r| r["id"].as_str() == Some(&entrada.id));
    let m2 = m.clone();
    let row = em_thread(move || m2.installer.install_snapshot(&pacote, "catalogo", ja)).await??;
    Ok(Json(public_plugin(&row)))
}

pub async fn versoes(_: Admin, State(st): State<Estado>, Path(id): Path<String>) -> ApiResult<Corpo> {
    let v: Vec<Value> = gerente(&st)?.store.versions(&id)?.iter().map(|r| json!({"digest": r["digest"], "version": r["version"], "capabilities": r["manifest"]["capabilities"]})).collect();
    Ok(Json(json!(v)))
}

pub async fn plano(_: Admin, State(st): State<Estado>, Path((id, digest)): Path<(String, String)>) -> ApiResult<Corpo> {
    let m = gerente(&st)?;
    Ok(Json(em_thread(move || m.installer.plan_update(&id, &digest)).await??))
}

struct Revisao {
    digest: String,
    capabilities: HashSet<String>,
    trusted_local: bool,
    authorized_remote: bool,
    classifications: Classificacoes,
    scope: String,
}

fn revisao(b: &Value) -> ApiResult<Revisao> {
    let o = b.as_object().ok_or_else(|| ApiError::new(422, "corpo inválido"))?;
    if let Some(k) = o.keys().find(|k| !["digest", "capabilities", "trusted_local", "authorized_remote", "classifications", "scope"].contains(&k.as_str())) {
        return erro(422, format!("campo desconhecido: {k}"));
    }
    let digest = b["digest"].as_str().unwrap_or("").to_string();
    conferir(&digest, r"^[a-f0-9]{64}$", "digest")?;
    let scope = b.get("scope").and_then(Value::as_str).unwrap_or("personal").to_string();
    conferir(&scope, r"^(personal|project:[a-f0-9]{32})$", "scope")?;
    let caps = match b.get("capabilities") {
        None => vec![],
        Some(Value::Array(a)) if a.len() <= 256 => a.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
        _ => return erro(422, "capabilities: valor inválido"),
    };
    let mut classifications: Classificacoes = HashMap::new();
    match b.get("classifications") {
        None => {}
        Some(Value::Object(m)) if m.len() <= 32 => {
            for (conn, tools) in m {
                let t = tools.as_object().ok_or_else(|| ApiError::new(422, "classifications: valor inválido"))?;
                let mut mm = HashMap::new();
                for (nome, r) in t {
                    mm.insert(nome.clone(), r.as_str().and_then(Risk::parse).ok_or_else(|| ApiError::new(422, "classifications: classe inválida"))?);
                }
                classifications.insert(conn.clone(), mm);
            }
        }
        _ => return erro(422, "classifications: valor inválido"),
    }
    Ok(Revisao { digest, capabilities: caps.into_iter().collect(), trusted_local: b.get("trusted_local").and_then(Value::as_bool).unwrap_or(false), authorized_remote: b.get("authorized_remote").and_then(Value::as_bool).unwrap_or(false), classifications, scope })
}

pub async fn ativar(_: Admin, State(st): State<Estado>, Path(id): Path<String>, Json(b): Corpo) -> ApiResult<Corpo> {
    let r = revisao(&b)?;
    let m = gerente(&st)?;
    validar_escopo(&st, &r.scope)?;
    let _g = m.lock.lock().await;
    Ok(Json(m.activate(&id, &r.digest, &r.capabilities, r.trusted_local, r.authorized_remote, &r.classifications, &r.scope).await?))
}

pub async fn desativar(_: Admin, State(st): State<Estado>, Path(id): Path<String>) -> ApiResult<Corpo> {
    let m = gerente(&st)?;
    let _g = m.lock.lock().await;
    Ok(Json(m.deactivate(&id, true).await?))
}

pub async fn escolher_versao(_: Admin, State(st): State<Estado>, Path(id): Path<String>, Json(b): Corpo) -> ApiResult<Corpo> {
    let r = revisao(&b)?;
    let m = gerente(&st)?;
    let _g = m.lock.lock().await;
    // antes de revogar a revisão atual, valida a revisão apresentada
    let (m2, i2, d2) = (m.clone(), id.clone(), r.digest.clone());
    em_thread(move || m2.installer.bundle(&i2, &d2)).await??;
    let alvo = m.store.version(&id, &r.digest)?;
    let caps: HashSet<String> = alvo["manifest"]["capabilities"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
    if r.capabilities != caps {
        return Err(perr("plugin_review_required").into());
    }
    m.deactivate(&id, true).await?;
    let (m2, i2) = (m.clone(), id.clone());
    let row = em_thread(move || m2.installer.change_version(&i2, &r.digest, &r.digest, &r.capabilities)).await??;
    Ok(Json(public_plugin(&row)))
}

pub async fn remover(_: Admin, State(st): State<Estado>, Path(id): Path<String>) -> ApiResult<Corpo> {
    let m = gerente(&st)?;
    let _g = m.lock.lock().await;
    m.deactivate(&id, true).await?;
    let m2 = m.clone();
    em_thread(move || m2.installer.uninstall(&id)).await??;
    Ok(Json(json!({"ok": true})))
}

pub async fn diagnostico(_: Admin, State(st): State<Estado>, Path(id): Path<String>) -> ApiResult<Corpo> {
    let m = gerente(&st)?;
    let row = m.store.get(&id)?;
    let cids = m.connections.lock().get(&id).cloned().unwrap_or_default();
    let conexoes: Vec<Value> = cids.iter().filter_map(|c| m.host.get(c)).map(|c| c.status()).collect();
    Ok(Json(json!({"id": id, "state": row["state"], "code": row["error"], "connections": conexoes})))
}

// ── conexões MCP revisadas ───────────────────────────────────────────────────

pub async fn conexoes(_: Admin, State(st): State<Estado>) -> ApiResult<Corpo> {
    Ok(Json(json!(gerente(&st)?.host.statuses())))
}

fn config_de(b: &Value) -> ApiResult<orion_core::extensions::host::ConnectionConfig> {
    parse_config(b).map_err(|_| ApiError::new(422, "connection_config_invalid"))
}

pub async fn configurar(_: Admin, State(st): State<Estado>, Json(b): Corpo) -> ApiResult<Corpo> {
    let c = config_de(&b)?;
    validar_escopo(&st, c.scope())?;
    let m = gerente(&st)?;
    let _g = m.lock.lock().await;
    if m.host.contains(c.id()) || m.host.len() >= 32 {
        return Err(perr("connection_id_conflict").into());
    }
    m.save_connection(&c)?;
    let id = c.id().to_string();
    m.host.insert(m.host.connection(c));
    Ok(Json(json!({"id": id, "state": "configured"})))
}

pub async fn reconfigurar(_: Admin, State(st): State<Estado>, Path(id): Path<String>, Json(b): Corpo) -> ApiResult<Corpo> {
    let c = config_de(&b)?;
    validar_escopo(&st, c.scope())?;
    if c.id() != id {
        return Err(perr("connection_id_conflict").into());
    }
    let m = gerente(&st)?;
    let _g = m.lock.lock().await;
    if m.dona_da_conexao(&id) {
        return Err(perr("plugin_disable_first").into());
    }
    let antiga = m.host.remove(&id).ok_or_else(|| perr("connection_not_found"))?;
    antiga.close().await;
    let _ = m.catalog.refresh().await;
    m.save_connection(&c)?;
    m.host.insert(m.host.connection(c));
    Ok(Json(json!({"id": id, "state": "configured"})))
}

pub async fn desligar_conexao(_: Admin, State(st): State<Estado>, Path(id): Path<String>) -> ApiResult<Corpo> {
    let m = gerente(&st)?;
    let _g = m.lock.lock().await;
    if m.dona_da_conexao(&id) {
        return Err(perr("plugin_disable_first").into());
    }
    let antiga = m.host.get(&id).ok_or_else(|| perr("connection_not_found"))?;
    antiga.close().await;
    let _ = m.catalog.refresh().await;
    let cfg = antiga.config.desligada();
    m.save_connection(&cfg)?;
    m.host.insert(m.host.connection(cfg));
    Ok(Json(json!({"id": id, "state": "disabled"})))
}

/// Só handshake/discovery. Nunca call_tool nem read_resource/prompt.
pub async fn testar_conexao(_: Admin, State(st): State<Estado>, Path(id): Path<String>) -> ApiResult<Corpo> {
    let m = gerente(&st)?;
    let _g = m.lock.lock().await;
    let c = m.host.get(&id).ok_or_else(|| perr("connection_not_found"))?;
    if c.state() == "failed" {
        c.close().await;
    }
    c.start().await;
    let tools = if c.state() == "connected" { c.list_tools().await? } else { vec![] };
    let _ = m.catalog.refresh().await;
    let mut s = c.status();
    s["tools"] = json!(tools.iter().take(128).map(|t| json!({"name": t.name})).collect::<Vec<_>>());
    Ok(Json(s))
}

pub async fn desconectar(_: Admin, State(st): State<Estado>, Path(id): Path<String>) -> ApiResult<Corpo> {
    let m = gerente(&st)?;
    let _g = m.lock.lock().await;
    if m.dona_da_conexao(&id) {
        return Err(perr("plugin_disable_first").into());
    }
    let c = m.host.remove(&id).ok_or_else(|| perr("connection_not_found"))?;
    c.close().await;
    let _ = m.catalog.refresh().await;
    m.forget_connection(&id)?;
    Ok(Json(json!({"ok": true})))
}

