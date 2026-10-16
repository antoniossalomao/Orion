//! Lado servidor do MCP: o despacho JSON-RPC (initialize, ping, tools, resources, prompts) sobre
//! um `Handler`. O transporte (stdio ou HTTP) fica com quem chama. O Orion como servidor só lê
//! (REGRAS.md: o Orion como servidor MCP só lê).

use crate::TESTED_PROTOCOLS;
use futures::future::BoxFuture;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// Quem está perguntando.
#[derive(Debug, Clone, Default)]
pub struct Ctx {
    /// `clientInfo.name` do `initialize` (preenchido pelo despacho em stdio).
    pub client_name: String,
    /// Token `Bearer` da requisição HTTP, se houver.
    pub bearer: Option<String>,
}

/// Erro JSON-RPC de um método.
#[derive(Debug, Clone)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
}

impl RpcError {
    pub fn new(code: i64, message: impl Into<String>) -> Self {
        RpcError { code, message: message.into() }
    }
}

pub type RpcResult<T> = Result<T, RpcError>;

pub trait Handler: Send + Sync {
    /// (nome, versão, instruções)
    fn info(&self) -> (String, String, Option<String>);
    /// Definições `{name, description, inputSchema, annotations?}`.
    fn list_tools<'a>(&'a self, _ctx: &'a Ctx) -> BoxFuture<'a, RpcResult<Vec<Value>>> {
        Box::pin(async { Ok(vec![]) })
    }
    /// O `CallToolResult` completo (`content`, `isError`).
    fn call_tool<'a>(&'a self, _ctx: &'a Ctx, name: &'a str, _args: Value) -> BoxFuture<'a, RpcResult<Value>> {
        Box::pin(async move { Err(RpcError::new(-32602, format!("ferramenta desconhecida: {name}"))) })
    }
    fn list_resources<'a>(&'a self, _ctx: &'a Ctx) -> BoxFuture<'a, RpcResult<Vec<Value>>> {
        Box::pin(async { Ok(vec![]) })
    }
    /// `contents` do `ReadResourceResult`.
    fn read_resource<'a>(&'a self, _ctx: &'a Ctx, uri: &'a str) -> BoxFuture<'a, RpcResult<Vec<Value>>> {
        Box::pin(async move { Err(RpcError::new(-32602, format!("recurso desconhecido: {uri}"))) })
    }
    fn list_prompts<'a>(&'a self, _ctx: &'a Ctx) -> BoxFuture<'a, RpcResult<Vec<Value>>> {
        Box::pin(async { Ok(vec![]) })
    }
    /// Texto do prompt.
    fn get_prompt<'a>(&'a self, _ctx: &'a Ctx, name: &'a str, _args: Value) -> BoxFuture<'a, RpcResult<String>> {
        Box::pin(async move { Err(RpcError::new(-32602, format!("prompt desconhecido: {name}"))) })
    }
    fn tem_prompts(&self) -> bool {
        false
    }
    fn tem_recursos(&self) -> bool {
        false
    }
}

fn erro(id: &Value, e: RpcError) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": e.code, "message": e.message}})
}

fn ok(id: &Value, r: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": r})
}

/// Trata uma mensagem. `None` para notificação (sem resposta).
pub async fn despachar(h: &dyn Handler, ctx: &mut Ctx, msg: Value) -> Option<Value> {
    let id = msg.get("id").cloned()?;
    let Some(metodo) = msg["method"].as_str() else { return Some(erro(&id, RpcError::new(-32600, "pedido inválido"))) };
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    let r: RpcResult<Value> = async {
        match metodo {
            "initialize" => {
                let pedida = params["protocolVersion"].as_str().unwrap_or("");
                let versao = if TESTED_PROTOCOLS.contains(&pedida) { pedida } else { TESTED_PROTOCOLS[0] };
                if let Some(n) = params["clientInfo"]["name"].as_str() {
                    ctx.client_name = n.chars().take(60).collect();
                }
                let (nome, ver, instr) = h.info();
                let mut caps = json!({"tools": {}});
                if h.tem_recursos() {
                    caps["resources"] = json!({});
                }
                if h.tem_prompts() {
                    caps["prompts"] = json!({});
                }
                let mut r = json!({"protocolVersion": versao, "capabilities": caps, "serverInfo": {"name": nome, "version": ver}});
                if let Some(i) = instr {
                    r["instructions"] = json!(i);
                }
                Ok(r)
            }
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({"tools": h.list_tools(ctx).await?})),
            "tools/call" => {
                let nome = params["name"].as_str().ok_or_else(|| RpcError::new(-32602, "falta o nome"))?;
                h.call_tool(ctx, nome, params.get("arguments").cloned().unwrap_or(json!({}))).await
            }
            "resources/list" => Ok(json!({"resources": h.list_resources(ctx).await?})),
            "resources/read" => {
                let uri = params["uri"].as_str().ok_or_else(|| RpcError::new(-32602, "falta a uri"))?;
                Ok(json!({"contents": h.read_resource(ctx, uri).await?}))
            }
            "prompts/list" => Ok(json!({"prompts": h.list_prompts(ctx).await?})),
            "prompts/get" => {
                let nome = params["name"].as_str().ok_or_else(|| RpcError::new(-32602, "falta o nome"))?;
                let t = h.get_prompt(ctx, nome, params.get("arguments").cloned().unwrap_or(json!({}))).await?;
                Ok(json!({"messages": [{"role": "user", "content": {"type": "text", "text": t}}]}))
            }
            outro => Err(RpcError::new(-32601, format!("método desconhecido: {outro}"))),
        }
    }
    .await;
    Some(match r {
        Ok(v) => ok(&id, v),
        Err(e) => erro(&id, e),
    })
}

/// Serve em stdin/stdout (uma mensagem por linha) até o cliente fechar.
pub async fn serve_stdio(h: &dyn Handler) -> std::io::Result<()> {
    let mut linhas = BufReader::new(tokio::io::stdin()).lines();
    let mut saida = tokio::io::stdout();
    let mut ctx = Ctx::default();
    while let Some(l) = linhas.next_line().await? {
        let Ok(msg) = serde_json::from_str::<Value>(&l) else { continue };
        if let Some(r) = despachar(h, &mut ctx, msg).await {
            saida.write_all(format!("{r}\n").as_bytes()).await?;
            saida.flush().await?;
        }
    }
    Ok(())
}
