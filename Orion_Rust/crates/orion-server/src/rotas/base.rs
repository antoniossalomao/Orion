//! Descoberta, saúde e login.

use crate::estado::Estado;
use crate::http::*;
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, HeaderValue};
use axum::response::{IntoResponse, Response};
use axum::Json;
use orion_core::auth::AuthError;
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::net::SocketAddr;

pub const VERSAO: &str = "0.1.0";

const PENDING_FEATURES: [&str; 9] = ["memory_categories", "metrics", "stats", "stats_history", "integrations", "upload", "tts", "voice", "model_selection"];

/// Contrato público de capacidades: flags de recurso utilizável, sem caminho, conta ou segredo.
pub fn describe(agent_ready: bool, admin: bool, voice: bool, tts: bool) -> Value {
    let mut f = Map::new();
    for p in PENDING_FEATURES {
        f.insert(p.into(), json!(false));
    }
    let lista = [
        ("chat", agent_ready && admin),
        ("sessions", admin),
        ("session_management", admin),
        ("session_search", admin),
        ("history", admin),
        ("history_clear", admin),
        ("message_edit", agent_ready && admin),
        ("export", admin),
        ("approvals", admin),
        ("notifications", admin),
        ("activity", admin),
        ("documents", admin),
        ("branches", admin),
        ("accounts", admin),
        ("calendar", admin),
        ("file_plans", admin),
        ("skills", admin),
        ("plugins", admin),
        ("plugin_profiles", admin),
        ("projects", admin),
        ("artifacts", admin),
        ("memory_facts", admin),
        ("memory_graph", admin),
        ("global_search", admin),
        ("screen_search", admin),
        ("library_panel", admin),
        ("research_ui", admin),
        ("automacao", admin),
        ("mcp_servers", admin),
        ("mcp", admin),
        ("mcp_export", admin),
        ("html_preview", admin && !cfg!(windows)),
        ("voice", admin && voice),
        ("tts", admin && tts),
    ];
    for (k, v) in lista {
        f.insert(k.into(), json!(v));
    }
    let mut u = Map::new();
    for p in PENDING_FEATURES {
        u.insert(p.into(), json!("not_implemented"));
    }
    for pronto in ["voice", "tts"] {
        if f[pronto] == json!(true) {
            u.remove(pronto);
        }
    }
    if f["html_preview"] != json!(true) {
        u.insert("html_preview".into(), json!("desktop_validation_pending"));
    }
    let motivo = if !admin { "auth_not_configured" } else { "gateway_not_configured" };
    if f["chat"] != json!(true) {
        u.insert("chat".into(), json!(motivo));
    }
    if f["message_edit"] != json!(true) {
        u.insert("message_edit".into(), json!(motivo));
    }
    if !admin {
        for (k, _) in lista.iter().take(31).filter(|(k, _)| *k != "chat") {
            u.insert((*k).into(), json!("auth_not_configured"));
        }
    }
    json!({
        "contract_version": 1, "backend": "orion", "app_version": VERSAO, "api": "online",
        "model": if agent_ready { "ready" } else { "unavailable" }, "auth_required": true,
        "features": f, "unavailable": u,
    })
}

pub async fn capabilities(State(st): State<Estado>) -> Json<Value> {
    let voice = st.settings.voice_enabled && st.agent.is_some() && st.transcriber.is_some();
    let tts = st.speaker.is_some();
    Json(describe(st.agent.is_some(), !st.settings.admin_token.is_empty() || st.auth.has_password(), voice, tts))
}

/// Informações da instalação nunca entram na descoberta anônima.
pub async fn capability_details(_: Admin, State(st): State<Estado>) -> Json<Value> {
    let permissions = *st.tool_permissions.lock();
    Json(json!({
        "gateway_model": st.agent.as_ref().map(|_| st.settings.gateway_model.clone()),
        "components": {
            "memory": st.memory.ping(), "vectors": st.memory.vectors_available, "jobs": st.jobs.is_some(),
            "telegram": st.telegram.is_some(), "desktop_tools": st.agent.is_some() && permissions.desktop_tools,
            "web_tools": st.agent.is_some() && permissions.web_tools,
            "without_confirmation": permissions.without_confirmation,
        },
        "tools": st.agent.as_ref().map(|a| a.tools.read().names().into_iter().filter(|n| st.policy.tool_enabled(n)).collect::<Vec<_>>()).unwrap_or_default(),
    }))
}

/// Sem login só o mínimo (serve de sonda); os detalhes pedem sessão ou token.
pub async fn health(State(st): State<Estado>, headers: HeaderMap) -> Json<Value> {
    let memoria = if st.memory.ping() { "ok" } else { "erro" };
    let mut basico = json!({"status": if memoria == "ok" { "ok" } else { "degradado" }, "version": VERSAO});
    if quem_e(&headers, &st).is_none() {
        return Json(basico);
    }
    basico["uptime_s"] = json!(((orion_core::util::agora() - st.started_at) * 10.0).round() / 10.0);
    basico["components"] = json!({
        "memory": memoria, "vectors": st.memory.vectors_available, "gateway": st.agent.is_some(),
        "jobs": st.jobs.is_some(), "telegram": st.telegram.is_some(),
        "mcp": st.mcp.as_ref().map(|m| json!(m.status())).unwrap_or_else(|| json!({})),
        "voice": {
            "click": st.settings.voice_enabled && st.agent.is_some() && st.transcriber.is_some(),
            "live": false,
        },
    });
    basico["pending_approvals"] = json!(st.policy.approvals.pending(None, false).len());
    basico["pending_notifications"] = json!(st.ops.pending_notifications(1000, true).map(|v| v.len()).unwrap_or(0));
    Json(basico)
}

/// Público: o front decide entre o login e o app. Se a senha ainda é a de fábrica, isso só
/// aparece depois do login.
pub async fn auth_status(State(st): State<Estado>, headers: HeaderMap) -> Json<Value> {
    let autenticado = quem_e(&headers, &st).is_some();
    let mut s = json!({"configured": st.auth.has_password(), "authenticated": autenticado, "token_auth": !st.settings.admin_token.is_empty()});
    if autenticado {
        s["default_password"] = json!(st.auth.uses_default_password());
    }
    Json(s)
}

fn cookie_seguro(st: &Estado, headers: &HeaderMap) -> bool {
    st.settings.cookie_secure || headers.get("x-forwarded-proto").and_then(|v| v.to_str().ok()) == Some("https")
}

pub fn abrir_sessao(r: &mut Response, st: &Estado, headers: &HeaderMap, token: &str) {
    let seguro = if cookie_seguro(st, headers) { "; Secure" } else { "" };
    let c = format!("{COOKIE_SESSAO}={token}; HttpOnly; Max-Age={}; Path=/; SameSite=strict{seguro}", st.settings.session_ttl_h * 3600);
    if let Ok(v) = HeaderValue::from_str(&c) {
        r.headers_mut().append("set-cookie", v);
    }
}

#[derive(Deserialize)]
pub struct Login {
    usuario: Option<String>,
    senha: String,
}

pub async fn login(State(st): State<Estado>, ConnectInfo(addr): ConnectInfo<SocketAddr>, headers: HeaderMap, Json(corpo): Json<Login>) -> ApiResult<Response> {
    if !mesma_origem(&headers) {
        return erro(403, "origem não permitida");
    }
    if corpo.senha.is_empty() || corpo.senha.chars().count() > 256 || corpo.usuario.as_ref().is_some_and(|u| u.chars().count() > 64) {
        return erro(422, "senha ou usuário inválido");
    }
    let cliente = addr.ip().to_string();
    let st2 = st.clone();
    let (senha, usuario) = (corpo.senha, corpo.usuario);
    let r = tokio::task::spawn_blocking(move || st2.auth.verify_credentials(&senha, &cliente, usuario.as_deref())).await.map_err(|e| anyhow::anyhow!(e))?;
    match r {
        Ok(()) => orion_core::faceid::zerar_falhas(),
        Err(AuthError::LockedOut(s)) => {
            let mut e = ApiError::new(429, format!("muitas tentativas; tente de novo em {s}s"));
            e.headers.push(("Retry-After", s.to_string()));
            return Err(e);
        }
        Err(AuthError::NotConfigured(m)) => return erro(503, m),
        Err(_) => {
            tracing::warn!(target: "orion.audit", "login_falhou cliente={}", addr.ip());
            return erro(401, "usuário ou senha incorretos");
        }
    };
    // REGRAS.md: login com senha e sessão revogável. Não há sessão antes do segundo fator.
    if st.settings.telegram_2fa {
        let Some(tg) = &st.telegram else {
            return erro(503, "Telegram indisponível para o segundo fator");
        };
        let (id, code) = st.two_factor.issue(&addr.ip().to_string()).map_err(|m| ApiError::new(429, m))?;
        if tg.send_login_code(&code).await.is_err() {
            st.two_factor.cancel(&id);
            return erro(503, "não foi possível entregar o código pelo Telegram");
        }
        return Ok((axum::http::StatusCode::ACCEPTED, Json(json!({"two_factor_required": true, "challenge": id, "expires_in": 300}))).into_response());
    }
    let token = st.auth.create_session().map_err(|e| anyhow::anyhow!(e))?;
    let mut r = Json(json!({"ok": true})).into_response();
    abrir_sessao(&mut r, &st, &headers, &token);
    Ok(r)
}

pub async fn logout(State(st): State<Estado>, headers: HeaderMap) -> Response {
    st.auth.logout(cookie(&headers, COOKIE_SESSAO).as_deref());
    let mut r = Json(json!({"ok": true})).into_response();
    r.headers_mut()
        .append("set-cookie", HeaderValue::from_static("orion_session=\"\"; Max-Age=0; Path=/; SameSite=lax; expires=Thu, 01 Jan 1970 00:00:00 GMT"));
    r
}

#[derive(Deserialize)]
pub struct TrocaDeSenha {
    #[serde(default)]
    senha_atual: String,
    nova: String,
}

/// Troca (ou define a primeira) senha. Revoga todas as sessões e abre uma nova para quem trocou.
pub async fn trocar_senha(Admin(quem): Admin, State(st): State<Estado>, headers: HeaderMap, Json(c): Json<TrocaDeSenha>) -> ApiResult<Response> {
    let st2 = st.clone();
    let r = tokio::task::spawn_blocking(move || -> ApiResult<Option<String>> {
        if st2.auth.has_password() && !st2.auth.check_password(&c.senha_atual) {
            return erro(401, "senha atual incorreta");
        }
        // valida antes de reembrulhar a chave do banco: senha fraca não pode deixar chave e login dessincronizados
        match st2.auth.validar_senha_nova(&c.nova) {
            Ok(()) => {}
            Err(AuthError::WeakPassword(m)) => return erro(422, m),
            Err(e) => return Err(anyhow::anyhow!(e).into()),
        }
        if st2.auth.has_password() {
            if let Err(e) = orion_core::onboarding::rewrap_key(&st2.settings.data_dir, &c.senha_atual, &c.nova) {
                return erro(422, e.to_string());
            }
        }
        match st2.auth.set_password(&c.nova) {
            Ok(()) => {}
            Err(AuthError::WeakPassword(m)) => return erro(422, m),
            Err(e) => return Err(anyhow::anyhow!(e).into()),
        }
        Ok(if quem == "sessao" { st2.auth.login(&c.nova, "troca-de-senha", None).ok() } else { None })
    })
    .await
    .map_err(|e| anyhow::anyhow!(e))??;
    let mut resp = Json(json!({"ok": true})).into_response();
    if let Some(t) = r {
        abrir_sessao(&mut resp, &st, &headers, &t);
    }
    Ok(resp)
}

#[derive(Deserialize)]
pub struct SecondFactor {
    challenge: String,
    code: String,
}
pub async fn second_factor(
    State(st): State<Estado>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(c): Json<SecondFactor>,
) -> ApiResult<Response> {
    if !mesma_origem(&headers) {
        return erro(403, "origem não permitida");
    }
    if !st.two_factor.verify(&c.challenge, &c.code, &addr.ip().to_string()) {
        return erro(401, "código inválido ou expirado");
    }
    let token = st.auth.create_session().map_err(|e| anyhow::anyhow!(e))?;
    let mut r = Json(json!({"ok": true})).into_response();
    abrir_sessao(&mut r, &st, &headers, &token);
    Ok(r)
}
