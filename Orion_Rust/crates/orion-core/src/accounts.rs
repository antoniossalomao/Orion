//! Contas OAuth MCP: metadados no SQLite, credenciais somente no cofre do SO.
//!
//! Cliente OAuth 2.1 para servidores MCP remotos: descoberta (RFC 9728/8414), registro dinâmico
//! de cliente, PKCE S256, troca de código e renovação. Os tokens ficam no cofre sob
//! `ORION_ACCOUNT_<id>_<revisão>_{TOKENS,CLIENT,METADATA}`; revogar troca a revisão, e a identidade
//! antiga deixa de valer antes mesmo de o cofre ser limpo.

use crate::extensions::host::{validar_endpoint, Connection, ConnectionConfig, HttpConfig, MCPError, MCPHost, OAuthFactory};
use base64::Engine;
use futures::future::BoxFuture;
use orion_ai::secrets::{get_secret, set_secret};
use orion_mcp::{Auth, McpError};
use orion_store::Store;
use parking_lot::{Mutex, RwLock};
use rand::RngCore;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

type R<T> = Result<T, MCPError>;

fn agora() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

fn mcp(e: impl std::fmt::Display) -> MCPError {
    MCPError::new(e.to_string())
}

fn aleatorio(n: usize) -> String {
    let mut b = vec![0u8; n];
    rand::thread_rng().fill_bytes(&mut b);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)
}

fn loopback(h: &str) -> bool {
    ["localhost", "127.0.0.1", "::1", "[::1]"].contains(&h)
}

pub struct Flow {
    pub revision: String,
    pub expires: f64,
    url: Mutex<Option<String>>,
    state: Mutex<Option<String>>,
    used: AtomicBool,
    tx: Mutex<Option<tokio::sync::oneshot::Sender<(String, Option<String>)>>>,
    rx: Mutex<Option<tokio::sync::oneshot::Receiver<(String, Option<String>)>>>,
    task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl Flow {
    fn novo(revision: String) -> Arc<Self> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        Arc::new(Flow { revision, expires: agora() + 180.0, url: Mutex::new(None), state: Mutex::new(None), used: AtomicBool::new(false), tx: Mutex::new(Some(tx)), rx: Mutex::new(Some(rx)), task: Mutex::new(None) })
    }
}

pub struct Accounts {
    pub memory: Arc<Store>,
    pub host: RwLock<Option<Arc<MCPHost>>>,
    flows: Mutex<HashMap<String, Arc<Flow>>>,
    http: reqwest::Client,
}

fn row_publico(a: &Accounts, row: &Map<String, Value>) -> Value {
    let id = row["id"].as_str().unwrap_or("");
    let flow = a.flows.lock().get(id).cloned();
    let url = flow.filter(|f| !f.used.load(Ordering::SeqCst) && f.expires > agora()).and_then(|f| f.url.lock().clone());
    let j = |k: &str| serde_json::from_str::<Value>(row[k].as_str().unwrap_or("[]")).unwrap_or(json!([]));
    let mut out = Map::new();
    for k in ["id", "name", "url", "scope", "state", "expires_at", "error"] {
        out.insert(k.into(), row[k].clone());
    }
    out.insert("scopes".into(), j("scopes"));
    out.insert("granted".into(), j("granted"));
    out.insert("authorization_url".into(), json!(url));
    Value::Object(out)
}

impl Accounts {
    pub fn new(memory: Arc<Store>) -> Arc<Self> {
        let http = reqwest::Client::builder().timeout(Duration::from_secs(30)).redirect(reqwest::redirect::Policy::none()).build().expect("cliente http");
        Arc::new(Accounts { memory, host: RwLock::new(None), flows: Mutex::new(HashMap::new()), http })
    }

    pub fn row(&self, id: &str) -> R<Map<String, Value>> {
        self.memory.query("SELECT * FROM oauth_accounts WHERE id=?", &[&id]).map_err(mcp)?.into_iter().next().ok_or_else(|| MCPError::new("account_not_found"))
    }

    pub fn public(&self, row: &Map<String, Value>) -> Value {
        row_publico(self, row)
    }

    pub fn listar(&self) -> R<Vec<Value>> {
        Ok(self.memory.query("SELECT * FROM oauth_accounts", &[]).map_err(mcp)?.iter().map(|r| self.public(r)).collect())
    }

    pub fn criar(&self, body: &Value) -> R<Value> {
        let o = body.as_object().ok_or_else(|| MCPError::new("account_invalid"))?;
        if let Some(_k) = o.keys().find(|k| !["name", "url", "scope", "scopes"].contains(&k.as_str())) {
            return Err(MCPError::new("account_invalid"));
        }
        let name = body["name"].as_str().filter(|n| !n.is_empty() && n.chars().count() <= 160).ok_or_else(|| MCPError::new("account_invalid"))?;
        let url = body["url"].as_str().filter(|u| u.len() <= 4096).ok_or_else(|| MCPError::new("account_invalid"))?;
        let scope = body.get("scope").and_then(Value::as_str).unwrap_or("personal");
        if !(scope == "personal" || (scope.len() == 40 && scope.starts_with("project:") && scope[8..].chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()))) {
            return Err(MCPError::new("account_invalid"));
        }
        let scopes: Vec<String> = body["scopes"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
        if scopes.is_empty() || scopes.len() > 32 {
            return Err(MCPError::new("account_invalid"));
        }
        validar_endpoint(url).map_err(|_| MCPError::new("account_endpoint_invalid"))?;
        if scopes.iter().any(|s| s.is_empty() || s.len() > 256 || s.chars().any(|c| (c as u32) < 33)) {
            return Err(MCPError::new("account_scope_invalid"));
        }
        let n = self.memory.query("SELECT count(*) AS n FROM oauth_accounts", &[]).map_err(mcp)?;
        if n[0]["n"].as_i64().unwrap_or(0) >= 32 {
            return Err(MCPError::new("account_limit"));
        }
        let id = orion_store::uuid_hex();
        let mut ord: Vec<String> = scopes;
        ord.sort();
        ord.dedup();
        let (sc, rev) = (json!(ord).to_string(), orion_store::uuid_hex());
        self.memory
            .tx(|c| Ok(c.execute("INSERT INTO oauth_accounts(id,name,url,scope,scopes,granted,revision,state) VALUES (?,?,?,?,?,?,?,?)", rusqlite::params![id, name, url, scope, sc, "[]", rev, "unauthorized"])?))
            .map_err(mcp)?;
        Ok(self.public(&self.row(&id)?))
    }

    /// `OAuthFactory` para o `MCPHost`: cada conexão com `account_id` ganha seu provedor.
    pub fn factory(self: &Arc<Self>) -> OAuthFactory {
        let a = self.clone();
        Arc::new(move |cfg: &HttpConfig| a.provider(cfg, None, None))
    }

    pub fn provider(self: &Arc<Self>, cfg: &HttpConfig, callback_url: Option<String>, flow: Option<Arc<Flow>>) -> R<Arc<dyn Auth>> {
        let id = cfg.account_id.clone().ok_or_else(|| MCPError::new("account_not_found"))?;
        let row = self.row(&id)?;
        if row["url"].as_str() != Some(&cfg.url) || row["scope"].as_str() != Some(&cfg.scope) || row["state"].as_str() == Some("revoked") {
            return Err(MCPError::new("account_out_of_scope_or_revoked"));
        }
        let scopes: Vec<String> = serde_json::from_str(row["scopes"].as_str().unwrap_or("[]")).unwrap_or_default();
        Ok(Arc::new(Provider {
            contas: self.clone(),
            id: id.clone(),
            revision: row["revision"].as_str().unwrap_or("").into(),
            url: cfg.url.clone(),
            scopes,
            callback_url: callback_url.unwrap_or_else(|| format!("http://127.0.0.1/accounts/callback/{id}")),
            flow,
            meta: tokio::sync::Mutex::new(None),
        }))
    }

    pub async fn autorizar(self: &Arc<Self>, id: &str, callback_url: &str) -> R<Value> {
        let u = url::Url::parse(callback_url).map_err(|_| MCPError::new("oauth_callback_url_invalid"))?;
        if u.scheme() != "http" || !loopback(u.host_str().unwrap_or("")) || u.path() != format!("/accounts/callback/{id}") || u.query().is_some() || u.fragment().is_some() {
            return Err(MCPError::new("oauth_callback_url_invalid"));
        }
        self.revogar(id, true).await?;
        let row = self.row(id)?;
        let chave = format!("ORION_ACCOUNT_{}_PROBE", id.to_uppercase());
        tokio::task::spawn_blocking(move || set_secret(&chave, "cofre-validado")).await.map_err(mcp)?.map_err(|_| MCPError::new("credential_store_unavailable"))?;
        self.memory.tx(|c| Ok(c.execute("UPDATE oauth_accounts SET state='authorizing',error=NULL WHERE id=?", [id])?)).map_err(mcp)?;
        let flow = Flow::novo(row["revision"].as_str().unwrap_or("").into());
        self.flows.lock().insert(id.into(), flow.clone());
        let (me, id2, cb, f2, rowc) = (self.clone(), id.to_string(), callback_url.to_string(), flow.clone(), row.clone());
        let t = tokio::spawn(async move {
            let cfg = HttpConfig { id: "oauth-probe".into(), url: rowc["url"].as_str().unwrap_or("").into(), resources: vec![], prompts: vec![], classifications: Default::default(), scope: rowc["scope"].as_str().unwrap_or("personal").into(), enabled: true, authorized: true, account_id: Some(id2.clone()), secret_ref: None };
            let (m2, cb2, f3) = (me.clone(), cb.clone(), f2.clone());
            let fabrica: OAuthFactory = Arc::new(move |c: &HttpConfig| m2.provider(c, Some(cb2.clone()), Some(f3.clone())));
            let probe = Connection::new(ConnectionConfig::Http(cfg), Duration::from_secs(185), Some(fabrica));
            probe.start().await;
            if probe.state() != "connected" {
                let rev = f2.revision.clone();
                let _ = me.memory.tx(|c| Ok(c.execute("UPDATE oauth_accounts SET state='unauthorized',error='authorization_failed' WHERE id=? AND revision=?", rusqlite::params![id2, rev])?));
            }
            f2.used.store(true, Ordering::SeqCst);
            probe.close().await;
        });
        *flow.task.lock() = Some(t);
        // dá um instante para o provedor publicar a URL de autorização
        for _ in 0..40 {
            if flow.url.lock().is_some() || flow.used.load(Ordering::SeqCst) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        Ok(self.public(&self.row(id)?))
    }

    pub fn callback(&self, id: &str, code: &str, state: &str, iss: Option<&str>) -> R<()> {
        let flow = self.flows.lock().get(id).cloned();
        let invalido = || MCPError::new("oauth_callback_invalid");
        let flow = flow.ok_or_else(invalido)?;
        let esperado = flow.state.lock().clone().ok_or_else(invalido)?;
        let igual = esperado.len() == state.len() && esperado.bytes().zip(state.bytes()).fold(0u8, |a, (x, y)| a | (x ^ y)) == 0;
        if flow.used.load(Ordering::SeqCst) || flow.expires <= agora() || !igual || code.is_empty() || code.len() > 4096 || self.row(id).map(|r| r["revision"].as_str() != Some(&flow.revision)).unwrap_or(true) {
            return Err(invalido());
        }
        flow.used.store(true, Ordering::SeqCst);
        if let Some(tx) = flow.tx.lock().take() {
            let _ = tx.send((code.into(), iss.map(String::from)));
        }
        Ok(())
    }

    pub async fn revogar(&self, id: &str, disable_only: bool) -> R<Value> {
        let row = self.row(id)?;
        let flow = self.flows.lock().remove(id);
        if let Some(f) = flow {
            let t = f.task.lock().take();
            if let Some(t) = t {
                t.abort();
                let _ = t.await;
            }
        }
        let nova = orion_store::uuid_hex();
        self.memory.tx(|c| Ok(c.execute("UPDATE oauth_accounts SET revision=?,state='revoked',error=NULL WHERE id=?", rusqlite::params![nova, id])?)).map_err(mcp)?;
        let host = self.host.read().clone();
        if let Some(h) = host {
            for c in h.connections.values() {
                if matches!(&c.config, ConnectionConfig::Http(x) if x.account_id.as_deref() == Some(id)) {
                    c.close().await;
                }
            }
        }
        if disable_only {
            self.memory.tx(|c| Ok(c.execute("UPDATE oauth_accounts SET state='unauthorized' WHERE id=?", [id])?)).map_err(mcp)?;
        } else {
            let rev = row["revision"].as_str().unwrap_or("").to_uppercase();
            for kind in ["TOKENS", "CLIENT", "METADATA"] {
                let k = format!("ORION_ACCOUNT_{}_{}_{}", id.to_uppercase(), rev, kind);
                // identidade revogada antes de limpar o cofre: falha aqui não reabre acesso
                let _ = tokio::task::spawn_blocking(move || set_secret(&k, "")).await;
            }
        }
        Ok(self.public(&self.row(id)?))
    }

    pub async fn close(&self) {
        let ts: Vec<_> = self.flows.lock().values().filter_map(|f| f.task.lock().take()).collect();
        for t in ts {
            t.abort();
            let _ = t.await;
        }
    }
}

// ── provedor OAuth ────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct Meta {
    authorization_endpoint: String,
    token_endpoint: String,
    registration_endpoint: Option<String>,
}

struct Provider {
    contas: Arc<Accounts>,
    id: String,
    revision: String,
    url: String,
    scopes: Vec<String>,
    callback_url: String,
    flow: Option<Arc<Flow>>,
    meta: tokio::sync::Mutex<Option<Meta>>,
}

fn oerr(code: &str) -> McpError {
    McpError::Servidor { code: 0, message: code.into() }
}

fn bloqueante<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> BoxFuture<'static, T> {
    Box::pin(async move { tokio::task::spawn_blocking(f).await.expect("tarefa de cofre") })
}

impl Provider {
    fn chave(&self, tipo: &str) -> String {
        format!("ORION_ACCOUNT_{}_{}_{}", self.id.to_uppercase(), self.revision.to_uppercase(), tipo)
    }

    fn valido(&self) -> Result<Map<String, Value>, McpError> {
        let row = self.contas.row(&self.id).map_err(|e| oerr(&e.code))?;
        if row["revision"].as_str() != Some(&self.revision) || row["state"].as_str() == Some("revoked") {
            return Err(oerr("account_revoked"));
        }
        Ok(row)
    }

    async fn ler(&self, tipo: &str) -> Option<Value> {
        let k = self.chave(tipo);
        bloqueante(move || get_secret(&k)).await.filter(|s| !s.is_empty()).and_then(|s| serde_json::from_str(&s).ok())
    }

    async fn gravar(&self, tipo: &str, v: &Value) -> Result<(), McpError> {
        let (k, s) = (self.chave(tipo), v.to_string());
        bloqueante(move || set_secret(&k, &s)).await.map_err(|_| oerr("credential_store_unavailable"))
    }

    async fn tokens(&self) -> Result<Option<(Value, Option<f64>)>, McpError> {
        self.valido()?;
        Ok(self.ler("TOKENS").await.map(|r| (r["token"].clone(), r["expires_at"].as_f64())))
    }

    async fn guardar_tokens(&self, token: &Value) -> Result<(), McpError> {
        let row = self.valido()?;
        let permitidos: Vec<String> = serde_json::from_str(row["scopes"].as_str().unwrap_or("[]")).unwrap_or_default();
        let mut concedidos: Vec<String> = token.get("scope").and_then(Value::as_str).unwrap_or("").split_whitespace().map(String::from).collect();
        if concedidos.iter().any(|s| !permitidos.contains(s)) {
            return Err(oerr("account_scope_exceeded"));
        }
        let expira = token.get("expires_in").and_then(Value::as_f64).map(|s| agora() + s);
        self.gravar("TOKENS", &json!({"token": token, "expires_at": expira})).await?;
        self.valido()?;
        concedidos.sort();
        let (g, rev, id) = (json!(concedidos).to_string(), self.revision.clone(), self.id.clone());
        self.contas.memory.tx(|c| Ok(c.execute("UPDATE oauth_accounts SET state='authorized',expires_at=?,granted=?,error=NULL WHERE id=? AND revision=?", rusqlite::params![expira, g, id, rev])?)).map_err(|e| McpError::Outro(e.to_string()))?;
        Ok(())
    }

    async fn get_json(&self, url: &str) -> Option<Value> {
        let mut m = orion_ai::saidas::medir(&format!("oauth:{}", self.id), "oauth", "descoberta", 0, "");
        let r = self.contas.http.get(url).header("accept", "application/json").send().await.ok()?;
        if !r.status().is_success() {
            return None;
        }
        let v = r.json::<Value>().await.ok();
        m.ok = v.is_some();
        v
    }

    /// RFC 9728 → RFC 8414/OIDC; sem metadados, os caminhos padrão do servidor.
    async fn descobrir(&self, www: Option<&str>) -> Result<Meta, McpError> {
        if let Some(m) = self.meta.lock().await.clone() {
            return Ok(m);
        }
        if let Some(raw) = self.ler("METADATA").await {
            if let (Some(a), Some(t)) = (raw["authorization_endpoint"].as_str(), raw["token_endpoint"].as_str()) {
                let m = Meta { authorization_endpoint: a.into(), token_endpoint: t.into(), registration_endpoint: raw["registration_endpoint"].as_str().map(String::from) };
                *self.meta.lock().await = Some(m.clone());
                return Ok(m);
            }
        }
        let base = url::Url::parse(&self.url).map_err(|_| oerr("account_endpoint_invalid"))?;
        let origem = format!("{}://{}", base.scheme(), base.host_str().unwrap_or("") .to_string() + &base.port().map(|p| format!(":{p}")).unwrap_or_default());
        let recurso = www
            .and_then(|w| w.split("resource_metadata=\"").nth(1))
            .and_then(|r| r.split('"').next())
            .map(String::from);
        let mut candidatos = vec![];
        if let Some(r) = recurso {
            candidatos.push(r);
        }
        let caminho = base.path().trim_end_matches('/');
        if !caminho.is_empty() {
            candidatos.push(format!("{origem}/.well-known/oauth-protected-resource{caminho}"));
        }
        candidatos.push(format!("{origem}/.well-known/oauth-protected-resource"));
        let mut servidor = origem.clone();
        for c in candidatos {
            if let Some(v) = self.get_json(&c).await {
                if let Some(a) = v["authorization_servers"].get(0).and_then(Value::as_str) {
                    servidor = a.trim_end_matches('/').to_string();
                    break;
                }
            }
        }
        let su = url::Url::parse(&servidor).map_err(|_| oerr("oauth_discovery_failed"))?;
        let so = format!("{}://{}{}", su.scheme(), su.host_str().unwrap_or(""), su.port().map(|p| format!(":{p}")).unwrap_or_default());
        let sp = su.path().trim_end_matches('/').to_string();
        let mut urls = vec![];
        if !sp.is_empty() {
            urls.push(format!("{so}/.well-known/oauth-authorization-server{sp}"));
            urls.push(format!("{so}/.well-known/openid-configuration{sp}"));
            urls.push(format!("{so}{sp}/.well-known/openid-configuration"));
        }
        urls.push(format!("{so}/.well-known/oauth-authorization-server"));
        urls.push(format!("{so}/.well-known/openid-configuration"));
        let mut meta = None;
        for u in urls {
            if let Some(v) = self.get_json(&u).await {
                if let (Some(a), Some(t)) = (v["authorization_endpoint"].as_str(), v["token_endpoint"].as_str()) {
                    meta = Some(Meta { authorization_endpoint: a.into(), token_endpoint: t.into(), registration_endpoint: v["registration_endpoint"].as_str().map(String::from) });
                    break;
                }
            }
        }
        let m = meta.unwrap_or(Meta { authorization_endpoint: format!("{so}/authorize"), token_endpoint: format!("{so}/token"), registration_endpoint: Some(format!("{so}/register")) });
        for e in [&m.authorization_endpoint, &m.token_endpoint] {
            let u = url::Url::parse(e).map_err(|_| oerr("oauth_discovery_failed"))?;
            if u.scheme() != "https" && !(u.scheme() == "http" && loopback(u.host_str().unwrap_or(""))) {
                return Err(oerr("account_authorization_url_invalid"));
            }
        }
        *self.meta.lock().await = Some(m.clone());
        Ok(m)
    }

    async fn cliente(&self, meta: &Meta) -> Result<String, McpError> {
        self.valido()?;
        if let Some(c) = self.ler("CLIENT").await {
            if let Some(id) = c["client_id"].as_str() {
                return Ok(id.into());
            }
        }
        let reg = meta.registration_endpoint.clone().ok_or_else(|| oerr("oauth_registration_unsupported"))?;
        let corpo = json!({"client_name": "Orion", "redirect_uris": [self.callback_url], "grant_types": ["authorization_code", "refresh_token"], "response_types": ["code"], "token_endpoint_auth_method": "none", "scope": self.scopes.join(" ")});
        let mut m = orion_ai::saidas::medir(&format!("oauth:{}", self.id), "oauth", "registro", corpo.to_string().len(), "");
        let r = self.contas.http.post(&reg).json(&corpo).send().await.map_err(|e| McpError::Perdida(e.to_string()))?;
        if !r.status().is_success() {
            return Err(oerr("oauth_registration_failed"));
        }
        let v: Value = r.json().await.map_err(|_| oerr("oauth_registration_failed"))?;
        m.ok = true;
        let id = v["client_id"].as_str().ok_or_else(|| oerr("oauth_registration_failed"))?.to_string();
        self.valido()?;
        self.gravar("CLIENT", &v).await?;
        Ok(id)
    }

    async fn pedir_token(&self, meta: &Meta, mut campos: Vec<(&str, String)>) -> Result<Value, McpError> {
        campos.push(("resource", self.url.clone()));
        let corpo: String = url::form_urlencoded::Serializer::new(String::new()).extend_pairs(campos.iter().map(|(k, v)| (*k, v.as_str()))).finish();
        let mut m = orion_ai::saidas::medir(&format!("oauth:{}", self.id), "oauth", "token", corpo.len(), "");
        let r = self.contas.http.post(&meta.token_endpoint).header("content-type", "application/x-www-form-urlencoded").header("accept", "application/json").body(corpo).send().await.map_err(|e| McpError::Perdida(e.to_string()))?;
        if !r.status().is_success() {
            return Err(oerr("oauth_token_failed"));
        }
        let v: Value = r.json().await.map_err(|_| oerr("oauth_token_failed"))?;
        if v["access_token"].as_str().is_none() {
            return Err(oerr("oauth_token_failed"));
        }
        m.ok = true;
        Ok(v)
    }

    async fn renovar(&self, meta: &Meta, atual: &Value) -> Result<bool, McpError> {
        let Some(refresh) = atual["refresh_token"].as_str() else { return Ok(false) };
        let client = self.cliente(meta).await?;
        match self.pedir_token(meta, vec![("grant_type", "refresh_token".into()), ("refresh_token", refresh.into()), ("client_id", client)]).await {
            Ok(mut novo) => {
                // o servidor pode omitir o refresh token novo: o antigo continua valendo
                if novo.get("refresh_token").is_none() {
                    novo["refresh_token"] = json!(refresh);
                }
                if novo.get("scope").is_none() {
                    if let Some(s) = atual.get("scope") {
                        novo["scope"] = s.clone();
                    }
                }
                self.guardar_tokens(&novo).await?;
                Ok(true)
            }
            Err(_) => Ok(false),
        }
    }

    async fn fluxo(&self, www: Option<&str>) -> Result<(), McpError> {
        self.valido()?;
        let meta = self.descobrir(www).await?;
        if let Some((tok, _)) = self.tokens().await? {
            if self.renovar(&meta, &tok).await? {
                return Ok(());
            }
        }
        let flow = self.flow.clone().ok_or_else(|| oerr("account_authorization_required"))?;
        let client = self.cliente(&meta).await?;
        let verificador = aleatorio(48);
        use sha2::{Digest, Sha256};
        let desafio = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verificador.as_bytes()));
        let state = aleatorio(24);
        let mut u = url::Url::parse(&meta.authorization_endpoint).map_err(|_| oerr("account_authorization_url_invalid"))?;
        u.query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", &client)
            .append_pair("redirect_uri", &self.callback_url)
            .append_pair("state", &state)
            .append_pair("code_challenge", &desafio)
            .append_pair("code_challenge_method", "S256")
            .append_pair("scope", &self.scopes.join(" "))
            .append_pair("resource", &self.url);
        if u.scheme() != "https" && !(u.scheme() == "http" && loopback(u.host_str().unwrap_or(""))) {
            return Err(oerr("account_authorization_url_invalid"));
        }
        *flow.state.lock() = Some(state);
        *flow.url.lock() = Some(u.to_string());
        let rx = flow.rx.lock().take().ok_or_else(|| oerr("oauth_callback_invalid"))?;
        let espera = (flow.expires - agora()).max(0.0);
        let (code, _iss) = tokio::time::timeout(Duration::from_secs_f64(espera), rx).await.map_err(|_| McpError::Tempo)?.map_err(|_| oerr("oauth_callback_invalid"))?;
        self.valido()?;
        let tok = self
            .pedir_token(&meta, vec![("grant_type", "authorization_code".into()), ("code", code), ("redirect_uri", self.callback_url.clone()), ("client_id", client), ("code_verifier", verificador)])
            .await?;
        self.guardar_tokens(&tok).await?;
        // metadados de descoberta persistem para a renovação na próxima sessão
        self.valido()?;
        self.gravar("METADATA", &json!({"authorization_endpoint": meta.authorization_endpoint, "token_endpoint": meta.token_endpoint, "registration_endpoint": meta.registration_endpoint})).await?;
        Ok(())
    }
}

impl Auth for Provider {
    fn bearer(&self) -> BoxFuture<'_, Result<Option<String>, McpError>> {
        Box::pin(async move {
            let Some((tok, expira)) = self.tokens().await? else { return Ok(None) };
            if expira.is_some_and(|e| e < agora() + 30.0) {
                if let Ok(meta) = self.descobrir(None).await {
                    if self.renovar(&meta, &tok).await? {
                        return Ok(self.tokens().await?.and_then(|(t, _)| t["access_token"].as_str().map(String::from)));
                    }
                }
                return Ok(None);
            }
            Ok(tok["access_token"].as_str().map(String::from))
        })
    }

    fn nao_autorizado<'a>(&'a self, www: Option<&'a str>) -> BoxFuture<'a, Result<(), McpError>> {
        Box::pin(self.fluxo(www))
    }
}
