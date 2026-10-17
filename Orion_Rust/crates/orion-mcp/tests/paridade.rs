//! Paridade com `tests/mcp_cliente/` e `tests/test_mcp_servidor.py` (Python): o protocolo MCP do
//! lado cliente (stdio e Streamable HTTP) e o despacho do lado servidor.
//!
//! O servidor MCP de teste do cliente stdio é o mesmo `servidor_cru.py` dos testes Python
//! (stdlib pura); sem `python3` no PATH esses testes se pulam. Ficam no `orion-core` (camada
//! acima): configuração `mcp.json`, nomes de ferramenta, classes de risco, `McpManager`,
//! reconexão, índice, recursos e prompts com allowlist, servidor somente leitura e exportação.

use futures::future::BoxFuture;
use orion_mcp::server::{despachar, Ctx, Handler, RpcError, RpcResult};
use orion_mcp::{Auth, McpClient, McpError, Transport, R};
use orion_testkit::http::{servidor, Req, Resp, Servidor};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const T: Duration = Duration::from_secs(20);

fn python() -> Option<String> {
    ["python3", "python"].iter().find(|p| std::process::Command::new(p).arg("--version").output().is_ok()).map(|s| s.to_string())
}

fn cru_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../tests/mcp_cliente/servidor_cru.py")
}

/// Conexão stdio com o servidor de teste; `None` (e o teste se pula) sem Python.
async fn cru(env: &[(&str, &str)]) -> Option<R<McpClient>> {
    let py = python()?;
    let env = env.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    Some(McpClient::connect(Transport::Stdio { command: py, args: vec![cru_path().to_string_lossy().into_owned()], env, cwd: None }, T).await)
}

macro_rules! cru_ou_pula {
    ($env:expr) => {
        match cru($env).await {
            Some(r) => r,
            None => {
                eprintln!("sem python3: teste pulado");
                return;
            }
        }
    };
}

/// Servidor MCP inline (Python) para o que o `servidor_cru.py` não cobre.
async fn inline(script: &str) -> Option<R<McpClient>> {
    let py = python()?;
    Some(McpClient::connect(Transport::Stdio { command: py, args: vec!["-u".into(), "-c".into(), script.into()], env: HashMap::new(), cwd: None }, T).await)
}

const PRELUDIO: &str = r#"
import json, sys, os
def enviar(m): sys.stdout.write(json.dumps(m) + "\n"); sys.stdout.flush()
def init(id_): enviar({"jsonrpc": "2.0", "id": id_, "result": {"protocolVersion": "2025-11-25", "capabilities": {"tools": {}}, "serverInfo": {"name": "i", "version": "0"}}})
"#;

fn args(v: Value) -> Map<String, Value> {
    v.as_object().unwrap().clone()
}

// ── E7.1: protocolo ─────────────────────────────────────────────────────────
#[tokio::test]
async fn servidor_com_protocolo_nao_comprovado_e_recusado_com_mensagem_clara() {
    for versao in ["2024-11-05", "1900-01-01"] {
        let r = cru_ou_pula!(&[("PROTOCOLO", versao)]);
        match r {
            Err(McpError::Incompativel(m)) => {
                assert!(m.contains(&format!("o servidor fala o protocolo {versao}, o Orion fala")), "{m}");
                assert!(m.contains("2025-11-25") && m.contains("2026-07-28"), "{m}");
            }
            Err(e) => panic!("{e:?}"),
            Ok(_) => panic!("{versao} deveria ser recusado"),
        }
    }
}

#[tokio::test]
async fn protocolo_comprovado_passa_e_fica_registrado() {
    for versao in ["2025-11-25", "2026-07-28"] {
        let c = cru_ou_pula!(&[("PROTOCOLO", versao)]).unwrap();
        assert_eq!(c.protocol, versao);
        assert!(c.vivo());
        c.fechar().await;
    }
}

// ── cliente stdio: ferramentas, recursos e prompts ──────────────────────────
#[tokio::test]
async fn lista_e_chama_ferramentas() {
    let c = cru_ou_pula!(&[]).unwrap();
    let p = c.list_tools(None).await.unwrap();
    let nomes: Vec<_> = p.tools.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(nomes, ["dormir", "eco"]);
    assert!(p.next_cursor.is_none());
    assert_eq!(p.tools[1].description.as_deref(), Some("Devolve o texto."));
    let r = c.call_tool("eco", args(json!({"texto": "olá"})), T).await.unwrap();
    assert_eq!(r.texto(), ("olá".to_string(), false));
    c.fechar().await;
}

#[tokio::test]
async fn muitas_ferramentas_chegam_inteiras() {
    let c = cru_ou_pula!(&[("N_FERRAMENTAS", "40")]).unwrap();
    assert_eq!(c.list_tools(None).await.unwrap().tools.len(), 42);
    c.fechar().await;
}

#[tokio::test]
async fn erro_do_servidor_vira_erro_tipado() {
    let c = cru_ou_pula!(&[]).unwrap();
    match c.request("metodo/inexistente", json!({}), T).await {
        Err(McpError::Servidor { code, message }) => assert!(code == -32601 && message.contains("desconhecido"), "{code} {message}"),
        o => panic!("{o:?}"),
    }
    assert!(c.vivo()); // erro de método não derruba a conexão
    c.fechar().await;
}

#[tokio::test]
async fn recursos_e_prompts() {
    let c = cru_ou_pula!(&[("COM_RECURSOS", "1")]).unwrap();
    let r = c.list_resources(None).await.unwrap();
    assert_eq!(r.uris, ["file:///docs/guia.md", "file:///segredo/chave.txt", "file:///docs/grande.txt"]);
    assert_eq!(c.read_resource("file:///docs/guia.md").await.unwrap(), vec![Some("# Guia\nconteudo do guia".to_string())]);
    assert!(matches!(c.read_resource("file:///nao/existe").await, Err(McpError::Servidor { code: -32002, .. })));
    let prompts = c.list_prompts().await.unwrap();
    assert_eq!((prompts[0].name.as_str(), prompts[0].arguments.clone()), ("revisar", vec![("texto".to_string(), true)]));
    let msgs = c.get_prompt("revisar", &HashMap::from([("texto".to_string(), "olá".to_string())])).await.unwrap();
    assert_eq!(msgs, vec![Some("Revise: olá".to_string())]);
    c.fechar().await;
}

// ── E7.2: cancelamento ──────────────────────────────────────────────────────
async fn esperar_linhas(arq: &std::path::Path, n: usize) -> Vec<String> {
    for _ in 0..80 {
        if let Ok(t) = std::fs::read_to_string(arq) {
            if t.lines().count() >= n {
                return t.lines().map(String::from).collect();
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    std::fs::read_to_string(arq).map(|t| t.lines().map(String::from).collect()).unwrap_or_default()
}

#[tokio::test]
async fn tempo_esgotado_avisa_o_servidor_e_a_conexao_segue_de_pe() {
    let d = tempfile::tempdir().unwrap();
    let log = d.path().join("cancelado.log");
    let c = cru_ou_pula!(&[("LOG_CANCELADO", log.to_str().unwrap())]).unwrap();
    let r = c.call_tool("dormir", Map::new(), Duration::from_millis(500)).await;
    assert!(matches!(r, Err(McpError::Tempo)), "{r:?}");
    let linhas = esperar_linhas(&log, 1).await;
    assert!(!linhas.is_empty(), "o servidor não recebeu notifications/cancelled");
    assert!(linhas[0].contains("requestId"), "{}", linhas[0]);
    assert!(c.vivo());
    assert_eq!(c.call_tool("eco", args(json!({"texto": "ainda aqui"})), T).await.unwrap().texto().0, "ainda aqui");
    c.fechar().await;
}

#[tokio::test]
async fn cancelar_quem_espera_avisa_o_servidor() {
    let d = tempfile::tempdir().unwrap();
    let log = d.path().join("cancelado.log");
    let c = Arc::new(cru_ou_pula!(&[("LOG_CANCELADO", log.to_str().unwrap())]).unwrap());
    let c2 = c.clone();
    let tarefa = tokio::spawn(async move { c2.call_tool("dormir", Map::new(), T).await });
    tokio::time::sleep(Duration::from_millis(1500)).await;
    tarefa.abort();
    assert!(tarefa.await.unwrap_err().is_cancelled());
    assert!(!esperar_linhas(&log, 1).await.is_empty(), "o servidor não recebeu notifications/cancelled");
    c.fechar().await;
}

// ── queda e ambiente ────────────────────────────────────────────────────────
#[cfg(unix)]
#[tokio::test]
async fn comando_que_nao_existe_e_servidor_que_fecha_a_saida() {
    match McpClient::connect(Transport::Stdio { command: "/nao/existe/binario".into(), args: vec![], env: HashMap::new(), cwd: None }, T).await {
        Err(McpError::Outro(m)) => assert!(m.contains("não consegui iniciar"), "{m}"),
        o => panic!("{:?}", o.err()),
    }
    let r = McpClient::connect(Transport::Stdio { command: "sh".into(), args: vec!["-c".into(), "exit 0".into()], env: HashMap::new(), cwd: None }, T).await;
    assert!(r.err().is_some_and(|e| e.conexao_perdida()));
}

#[tokio::test]
async fn servidor_que_morre_durante_a_chamada_derruba_a_conexao() {
    let script = format!(
        "{PRELUDIO}\nfor l in sys.stdin:\n    m = json.loads(l)\n    if m.get('method') == 'initialize': init(m['id'])\n    elif m.get('method') == 'tools/call': os._exit(1)\n"
    );
    let Some(r) = inline(&script).await else { return };
    let c = r.unwrap();
    let e = c.call_tool("sair", Map::new(), T).await.unwrap_err();
    assert!(e.conexao_perdida(), "{e:?}");
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!c.vivo());
    // depois da queda, pedir de novo falha rápido, sem esperar o tempo-limite
    assert!(c.request("ping", json!({}), T).await.unwrap_err().conexao_perdida());
}

#[tokio::test]
async fn servidor_nao_herda_orion_nem_segredos_do_ambiente() {
    std::env::set_var("ORION_ADMIN_TOKEN", "segredo-do-orion-123456");
    std::env::set_var("OPENAI_API_KEY", "sk-segredo");
    let script = format!(
        "{PRELUDIO}\nfor l in sys.stdin:\n    m = json.loads(l)\n    if m.get('method') == 'initialize': init(m['id'])\n    elif m.get('method') == 'tools/call':\n        enviar({{'jsonrpc': '2.0', 'id': m['id'], 'result': {{'content': [{{'type': 'text', 'text': json.dumps(sorted(k for k in os.environ if k.startswith(('ORION_', 'OPENAI', 'CHAVE'))))}}]}}}})\n"
    );
    let Some(py) = python() else { return };
    let env = HashMap::from([("CHAVE_EXPLICITA".to_string(), "do-mcp-json".to_string())]);
    let c = McpClient::connect(Transport::Stdio { command: py, args: vec!["-u".into(), "-c".into(), script], env, cwd: None }, T).await.unwrap();
    let (texto, _) = c.call_tool("env", Map::new(), T).await.unwrap().texto();
    let visiveis: Vec<String> = serde_json::from_str(&texto).unwrap();
    assert_eq!(visiveis, ["CHAVE_EXPLICITA"], "o filho enxergou {visiveis:?}");
    c.fechar().await;
}

#[tokio::test]
async fn pedidos_do_servidor_ao_cliente_sao_recusados_e_ping_responde() {
    let script = format!(
        "{PRELUDIO}\nfor l in sys.stdin:\n    m = json.loads(l)\n    if m.get('method') == 'initialize': init(m['id'])\n    elif m.get('method') == 'tools/call':\n        enviar({{'jsonrpc': '2.0', 'id': 's1', 'method': 'sampling/createMessage', 'params': {{}}}})\n        enviar({{'jsonrpc': '2.0', 'id': 's2', 'method': 'ping'}})\n        r1 = json.loads(sys.stdin.readline()); r2 = json.loads(sys.stdin.readline())\n        enviar({{'jsonrpc': '2.0', 'id': m['id'], 'result': {{'content': [{{'type': 'text', 'text': json.dumps([r1, r2])}}]}}}})\n"
    );
    let Some(r) = inline(&script).await else { return };
    let c = r.unwrap();
    let (t, _) = c.call_tool("x", Map::new(), T).await.unwrap().texto();
    let v: Vec<Value> = serde_json::from_str(&t).unwrap();
    assert!(v[0]["id"] == "s1" && v[0]["error"]["code"] == -32601, "{v:?}");
    assert!(v[1]["id"] == "s2" && v[1]["result"] == json!({}), "{v:?}");
    c.fechar().await;
}

// ── Streamable HTTP (C10) ───────────────────────────────────────────────────
fn rpc_ok(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn init_resp(req: &Req) -> Resp {
    Resp::json(200, rpc_ok(&req.json()["id"], json!({"protocolVersion": "2025-11-25", "capabilities": {"tools": {}}, "serverInfo": {"name": "h", "version": "0"}})))
        .com_header("mcp-session-id", "sessao-1")
}

fn http(srv: &Servidor, headers: &[(&str, &str)]) -> Transport {
    Transport::Http { url: srv.url("/mcp"), headers: headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(), auth: None }
}

/// Servidor HTTP básico: initialize, tools/list e tools/call (soma `a`+`b`).
fn mcp_http(extra: impl Fn(&Req) -> Option<Resp> + Send + Sync + 'static) -> Servidor {
    servidor(move |r| {
        if let Some(x) = extra(r) {
            return x;
        }
        if r.method == "DELETE" {
            return Resp::texto(200, "");
        }
        let m = r.json();
        match m["method"].as_str().unwrap_or("") {
            "initialize" => init_resp(r),
            "notifications/initialized" => Resp::texto(202, ""),
            "tools/list" => Resp::json(200, rpc_ok(&m["id"], json!({"tools": [{"name": "somar", "description": "Soma.", "inputSchema": {"type": "object"}}]}))),
            "tools/call" => {
                let s = m["params"]["arguments"]["a"].as_i64().unwrap_or(0) + m["params"]["arguments"]["b"].as_i64().unwrap_or(0);
                Resp::json(200, rpc_ok(&m["id"], json!({"content": [{"type": "text", "text": s.to_string()}]})))
            }
            _ => Resp::json(200, json!({"jsonrpc": "2.0", "id": m["id"], "error": {"code": -32601, "message": "método desconhecido"}})),
        }
    })
}

#[tokio::test]
async fn conecta_por_http_lista_e_chama_com_sessao_e_cabecalhos() {
    let srv = mcp_http(|_| None);
    let c = McpClient::connect(http(&srv, &[("X-Chave", "valor")]), T).await.unwrap();
    assert_eq!(c.protocol, "2025-11-25");
    assert_eq!(c.list_tools(None).await.unwrap().tools[0].name, "somar");
    assert_eq!(c.call_tool("somar", args(json!({"a": 2, "b": 3})), T).await.unwrap().texto().0, "5");
    // initialize, initialized, tools/list, tools/call
    let metodos: Vec<_> = (0..srv.n()).map(|i| srv.req(i).json()["method"].as_str().unwrap_or("").to_string()).collect();
    assert_eq!(metodos, ["initialize", "notifications/initialized", "tools/list", "tools/call"]);
    for i in 0..srv.n() {
        assert_eq!(srv.req(i).header("x-chave"), "valor"); // cabeçalho do mcp.json em todas
    }
    assert_eq!(srv.req(0).header("mcp-session-id"), ""); // a sessão nasce na resposta do initialize
    assert_eq!(srv.req(2).header("mcp-session-id"), "sessao-1");
    assert_eq!(srv.req(2).header("mcp-protocol-version"), "2025-11-25");
    assert!(srv.req(0).header("accept").contains("text/event-stream"));
    c.fechar().await;
    let ultimo = srv.req(srv.n() - 1);
    assert_eq!((ultimo.method.as_str(), ultimo.header("mcp-session-id")), ("DELETE", "sessao-1")); // encerra a sessão
}

#[tokio::test]
async fn resposta_em_sse_com_notificacao_no_meio() {
    let srv = mcp_http(|r| {
        let m = r.json();
        if m["method"] != "tools/call" {
            return None;
        }
        let corpo = format!(
            "data: {}\n\ndata: {}\n\n",
            json!({"jsonrpc": "2.0", "method": "notifications/message", "params": {"level": "info"}}),
            rpc_ok(&m["id"], json!({"content": [{"type": "text", "text": "pelo sse"}]}))
        );
        Some(Resp::sse(corpo.into_bytes()))
    });
    let c = McpClient::connect(http(&srv, &[]), T).await.unwrap();
    assert_eq!(c.call_tool("x", Map::new(), T).await.unwrap().texto().0, "pelo sse");
    c.fechar().await;
}

#[tokio::test]
async fn pedido_do_servidor_dentro_do_sse_e_recusado_por_post() {
    let srv = mcp_http(|r| {
        let m = r.json();
        if m["method"] == "tools/call" {
            let corpo = format!(
                "data: {}\n\ndata: {}\n\n",
                json!({"jsonrpc": "2.0", "id": "srv-1", "method": "sampling/createMessage", "params": {}}),
                rpc_ok(&m["id"], json!({"content": [{"type": "text", "text": "ok"}]}))
            );
            return Some(Resp::sse(corpo.into_bytes()));
        }
        // a recusa chega como POST com o id do servidor
        if m["id"] == "srv-1" {
            return Some(Resp::texto(202, ""));
        }
        None
    });
    let c = McpClient::connect(http(&srv, &[]), T).await.unwrap();
    assert_eq!(c.call_tool("x", Map::new(), T).await.unwrap().texto().0, "ok");
    let recusa = (0..srv.n()).map(|i| srv.req(i).json()).find(|m| m["id"] == "srv-1").expect("a recusa não foi enviada");
    assert_eq!(recusa["error"]["code"], -32601);
    c.fechar().await;
}

#[tokio::test]
async fn http_404_marca_a_sessao_como_morta() {
    let morta = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let m2 = morta.clone();
    let srv = mcp_http(move |r| (m2.load(Ordering::SeqCst) && r.method == "POST").then(|| Resp::texto(404, "sessão expirou")));
    let c = McpClient::connect(http(&srv, &[]), T).await.unwrap();
    morta.store(true, Ordering::SeqCst);
    assert!(matches!(c.list_tools(None).await, Err(McpError::Http(404))));
    assert!(!c.vivo());
    assert!(c.list_tools(None).await.unwrap_err().conexao_perdida());
}

struct Cofre {
    token: Mutex<Option<String>>,
    avisos: AtomicUsize,
    www: Mutex<String>,
}

impl Auth for Cofre {
    fn bearer(&self) -> BoxFuture<'_, R<Option<String>>> {
        Box::pin(async move { Ok(self.token.lock().unwrap().clone()) })
    }
    fn nao_autorizado<'a>(&'a self, www: Option<&'a str>) -> BoxFuture<'a, R<()>> {
        Box::pin(async move {
            self.avisos.fetch_add(1, Ordering::SeqCst);
            *self.www.lock().unwrap() = www.unwrap_or("").to_string();
            *self.token.lock().unwrap() = Some("token-novo".into());
            Ok(())
        })
    }
}

#[tokio::test]
async fn http_401_roda_o_fluxo_de_autorizacao_e_repete_uma_vez() {
    let srv = mcp_http(|r| match r.header("authorization") {
        "Bearer token-novo" => None,
        _ => Some(Resp::texto(401, "").com_header("www-authenticate", "Bearer resource_metadata=\"https://x/meta\"")),
    });
    let cofre = Arc::new(Cofre { token: Mutex::new(None), avisos: AtomicUsize::new(0), www: Mutex::new(String::new()) });
    let t = Transport::Http { url: srv.url("/mcp"), headers: HashMap::new(), auth: Some(cofre.clone()) };
    let c = McpClient::connect(t, T).await.unwrap();
    assert_eq!(cofre.avisos.load(Ordering::SeqCst), 1);
    assert!(cofre.www.lock().unwrap().contains("resource_metadata"));
    assert_eq!(c.list_tools(None).await.unwrap().tools.len(), 1);
    assert_eq!(cofre.avisos.load(Ordering::SeqCst), 1); // o token vale: não pergunta de novo
    c.fechar().await;
}

#[tokio::test]
async fn servidor_remoto_fora_do_ar_falha_sem_vazar_o_cabecalho() {
    let porta = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let t = Transport::Http { url: format!("http://127.0.0.1:{porta}/mcp"), headers: HashMap::from([("Authorization".into(), "Bearer SEGREDO-XYZ".into())]), auth: None };
    let e = McpClient::connect(t, Duration::from_secs(5)).await.err().expect("devia falhar");
    assert!(e.conexao_perdida());
    assert!(!format!("{e:?}{e}").contains("SEGREDO-XYZ"));
}

// ── test_mcp_servidor.py: o despacho do lado servidor ───────────────────────
struct Fake {
    recursos: bool,
    prompts: bool,
}

impl Handler for Fake {
    fn info(&self) -> (String, String, Option<String>) {
        ("fake".into(), "1".into(), Some("só lê".into()))
    }
    fn list_tools<'a>(&'a self, _: &'a Ctx) -> BoxFuture<'a, RpcResult<Vec<Value>>> {
        Box::pin(async { Ok(vec![json!({"name": "buscar", "description": "d", "inputSchema": {"type": "object"}})]) })
    }
    fn call_tool<'a>(&'a self, ctx: &'a Ctx, name: &'a str, a: Value) -> BoxFuture<'a, RpcResult<Value>> {
        Box::pin(async move {
            if name != "buscar" {
                return Err(RpcError::new(-32602, format!("ferramenta desconhecida: {name}")));
            }
            Ok(json!({"content": [{"type": "text", "text": format!("{}|{}", ctx.client_name, a)}], "isError": false}))
        })
    }
    fn list_resources<'a>(&'a self, _: &'a Ctx) -> BoxFuture<'a, RpcResult<Vec<Value>>> {
        Box::pin(async { Ok(vec![json!({"uri": "orion://fatos", "name": "fatos"})]) })
    }
    fn read_resource<'a>(&'a self, _: &'a Ctx, uri: &'a str) -> BoxFuture<'a, RpcResult<Vec<Value>>> {
        Box::pin(async move { Ok(vec![json!({"uri": uri, "text": "conteúdo"})]) })
    }
    fn list_prompts<'a>(&'a self, _: &'a Ctx) -> BoxFuture<'a, RpcResult<Vec<Value>>> {
        Box::pin(async { Ok(vec![json!({"name": "p"})]) })
    }
    fn get_prompt<'a>(&'a self, _: &'a Ctx, name: &'a str, _: Value) -> BoxFuture<'a, RpcResult<String>> {
        Box::pin(async move { Ok(format!("prompt {name}")) })
    }
    fn tem_prompts(&self) -> bool {
        self.prompts
    }
    fn tem_recursos(&self) -> bool {
        self.recursos
    }
}

async fn rpc(h: &Fake, ctx: &mut Ctx, m: Value) -> Option<Value> {
    despachar(h, ctx, m).await
}

#[tokio::test]
async fn despacho_negocia_protocolo_e_anuncia_so_o_que_tem() {
    let h = Fake { recursos: false, prompts: false };
    let mut ctx = Ctx::default();
    let r = rpc(&h, &mut ctx, json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2026-07-28", "clientInfo": {"name": "claude-code"}}})).await.unwrap();
    assert_eq!(r["result"]["protocolVersion"], "2026-07-28");
    assert_eq!(r["result"]["serverInfo"], json!({"name": "fake", "version": "1"}));
    assert_eq!(r["result"]["instructions"], "só lê");
    assert_eq!(r["result"]["capabilities"], json!({"tools": {}}));
    assert_eq!(ctx.client_name, "claude-code");
    // protocolo que o Orion não comprovou: responde com o primeiro dos comprovados
    let r = rpc(&h, &mut ctx, json!({"jsonrpc": "2.0", "id": 2, "method": "initialize", "params": {"protocolVersion": "1900-01-01"}})).await.unwrap();
    assert_eq!(r["result"]["protocolVersion"], "2025-11-25");
    let h = Fake { recursos: true, prompts: true };
    let r = rpc(&h, &mut Ctx::default(), json!({"jsonrpc": "2.0", "id": 3, "method": "initialize", "params": {}})).await.unwrap();
    assert_eq!(r["result"]["capabilities"], json!({"tools": {}, "resources": {}, "prompts": {}}));
}

#[tokio::test]
async fn despacho_limita_o_nome_do_cliente() {
    let mut ctx = Ctx::default();
    let longo = "n".repeat(500);
    rpc(&Fake { recursos: false, prompts: false }, &mut ctx, json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"clientInfo": {"name": longo}}})).await;
    assert_eq!(ctx.client_name.len(), 60);
}

#[tokio::test]
async fn despacho_ping_notificacao_e_erros() {
    let h = Fake { recursos: false, prompts: false };
    let mut ctx = Ctx::default();
    assert_eq!(rpc(&h, &mut ctx, json!({"jsonrpc": "2.0", "id": 1, "method": "ping"})).await.unwrap()["result"], json!({}));
    assert!(rpc(&h, &mut ctx, json!({"jsonrpc": "2.0", "method": "notifications/initialized"})).await.is_none()); // sem id: sem resposta
    let r = rpc(&h, &mut ctx, json!({"jsonrpc": "2.0", "id": 2, "method": "nada/existe"})).await.unwrap();
    assert_eq!(r["error"]["code"], -32601);
    let r = rpc(&h, &mut ctx, json!({"jsonrpc": "2.0", "id": 3})).await.unwrap();
    assert_eq!(r["error"]["code"], -32600);
    let r = rpc(&h, &mut ctx, json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {}})).await.unwrap();
    assert_eq!(r["error"]["code"], -32602);
    let r = rpc(&h, &mut ctx, json!({"jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": {"name": "apagar_tudo"}})).await.unwrap();
    assert!(r["error"]["code"] == -32602 && r["error"]["message"].as_str().unwrap().contains("apagar_tudo"));
}

#[tokio::test]
async fn despacho_ferramentas_recursos_e_prompts() {
    let h = Fake { recursos: true, prompts: true };
    let mut ctx = Ctx { client_name: "cli".into(), bearer: None };
    let r = rpc(&h, &mut ctx, json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"})).await.unwrap();
    assert_eq!(r["result"]["tools"][0]["name"], "buscar");
    let r = rpc(&h, &mut ctx, json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "buscar", "arguments": {"q": "x"}}})).await.unwrap();
    assert_eq!(r["result"]["content"][0]["text"], "cli|{\"q\":\"x\"}");
    assert_eq!(r["id"], 2);
    let r = rpc(&h, &mut ctx, json!({"jsonrpc": "2.0", "id": 3, "method": "resources/list"})).await.unwrap();
    assert_eq!(r["result"]["resources"][0]["uri"], "orion://fatos");
    let r = rpc(&h, &mut ctx, json!({"jsonrpc": "2.0", "id": 4, "method": "resources/read", "params": {"uri": "orion://fatos"}})).await.unwrap();
    assert_eq!(r["result"]["contents"][0]["text"], "conteúdo");
    let r = rpc(&h, &mut ctx, json!({"jsonrpc": "2.0", "id": 5, "method": "resources/read", "params": {}})).await.unwrap();
    assert_eq!(r["error"]["code"], -32602);
    let r = rpc(&h, &mut ctx, json!({"jsonrpc": "2.0", "id": 6, "method": "prompts/list"})).await.unwrap();
    assert_eq!(r["result"]["prompts"][0]["name"], "p");
    let r = rpc(&h, &mut ctx, json!({"jsonrpc": "2.0", "id": 7, "method": "prompts/get", "params": {"name": "p"}})).await.unwrap();
    assert_eq!(r["result"]["messages"][0], json!({"role": "user", "content": {"type": "text", "text": "prompt p"}}));
}

#[tokio::test]
async fn handler_padrao_so_anuncia_ferramentas_e_recusa_o_resto() {
    struct Vazio;
    impl Handler for Vazio {
        fn info(&self) -> (String, String, Option<String>) {
            ("v".into(), "0".into(), None)
        }
    }
    let mut ctx = Ctx::default();
    let r = despachar(&Vazio, &mut ctx, json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}})).await.unwrap();
    assert!(r["result"].get("instructions").is_none());
    let r = despachar(&Vazio, &mut ctx, json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})).await.unwrap();
    assert_eq!(r["result"]["tools"], json!([]));
    let r = despachar(&Vazio, &mut ctx, json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "x"}})).await.unwrap();
    assert_eq!(r["error"]["code"], -32602);
    let r = despachar(&Vazio, &mut ctx, json!({"jsonrpc": "2.0", "id": 4, "method": "prompts/get", "params": {"name": "x"}})).await.unwrap();
    assert_eq!(r["error"]["code"], -32602);
}
