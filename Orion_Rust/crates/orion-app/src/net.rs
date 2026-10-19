//! Cliente HTTP do servidor local do Orion (`orion serve`): login por senha com cookie de sessão,
//! leituras tolerantes (JSON solto, nada de schema rígido) e o chat por SSE.

use futures::{Stream, StreamExt};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

pub const COOKIE: &str = "orion_session";

#[derive(Clone)]
pub struct Api {
    pub base: String,
    http: reqwest::Client,
    cookie: Arc<Mutex<Option<String>>>,
    bearer: Option<String>,
}

#[derive(Debug, Clone)]
pub enum LoginResult {
    Ok,
    /// Segundo fator pelo Telegram: guarda o desafio.
    TwoFactor(String),
}

#[derive(Debug, Clone)]
pub enum Chat {
    Text(String),
    Model(String),
    Tool(Value),
    Approval { id: String, tool: String, reason: String, args: Value },
    Error(String),
    Done { message_id: Option<i64>, session_id: Option<String> },
}

/// Mensagem em português para cada falha de HTTP (mesmas do front HTML).
pub fn mensagem_http(status: u16, corpo: &str, caminho: &str) -> String {
    let detalhe = serde_json::from_str::<Value>(corpo).ok().and_then(|j| j.get("detail").or(j.get("erro")).or(j.get("error")).map(|d| d.as_str().map(String::from).unwrap_or_else(|| d.to_string()))).unwrap_or_else(|| corpo.chars().take(160).collect());
    if caminho == "/auth/login" {
        match status {
            401 => return "Usuário ou senha incorretos.".into(),
            429 => return "Muitas tentativas erradas. Espere um pouco para tentar de novo.".into(),
            503 => return "Ainda não há senha definida: rode \"orion set-password\" no computador do Orion.".into(),
            _ => {}
        }
    }
    match status {
        401 | 403 => "Acesso negado: entre com a senha.".into(),
        404 => if detalhe.is_empty() { "Recurso não encontrado neste servidor.".into() } else { detalhe },
        409 => if detalhe.is_empty() { "Essa ação já foi tratada.".into() } else { detalhe },
        s if s >= 500 => format!("O servidor falhou ({s}){}.", if detalhe.is_empty() { String::new() } else { format!(": {detalhe}") }),
        _ => if detalhe.is_empty() { format!("Erro {status}.") } else { detalhe },
    }
}

impl Api {
    /// `ORION_URL` (padrão `http://127.0.0.1:8000`) e, opcionalmente, `ORION_ADMIN_TOKEN`.
    pub fn from_env() -> Self {
        let base = std::env::var("ORION_URL").unwrap_or_else(|_| "http://127.0.0.1:8000".into()).trim_end_matches('/').to_string();
        let bearer = std::env::var("ORION_ADMIN_TOKEN").ok().filter(|t| !t.is_empty());
        Self::new(&base, bearer)
    }

    pub fn new(base: &str, bearer: Option<String>) -> Self {
        let http = reqwest::Client::builder().timeout(std::time::Duration::from_secs(30)).connect_timeout(std::time::Duration::from_secs(3)).build().expect("cliente http");
        Api { base: base.trim_end_matches('/').to_string(), http, cookie: Arc::new(Mutex::new(None)), bearer }
    }

    fn req(&self, m: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let mut r = self.http.request(m, format!("{}{}", self.base, path)).header("Origin", &self.base);
        if let Some(b) = &self.bearer {
            r = r.bearer_auth(b);
        } else if let Some(c) = self.cookie.lock().ok().and_then(|c| c.clone()) {
            r = r.header("Cookie", format!("{COOKIE}={c}"));
        }
        r
    }

    async fn send(&self, r: reqwest::RequestBuilder, path: &str) -> Result<Value, String> {
        let resp = r.send().await.map_err(|e| if e.is_connect() || e.is_timeout() { format!("Servidor do Orion indisponível em {}.", self.base) } else { e.to_string() })?;
        let status = resp.status().as_u16();
        let text = resp.text().await.map_err(|e| e.to_string())?;
        if !(200..300).contains(&status) {
            return Err(mensagem_http(status, &text, path));
        }
        Ok(serde_json::from_str(&text).unwrap_or(Value::Null))
    }

    pub async fn get(&self, path: &str) -> Result<Value, String> {
        self.send(self.req(reqwest::Method::GET, path), path).await
    }
    pub async fn post(&self, path: &str, body: Value) -> Result<Value, String> {
        self.send(self.req(reqwest::Method::POST, path).json(&body), path).await
    }
    pub async fn patch(&self, path: &str, body: Value) -> Result<Value, String> {
        self.send(self.req(reqwest::Method::PATCH, path).json(&body), path).await
    }
    pub async fn put(&self, path: &str, body: Value) -> Result<Value, String> {
        self.send(self.req(reqwest::Method::PUT, path).json(&body), path).await
    }
    pub async fn delete(&self, path: &str) -> Result<Value, String> {
        self.send(self.req(reqwest::Method::DELETE, path), path).await
    }

    /// Servidor no ar? (`/health` não exige login). Com token de máquina já vale como entrada.
    pub async fn health(&self) -> Result<(), String> {
        let r = self.http.get(format!("{}/health", self.base)).send().await.map_err(|_| format!("Servidor do Orion indisponível em {}.", self.base))?;
        if r.status().is_success() {
            Ok(())
        } else {
            Err(format!("O servidor respondeu {}.", r.status().as_u16()))
        }
    }

    pub async fn auth_ok(&self) -> bool {
        self.get("/auth/status").await.map(|v| v.get("authenticated").and_then(Value::as_bool).unwrap_or(false)).unwrap_or(false)
    }

    pub async fn login(&self, senha: &str) -> Result<LoginResult, String> {
        let r = self.req(reqwest::Method::POST, "/auth/login").json(&json!({"senha": senha}));
        let resp = r.send().await.map_err(|_| format!("Servidor do Orion indisponível em {}.", self.base))?;
        let status = resp.status().as_u16();
        let set = resp.headers().get_all("set-cookie").iter().filter_map(|v| v.to_str().ok()).find(|c| c.starts_with(COOKIE)).map(|c| c.split(';').next().unwrap_or("").trim_start_matches(&format!("{COOKIE}=")).trim_matches('"').to_string());
        let text = resp.text().await.map_err(|e| e.to_string())?;
        match status {
            200 => {
                if let (Some(c), Ok(mut slot)) = (set, self.cookie.lock()) {
                    *slot = Some(c);
                }
                Ok(LoginResult::Ok)
            }
            202 => Ok(LoginResult::TwoFactor(serde_json::from_str::<Value>(&text).ok().and_then(|v| v.get("challenge").and_then(Value::as_str).map(String::from)).unwrap_or_default())),
            s => Err(mensagem_http(s, &text, "/auth/login")),
        }
    }

    pub async fn second_factor(&self, challenge: &str, code: &str) -> Result<(), String> {
        let r = self.req(reqwest::Method::POST, "/auth/2fa").json(&json!({"challenge": challenge, "code": code}));
        let resp = r.send().await.map_err(|_| format!("Servidor do Orion indisponível em {}.", self.base))?;
        let status = resp.status().as_u16();
        let set = resp.headers().get_all("set-cookie").iter().filter_map(|v| v.to_str().ok()).find(|c| c.starts_with(COOKIE)).map(|c| c.split(';').next().unwrap_or("").trim_start_matches(&format!("{COOKIE}=")).trim_matches('"').to_string());
        let text = resp.text().await.map_err(|e| e.to_string())?;
        if status == 200 {
            if let (Some(c), Ok(mut slot)) = (set, self.cookie.lock()) {
                *slot = Some(c);
            }
            Ok(())
        } else {
            Err(mensagem_http(status, &text, "/auth/2fa"))
        }
    }

    pub async fn logout(&self) {
        let _ = self.post("/auth/logout", json!({})).await;
        if let Ok(mut c) = self.cookie.lock() {
            *c = None;
        }
    }

    /// POST em rota SSE (`/chat`, `/approvals/:id/resume`): devolve os eventos já normalizados.
    pub fn stream(&self, path: &str, body: Value) -> impl Stream<Item = Chat> + Send + 'static {
        let r = self.req(reqwest::Method::POST, path).json(&body).timeout(std::time::Duration::from_secs(600));
        let (base, path) = (self.base.clone(), path.to_string());
        async_stream::stream! {
            let resp = match r.send().await {
                Ok(r) => r,
                Err(_) => { yield Chat::Error(format!("Servidor do Orion indisponível em {base}.")); yield Chat::Done { message_id: None, session_id: None }; return; }
            };
            let status = resp.status().as_u16();
            if !(200..300).contains(&status) {
                let t = resp.text().await.unwrap_or_default();
                yield Chat::Error(mensagem_http(status, &t, &path));
                yield Chat::Done { message_id: None, session_id: None };
                return;
            }
            let mut buf = String::new();
            let mut bytes = resp.bytes_stream();
            let mut fim = false;
            while let Some(chunk) = bytes.next().await {
                let Ok(chunk) = chunk else { break };
                buf.push_str(&String::from_utf8_lossy(&chunk));
                while let Some(i) = buf.find("\n\n") {
                    let bloco: String = buf.drain(..i + 2).collect();
                    for ev in parse_bloco(&bloco) {
                        fim = fim || matches!(ev, Chat::Done { .. });
                        yield ev;
                    }
                }
            }
            if !fim { yield Chat::Done { message_id: None, session_id: None }; }
        }
    }
}

/// Um bloco SSE (`data: {...}` / `data: [DONE]`) vira zero ou mais eventos.
pub fn parse_bloco(bloco: &str) -> Vec<Chat> {
    let dados: Vec<&str> = bloco.lines().filter_map(|l| l.strip_prefix("data:")).map(|l| l.strip_prefix(' ').unwrap_or(l)).collect();
    if dados.is_empty() {
        return vec![];
    }
    let bruto = dados.join("\n");
    if bruto == "[DONE]" {
        return vec![Chat::Done { message_id: None, session_id: None }];
    }
    let Ok(v) = serde_json::from_str::<Value>(&bruto) else { return vec![] };
    if let Some(t) = v.get("text").and_then(Value::as_str) {
        return vec![Chat::Text(t.to_string())];
    }
    if let Some(t) = v.get("tier").and_then(Value::as_str) {
        return vec![Chat::Model(t.to_string())];
    }
    if v.get("provenance").is_some() {
        return vec![Chat::Done { message_id: v.get("message_id").and_then(Value::as_i64), session_id: v.get("session_id").and_then(Value::as_str).map(String::from) }];
    }
    if let Some(t) = v.get("tool") {
        return vec![Chat::Tool(t.clone())];
    }
    if let Some(a) = v.get("approval") {
        let s = |k: &str| a.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        return vec![Chat::Approval { id: s("id"), tool: s("tool"), reason: s("reason"), args: a.get("args").cloned().unwrap_or(Value::Null) }];
    }
    if let Some(e) = v.get("error").and_then(Value::as_str) {
        return vec![Chat::Error(e.to_string())];
    }
    vec![]
}

// ── leitura tolerante ──
pub fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(Value::as_str).unwrap_or("")
}
pub fn arr<'a>(v: &'a Value, k: &str) -> &'a [Value] {
    v.get(k).and_then(Value::as_array).map(|a| a.as_slice()).unwrap_or(&[])
}
