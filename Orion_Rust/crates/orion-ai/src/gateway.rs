//! Cliente dos provedores de modelo (API compatível com a da OpenAI) com streaming.
//!
//! Recebe uma lista de endpoints (um por modelo) e, se um cair ou recusar (429, 5xx, timeout),
//! tenta o próximo:
//! - 429 põe aquele modelo em quarentena pelo `Retry-After` (ou `cooldown_s`);
//! - `FALHAS_PARA_PAUSA` falhas seguidas o pausam, com espera que dobra até `PAUSA_MAX_S`;
//! - `orcamento(ep)` falso (já gastou o `limite_dia` do provedor hoje) o manda para o fim da
//!   fila: o chat nunca é bloqueado (REGRAS.md: custo zero (sem API paga)).
//!
//! Falha no meio do texto não troca de endpoint e vira `GatewayError`. Cada tentativa vira uma
//! linha no registro de saída. O modelo local entra como camada `local`: sempre o último e nunca
//! recebe `tools` (REGRAS.md: modelo local é reserva sem ferramentas).

use crate::saidas;
use futures::{Stream, StreamExt};
use parking_lot::Mutex;
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap};
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const CAMADA_LOCAL: &str = "local";
pub const AVISO_RESERVA: &str = "Modo reserva: sem ferramentas. Responda só com texto; não prometa executar ações.";
/// Falhas seguidas (sem ser 429) que pausam o modelo.
pub const FALHAS_PARA_PAUSA: u32 = 3;
/// A pausa dobra a cada nova falha, até 15 min.
pub const PAUSA_MAX_S: f64 = 900.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Endpoint {
    pub name: String,
    /// ex.: `https://api.groq.com/openai/v1`
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    pub timeout_s: f64,
    /// Camada de roteamento: "rapido", "pesado", "visao", "local" ou vazio (padrão).
    pub tier: String,
    /// `false`: a requisição sai sem `tools` (modelo local).
    pub tools: bool,
    /// id em `provedores`; vazio = endpoint avulso (`ORION_GATEWAY_URL`).
    pub provedor: String,
}

impl Endpoint {
    pub fn new(name: &str, base_url: &str, model: &str) -> Self {
        Endpoint {
            name: name.into(),
            base_url: base_url.into(),
            model: model.into(),
            api_key: None,
            timeout_s: 60.0,
            tier: String::new(),
            tools: true,
            provedor: String::new(),
        }
    }

    /// Nome no registro de saída: `gateway:<provedor>`, `gateway:<camada>` ou `ollama`.
    pub fn provider(&self) -> String {
        if self.tier == CAMADA_LOCAL {
            return if self.base_url == "embedded://llama.cpp" { "local" } else { "ollama" }.into();
        }
        let n = if !self.provedor.is_empty() {
            &self.provedor
        } else if !self.tier.is_empty() {
            &self.tier
        } else {
            "padrão"
        };
        format!("gateway:{n}")
    }
}

/// Contadores de um endpoint desde que o Orion subiu (o painel mostra; reiniciar zera).
#[derive(Debug, Clone, Default, Serialize)]
pub struct EndpointStats {
    pub chamadas: u64,
    pub ok: u64,
    pub falhas: u64,
    pub limitada: u64,
    pub pulos: u64,
    pub ultimo_ok: Option<f64>,
    /// Só o tipo ("HTTP 502", "ConnectError"), nunca corpo nem URL.
    pub ultimo_erro: Option<String>,
    pub seguidas: u32,
    pub pausas: u64,
    pub orcamento: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolCallRequest {
    pub id: String,
    pub name: String,
    pub arguments: Map<String, Value>,
    /// Argumentos que não são JSON válido.
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    TextDelta(String),
    ToolCall(ToolCallRequest),
    Finish { reason: String, endpoint: String, model: String },
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("{mensagem}")]
pub struct GatewayError {
    pub mensagem: String,
    pub tentativas: Vec<(String, String)>,
}

impl GatewayError {
    fn new(m: impl Into<String>) -> Self {
        GatewayError { mensagem: m.into(), tentativas: vec![] }
    }
}

pub type EventStream<'a> = Pin<Box<dyn Stream<Item = Result<Event, GatewayError>> + Send + 'a>>;
pub type LocalGenerator = Arc<dyn Fn(Vec<Value>) -> EventStream<'static> + Send + Sync>;

/// Orçamento diário por provedor: `false` = já gastou o de hoje.
pub type Orcamento = Arc<dyn Fn(&Endpoint) -> bool + Send + Sync>;

#[derive(Default)]
struct Estado {
    quarentena: HashMap<String, Instant>,
    motivo: HashMap<String, &'static str>,
    stats: HashMap<String, EndpointStats>,
}

pub struct ChatGateway {
    pub endpoints: Vec<Endpoint>,
    client: reqwest::Client,
    cooldown_s: f64,
    estado: Mutex<Estado>,
    pub orcamento: Mutex<Option<Orcamento>>,
    pub weekly_budget: Mutex<Option<Arc<crate::budget::Budget>>>,
    pub embedded_local: Mutex<Option<LocalGenerator>>,
}

fn agora_epoch() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

fn tipo_de_conteudo(messages: &[Value]) -> &'static str {
    for m in messages {
        if let Some(partes) = m.get("content").and_then(Value::as_array) {
            if partes.iter().any(|p| p.get("type").and_then(Value::as_str) == Some("image_url")) {
                return "imagem";
            }
        }
    }
    "texto"
}

enum Falha {
    /// Antes de qualquer saída: pode tentar o próximo.
    Antes(String),
    /// Erro de transporte; `comecou` diz se já saiu texto.
    Transporte(String),
}

impl ChatGateway {
    pub fn new(endpoints: Vec<Endpoint>) -> anyhow::Result<Self> {
        Self::with_client(endpoints, reqwest::Client::builder().build()?, 60.0)
    }

    pub fn with_client(endpoints: Vec<Endpoint>, client: reqwest::Client, cooldown_s: f64) -> anyhow::Result<Self> {
        anyhow::ensure!(!endpoints.is_empty(), "ao menos um endpoint");
        let mut estado = Estado::default();
        for ep in &endpoints {
            estado.stats.insert(ep.name.clone(), EndpointStats::default());
        }
        Ok(ChatGateway { endpoints, client, cooldown_s, estado: Mutex::new(estado), weekly_budget: Mutex::new(None), orcamento: Mutex::new(None), embedded_local: Mutex::new(None) })
    }

    /// Um objeto por endpoint, na ordem de tentativa (para o painel). Nunca mostra segredo
    /// (REGRAS.md: painel só lê e nunca mostra segredo).
    pub fn stats(&self) -> Vec<Value> {
        let st = self.estado.lock();
        let agora = Instant::now();
        self.endpoints
            .iter()
            .map(|ep| {
                let s = st.stats.get(&ep.name).cloned().unwrap_or_default();
                let restante = st.quarentena.get(&ep.name).map(|t| t.saturating_duration_since(agora).as_secs_f64()).unwrap_or(0.0);
                json!({
                    "nome": ep.name,
                    "modelo": ep.model,
                    "camada": if ep.tier.is_empty() { "padrão" } else { &ep.tier },
                    "chamadas": s.chamadas,
                    "ok": s.ok,
                    "falhas": s.falhas,
                    "limitada": s.limitada,
                    "pulos": s.pulos,
                    "quarentena_s": restante.round() as i64,
                    "motivo": if restante > 0.0 { st.motivo.get(&ep.name).copied().unwrap_or("") } else { "" },
                    "provedor": ep.provedor,
                    "ultimo_ok": s.ultimo_ok,
                    "ultimo_erro": s.ultimo_erro,
                    "pausas": s.pausas,
                    "orcamento": s.orcamento,
                })
            })
            .collect()
    }

    /// Quem tentar, em ordem: os da camada pedida, depois os padrão (reserva). Sem camada, só os
    /// padrão; se não houver padrão, todos. O local sempre por último.
    fn ordem(&self, tier: Option<&str>) -> Vec<Endpoint> {
        let locais: Vec<&Endpoint> = self.endpoints.iter().filter(|e| e.tier == CAMADA_LOCAL).collect();
        let remotos: Vec<&Endpoint> = self.endpoints.iter().filter(|e| e.tier != CAMADA_LOCAL).collect();
        let padrao: Vec<&Endpoint> = remotos.iter().copied().filter(|e| e.tier.is_empty()).collect();
        let proprios: Vec<&Endpoint> = remotos.iter().copied().filter(|e| tier.is_some_and(|t| !t.is_empty() && e.tier == t)).collect();
        let mut base: Vec<&Endpoint> = proprios.into_iter().chain(padrao).collect();
        if base.is_empty() {
            base = remotos;
        }
        base.into_iter().chain(locais).cloned().collect()
    }

    fn falhou(&self, ep: &Endpoint, tipo: &str, limitada: bool) {
        let mut st = self.estado.lock();
        let cooldown = self.cooldown_s;
        let s = st.stats.entry(ep.name.clone()).or_default();
        s.falhas += 1;
        s.ultimo_erro = Some(tipo.chars().take(40).collect());
        if limitada {
            return;
        }
        s.seguidas += 1;
        if s.seguidas >= FALHAS_PARA_PAUSA {
            let espera = (cooldown * 2f64.powi((s.seguidas - FALHAS_PARA_PAUSA) as i32)).min(PAUSA_MAX_S);
            s.pausas += 1;
            let seguidas = s.seguidas;
            st.quarentena.insert(ep.name.clone(), Instant::now() + Duration::from_secs_f64(espera));
            st.motivo.insert(ep.name.clone(), "falhas");
            tracing::warn!("modelo {} em pausa por {espera:.0} s ({seguidas} falhas seguidas)", ep.name);
        }
    }

    /// Resposta em streaming. Os eventos chegam na ordem: textos, chamadas de ferramenta e o fim.
    pub fn stream<'a>(&'a self, messages: Vec<Value>, tools: Option<Vec<Value>>, tier: Option<&'a str>) -> EventStream<'a> {
        self.stream_for_task(messages, tools, tier, orion_store::uuid_hex())
    }

    pub fn stream_for_task<'a>(&'a self, messages: Vec<Value>, tools: Option<Vec<Value>>, tier: Option<&'a str>, task_id: String) -> EventStream<'a> {
        Box::pin(async_stream::stream! {
            let mut tentativas: Vec<(String, String)> = vec![];
            let mut ordem = vec![];
            let mut adiados = vec![];
            let orc = self.orcamento.lock().clone();
            for ep in self.ordem(tier) {
                if let Some(o) = &orc {
                    if !ep.provedor.is_empty() && !o(&ep) {
                        self.estado.lock().stats.entry(ep.name.clone()).or_default().orcamento += 1;
                        adiados.push(ep);
                        continue;
                    }
                }
                ordem.push(ep);
            }
            let conteudo = tipo_de_conteudo(&messages);
            for ep in ordem.into_iter().chain(adiados) {
                {
                    let mut st = self.estado.lock();
                    if st.quarentena.get(&ep.name).is_some_and(|t| *t > Instant::now()) {
                        let motivo = if st.motivo.get(&ep.name) == Some(&"falhas") {
                            "pausa (falhas seguidas)"
                        } else {
                            "quarentena (cota)"
                        };
                        tentativas.push((ep.name.clone(), format!("em {motivo}")));
                        st.stats.entry(ep.name.clone()).or_default().pulos += 1;
                        continue;
                    }
                    st.stats.entry(ep.name.clone()).or_default().chamadas += 1;
                }
                let budget = self.weekly_budget.lock().clone();
                if let Some(budget) = budget.filter(|_| ep.tier != CAMADA_LOCAL) {
                    if let Err(e) = budget.reserve(&ep.provider(), &task_id) {
                        tentativas.push((ep.name.clone(), e.to_string()));
                        continue;
                    }
                }
                let inicio = Instant::now();
                let mut bytes_out = 0usize;
                let mut bytes_in = 0usize;
                let mut comecou = false;
                let mut falha: Option<Falha> = None;
                {
                    let mut s = self.run(&ep, &messages, tools.as_deref(), &mut bytes_out, &mut bytes_in);
                    while let Some(r) = s.next().await {
                        match r {
                            Ok(ev) => {
                                comecou = true;
                                yield Ok(ev);
                            }
                            Err(f) => {
                                falha = Some(f);
                                break;
                            }
                        }
                    }
                }
                let ok = falha.is_none() && comecou;
                saidas::registrar(
                    &ep.provider(), "chat", ok, inicio.elapsed().as_secs_f64() * 1000.0,
                    &ep.model, bytes_out, bytes_in, conteudo,
                );
                match falha {
                    None => {
                        let mut st = self.estado.lock();
                        let s = st.stats.entry(ep.name.clone()).or_default();
                        s.ok += 1;
                        s.seguidas = 0;
                        s.ultimo_ok = Some(agora_epoch());
                        return;
                    }
                    Some(Falha::Antes(e)) => {
                        let tipo = e.split(':').next().unwrap_or("").to_string();
                        self.falhou(&ep, &tipo, e.starts_with("HTTP 429"));
                        tracing::warn!("endpoint {} falhou, tentando o próximo: {}", ep.name, tipo);
                        tentativas.push((ep.name.clone(), e));
                    }
                    Some(Falha::Transporte(e)) => {
                        let tipo = e.split(':').next().unwrap_or("").to_string();
                        self.falhou(&ep, &tipo, false);
                        if comecou {
                            yield Err(GatewayError::new(format!("{} interrompeu no meio da resposta: {e}", ep.name)));
                            return;
                        }
                        tracing::warn!("endpoint {} falhou, tentando o próximo: {}", ep.name, tipo);
                        tentativas.push((ep.name.clone(), e));
                    }
                }
            }
            yield Err(GatewayError { mensagem: "nenhum endpoint respondeu".into(), tentativas });
        })
    }

    /// Resposta inteira em texto, sem ferramentas (jobs como a consolidação da memória).
    pub async fn complete(&self, messages: Vec<Value>) -> Result<String, GatewayError> {
        self.complete_tier(messages, None).await
    }

    pub async fn complete_tier(&self, messages: Vec<Value>, tier: Option<&str>) -> Result<String, GatewayError> {
        let mut texto = String::new();
        let mut s = self.stream(messages, None, tier);
        while let Some(ev) = s.next().await {
            if let Event::TextDelta(t) = ev? {
                texto.push_str(&t);
            }
        }
        Ok(texto)
    }

    fn run<'a>(
        &'a self,
        ep: &'a Endpoint,
        messages: &'a [Value],
        tools: Option<&'a [Value]>,
        bytes_out: &'a mut usize,
        bytes_in: &'a mut usize,
    ) -> Pin<Box<dyn Stream<Item = Result<Event, Falha>> + Send + 'a>> {
        Box::pin(async_stream::stream! {
            let mut msgs: Vec<Value> = vec![];
            if !ep.tools {
                msgs.push(json!({"role": "system", "content": AVISO_RESERVA}));
            }
            msgs.extend(messages.iter().cloned());
            if ep.tier == CAMADA_LOCAL && ep.base_url == "embedded://llama.cpp" {
                let generator = self.embedded_local.lock().clone();
                let Some(generator) = generator else {
                    yield Err(Falha::Antes("backend embarcado não disponível nesta compilação".into()));
                    return;
                };
                // REGRAS.md: modelo local é reserva sem ferramentas. O callback só recebe mensagens.
                let mut local = generator(msgs);
                while let Some(result) = local.next().await {
                    match result {
                        Ok(Event::ToolCall(_)) => {
                            yield Err(Falha::Antes("backend local tentou usar ferramentas".into()));
                            return;
                        }
                        Ok(event) => yield Ok(event),
                        Err(error) => { yield Err(Falha::Transporte(error.mensagem)); return; }
                    }
                }
                return;
            }
            let mut corpo = json!({"model": ep.model, "messages": msgs, "stream": true});
            if let Some(t) = tools.filter(|t| !t.is_empty() && ep.tools) {
                corpo["tools"] = Value::Array(t.to_vec());
            }
            let corpo_txt = corpo.to_string();
            *bytes_out = corpo_txt.len();
            let url = format!("{}/chat/completions", ep.base_url.trim_end_matches('/'));
            let mut req = self
                .client
                .post(&url)
                .header("content-type", "application/json")
                .timeout(Duration::from_secs_f64(ep.timeout_s))
                .body(corpo_txt);
            if let Some(k) = &ep.api_key {
                req = req.bearer_auth(k);
            }
            let resp = match req.send().await {
                Ok(r) => r,
                Err(e) => {
                    yield Err(Falha::Transporte(format!("{}: {}", tipo_erro(&e), e.without_url())));
                    return;
                }
            };
            let status = resp.status().as_u16();
            if status >= 400 {
                let retry = resp.headers().get("retry-after").and_then(|v| v.to_str().ok()).map(String::from);
                let corpo = resp.text().await.unwrap_or_default();
                *bytes_in = corpo.len();
                if status == 429 {
                    let espera = retry_after(retry.as_deref(), self.cooldown_s);
                    let mut st = self.estado.lock();
                    st.quarentena.insert(ep.name.clone(), Instant::now() + Duration::from_secs_f64(espera));
                    st.motivo.insert(ep.name.clone(), "cota");
                    st.stats.entry(ep.name.clone()).or_default().limitada += 1;
                }
                let curto: String = corpo.chars().take(200).collect();
                yield Err(Falha::Antes(format!("HTTP {status}: {curto}")));
                return;
            }
            let mut chamadas: BTreeMap<i64, (String, String, String)> = BTreeMap::new();
            let mut razao = "stop".to_string();
            let mut houve_saida = false;
            let mut bytes = resp.bytes_stream();
            let mut buffer: Vec<u8> = vec![];
            let mut fim = false;
            while !fim {
                let pedaco = match bytes.next().await {
                    None => break,
                    Some(Ok(b)) => b,
                    Some(Err(e)) => {
                        yield Err(Falha::Transporte(format!("{}: {}", tipo_erro(&e), e.without_url())));
                        return;
                    }
                };
                buffer.extend_from_slice(&pedaco);
                while let Some(pos) = buffer.iter().position(|b| *b == b'\n') {
                    let linha_bytes: Vec<u8> = buffer.drain(..=pos).collect();
                    *bytes_in += linha_bytes.len();
                    let linha = String::from_utf8_lossy(&linha_bytes);
                    let linha = linha.trim_end_matches(['\n', '\r']);
                    let Some(dado) = linha.strip_prefix("data:") else { continue };
                    let dado = dado.trim();
                    if dado == "[DONE]" {
                        fim = true;
                        break;
                    }
                    if dado.is_empty() {
                        continue;
                    }
                    let Ok(p) = serde_json::from_str::<Value>(dado) else { continue };
                    for escolha in p.get("choices").and_then(Value::as_array).cloned().unwrap_or_default() {
                        let delta = escolha.get("delta").cloned().unwrap_or(Value::Null);
                        if let Some(t) = delta.get("content").and_then(Value::as_str).filter(|t| !t.is_empty()) {
                            houve_saida = true;
                            yield Ok(Event::TextDelta(t.to_string()));
                        }
                        for tc in delta.get("tool_calls").and_then(Value::as_array).cloned().unwrap_or_default() {
                            let n = chamadas.len() as i64;
                            let idx = tc.get("index").and_then(Value::as_i64).unwrap_or(n);
                            let slot = chamadas.entry(idx).or_default();
                            if let Some(id) = tc.get("id").and_then(Value::as_str).filter(|s| !s.is_empty()) {
                                slot.0 = id.to_string();
                            }
                            let f = tc.get("function").cloned().unwrap_or(Value::Null);
                            slot.1.push_str(f.get("name").and_then(Value::as_str).unwrap_or(""));
                            slot.2.push_str(f.get("arguments").and_then(Value::as_str).unwrap_or(""));
                        }
                        if let Some(r) = escolha.get("finish_reason").and_then(Value::as_str).filter(|r| !r.is_empty()) {
                            razao = r.to_string();
                        }
                    }
                }
            }
            if !houve_saida && chamadas.is_empty() {
                yield Err(Falha::Antes("resposta vazia".into()));
                return;
            }
            for (i, (id, nome, args)) in chamadas {
                yield Ok(Event::ToolCall(tool_call(i, id, nome, &args)));
            }
            yield Ok(Event::Finish { reason: razao, endpoint: ep.name.clone(), model: ep.model.clone() });
        })
    }
}

fn tipo_erro(e: &reqwest::Error) -> &'static str {
    if e.is_timeout() {
        "ReadTimeout"
    } else if e.is_connect() {
        "ConnectError"
    } else if e.is_decode() || e.is_body() {
        "RemoteProtocolError"
    } else {
        "HTTPError"
    }
}

fn tool_call(indice: i64, id: String, nome: String, args: &str) -> ToolCallRequest {
    let ident = if id.is_empty() { format!("call_{indice}") } else { id };
    let texto = if args.is_empty() { "{}" } else { args };
    match serde_json::from_str::<Value>(texto) {
        Ok(Value::Object(m)) => ToolCallRequest { id: ident, name: nome, arguments: m, error: None },
        Ok(_) => {
            ToolCallRequest { id: ident, name: nome, arguments: Map::new(), error: Some("argumentos inválidos: argumentos não são um objeto JSON".into()) }
        }
        Err(e) => ToolCallRequest { id: ident, name: nome, arguments: Map::new(), error: Some(format!("argumentos inválidos: {e}")) },
    }
}

fn retry_after(valor: Option<&str>, padrao: f64) -> f64 {
    match valor.and_then(|v| v.trim().parse::<f64>().ok()) {
        Some(v) => v.max(1.0),
        None => padrao,
    }
}

#[cfg(test)]
mod embedded_tests {
    use super::*;

    #[tokio::test]
    async fn reserva_embarcada_nao_recebe_ferramentas_nem_abre_http() {
        let mut ep = Endpoint::new("local", "embedded://llama.cpp", "modelo.gguf");
        ep.tier = CAMADA_LOCAL.into();
        ep.tools = false;
        let gateway = ChatGateway::new(vec![ep]).unwrap();
        *gateway.embedded_local.lock() = Some(Arc::new(|messages| {
            assert_eq!(messages[0]["content"], AVISO_RESERVA);
            assert!(messages.iter().all(|m| m.get("tools").is_none()));
            Box::pin(futures::stream::iter([
                Ok(Event::TextDelta("local".into())),
                Ok(Event::Finish { reason: "stop".into(), endpoint: "local".into(), model: "modelo.gguf".into() }),
            ]))
        }));
        let events: Vec<_> = gateway.stream(vec![json!({"role":"user","content":"olá"})], Some(vec![json!({"type":"function"})]), None).collect().await;
        assert_eq!(events[0].as_ref().unwrap(), &Event::TextDelta("local".into()));
        assert!(events.iter().all(Result::is_ok));
    }

    #[tokio::test]
    async fn chamada_de_ferramenta_do_backend_local_e_recusada() {
        let mut ep = Endpoint::new("local", "embedded://llama.cpp", "modelo.gguf");
        ep.tier = CAMADA_LOCAL.into();
        ep.tools = false;
        let gateway = ChatGateway::new(vec![ep]).unwrap();
        *gateway.embedded_local.lock() = Some(Arc::new(|_| {
            Box::pin(futures::stream::iter([Ok(Event::ToolCall(ToolCallRequest { id: "x".into(), name: "executar".into(), arguments: Map::new(), error: None }))]))
        }));
        assert!(gateway.complete(vec![json!({"role":"user","content":"teste"})]).await.is_err());
    }
}
