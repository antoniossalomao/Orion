//! Servidor HTTP de mentira para os testes de paridade (substitui o `httpx.MockTransport`).
//!
//! Sobe numa porta local, guarda cada requisição e responde com o que o teste mandar. Síncrono
//! (threads), para servir tanto clientes `async` quanto `blocking`.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct Req {
    pub method: String,
    pub path: String,
    /// Nomes em minúsculas.
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl Req {
    pub fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body).unwrap_or(serde_json::Value::Null)
    }
    pub fn header(&self, k: &str) -> &str {
        self.headers.get(k).map(String::as_str).unwrap_or("")
    }
}

pub enum Resp {
    Full { status: u16, headers: Vec<(String, String)>, body: Vec<u8> },
    /// Anuncia mais bytes do que manda e fecha: o cliente vê a conexão cair no meio.
    Cut(Vec<u8>),
    /// Aceita e fecha sem responder.
    Drop,
}

impl Resp {
    pub fn sse(corpo: Vec<u8>) -> Resp {
        Resp::Full { status: 200, headers: vec![("content-type".into(), "text/event-stream".into())], body: corpo }
    }
    pub fn json(status: u16, v: serde_json::Value) -> Resp {
        Resp::Full { status, headers: vec![("content-type".into(), "application/json".into())], body: v.to_string().into_bytes() }
    }
    pub fn texto(status: u16, t: &str) -> Resp {
        Resp::Full { status, headers: vec![("content-type".into(), "text/plain".into())], body: t.as_bytes().to_vec() }
    }
    pub fn com_header(self, k: &str, v: &str) -> Resp {
        match self {
            Resp::Full { status, mut headers, body } => {
                headers.push((k.into(), v.into()));
                Resp::Full { status, headers, body }
            }
            o => o,
        }
    }
}

pub struct Servidor {
    pub porta: u16,
    pub reqs: Arc<Mutex<Vec<Req>>>,
}

impl Servidor {
    pub fn url(&self, caminho: &str) -> String {
        format!("http://127.0.0.1:{}{}", self.porta, caminho)
    }
    pub fn n(&self) -> usize {
        self.reqs.lock().unwrap().len()
    }
    pub fn req(&self, i: usize) -> Req {
        self.reqs.lock().unwrap()[i].clone()
    }
    pub fn caminhos(&self) -> Vec<String> {
        self.reqs.lock().unwrap().iter().map(|r| r.path.clone()).collect()
    }
}

fn ler(mut s: &TcpStream) -> Option<Req> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    let fim = loop {
        let n = s.read(&mut tmp).ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break p + 4;
        }
    };
    let cab = String::from_utf8_lossy(&buf[..fim]).into_owned();
    let mut linhas = cab.split("\r\n");
    let mut inicio = linhas.next()?.split(' ');
    let method = inicio.next()?.to_string();
    let path = inicio.next()?.to_string();
    let mut headers = HashMap::new();
    for l in linhas {
        if let Some((k, v)) = l.split_once(':') {
            headers.insert(k.trim().to_lowercase(), v.trim().to_string());
        }
    }
    let tamanho: usize = headers.get("content-length").and_then(|v| v.parse().ok()).unwrap_or(0);
    let mut corpo = buf[fim..].to_vec();
    while corpo.len() < tamanho {
        let n = s.read(&mut tmp).ok()?;
        if n == 0 {
            break;
        }
        corpo.extend_from_slice(&tmp[..n]);
    }
    Some(Req { method, path, headers, body: corpo })
}

fn escrever(mut s: &TcpStream, r: Resp) {
    match r {
        Resp::Full { status, headers, body } => {
            let mut cab = format!("HTTP/1.1 {status} X\r\nConnection: close\r\nContent-Length: {}\r\n", body.len());
            for (k, v) in headers {
                cab.push_str(&format!("{k}: {v}\r\n"));
            }
            cab.push_str("\r\n");
            let _ = s.write_all(cab.as_bytes());
            let _ = s.write_all(&body);
        }
        Resp::Cut(prefixo) => {
            let cab = format!(
                "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\n\r\n",
                prefixo.len() + 100_000
            );
            let _ = s.write_all(cab.as_bytes());
            let _ = s.write_all(&prefixo);
            let _ = s.flush();
        }
        Resp::Drop => {}
    }
    let _ = s.shutdown(std::net::Shutdown::Both);
}

/// Sobe o servidor; `h` decide a resposta de cada requisição.
pub fn servidor(h: impl Fn(&Req) -> Resp + Send + Sync + 'static) -> Servidor {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let porta = l.local_addr().unwrap().port();
    let reqs: Arc<Mutex<Vec<Req>>> = Arc::default();
    let h = Arc::new(h);
    let log = reqs.clone();
    std::thread::spawn(move || {
        for conn in l.incoming() {
            let Ok(conn) = conn else { continue };
            let (h, log) = (h.clone(), log.clone());
            std::thread::spawn(move || {
                if let Some(r) = ler(&conn) {
                    log.lock().unwrap().push(r.clone());
                    escrever(&conn, h(&r));
                }
            });
        }
    });
    Servidor { porta, reqs }
}

// ── SSE no formato da API de chat ───────────────────────────────────────────
pub fn sse(pedacos: &[serde_json::Value]) -> Vec<u8> {
    let mut s = String::new();
    for p in pedacos {
        match p.as_str() {
            Some(t) => s.push_str(&format!("data: {t}\n\n")),
            None => s.push_str(&format!("data: {p}\n\n")),
        }
    }
    s.into_bytes()
}

pub fn texto(t: &str, fim: Option<&str>) -> serde_json::Value {
    serde_json::json!({"choices": [{"delta": {"content": t}, "finish_reason": fim}]})
}
