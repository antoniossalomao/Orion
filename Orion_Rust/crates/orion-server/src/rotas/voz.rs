//! Rotas WebSocket de voz (`/ws/voz` e `/ws/voice`, fase 6).
//!
//! `REGRAS.md: voz ao vivo avisa antes e não tem ferramentas; aprovação nunca é por voz`.

use crate::estado::Estado;
use crate::http::{mesma_origem, quem_e};
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::Response;
use futures::future::BoxFuture;
use futures::{SinkExt, StreamExt};
use orion_ai::transcribe::MAX_AUDIO;
use orion_core::voice::{turno_de_voz, CanalVoz};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

const AUTH_WS_S: u64 = 5;

fn fechar(codigo: u16) -> Message {
    Message::Close(Some(CloseFrame { code: codigo, reason: "".into() }))
}

struct WsCanal {
    tx: Arc<Mutex<futures::stream::SplitSink<WebSocket, Message>>>,
}

impl CanalVoz for WsCanal {
    fn send_json<'a>(&'a self, v: Value) -> BoxFuture<'a, bool> {
        let tx = self.tx.clone();
        Box::pin(async move { tx.lock().await.send(Message::Text(v.to_string())).await.is_ok() })
    }

    fn send_bytes<'a>(&'a self, b: Vec<u8>) -> BoxFuture<'a, bool> {
        let tx = self.tx.clone();
        Box::pin(async move { tx.lock().await.send(Message::Binary(b)).await.is_ok() })
    }
}

fn token_vale(st: &Estado, enviado: &str) -> bool {
    let esperado = &st.settings.admin_token;
    !esperado.is_empty() && subtle::ConstantTimeEq::ct_eq(enviado.as_bytes(), esperado.as_bytes()).into()
}

async fn autenticar(
    st: &Estado,
    ja_autenticado: Option<bool>,
    tx: &Arc<Mutex<futures::stream::SplitSink<WebSocket, Message>>>,
    rx: &mut futures::stream::SplitStream<WebSocket>,
) -> bool {
    let negar = |msg: &'static str, codigo: u16| {
        let tx = tx.clone();
        async move {
            let _ = tx.lock().await.send(Message::Text(json!({"type": "error", "msg": msg}).to_string())).await;
            let _ = tx.lock().await.send(fechar(codigo)).await;
            false
        }
    };
    if st.settings.admin_token.is_empty() && !st.auth.has_password() {
        return negar("autenticação não configurada", 4503).await;
    }
    match ja_autenticado {
        Some(true) => return true,
        Some(false) => return negar("login necessário", 4401).await,
        None => {}
    }
    let msg = match tokio::time::timeout(Duration::from_secs(AUTH_WS_S), rx.next()).await {
        Ok(Some(Ok(Message::Text(t)))) => serde_json::from_str::<Value>(&t).ok(),
        _ => None,
    };
    if let Some(m) = msg {
        if m.get("cmd").and_then(Value::as_str) == Some("auth")
            && m.get("token").and_then(Value::as_str).is_some_and(|t| token_vale(st, t))
        {
            return true;
        }
    }
    negar("login necessário", 4401).await
}

async fn erro_e_fecha(tx: &Arc<Mutex<futures::stream::SplitSink<WebSocket, Message>>>, msg: &str) {
    let _ = tx.lock().await.send(Message::Text(json!({"type": "error", "msg": msg}).to_string())).await;
    let _ = tx.lock().await.send(fechar(1000)).await;
}

fn pre_auth(headers: &HeaderMap, st: &Estado) -> Option<bool> {
    if headers.get("authorization").is_some() {
        return Some(matches!(quem_e(headers, st), Some("token")));
    }
    if matches!(quem_e(headers, st), Some("sessao" | "token")) {
        return Some(true);
    }
    None
}

pub async fn ws_voz(State(st): State<Estado>, headers: HeaderMap, ws: WebSocketUpgrade) -> Response {
    let origem_ok = mesma_origem(&headers);
    let ja_auth = pre_auth(&headers, &st);
    ws.on_upgrade(move |socket| socket_voz(st, origem_ok, ja_auth, socket))
}

async fn socket_voz(st: Estado, origem_ok: bool, ja_auth: Option<bool>, socket: WebSocket) {
    let (sink, mut rx) = socket.split();
    let tx = Arc::new(Mutex::new(sink));
    if !origem_ok {
        let _ = tx.lock().await.send(fechar(1008)).await;
        return;
    }
    if !autenticar(&st, ja_auth, &tx, &mut rx).await {
        return;
    }
    if !st.settings.voice_enabled {
        erro_e_fecha(&tx, "voz desligada (ORION_VOICE_ENABLED)").await;
        return;
    }
    let Some(agent) = st.agent.clone() else {
        erro_e_fecha(&tx, "gateway de modelos não configurado").await;
        return;
    };
    let Some(transcriber) = st.transcriber.clone() else {
        erro_e_fecha(&tx, "sem chave de transcrição (ORION_TRANSCRIBE_API_KEY)").await;
        return;
    };
    let mut tarefa: Option<tokio::task::JoinHandle<()>> = None;
    let mut acumulando = false;
    let mut buffer: Vec<u8> = Vec::new();

    while let Some(Ok(msg)) = rx.next().await {
        match msg {
            Message::Close(_) => break,
            Message::Binary(dados) => {
                if acumulando {
                    if buffer.len() + dados.len() > MAX_AUDIO {
                        buffer.clear();
                        acumulando = false;
                        let _ = tx.lock().await.send(Message::Text(json!({"type": "error", "msg": "fala longa demais"}).to_string())).await;
                    } else {
                        buffer.extend_from_slice(&dados);
                    }
                } else if tarefa.as_ref().is_some_and(|t| !t.is_finished()) {
                    let _ = tx.lock().await.send(Message::Text(json!({"type": "error", "msg": "ainda estou no turno anterior"}).to_string())).await;
                } else if dados.len() > MAX_AUDIO {
                    let _ = tx.lock().await.send(Message::Text(json!({"type": "error", "msg": "fala longa demais"}).to_string())).await;
                } else {
                    let canal = WsCanal { tx: tx.clone() };
                    let (ag, tr, sp, md, vz, falar) = (
                        Some(agent.clone()),
                        Some(transcriber.clone()),
                        st.speaker.clone(),
                        Some(st.modos.clone()),
                        st.voz.clone(),
                        st.settings.voice_speak,
                    );
                    tarefa = Some(tokio::spawn(async move {
                        turno_de_voz(&canal, dados, ag, tr, sp, md, vz, falar, None).await;
                    }));
                }
            }
            Message::Text(texto) => {
                let Ok(v) = serde_json::from_str::<Value>(&texto) else { continue };
                let cmd = v.get("cmd").or_else(|| v.get("type")).and_then(Value::as_str).unwrap_or("");
                match cmd {
                    "ping" => {
                        let _ = tx.lock().await.send(Message::Text(json!({"type": "pong"}).to_string())).await;
                    }
                    "start" => {
                        buffer.clear();
                        acumulando = true;
                    }
                    "stop" => {
                        acumulando = false;
                        let audio = std::mem::take(&mut buffer);
                        if !audio.is_empty() {
                            let canal = WsCanal { tx: tx.clone() };
                            let (ag, tr, sp, md, vz, falar) = (
                                Some(agent.clone()),
                                Some(transcriber.clone()),
                                st.speaker.clone(),
                                Some(st.modos.clone()),
                                st.voz.clone(),
                                st.settings.voice_speak,
                            );
                            tarefa = Some(tokio::spawn(async move {
                                turno_de_voz(&canal, audio, ag, tr, sp, md, vz, falar, None).await;
                            }));
                        }
                    }
                    "cancel" | "interrupt" => {
                        if let Some(t) = tarefa.take() {
                            if !t.is_finished() {
                                t.abort();
                                let _ = tx.lock().await.send(Message::Text(json!({"type": "done"}).to_string())).await;
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    if let Some(t) = tarefa {
        t.abort();
    }
}

pub async fn ws_voice(State(st): State<Estado>, headers: HeaderMap, ws: WebSocketUpgrade) -> Response {
    let origem_ok = mesma_origem(&headers);
    let ja_auth = pre_auth(&headers, &st);
    ws.on_upgrade(move |socket| socket_voice_live(st, origem_ok, ja_auth, socket))
}

async fn socket_voice_live(st: Estado, origem_ok: bool, ja_auth: Option<bool>, socket: WebSocket) {
    let (sink, mut rx) = socket.split();
    let tx = Arc::new(Mutex::new(sink));
    if !origem_ok {
        let _ = tx.lock().await.send(fechar(1008)).await;
        return;
    }
    if !autenticar(&st, ja_auth, &tx, &mut rx).await {
        return;
    }
    if !st.settings.voice_live_enabled {
        erro_e_fecha(&tx, "voz ao vivo desligada (ORION_VOICE_LIVE_ENABLED)").await;
        return;
    }
    let (Some(transcriber),Some(gateway))=(&st.transcriber,&st.gateway)else{erro_e_fecha(&tx,"configure a transcrição e um provedor nas Configurações").await;return;};
    if st.modos.panico(){erro_e_fecha(&tx,"modo pânico").await;return;}
    let _=st.ops.audit_add(&json!({"tool":"voice_live","action":"start"}));
    let deadline=tokio::time::Instant::now()+Duration::from_secs(st.settings.voice_live_max_min as u64*60);
    let mut history=vec![json!({"role":"system","content":"Você é Orion. Esta conversa por voz é temporária, sem ferramentas, memória ou acesso ao computador. Não alegue executar ações."})];
    let _=tx.lock().await.send(Message::Text(json!({"type":"ready","tools":false,"memory":false}).to_string())).await;
    while tokio::time::Instant::now()<deadline && !st.modos.panico(){
        let frame=match tokio::time::timeout(Duration::from_secs(1),rx.next()).await{Err(_)=>continue,Ok(Some(Ok(frame)))=>frame,_=>break};
        let audio=match frame{Message::Binary(audio) if !audio.is_empty() && audio.len()<=MAX_AUDIO=>audio,Message::Close(_)=>break,Message::Text(t) if t.contains("stop")=>break,Message::Ping(p)=>{let _=tx.lock().await.send(Message::Pong(p)).await;continue;},_=>continue};
        let Some((name,mime))=orion_core::voice::tipo_do_audio(&audio[..audio.len().min(16)])else{continue;};
        let response=async{
            let text=transcriber.transcribe(audio,name,mime).await.map_err(|e|e.to_string())?;
            tx.lock().await.send(Message::Text(json!({"type":"transcript","text":text}).to_string())).await.map_err(|e|e.to_string())?;
            history.push(json!({"role":"user","content":text}));if history.len()>13{history.drain(1..3);}
            let text=gateway.complete(history.clone()).await.map_err(|e|e.to_string())?;
            history.push(json!({"role":"assistant","content":text}));
            tx.lock().await.send(Message::Text(json!({"type":"text","text":text}).to_string())).await.map_err(|e|e.to_string())?;
            if let Some(speaker)=&st.speaker{let audio=speaker.synthesize(&text).await.map_err(|e|e.to_string())?;tx.lock().await.send(Message::Binary(audio)).await.map_err(|e|e.to_string())?;}
            Ok::<(),String>(())
        };
        let cancelled=async{loop{tokio::time::sleep(Duration::from_millis(100)).await;if st.modos.panico()||tokio::time::Instant::now()>=deadline{break;}}};
        let result=tokio::select!{r=response=>r,_=cancelled=>break};
        if let Err(e)=result{erro_e_fecha(&tx,&e).await;break;}
    }
    let _=st.ops.audit_add(&json!({"tool":"voice_live","action":"stop"}));
    let _=tx.lock().await.send(fechar(1000)).await;
}
