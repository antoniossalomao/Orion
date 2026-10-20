//! Cliente HTTP do servidor local do Orion (`orion serve`): login por senha com cookie de sessão,
//! leituras tolerantes (JSON solto, nada de schema rígido) e o chat por SSE.

use futures::{Stream, StreamExt};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

pub const COOKIE: &str = "orion_session";

/// Corpo da resposta em pedaços (o chat por SSE chega aos poucos).
pub type Corpo = futures::stream::BoxStream<'static, Result<Vec<u8>, String>>;

pub struct Resposta {
    pub status: u16,
    pub set_cookie: Option<String>,
    pub corpo: Corpo,
}

/// Quem pediu a janela: a palavra de ativação (mini barra) ou outra cópia do Orion aberta (janela inteira).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acordar {
    Voz,
    Janela,
}

/// Ligação direta ao núcleo, dentro do mesmo processo: o pedido vai para as rotas sem passar por
/// socket, porta ou rede (é o que o `orion app` usa). Quem implementa fica no servidor.
pub trait Local: Send + Sync + 'static {
    fn chamar(&self, metodo: &str, caminho: &str, corpo: Option<Vec<u8>>, bearer: Option<String>) -> futures::future::BoxFuture<'static, Result<Resposta, String>>;
    /// Pedidos de "mostre a janela" vindos de fora da interface (palavra de ativação, outra instância).
    fn acordar(&self) -> Option<Acordar> {
        None
    }
    /// Banco cifrado ainda trancado: o núcleo só abre depois de `desbloquear`.
    fn bloqueado(&self) -> bool {
        false
    }
    fn desbloquear(&self, _senha: String) -> futures::future::BoxFuture<'static, Result<(), String>> {
        Box::pin(async { Err("banco não está bloqueado".to_string()) })
    }
    /// Atalho de pânico da bandeja (corta tudo, sem pedir senha; retomar exige a senha).
    fn panico(&self) -> Result<String, String> {
        Err("pânico indisponível".into())
    }
}

#[derive(Clone)]
pub struct Api {
    pub base: String,
    http: reqwest::Client,
    cookie: Arc<Mutex<Option<String>>>,
    bearer: Option<String>,
    local: Option<Arc<dyn Local>>,
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
        Api { base: base.trim_end_matches('/').to_string(), http, cookie: Arc::new(Mutex::new(None)), bearer, local: None }
    }

    /// Núcleo no mesmo processo: sem rede nenhuma.
    pub fn local(ligacao: Arc<dyn Local>, token: String) -> Self {
        let mut a = Self::new("orion://local", Some(token));
        a.local = Some(ligacao);
        a
    }

    pub fn bloqueado(&self) -> bool {
        self.local.as_ref().is_some_and(|l| l.bloqueado())
    }

    pub async fn desbloquear(&self, senha: String) -> Result<(), String> {
        match &self.local {
            Some(l) => l.desbloquear(senha).await,
            None => Err("só no app com o núcleo dentro".into()),
        }
    }

    pub fn acordar(&self) -> Option<Acordar> {
        self.local.as_ref().and_then(|l| l.acordar())
    }

    pub fn panico(&self) -> Result<String, String> {
        self.local.as_ref().ok_or_else(|| "pânico só no app com o núcleo dentro".to_string())?.panico()
    }

    fn indisponivel(&self) -> String {
        if self.local.is_some() { "O núcleo do Orion não respondeu.".into() } else { format!("Servidor do Orion indisponível em {}.", self.base) }
    }

    /// Único ponto que fala com o núcleo: pela ligação direta ou por HTTP.
    async fn raw(&self, metodo: reqwest::Method, path: &str, body: Option<Value>, longo: bool) -> Result<Resposta, String> {
        if let Some(l) = &self.local {
            return l.chamar(metodo.as_str(), path, body.map(|b| b.to_string().into_bytes()), self.bearer.clone()).await;
        }
        let mut r = self.http.request(metodo, format!("{}{}", self.base, path)).header("Origin", &self.base);
        if let Some(b) = &self.bearer {
            r = r.bearer_auth(b);
        } else if let Some(c) = self.cookie.lock().ok().and_then(|c| c.clone()) {
            r = r.header("Cookie", format!("{COOKIE}={c}"));
        }
        if let Some(b) = &body {
            r = r.json(b);
        }
        if longo {
            r = r.timeout(std::time::Duration::from_secs(600));
        }
        let resp = r.send().await.map_err(|e| if e.is_connect() || e.is_timeout() { self.indisponivel() } else { e.to_string() })?;
        let status = resp.status().as_u16();
        let set_cookie = resp.headers().get_all("set-cookie").iter().filter_map(|v| v.to_str().ok()).find(|c| c.starts_with(COOKIE)).map(String::from);
        Ok(Resposta { status, set_cookie, corpo: resp.bytes_stream().map(|c| c.map(|b| b.to_vec()).map_err(|e| e.to_string())).boxed() })
    }

    async fn texto(r: Resposta) -> String {
        let mut corpo = r.corpo;
        let mut out = Vec::new();
        while let Some(Ok(c)) = corpo.next().await {
            out.extend_from_slice(&c);
        }
        String::from_utf8_lossy(&out).into_owned()
    }

    async fn send(&self, metodo: reqwest::Method, path: &str, body: Option<Value>) -> Result<Value, String> {
        let r = self.raw(metodo, path, body, false).await?;
        let status = r.status;
        let text = Self::texto(r).await;
        if !(200..300).contains(&status) {
            return Err(mensagem_http(status, &text, path));
        }
        Ok(serde_json::from_str(&text).unwrap_or(Value::Null))
    }

    pub async fn get(&self, path: &str) -> Result<Value, String> {
        self.send(reqwest::Method::GET, path, None).await
    }
    pub async fn post(&self, path: &str, body: Value) -> Result<Value, String> {
        self.send(reqwest::Method::POST, path, Some(body)).await
    }
    pub async fn patch(&self, path: &str, body: Value) -> Result<Value, String> {
        self.send(reqwest::Method::PATCH, path, Some(body)).await
    }
    pub async fn put(&self, path: &str, body: Value) -> Result<Value, String> {
        self.send(reqwest::Method::PUT, path, Some(body)).await
    }
    pub async fn delete(&self, path: &str) -> Result<Value, String> {
        self.send(reqwest::Method::DELETE, path, None).await
    }

    /// Núcleo no ar? (`/health` não exige login). Com token de máquina já vale como entrada.
    pub async fn health(&self) -> Result<(), String> {
        let r = self.raw(reqwest::Method::GET, "/health", None, false).await.map_err(|_| self.indisponivel())?;
        if (200..300).contains(&r.status) {
            Ok(())
        } else {
            Err(format!("O servidor respondeu {}.", r.status))
        }
    }

    pub async fn auth_ok(&self) -> bool {
        self.get("/auth/status").await.map(|v| v.get("authenticated").and_then(Value::as_bool).unwrap_or(false)).unwrap_or(false)
    }

    fn guardar_cookie(&self, set: Option<String>) {
        let valor = set.map(|c| c.split(';').next().unwrap_or("").trim_start_matches(&format!("{COOKIE}=")).trim_matches('"').to_string());
        if let (Some(c), Ok(mut slot)) = (valor, self.cookie.lock()) {
            *slot = Some(c);
        }
    }

    pub async fn login(&self, usuario: &str, senha: &str) -> Result<LoginResult, String> {
        let corpo = if usuario.trim().is_empty() { json!({"senha": senha}) } else { json!({"usuario": usuario.trim(), "senha": senha}) };
        let resp = self.raw(reqwest::Method::POST, "/auth/login", Some(corpo), false).await.map_err(|_| self.indisponivel())?;
        let (status, set) = (resp.status, resp.set_cookie.clone());
        let text = Self::texto(resp).await;
        match status {
            200 => {
                self.guardar_cookie(set);
                Ok(LoginResult::Ok)
            }
            202 => Ok(LoginResult::TwoFactor(serde_json::from_str::<Value>(&text).ok().and_then(|v| v.get("challenge").and_then(Value::as_str).map(String::from)).unwrap_or_default())),
            s => Err(mensagem_http(s, &text, "/auth/login")),
        }
    }

    pub async fn second_factor(&self, challenge: &str, code: &str) -> Result<(), String> {
        let resp = self.raw(reqwest::Method::POST, "/auth/2fa", Some(json!({"challenge": challenge, "code": code})), false).await.map_err(|_| self.indisponivel())?;
        let (status, set) = (resp.status, resp.set_cookie.clone());
        let text = Self::texto(resp).await;
        if status == 200 {
            self.guardar_cookie(set);
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
        let (api, path) = (self.clone(), path.to_string());
        async_stream::stream! {
            let resp = match api.raw(reqwest::Method::POST, &path, Some(body), true).await {
                Ok(r) => r,
                Err(e) => { yield Chat::Error(e); yield Chat::Done { message_id: None, session_id: None }; return; }
            };
            if !(200..300).contains(&resp.status) {
                let status = resp.status;
                let t = Self::texto(resp).await;
                yield Chat::Error(mensagem_http(status, &t, &path));
                yield Chat::Done { message_id: None, session_id: None };
                return;
            }
            let mut buf = String::new();
            let mut bytes = resp.corpo;
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
