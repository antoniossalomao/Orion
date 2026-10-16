//! Peças HTTP comuns: erro no formato do FastAPI, login obrigatório, `Host` permitido,
//! request-id e o stream SSE do chat.

use crate::estado::Estado;
use axum::body::Body;
use axum::extract::{FromRequestParts, Request, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures::{Stream, StreamExt};
use orion_core::agent::AgentEvent;
use serde_json::{json, Value};
use subtle::ConstantTimeEq;

pub const COOKIE_SESSAO: &str = "orion_session";
pub const CANAL: &str = r"^[a-z0-9_-]{1,32}$";

/// Erro de rota: `{"detail": ...}` com o status, como o `HTTPException` do FastAPI.
#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub detail: Value,
    pub headers: Vec<(&'static str, String)>,
}

impl ApiError {
    pub fn new(status: u16, detail: impl Into<String>) -> Self {
        ApiError { status: StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR), detail: Value::String(detail.into()), headers: vec![] }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut r = (self.status, Json(json!({"detail": self.detail}))).into_response();
        for (k, v) in self.headers {
            if let Ok(v) = HeaderValue::from_str(&v) {
                r.headers_mut().insert(k, v);
            }
        }
        r
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        tracing::error!("erro interno: {e:#}");
        ApiError::new(500, "Internal Server Error")
    }
}

impl From<orion_store::StoreError> for ApiError {
    fn from(e: orion_store::StoreError) -> Self {
        match e {
            orion_store::StoreError::NotFound(m) => ApiError::new(404, m),
            orion_store::StoreError::Invalid(m) => ApiError::new(409, m),
            outro => anyhow::anyhow!(outro).into(),
        }
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

pub fn erro<T>(status: u16, detail: impl Into<String>) -> ApiResult<T> {
    Err(ApiError::new(status, detail))
}

/// Confere com regex um campo de entrada (o `pattern` do pydantic).
pub fn conferir(valor: &str, padrao: &str, campo: &str) -> ApiResult<()> {
    if regex::Regex::new(padrao).map(|r| r.is_match(valor)).unwrap_or(false) {
        Ok(())
    } else {
        erro(422, format!("{campo}: valor inválido"))
    }
}

pub fn cookie(headers: &HeaderMap, nome: &str) -> Option<String> {
    headers.get_all("cookie").iter().filter_map(|v| v.to_str().ok()).flat_map(|v| v.split(';')).find_map(|par| {
        let (k, v) = par.trim().split_once('=')?;
        (k == nome).then(|| v.to_string())
    })
}

/// Defesa extra contra CSRF para quem entra por cookie: um POST de outro site traz `Origin`
/// diferente do `Host` e é recusado.
pub fn mesma_origem(headers: &HeaderMap) -> bool {
    let host = headers.get("host").and_then(|h| h.to_str().ok()).unwrap_or("");
    match headers.get("origin").and_then(|o| o.to_str().ok()) {
        Some(origem) => url::Url::parse(origem).ok().is_some_and(|u| {
            let netloc = match u.port() {
                Some(p) => format!("{}:{p}", u.host_str().unwrap_or("")),
                None => u.host_str().unwrap_or("").to_string(),
            };
            netloc == host
        }),
        None => matches!(headers.get("sec-fetch-site").and_then(|v| v.to_str().ok()), None | Some("same-origin") | Some("none")),
    }
}

fn token_igual(enviado: &str, esperado: &str) -> bool {
    !esperado.is_empty() && bool::from(enviado.as_bytes().ct_eq(esperado.as_bytes()))
}

/// "token" (credencial de máquina), "ponte", "sessao" (login com senha) ou `None`. Cabeçalho
/// `Authorization` presente e errado **não** cai para o cookie
/// (REGRAS.md: toda rota da API exige login).
pub fn quem_e(headers: &HeaderMap, st: &Estado) -> Option<&'static str> {
    if let Some(a) = headers.get("authorization") {
        let enviado = a.to_str().unwrap_or("");
        let enviado = enviado.strip_prefix("Bearer ").unwrap_or(enviado).trim();
        if token_igual(enviado, &st.settings.admin_token) {
            return Some("token");
        }
        if st.auth.device_scope(Some(enviado)).as_deref() == Some(crate::rotas::ponte::ESCOPO) {
            return Some("ponte");
        }
        return None;
    }
    st.auth.validate(cookie(headers, COOKIE_SESSAO).as_deref()).map(|_| "sessao")
}

/// Toda rota da API exige login: token de admin ou sessão do login.
pub struct Admin(pub &'static str);

#[axum::async_trait]
impl FromRequestParts<Estado> for Admin {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, st: &Estado) -> Result<Self, Self::Rejection> {
        if st.settings.admin_token.is_empty() && !st.auth.has_password() {
            return erro(503, "autenticação não configurada (ORION_ADMIN_TOKEN ou `orion set-password`)");
        }
        let Some(quem) = quem_e(&parts.headers, st) else { return erro(401, "login necessário") };
        let caminho = parts.uri.path();
        if quem == "ponte"
            && !(crate::rotas::ponte::rota_permitida(parts.method.as_str(), caminho) && !(st.modos.panico() && caminho != "/modo/panico"))
        {
            return erro(403, "o token da ponte não alcança esta rota");
        }
        let seguro = matches!(parts.method, Method::GET | Method::HEAD | Method::OPTIONS);
        if quem == "sessao" && !seguro && !mesma_origem(&parts.headers) {
            return erro(403, "origem não permitida");
        }
        Ok(Admin(quem))
    }
}

/// Recusa `Host` fora da lista (DNS rebinding a partir de uma página web).
pub async fn host_permitido(State(st): State<Estado>, req: Request, next: Next) -> Response {
    let host = req.headers().get("host").and_then(|h| h.to_str().ok()).unwrap_or("");
    let nome = host.rsplit_once(':').filter(|(_, p)| p.chars().all(|c| c.is_ascii_digit())).map(|(h, _)| h).unwrap_or(host);
    let nome = nome.trim_matches(['[', ']']);
    let ok = st.settings.allowed_hosts.iter().any(|p| {
        p == "*" || p == nome || p.strip_prefix("*.").is_some_and(|suf| nome.ends_with(&format!(".{suf}")))
    });
    if !ok {
        return (StatusCode::BAD_REQUEST, "Invalid host header").into_response();
    }
    next.run(req).await
}

pub async fn request_id(req: Request, next: Next) -> Response {
    let rid = req
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.chars().take(64).collect::<String>())
        .unwrap_or_else(|| uuid::Uuid::new_v4().simple().to_string()[..12].to_string());
    let mut r = next.run(req).instrument_rid(&rid).await;
    if let Ok(v) = HeaderValue::from_str(&rid) {
        r.headers_mut().insert("x-request-id", v);
    }
    r
}

trait Rid: Sized {
    fn instrument_rid(self, rid: &str) -> tracing::instrument::Instrumented<Self>;
}

impl<F: std::future::Future> Rid for F {
    fn instrument_rid(self, rid: &str) -> tracing::instrument::Instrumented<Self> {
        use tracing::Instrument;
        self.instrument(tracing::info_span!("req", request_id = %rid))
    }
}

/// Mesmo formato do `/chat` do legado (`text`, `tier`, `[DONE]`) + `tool`, `approval`, `error`.
pub fn sse(ev: &AgentEvent) -> String {
    let d = &ev.data;
    if ev.kind == "done" {
        let mut p = json!({"provenance": d.get("provenance").cloned().unwrap_or(json!({}))});
        for k in ["message_id", "session_id"] {
            if let Some(v) = d.get(k) {
                p[k] = v.clone();
            }
        }
        return format!("data: {}\n\ndata: [DONE]\n\n", orion_policy::pyjson::dumps(&p, false));
    }
    let corpo = match ev.kind {
        "text" => json!({"text": d.get("text")}),
        "tier" => {
            let s = |k: &str| d.get(k).and_then(Value::as_str).map(String::from).unwrap_or_else(|| "None".into());
            json!({"tier": format!("{}/{}", s("endpoint"), s("model"))})
        }
        "tool" | "activity" => json!({"tool": d}),
        "approval" => json!({"approval": d}),
        _ => json!({"error": d.get("message")}),
    };
    format!("data: {}\n\n", orion_policy::pyjson::dumps(&corpo, false))
}

/// Resposta `text/event-stream` com os eventos do agente; sempre termina com `[DONE]`.
pub fn stream_sse<S>(eventos: S) -> Response
where
    S: Stream<Item = AgentEvent> + Send + 'static,
{
    let corpo = async_stream::stream! {
        let mut terminou = false;
        futures::pin_mut!(eventos);
        while let Some(ev) = eventos.next().await {
            terminou = terminou || ev.kind == "done";
            yield Ok::<_, std::convert::Infallible>(sse(&ev));
        }
        if !terminou {
            yield Ok("data: [DONE]\n\n".to_string());
        }
    };
    Response::builder()
        .header("content-type", "text/event-stream; charset=utf-8")
        .header("cache-control", "no-cache")
        .body(Body::from_stream(corpo))
        .unwrap()
}

/// `datetime.fromtimestamp(ts, UTC).isoformat()`.
pub fn iso(ts: f64) -> String {
    let micros = ((ts.fract()) * 1e6).round() as u32;
    let dt = chrono::DateTime::from_timestamp(ts.trunc() as i64, micros.min(999_999) * 1000).unwrap_or_default();
    if micros == 0 {
        dt.format("%Y-%m-%dT%H:%M:%S+00:00").to_string()
    } else {
        dt.format("%Y-%m-%dT%H:%M:%S%.6f+00:00").to_string()
    }
}

impl From<orion_core::extensions::host::MCPError> for ApiError {
    fn from(e: orion_core::extensions::host::MCPError) -> Self {
        ApiError::new(422, e.code)
    }
}

impl From<orion_core::extensions::plugins::PluginError> for ApiError {
    fn from(e: orion_core::extensions::plugins::PluginError) -> Self {
        ApiError::new(422, e.0)
    }
}

impl From<orion_core::extensions::skills::SkillError> for ApiError {
    fn from(e: orion_core::extensions::skills::SkillError) -> Self {
        ApiError::new(422, e.0)
    }
}
