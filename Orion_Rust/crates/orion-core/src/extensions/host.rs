//! Conexões MCP locais explícitas, com lifecycle de uma única dona (`Connection`) e o conjunto delas
//! (`MCPHost`). Diferente do `mcp.json` (`mcp_client`), aqui cada conexão é revisada e autorizada
//! por você (confiança no executável local; endereço HTTP autorizado) e as ferramentas só entram
//! pelo catálogo, com a classe de risco que VOCÊ deu.

use orion_ai::secrets::get_secret;
use orion_mcp::{Auth, McpClient, McpError, ToolInfo, Transport};
use orion_policy::Risk;
use parking_lot::{Mutex, RwLock};
use regex::Regex;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

static ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-z0-9][a-z0-9_-]{0,47}$").unwrap());
static ESCOPO: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(personal|project:[a-f0-9]{32})$").unwrap());
static SECRET_REF: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^ORION_MCP_[A-Z0-9_]{1,80}$").unwrap());
static HEX32: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-f0-9]{32}$").unwrap());

#[derive(Debug, Clone, thiserror::Error)]
#[error("{code}")]
pub struct MCPError {
    pub code: String,
    pub possibly_active: bool,
}

impl MCPError {
    pub fn new(code: impl Into<String>) -> Self {
        MCPError { code: code.into(), possibly_active: false }
    }
}

pub type R<T> = Result<T, MCPError>;

#[derive(Debug, Clone)]
pub struct StdioConfig {
    pub id: String,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub resources: Vec<String>,
    pub prompts: Vec<String>,
    pub classifications: BTreeMap<String, Risk>,
    pub scope: String,
    pub enabled: bool,
    pub trusted: bool,
}

#[derive(Debug, Clone)]
pub struct HttpConfig {
    pub id: String,
    pub url: String,
    pub resources: Vec<String>,
    pub prompts: Vec<String>,
    pub classifications: BTreeMap<String, Risk>,
    pub scope: String,
    pub enabled: bool,
    pub authorized: bool,
    pub account_id: Option<String>,
    pub secret_ref: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ConnectionConfig {
    Stdio(StdioConfig),
    Http(HttpConfig),
}

impl ConnectionConfig {
    pub fn id(&self) -> &str {
        match self {
            Self::Stdio(c) => &c.id,
            Self::Http(c) => &c.id,
        }
    }
    pub fn scope(&self) -> &str {
        match self {
            Self::Stdio(c) => &c.scope,
            Self::Http(c) => &c.scope,
        }
    }
    pub fn enabled(&self) -> bool {
        match self {
            Self::Stdio(c) => c.enabled,
            Self::Http(c) => c.enabled,
        }
    }
    pub fn transport(&self) -> &'static str {
        match self {
            Self::Stdio(_) => "stdio",
            Self::Http(_) => "http",
        }
    }
    pub fn classifications(&self) -> &BTreeMap<String, Risk> {
        match self {
            Self::Stdio(c) => &c.classifications,
            Self::Http(c) => &c.classifications,
        }
    }
    pub fn resources(&self) -> &[String] {
        match self {
            Self::Stdio(c) => &c.resources,
            Self::Http(c) => &c.resources,
        }
    }
    pub fn prompts(&self) -> &[String] {
        match self {
            Self::Stdio(c) => &c.prompts,
            Self::Http(c) => &c.prompts,
        }
    }

    /// A configuração completa (gravada em `extension_connections`).
    pub fn to_json(&self) -> Value {
        let mut v = self.identidade_json();
        let o = v.as_object_mut().unwrap();
        match self {
            Self::Stdio(c) => {
                o.insert("enabled".into(), json!(c.enabled));
                o.insert("trusted".into(), json!(c.trusted));
            }
            Self::Http(c) => {
                o.insert("enabled".into(), json!(c.enabled));
                o.insert("authorized".into(), json!(c.authorized));
            }
        }
        v
    }

    pub fn desligada(&self) -> ConnectionConfig {
        match self.clone() {
            Self::Stdio(mut c) => {
                c.enabled = false;
                Self::Stdio(c)
            }
            Self::Http(mut c) => {
                c.enabled = false;
                Self::Http(c)
            }
        }
    }

    /// A configuração sem `enabled`/`authorized`/`trusted`, como o Python serializa para a identidade.
    pub fn identidade_json(&self) -> Value {
        let cls: Map<String, Value> = self.classifications().iter().map(|(k, v)| (k.clone(), json!(v.as_str()))).collect();
        match self {
            Self::Stdio(c) => json!({"id": c.id, "transport": "stdio", "command": c.command, "args": c.args, "cwd": c.cwd, "resources": c.resources, "prompts": c.prompts, "classifications": cls, "scope": c.scope}),
            Self::Http(c) => json!({"id": c.id, "transport": "http", "url": c.url, "resources": c.resources, "prompts": c.prompts, "classifications": cls, "scope": c.scope, "account_id": c.account_id, "secret_ref": c.secret_ref}),
        }
    }
}

fn lista(v: &Value, k: &str, max: usize) -> Result<Vec<String>, String> {
    let Some(a) = v.get(k) else { return Ok(vec![]) };
    let a = a.as_array().ok_or_else(|| format!("{k}: esperava lista"))?;
    if a.len() > max {
        return Err(format!("{k}: no máximo {max}"));
    }
    a.iter().map(|x| x.as_str().map(String::from).ok_or_else(|| format!("{k}: esperava texto"))).collect()
}

/// Valida um item de `ORION_MCP_CONNECTIONS` (mesmas regras do Python).
pub fn parse_config(v: &Value) -> Result<ConnectionConfig, String> {
    let o = v.as_object().ok_or("conexão precisa ser um objeto")?;
    let permitidos = ["id", "transport", "command", "args", "cwd", "resources", "prompts", "classifications", "scope", "enabled", "trusted", "url", "authorized", "account_id", "secret_ref"];
    if let Some(k) = o.keys().find(|k| !permitidos.contains(&k.as_str())) {
        return Err(format!("campo desconhecido: {k}"));
    }
    let id = v["id"].as_str().filter(|i| ID.is_match(i)).ok_or("id inválido")?.to_string();
    let scope = v.get("scope").and_then(Value::as_str).unwrap_or("personal").to_string();
    if !ESCOPO.is_match(&scope) {
        return Err("scope inválido".into());
    }
    let mut cls = BTreeMap::new();
    if let Some(c) = v.get("classifications") {
        let m = c.as_object().ok_or("classifications: esperava objeto")?;
        if m.len() > 256 {
            return Err("classifications: no máximo 256".into());
        }
        for (k, x) in m {
            cls.insert(k.clone(), x.as_str().and_then(Risk::parse).ok_or("classe de risco inválida")?);
        }
    }
    let enabled = v.get("enabled").and_then(Value::as_bool).unwrap_or(false);
    let transport = v.get("transport").and_then(Value::as_str).unwrap_or(if v.get("url").is_some() { "http" } else { "stdio" });
    match transport {
        "stdio" => {
            let command = v["command"].as_str().filter(|c| !c.is_empty() && c.len() <= 4096).ok_or("command inválido")?.to_string();
            if !Path::new(&command).is_absolute() || command.contains('\0') {
                return Err("executável precisa ser caminho absoluto".into());
            }
            let args = lista(v, "args", 128)?;
            if args.iter().any(|a| a.contains('\0') || a.len() > 8192) {
                return Err("argv inválido".into());
            }
            let cwd = v.get("cwd").and_then(Value::as_str).map(String::from);
            if cwd.as_ref().is_some_and(|c| !Path::new(c).is_absolute()) {
                return Err("cwd precisa ser absoluto".into());
            }
            let trusted = v.get("trusted").and_then(Value::as_bool).unwrap_or(false);
            if enabled && !trusted {
                return Err("execução local exige revisão e confiança explícitas".into());
            }
            Ok(ConnectionConfig::Stdio(StdioConfig { id, command, args, cwd, resources: lista(v, "resources", 128)?, prompts: lista(v, "prompts", 128)?, classifications: cls, scope, enabled, trusted }))
        }
        "http" => {
            let url = v["url"].as_str().filter(|u| u.len() <= 4096).ok_or("url inválida")?.to_string();
            let secret_ref = v.get("secret_ref").and_then(Value::as_str).map(String::from);
            let account_id = v.get("account_id").and_then(Value::as_str).map(String::from);
            if secret_ref.as_ref().is_some_and(|s| !SECRET_REF.is_match(s)) || account_id.as_ref().is_some_and(|a| !HEX32.is_match(a)) {
                return Err("secret_ref ou account_id inválido".into());
            }
            if secret_ref.is_some() && account_id.is_some() {
                return Err("escolha conta OAuth ou referência Bearer".into());
            }
            validar_endpoint(&url)?;
            let authorized = v.get("authorized").and_then(Value::as_bool).unwrap_or(false);
            if enabled && !authorized {
                return Err("conexão exige endereço explicitamente autorizado".into());
            }
            Ok(ConnectionConfig::Http(HttpConfig { id, url, resources: lista(v, "resources", 128)?, prompts: lista(v, "prompts", 128)?, classifications: cls, scope, enabled, authorized, account_id, secret_ref }))
        }
        _ => Err("transport inválido".into()),
    }
}

/// Endereço HTTP sem credenciais, query ou fragmento; sem TLS só no loopback.
pub fn validar_endpoint(url: &str) -> Result<(), String> {
    let u = url::Url::parse(url).map_err(|_| "endpoint precisa ser URL HTTP sem credenciais, query ou fragmento")?;
    if !["http", "https"].contains(&u.scheme()) || u.host_str().is_none() || !u.username().is_empty() || u.password().is_some() || u.query().is_some() || u.fragment().is_some() {
        return Err("endpoint precisa ser URL HTTP sem credenciais, query ou fragmento".into());
    }
    if u.scheme() == "http" && !["localhost", "127.0.0.1", "[::1]", "::1"].contains(&u.host_str().unwrap_or("")) {
        return Err("HTTP sem TLS permitido apenas no loopback".into());
    }
    Ok(())
}

#[derive(Default)]
struct Estado {
    state: String,
    error: Option<String>,
    protocol: Option<String>,
    generation: String,
    last_call: Option<Value>,
    client: Option<Arc<McpClient>>,
}

pub type OAuthFactory = Arc<dyn Fn(&HttpConfig) -> R<Arc<dyn Auth>> + Send + Sync>;

pub struct Connection {
    host_enabled:Arc<std::sync::atomic::AtomicBool>,
    pub config: ConnectionConfig,
    pub label: parking_lot::RwLock<String>,
    pub timeout: Duration,
    oauth: Option<OAuthFactory>,
    /// Confere a conta (revogada derruba a chamada).
    pub account_check: Mutex<Option<Arc<dyn Fn() -> R<()> + Send + Sync>>>,
    /// `false` quando a origem foi revogada.
    pub authorized: Mutex<Option<Arc<dyn Fn() -> bool + Send + Sync>>>,
    st: RwLock<Estado>,
    slots: tokio::sync::Semaphore,
}

impl Connection {
    pub fn new(config: ConnectionConfig, timeout: Duration, oauth: Option<OAuthFactory>) -> Self {
        Connection { host_enabled:Arc::new(std::sync::atomic::AtomicBool::new(true)), label: parking_lot::RwLock::new(config.id().to_string()), config, timeout, oauth, account_check: Mutex::new(None), authorized: Mutex::new(None), st: RwLock::new(Estado { state: "disabled".into(), ..Default::default() }), slots: tokio::sync::Semaphore::new(4) }
    }

    pub fn state(&self) -> String {
        self.st.read().state.clone()
    }
    pub fn error(&self) -> Option<String> {
        self.st.read().error.clone()
    }
    pub fn set_error(&self, e: &str) {
        self.st.write().error = Some(e.into());
    }
    pub fn generation(&self) -> String {
        self.st.read().generation.clone()
    }

    /// Nunca serializar configuração/argv/erro bruto: podem conter dados sensíveis.
    pub fn status(&self) -> Value {
        let s = self.st.read();
        json!({"id": self.config.id(), "transport": self.config.transport(), "scope": self.config.scope(), "state": s.state, "protocol": s.protocol, "error": s.error, "last_call": s.last_call})
    }

    fn transporte(&self) -> R<Transport> {
        match &self.config {
            ConnectionConfig::Http(c) => {
                let mut headers = HashMap::new();
                if let Some(r) = &c.secret_ref {
                    let s = get_secret(r).filter(|s| !s.is_empty()).ok_or_else(|| MCPError::new("auth_missing"))?;
                    headers.insert("Authorization".to_string(), format!("Bearer {s}"));
                }
                let mut auth = None;
                if c.account_id.is_some() {
                    let f = self.oauth.as_ref().ok_or_else(|| MCPError::new("account_authorization_required"))?;
                    auth = Some(f(c)?);
                }
                Ok(Transport::Http { url: c.url.clone(), headers, auth })
            }
            ConnectionConfig::Stdio(c) => Ok(Transport::Stdio { command: c.command.clone(), args: c.args.clone(), env: HashMap::from([("PYTHONUNBUFFERED".to_string(), "1".to_string())]), cwd: c.cwd.clone() }),
        }
    }

    pub async fn start(&self) {
        if !self.host_enabled.load(std::sync::atomic::Ordering::SeqCst) || !self.config.enabled() || self.st.read().client.is_some() {
            return;
        }
        {
            let mut s = self.st.write();
            s.error = None;
            s.generation = orion_store::uuid_hex();
            s.state = "connecting".into();
        }
        let r: Result<McpClient, String> = match self.transporte() {
            Err(e) => Err(e.code),
            Ok(t) => match tokio::time::timeout(self.timeout, McpClient::connect(t, self.timeout)).await {
                Err(_) => Err(codigo_de_erro(&McpError::Tempo, false)),
                Ok(Err(e)) => Err(codigo_de_erro(&e, matches!(self.config, ConnectionConfig::Http(_)))),
                Ok(Ok(c)) => Ok(c),
            },
        };
        let mut s = self.st.write();
        match r {
            Ok(c) => {
                s.protocol = Some(c.protocol.clone());
                s.client = Some(Arc::new(c));
                s.state = "connected".into();
            }
            Err(e) => {
                s.error = Some(e);
                s.state = "failed".into();
            }
        }
        tracing::info!("mcp_state connection={} state={} code={:?}", self.config.id(), s.state, s.error);
    }

    pub async fn close(&self) {
        let c = self.st.write().client.take();
        if let Some(c) = c {
            c.fechar().await;
        }
        let mut s = self.st.write();
        s.state = if s.error.is_some() { "failed".into() } else { "disconnected".into() };
    }

    pub fn connected(&self) -> R<Arc<McpClient>> {
        if !self.host_enabled.load(std::sync::atomic::Ordering::SeqCst){return Err(MCPError::new("mcp_disabled"));}
        if let Some(f) = self.account_check.lock().clone() {
            f()?;
        }
        let s = self.st.read();
        match (&s.client, s.state.as_str()) {
            (Some(c), "connected") if c.vivo() => Ok(c.clone()),
            _ => Err(MCPError::new("not_connected")),
        }
    }

    pub async fn list_tools(&self) -> R<Vec<ToolInfo>> {
        let c = self.connected()?;
        let mut out = vec![];
        let mut cursor: Option<String> = None;
        for _ in 0..20 {
            let p = c.list_tools(cursor.as_deref()).await.map_err(|e| MCPError::new(codigo_de_erro(&e, true)))?;
            out.extend(p.tools);
            cursor = p.next_cursor;
            if cursor.is_none() {
                break;
            }
        }
        Ok(out)
    }

    /// Nunca repetir um POST/tool após timeout: o efeito pode já ter ocorrido.
    pub async fn call(&self, name: &str, arguments: Map<String, Value>) -> R<orion_mcp::CallResult> {
        let c = self.connected()?;
        // servidor remoto: os argumentos saem do computador, então a chamada entra no registro de
        // saída (REGRAS.md: toda saída de dado fica registrada, sem conteúdo)
        let mut medida = matches!(self.config, ConnectionConfig::Http(_)).then(|| orion_ai::saidas::medir(&format!("mcp:{}", self.config.id()), "mcp", name, Value::Object(arguments.clone()).to_string().len(), ""));
        let _slot = self.slots.acquire().await.map_err(|_| MCPError::new("call_failed"))?;
        let r = c.call_tool(name, arguments, self.timeout).await;
        match r {
            Ok(res) => {
                if let Some(m) = medida.as_mut() {
                    m.ok = !res.is_error;
                }
                self.st.write().last_call = Some(json!({"state": "completed", "possibly_active": false}));
                Ok(res)
            }
            Err(McpError::Tempo) => {
                self.st.write().last_call = Some(json!({"state": "timeout", "possibly_active": true}));
                Err(MCPError { code: "call_timeout".into(), possibly_active: true })
            }
            Err(e) => {
                let code = if matches!(e, McpError::Http(401 | 403)) { "auth_failed" } else { "call_failed" };
                {
                    let mut s = self.st.write();
                    s.last_call = Some(json!({"state": "failed", "possibly_active": true}));
                    s.error = Some(code.into());
                }
                drop(_slot);
                self.close().await;
                Err(MCPError { code: code.into(), possibly_active: true })
            }
        }
    }

    pub fn client(&self) -> Option<Arc<McpClient>> {
        self.st.read().client.clone()
    }
}

fn codigo_de_erro(e: &McpError, http: bool) -> String {
    match e {
        McpError::Incompativel(_) => "protocol_incompatible".into(),
        McpError::Tempo => "start_timeout".into(),
        McpError::Http(401 | 403) => "auth_failed".into(),
        McpError::Servidor { message, .. } if message.starts_with("account_") || message.starts_with("oauth_") => message.clone(),
        _ if http => "transport_error".into(),
        _ => "start_failed".into(),
    }
}

pub struct MCPHost {
    enabled:Arc<std::sync::atomic::AtomicBool>,
    connections: RwLock<BTreeMap<String, Arc<Connection>>>,
    timeout: Duration,
    oauth: Option<OAuthFactory>,
}

impl MCPHost {
    pub fn new(configs: Vec<ConnectionConfig>, timeout: Duration, oauth: Option<OAuthFactory>) -> Result<Self, String> {
        let mut ids: Vec<&str> = configs.iter().map(|c| c.id()).collect();
        ids.sort_unstable();
        ids.dedup();
        if configs.len() > 32 || ids.len() != configs.len() {
            return Err("conexões duplicadas ou acima do limite".into());
        }
        let host = MCPHost { enabled:Arc::new(std::sync::atomic::AtomicBool::new(true)), connections: RwLock::new(BTreeMap::new()), timeout, oauth };
        for c in configs {
            host.insert(host.connection(c));
        }
        Ok(host)
    }

    /// Conexão nova (ainda não registrada no host).
    pub fn connection(&self, config: ConnectionConfig) -> Arc<Connection> {
        let mut connection=Connection::new(config,self.timeout,self.oauth.clone());connection.host_enabled=self.enabled.clone();Arc::new(connection)
    }

    pub fn set_enabled(&self,on:bool){self.enabled.store(on,std::sync::atomic::Ordering::SeqCst);}
    pub fn get(&self, id: &str) -> Option<Arc<Connection>> {
        self.connections.read().get(id).cloned()
    }

    pub fn list(&self) -> Vec<Arc<Connection>> {
        self.connections.read().values().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.connections.read().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn contains(&self, id: &str) -> bool {
        self.connections.read().contains_key(id)
    }

    pub fn insert(&self, c: Arc<Connection>) {
        self.connections.write().insert(c.config.id().to_string(), c);
    }

    pub fn remove(&self, id: &str) -> Option<Arc<Connection>> {
        self.connections.write().remove(id)
    }

    pub async fn start(&self) {
        futures::future::join_all(self.list().iter().map(|c| c.start())).await;
    }

    pub async fn close(&self) {
        futures::future::join_all(self.list().iter().map(|c| c.close())).await;
    }

    pub fn statuses(&self) -> Vec<Value> {
        self.list().iter().map(|c| c.status()).collect()
    }
}
