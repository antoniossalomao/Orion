//! Cliente MCP mínimo e auditável: JSON-RPC sobre stdio (uma mensagem por linha) ou Streamable
//! HTTP (POST com resposta JSON ou SSE). Só o que o Orion usa: `initialize`, `tools/*`,
//! `resources/*` e `prompts/*`. Pedidos do servidor ao cliente (sampling, roots, elicitation)
//! são recusados: o servidor MCP é fonte de dado, nunca de comando (REGRAS.md: MCP).

pub mod server;

use futures::StreamExt;
use parking_lot::Mutex;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{oneshot, Mutex as AsyncMutex};

/// Protocolos que o Orion comprovou (E7.1).
pub const TESTED_PROTOCOLS: [&str; 2] = ["2025-11-25", "2026-07-28"];

#[derive(Debug, Clone, thiserror::Error)]
pub enum McpError {
    /// O servidor fala um protocolo que o Orion não comprovou.
    #[error("{0}")]
    Incompativel(String),
    /// A conexão acabou (processo morto, stream fechado).
    #[error("conexão perdida: {0}")]
    Perdida(String),
    #[error("tempo esgotado")]
    Tempo,
    /// Erro JSON-RPC devolvido pelo servidor.
    #[error("{message}")]
    Servidor { code: i64, message: String },
    #[error("http {0}")]
    Http(u16),
    #[error("{0}")]
    Outro(String),
}

impl McpError {
    pub fn conexao_perdida(&self) -> bool {
        matches!(self, McpError::Perdida(_))
    }
}

pub type R<T> = Result<T, McpError>;

#[derive(Debug, Clone)]
pub enum Transport {
    Stdio { command: String, args: Vec<String>, env: HashMap<String, String>, cwd: Option<String> },
    Http { url: String, headers: HashMap<String, String> },
}

#[derive(Debug, Clone)]
pub struct ToolInfo {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Value,
}

#[derive(Debug, Clone, Default)]
pub struct ToolsPage {
    pub tools: Vec<ToolInfo>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CallResult {
    pub content: Vec<Value>,
    pub structured: Option<Value>,
    pub is_error: bool,
}

impl CallResult {
    /// (texto, erro): junta os blocos de texto; imagem vira aviso.
    pub fn texto(&self) -> (String, bool) {
        let partes: Vec<String> = self
            .content
            .iter()
            .map(|b| match b.get("text").and_then(Value::as_str) {
                Some(t) => t.to_string(),
                None => format!("[conteúdo {} omitido]", b.get("type").and_then(Value::as_str).unwrap_or("?")),
            })
            .collect();
        let mut t = partes.join("\n");
        if t.is_empty() {
            if let Some(s) = &self.structured {
                t = s.to_string();
            }
        }
        (t, self.is_error)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ResourcesPage {
    pub uris: Vec<String>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PromptInfo {
    pub name: String,
    pub description: Option<String>,
    /// (nome, obrigatório)
    pub arguments: Vec<(String, bool)>,
}

type Pendentes = Arc<Mutex<HashMap<i64, oneshot::Sender<R<Value>>>>>;

enum Conn {
    Stdio { stdin: Arc<AsyncMutex<ChildStdin>>, child: AsyncMutex<Child>, pendentes: Pendentes },
    Http { http: reqwest::Client, url: String, headers: HashMap<String, String>, sessao: Mutex<Option<String>> },
}

pub struct McpClient {
    conn: Conn,
    next: AtomicI64,
    vivo: Arc<AtomicBool>,
    timeout: Duration,
    pub protocol: String,
}

/// Ambiente mínimo herdado por servidores stdio (nunca `ORION_*` nem chaves).
fn ambiente_base() -> HashMap<String, String> {
    let nomes: &[&str] = if cfg!(windows) {
        &["APPDATA", "HOMEDRIVE", "HOMEPATH", "LOCALAPPDATA", "PATH", "PROCESSOR_ARCHITECTURE", "SYSTEMDRIVE", "SYSTEMROOT", "TEMP", "USERNAME", "USERPROFILE"]
    } else {
        &["HOME", "LOGNAME", "PATH", "SHELL", "TERM", "USER"]
    };
    nomes.iter().filter_map(|n| std::env::var(n).ok().filter(|v| !v.starts_with("()")).map(|v| ((*n).to_string(), v))).collect()
}

fn id_de(v: &Value) -> Option<i64> {
    v.get("id").and_then(Value::as_i64)
}

fn erro_rpc(v: &Value) -> Option<McpError> {
    v.get("error").map(|e| McpError::Servidor { code: e["code"].as_i64().unwrap_or(0), message: e["message"].as_str().unwrap_or("erro").to_string() })
}

/// Resposta a um pedido do servidor ao cliente: só `ping` é aceito.
fn resposta_a_pedido_do_servidor(msg: &Value) -> Value {
    let id = msg["id"].clone();
    if msg["method"] == "ping" {
        json!({"jsonrpc": "2.0", "id": id, "result": {}})
    } else {
        json!({"jsonrpc": "2.0", "id": id, "error": {"code": -32601, "message": "método não suportado pelo cliente"}})
    }
}

impl McpClient {
    pub async fn connect(t: Transport, timeout: Duration) -> R<Self> {
        let vivo = Arc::new(AtomicBool::new(true));
        let conn = match t {
            Transport::Stdio { command, args, env, cwd } => {
                let mut cmd = Command::new(&command);
                cmd.args(&args).env_clear().envs(ambiente_base()).envs(env).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true);
                if let Some(c) = cwd {
                    cmd.current_dir(c);
                }
                let mut child = cmd.spawn().map_err(|e| McpError::Outro(format!("não consegui iniciar o servidor: {e}")))?;
                let stdin = child.stdin.take().ok_or_else(|| McpError::Outro("sem stdin".into()))?;
                let stdout = child.stdout.take().ok_or_else(|| McpError::Outro("sem stdout".into()))?;
                let pendentes: Pendentes = Arc::new(Mutex::new(HashMap::new()));
                let stdin = Arc::new(AsyncMutex::new(stdin));
                let (p2, v2, s2) = (pendentes.clone(), vivo.clone(), stdin.clone());
                tokio::spawn(async move {
                    let mut linhas = BufReader::new(stdout).lines();
                    while let Ok(Some(l)) = linhas.next_line().await {
                        let Ok(msg) = serde_json::from_str::<Value>(&l) else { continue };
                        if msg.get("method").is_some() {
                            // pedido do servidor: só `ping` é atendido; notificação é ignorada
                            if msg.get("id").is_some() {
                                let _ = McpClient::enviar_stdio(&s2, &resposta_a_pedido_do_servidor(&msg)).await;
                            }
                            continue;
                        }
                        if let Some(id) = id_de(&msg) {
                            if let Some(tx) = p2.lock().remove(&id) {
                                let _ = tx.send(erro_rpc(&msg).map(Err).unwrap_or_else(|| Ok(msg["result"].clone())));
                            }
                        }
                    }
                    v2.store(false, Ordering::SeqCst);
                    for (_, tx) in p2.lock().drain() {
                        let _ = tx.send(Err(McpError::Perdida("o servidor fechou a saída".into())));
                    }
                });
                Conn::Stdio { stdin, child: AsyncMutex::new(child), pendentes }
            }
            Transport::Http { url, headers } => {
                let http = reqwest::Client::builder().timeout(Duration::from_secs(300)).connect_timeout(Duration::from_secs(30)).redirect(reqwest::redirect::Policy::none()).build().map_err(|e| McpError::Outro(e.to_string()))?;
                Conn::Http { http, url, headers, sessao: Mutex::new(None) }
            }
        };
        let mut c = McpClient { conn, next: AtomicI64::new(1), vivo, timeout, protocol: String::new() };
        let r = c
            .request(
                "initialize",
                json!({"protocolVersion": TESTED_PROTOCOLS[0], "capabilities": {}, "clientInfo": {"name": "orion", "version": env!("CARGO_PKG_VERSION")}}),
                timeout,
            )
            .await?;
        let versao = r["protocolVersion"].as_str().unwrap_or("").to_string();
        if !TESTED_PROTOCOLS.contains(&versao.as_str()) {
            c.fechar().await;
            return Err(McpError::Incompativel(format!("o servidor fala o protocolo {}, o Orion fala {}", if versao.is_empty() { "?" } else { &versao }, TESTED_PROTOCOLS.join(" e "))));
        }
        c.protocol = versao;
        c.notify("notifications/initialized", json!({})).await?;
        Ok(c)
    }

    pub fn vivo(&self) -> bool {
        self.vivo.load(Ordering::SeqCst)
    }

    async fn enviar_stdio(stdin: &AsyncMutex<ChildStdin>, msg: &Value) -> R<()> {
        let mut linha = msg.to_string();
        linha.push('\n');
        let mut g = stdin.lock().await;
        g.write_all(linha.as_bytes()).await.map_err(|e| McpError::Perdida(e.to_string()))?;
        g.flush().await.map_err(|e| McpError::Perdida(e.to_string()))
    }

    async fn notify(&self, method: &str, params: Value) -> R<()> {
        let msg = json!({"jsonrpc": "2.0", "method": method, "params": params});
        match &self.conn {
            Conn::Stdio { stdin, .. } => Self::enviar_stdio(stdin, &msg).await,
            Conn::Http { .. } => self.post_http(&msg, None).await.map(|_| ()),
        }
    }

    /// Faz o POST; para pedidos devolve o resultado da resposta com o `id` dado.
    async fn post_http(&self, msg: &Value, id: Option<i64>) -> R<Value> {
        let Conn::Http { http, url, headers, sessao } = &self.conn else { return Err(McpError::Outro("não é HTTP".into())) };
        let mut rb = http.post(url).header("content-type", "application/json").header("accept", "application/json, text/event-stream").json(msg);
        for (k, v) in headers {
            rb = rb.header(k.as_str(), v.as_str());
        }
        if let Some(s) = sessao.lock().clone() {
            rb = rb.header("mcp-session-id", s);
        }
        if !self.protocol.is_empty() {
            rb = rb.header("mcp-protocol-version", self.protocol.as_str());
        }
        let resp = rb.send().await.map_err(|e| McpError::Perdida(e.to_string()))?;
        let status = resp.status().as_u16();
        if let Some(s) = resp.headers().get("mcp-session-id").and_then(|v| v.to_str().ok()) {
            *sessao.lock() = Some(s.to_string());
        }
        if status == 202 || id.is_none() && status < 300 {
            return Ok(Value::Null);
        }
        if status >= 400 {
            if status == 404 {
                self.vivo.store(false, Ordering::SeqCst);
            }
            return Err(McpError::Http(status));
        }
        let tipo = resp.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_lowercase();
        let id = id.unwrap_or(0);
        if tipo.contains("text/event-stream") {
            let mut corpo = resp.bytes_stream();
            let mut buf = String::new();
            while let Some(parte) = corpo.next().await {
                let parte = parte.map_err(|e| McpError::Perdida(e.to_string()))?;
                buf.push_str(&String::from_utf8_lossy(&parte));
                while let Some(fim) = buf.find("\n\n") {
                    let evento: String = buf.drain(..fim + 2).collect();
                    let dados: Vec<&str> = evento.lines().filter_map(|l| l.strip_prefix("data:")).map(str::trim_start).collect();
                    if dados.is_empty() {
                        continue;
                    }
                    let Ok(m) = serde_json::from_str::<Value>(&dados.join("\n")) else { continue };
                    if m.get("method").is_some() {
                        if m.get("id").is_some() {
                            // pedido do servidor: recusa (ping responde), sem esperar
                            let r = resposta_a_pedido_do_servidor(&m);
                            let _ = Box::pin(self.post_http(&r, None)).await;
                        }
                        continue;
                    }
                    if id_de(&m) == Some(id) {
                        return erro_rpc(&m).map(Err).unwrap_or_else(|| Ok(m["result"].clone()));
                    }
                }
            }
            Err(McpError::Perdida("a resposta terminou sem resultado".into()))
        } else {
            let m: Value = resp.json().await.map_err(|e| McpError::Outro(e.to_string()))?;
            erro_rpc(&m).map(Err).unwrap_or_else(|| Ok(m["result"].clone()))
        }
    }

    pub async fn request(&self, method: &str, params: Value, timeout: Duration) -> R<Value> {
        if !self.vivo() {
            return Err(McpError::Perdida("desconectado".into()));
        }
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let msg = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        let fut = async {
            match &self.conn {
                Conn::Stdio { stdin, pendentes, .. } => {
                    let (tx, rx) = oneshot::channel();
                    pendentes.lock().insert(id, tx);
                    if let Err(e) = Self::enviar_stdio(stdin, &msg).await {
                        pendentes.lock().remove(&id);
                        return Err(e);
                    }
                    rx.await.unwrap_or_else(|_| Err(McpError::Perdida("resposta perdida".into())))
                }
                Conn::Http { .. } => self.post_http(&msg, Some(id)).await,
            }
        };
        // cancelar quem espera (queda do future) avisa o servidor
        struct Aviso<'a> {
            c: &'a McpClient,
            id: i64,
            fim: bool,
        }
        impl Drop for Aviso<'_> {
            fn drop(&mut self) {
                if !self.fim {
                    self.c.cancelado(self.id);
                }
            }
        }
        let mut aviso = Aviso { c: self, id, fim: false };
        let r = match tokio::time::timeout(timeout, fut).await {
            Ok(r) => r,
            Err(_) => {
                aviso.fim = true;
                self.cancelado(id);
                Err(McpError::Tempo)
            }
        };
        aviso.fim = true;
        if let Conn::Stdio { pendentes, .. } = &self.conn {
            pendentes.lock().remove(&id);
        }
        r
    }

    /// `notifications/cancelled` (melhor esforço, sem esperar).
    fn cancelado(&self, id: i64) {
        if let Conn::Stdio { stdin, pendentes, .. } = &self.conn {
            pendentes.lock().remove(&id);
            let msg = json!({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": id, "reason": "cancelado pelo Orion"}});
            if let Ok(h) = tokio::runtime::Handle::try_current() {
                let stdin = stdin.clone();
                h.spawn(async move {
                    let _ = McpClient::enviar_stdio(&stdin, &msg).await;
                });
            }
        }
    }

    pub async fn list_tools(&self, cursor: Option<&str>) -> R<ToolsPage> {
        let p = match cursor {
            Some(c) => json!({"cursor": c}),
            None => json!({}),
        };
        let r = self.request("tools/list", p, self.timeout).await?;
        let tools = r["tools"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|t| {
                        Some(ToolInfo { name: t["name"].as_str()?.to_string(), description: t["description"].as_str().map(String::from), input_schema: t.get("inputSchema").cloned().unwrap_or(Value::Null) })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(ToolsPage { tools, next_cursor: r["nextCursor"].as_str().map(String::from) })
    }

    pub async fn call_tool(&self, name: &str, arguments: Map<String, Value>, timeout: Duration) -> R<CallResult> {
        let r = self.request("tools/call", json!({"name": name, "arguments": arguments}), timeout).await?;
        Ok(CallResult { content: r["content"].as_array().cloned().unwrap_or_default(), structured: r.get("structuredContent").filter(|v| !v.is_null()).cloned(), is_error: r["isError"].as_bool().unwrap_or(false) })
    }

    pub async fn list_resources(&self, cursor: Option<&str>) -> R<ResourcesPage> {
        let p = cursor.map(|c| json!({"cursor": c})).unwrap_or_else(|| json!({}));
        let r = self.request("resources/list", p, self.timeout).await?;
        Ok(ResourcesPage { uris: r["resources"].as_array().map(|a| a.iter().filter_map(|x| x["uri"].as_str().map(String::from)).collect()).unwrap_or_default(), next_cursor: r["nextCursor"].as_str().map(String::from) })
    }

    /// Textos do recurso (`None` para conteúdo binário).
    pub async fn read_resource(&self, uri: &str) -> R<Vec<Option<String>>> {
        let r = self.request("resources/read", json!({"uri": uri}), self.timeout).await?;
        Ok(r["contents"].as_array().map(|a| a.iter().map(|c| c["text"].as_str().map(String::from)).collect()).unwrap_or_default())
    }

    pub async fn list_prompts(&self) -> R<Vec<PromptInfo>> {
        let r = self.request("prompts/list", json!({}), self.timeout).await?;
        Ok(r["prompts"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|p| {
                        Some(PromptInfo {
                            name: p["name"].as_str()?.to_string(),
                            description: p["description"].as_str().map(String::from),
                            arguments: p["arguments"].as_array().map(|x| x.iter().filter_map(|a| Some((a["name"].as_str()?.to_string(), a["required"].as_bool().unwrap_or(false)))).collect()).unwrap_or_default(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    /// Textos das mensagens do prompt (`None` para conteúdo não textual).
    pub async fn get_prompt(&self, name: &str, arguments: &HashMap<String, String>) -> R<Vec<Option<String>>> {
        let r = self.request("prompts/get", json!({"name": name, "arguments": arguments}), self.timeout).await?;
        Ok(r["messages"].as_array().map(|a| a.iter().map(|m| m["content"]["text"].as_str().map(String::from)).collect()).unwrap_or_default())
    }

    pub async fn fechar(&self) {
        self.vivo.store(false, Ordering::SeqCst);
        match &self.conn {
            Conn::Stdio { child, pendentes, .. } => {
                let _ = child.lock().await.start_kill();
                for (_, tx) in pendentes.lock().drain() {
                    let _ = tx.send(Err(McpError::Perdida("fechado".into())));
                }
            }
            Conn::Http { http, url, headers, sessao } => {
                let s = sessao.lock().take();
                if let Some(s) = s {
                    let mut rb = http.delete(url).header("mcp-session-id", s).timeout(Duration::from_secs(3));
                    for (k, v) in headers {
                        rb = rb.header(k.as_str(), v.as_str());
                    }
                    let _ = rb.send().await;
                }
            }
        }
    }
}
