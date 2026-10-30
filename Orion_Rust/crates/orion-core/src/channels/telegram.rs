//! Canal Telegram (fase 5 do NUCLEO): o Orion no celular, sem expor porta.
//!
//! Long polling (`getUpdates`): o notebook só faz chamadas de saída, não precisa de IP público nem
//! de Tailscale para este canal. O que o canal garante:
//!
//! - default-deny: só responde a usuários da lista, em conversa privada; qualquer outro é ignorado
//!   em silêncio (só vai para o log), para o bot não revelar que existe;
//! - aprovação fora de banda (REGRAS.md: confirmação fora de banda (canal autenticado)): ação que a
//!   política manda confirmar vira uma mensagem com botões ✅/❌; só o clique de um usuário
//!   permitido decide, nunca uma frase;
//! - avisos: lembretes e agendamentos da fila saem aqui e só são confirmados (`ack`) depois de
//!   enviados;
//! - segredo: o token vai na URL da API do Telegram, então nenhuma mensagem de erro ou log leva a
//!   URL (só o tipo do erro).
//!
//! Voz e foto: a mensagem de voz é transcrita e o texto entendido é mostrado antes da resposta, para
//! quem falou conferir; a foto vai ao modelo como imagem SÓ naquele turno. Aprovação continua só por
//! botão. Texto puro (sem `parse_mode`): resposta de modelo com `*` ou `_` não quebra a mensagem.

use crate::agent::{Agent, EventStream, TurnRequest};
use crate::capture::{Capturer, EXTENSOES_FOTO};
use crate::ler_depois::LerDepois;
use crate::modos::Modos;
use crate::rotinas::{Executor, RotinaStore};
use crate::vida::VidaCanal;
use base64::Engine as _;
use futures::StreamExt;
use orion_ai::transcribe::{Transcriber, MAX_AUDIO};
use orion_policy::ApprovalStore;
use orion_store::ops::Operations;
use orion_store::Store;
use parking_lot::Mutex;
use regex::Regex;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

pub const CANAL: &str = "telegram";
pub const API: &str = "https://api.telegram.org";
/// O Telegram aceita 4096; folga para o que o cliente conta diferente.
pub const LIMITE_MENSAGEM: usize = 4000;
/// O mesmo teto do POST /chat.
pub const LIMITE_ENTRADA: usize = 8000;
pub const MAX_VOZ_S: i64 = 300;
pub const MAX_FOTO: usize = 4 * 1024 * 1024;
pub const MAX_ARGS_REVISAVEL: usize = 3000;
/// Depois do `/capturar` sozinho, o próximo envio vale por 5 minutos.
pub const CAPTURA_ESPERA_S: u64 = 300;
const METODOS_DE_SAIDA: [&str; 3] = ["send", "edit", "answer"];
static CAMINHO_ARQUIVO: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_./-]{1,200}$").unwrap());
static CALLBACK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^ap:([A-Za-z0-9_-]{6,32}):([yn])$").unwrap());
static LEMBRETE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^lem:(\d{1,9}):([fa])$").unwrap());
static LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^https?://\S+$").unwrap());
static REF_LEMBRETE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^reminder:(\d{1,9})(?::n\d+)?$").unwrap());

pub const BOAS_VINDAS: &str = "Orion aqui. Escreva o que precisa e eu respondo.\n\n/nova — começa uma conversa nova (a atual fica arquivada)\n/ler <link> — guarda para ler depois (resumo de 5 linhas no vault)\n/capturar — guarda no vault o próximo texto, link, foto ou voz (ou /capturar <texto>)\n/briefing — o que há para hoje\n/painel — modelos, CLIs, aprovações e política num só lugar\n/tela — memória da tela: estado, pausar, retomar ou limpar\n/camera [pergunta] — uma foto da webcam, descrita (pede aprovação no app)\n/panico — corta rede, execução, tela e escuta; /panico sair para voltar\n/ajuda — mostra isto de novo\n\nAções que mexem no computador chegam aqui com botões para aprovar ou negar.";

/// Erro da API do Telegram. Nunca carrega a URL (ela contém o token).
#[derive(Debug, Clone, thiserror::Error)]
#[error("{mensagem}")]
pub struct TelegramError {
    pub mensagem: String,
    pub codigo: Option<u16>,
}

fn terr(m: impl Into<String>, codigo: Option<u16>) -> TelegramError {
    TelegramError { mensagem: m.into(), codigo }
}

type R<T> = Result<T, TelegramError>;
type Texto = Arc<dyn Fn() -> String + Send + Sync>;
type TextoComArg = Arc<dyn Fn(&str) -> String + Send + Sync>;

pub struct TelegramChannel {
    token: String,
    allowed: HashSet<i64>,
    home: i64,
    pub agent: Arc<Agent>,
    pub memory: Arc<Store>,
    pub approvals: Arc<ApprovalStore>,
    pub ops: Arc<Operations>,
    client: reqwest::Client,
    base: String,
    transcriber: Option<Arc<Transcriber>>,
    capture: Option<Arc<Capturer>>,
    poll_timeout: u64,
    notify_every: Duration,
    /// Texto do /painel (o app liga depois).
    pub painel: Mutex<Option<Texto>>,
    /// Agenda do briefing (REGRAS.md: agenda do briefing é leitura fixa, sem modelo).
    pub agenda: Mutex<Option<Arc<crate::briefing::Agenda>>>,
    /// `/tela` (REGRAS.md: memória da tela: só texto, local e com opt-in).
    pub tela: Mutex<Option<TextoComArg>>,
    /// `/camera` (REGRAS.md: a câmera só liga por pedido seu, e cada uso fica registrado).
    pub camera: Mutex<Option<TextoComArg>>,
    /// `/panico` (REGRAS.md: pânico é corte; não perturbe só segura).
    pub modos: Mutex<Option<Arc<Modos>>>,
    /// `/desligar` (E9.5).
    pub desligar: Mutex<Option<Texto>>,
    pub ler_depois: Mutex<Option<Arc<LerDepois>>>,
    /// VidaCanal (E10): /financas, /contas, /foco, /revisao, gastos.
    pub vida: Mutex<Option<Arc<VidaCanal>>>,
    /// RotinaStore (REGRAS.md: automação sua vale pelo arquivo concedido).
    pub rotinas: Mutex<Option<Arc<RotinaStore>>>,
    pub executor_rotinas: Mutex<Option<Arc<Executor>>>,
    pub speaker: Mutex<Option<Arc<dyn crate::voice::Speaker>>>,
    remote_cards: Mutex<HashMap<String, (i64, i64)>>,
    approval_delivery: tokio::sync::Mutex<()>,
    /// Um turno do agente por vez: mensagem que chega com o PC ocupado espera na fila, em ordem (F14.04).
    turno: tokio::sync::Mutex<()>,
    /// F04.31: tipos de aviso que este canal entrega; vazio = todos. Os outros ficam só no app.
    pub eventos: Mutex<Vec<String>>,
    armados: Mutex<HashMap<i64, Instant>>,
    offset: Mutex<i64>,
    recusados: Mutex<HashSet<i64>>,
}

impl TelegramChannel {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        token: &str,
        allowed: &[i64],
        agent: Arc<Agent>,
        memory: Arc<Store>,
        approvals: Arc<ApprovalStore>,
        ops: Arc<Operations>,
        transcriber: Option<Arc<Transcriber>>,
        capture: Option<Arc<Capturer>>,
    ) -> Result<Self, String> {
        if token.is_empty() {
            return Err("token do bot vazio".into());
        }
        if allowed.is_empty() || allowed.iter().any(|id| *id <= 0) {
            return Err("use apenas IDs positivos de usuários em conversa privada (default-deny)".into());
        }
        Ok(TelegramChannel {
            token: token.into(),
            allowed: allowed.iter().copied().collect(),
            home: allowed[0], // onde caem os avisos
            agent,
            memory,
            approvals,
            ops,
            client: reqwest::Client::builder().build().map_err(|e| e.to_string())?,
            base: API.into(),
            transcriber,
            capture,
            poll_timeout: 25,
            notify_every: Duration::from_secs(15),
            painel: Mutex::new(None),
            agenda: Mutex::new(None),
            tela: Mutex::new(None),
            camera: Mutex::new(None),
            modos: Mutex::new(None),
            desligar: Mutex::new(None),
            ler_depois: Mutex::new(None),
            vida: Mutex::new(None),
            rotinas: Mutex::new(None),
            executor_rotinas: Mutex::new(None),
            speaker: Mutex::new(None),
            remote_cards: Mutex::new(HashMap::new()),
            approval_delivery: tokio::sync::Mutex::new(()),
            turno: tokio::sync::Mutex::new(()),
            eventos: Mutex::new(vec![]),
            armados: Mutex::new(HashMap::new()),
            offset: Mutex::new(0),
            recusados: Mutex::new(HashSet::new()),
        })
    }

    /// `/panico` liga; `/panico sair` desliga (comando explícito do usuário da lista, em conversa
    /// privada); `/panico estado` só mostra.
    fn panico(&self, argumento: &str) -> String {
        let Some(m) = self.modos.lock().clone() else { return "Modo pânico indisponível neste canal.".into() };
        match argumento.trim().to_lowercase().as_str() {
            "estado" => (if m.panico() { "Modo pânico LIGADO." } else { "Modo pânico desligado." }).into(),
            "sair" => match m.sair_panico("telegram") {
                Ok(true) => "Modo pânico desligado: tudo volta ao normal.".into(),
                Ok(false) => "Não estava ligado.".into(),
                Err(e) => e.to_string(),
            },
            "" => {
                let _ = m.entrar_panico("telegram");
                "🚨 Modo pânico LIGADO: ferramentas de rede e de execução cortadas, memória da tela, escuta e jobs de rede parados. Nada volta sozinho: /panico sair para desligar.".into()
            }
            _ => "Use /panico, /panico sair ou /panico estado.".into(),
        }
    }

    /// REGRAS.md: login com senha e sessão revogável. Código entregue só ao dono configurado.
    pub async fn send_login_code(&self, code: &str) -> Result<(), TelegramError> {
        self.enviar(self.home, &format!("Código de entrada do Orion: {code}\nExpira em 5 minutos. Se não foi você, ignore."), None).await
    }

    /// Aviso ao dono a cada senha errada no login (F14.11, ligado a F02.12). Só o IP do cliente;
    /// nunca a senha nem o usuário digitado.
    pub async fn send_login_failure(&self, cliente: &str) -> Result<(), TelegramError> {
        self.enviar(self.home, &format!("⚠️ Tentativa de entrada no Orion com senha errada (cliente {cliente}). Se não foi você, troque a senha."), None).await
    }

    async fn send_approval(&self, chat_id: i64, id: &str) -> R<()> {
        let _guard = self.approval_delivery.lock().await;
        if self.remote_cards.lock().contains_key(id) {
            return Ok(());
        }
        let Some(a) = self.approvals.get(id) else { return Ok(()) };
        let cartao = json!({"id": a.id, "tool": a.tool, "reason": a.reason, "args": orion_policy::redact(&Value::Object(a.args.clone()), 2000), "args_truncated": !revisavel(&a.args)});
        let (text, review) = cartao_de(&cartao);
        let result = self.api("sendMessage", json!({"chat_id": chat_id, "text": text, "reply_markup": {"inline_keyboard": teclado(id, review)}})).await?;
        if let Some(mid) = result["message_id"].as_i64() {
            self.remote_cards.lock().insert(id.into(), (chat_id, mid));
        }
        Ok(())
    }

    /// Aprovações do PC chegam ao dono e continuam vinculadas à sessão e argumentos originais.
    pub async fn deliver_approvals(&self) -> R<()> {
        let pending = self.approvals.pending(None, false);
        self.remote_cards.lock().retain(|id, _| pending.iter().any(|a| a.id == *id));
        for a in pending {
            if !self.remote_cards.lock().contains_key(&a.id) {
                self.send_approval(self.home, &a.id).await?;
            }
        }
        Ok(())
    }

    async fn send_audio_reply(&self, chat_id: i64, text: &str) -> R<()> {
        let speaker = self.speaker.lock().clone();
        let Some(speaker) = speaker else { return Ok(()) };
        if self.modos.lock().as_ref().is_some_and(|m| m.panico()) {
            return Ok(());
        }
        let bytes = speaker.synthesize(text).await.map_err(|_| terr("síntese indisponível", None))?;
        let mut measure = orion_ai::saidas::medir("telegram", "audio", "", bytes.len(), "audio");
        let part = reqwest::multipart::Part::bytes(bytes).file_name("orion.mp3").mime_str("audio/mpeg").map_err(|_| terr("tipo inválido", None))?;
        let response = self
            .client
            .post(format!("{}/bot{}/sendAudio", self.base, self.token))
            .multipart(reqwest::multipart::Form::new().text("chat_id", chat_id.to_string()).part("audio", part))
            .timeout(Duration::from_secs(60))
            .send()
            .await
            .map_err(|_| terr("envio de áudio falhou", None))?;
        measure.ok = response.status().is_success();
        if !measure.ok {
            return Err(terr("Telegram recusou o áudio", Some(response.status().as_u16())));
        }
        Ok(())
    }

    // ── API do Telegram ───────────────────────────────────────────────────
    async fn api(&self, metodo: &str, payload: Value) -> R<Value> {
        let url = format!("{}/bot{}/{metodo}", self.base, self.token);
        // o que leva texto seu para fora entra no registro de saída (REGRAS.md: toda saída de dado
        // fica registrada, sem conteúdo); a escuta e o download não levam nada daqui
        let mut medida =
            METODOS_DE_SAIDA.iter().any(|p| metodo.starts_with(p)).then(|| orion_ai::saidas::medir("telegram", "canal", "", payload.to_string().len(), ""));
        let resp = self
            .client
            .post(&url)
            .json(&payload)
            .timeout(Duration::from_secs(self.poll_timeout + 15))
            .send()
            .await
            .map_err(|e| terr(format!("{metodo}: {}", tipo_do_erro(&e)), None))?; // sem a URL
        let status = resp.status().as_u16();
        let bytes = resp.bytes().await.map_err(|e| terr(format!("{metodo}: {}", tipo_do_erro(&e)), None))?;
        if let Some(m) = medida.as_mut() {
            m.ok = status < 400;
            m.bytes_in = bytes.len();
        }
        let corpo: Value = serde_json::from_slice(&bytes).ok().filter(Value::is_object).unwrap_or(json!({}));
        if status >= 400 || corpo["ok"] != json!(true) {
            return Err(terr(format!("{metodo}: HTTP {status}"), Some(status)));
        }
        Ok(corpo["result"].clone())
    }

    /// Baixa um arquivo do Telegram (até `limite` bytes). O token vai na URL de download: nenhum erro
    /// carrega a URL.
    async fn baixar(&self, file_id: &str, limite: usize) -> R<(Vec<u8>, String)> {
        let info = self.api("getFile", json!({"file_id": file_id})).await?;
        let caminho = info["file_path"].as_str().unwrap_or("").to_string();
        if !CAMINHO_ARQUIVO.is_match(&caminho) || caminho.contains("..") {
            return Err(terr("getFile: caminho de arquivo inesperado", None));
        }
        if info["file_size"].as_u64().unwrap_or(0) as usize > limite {
            return Err(terr("arquivo grande demais", None));
        }
        let resp = self
            .client
            .get(format!("{}/file/bot{}/{caminho}", self.base, self.token))
            .timeout(Duration::from_secs(self.poll_timeout + 15))
            .send()
            .await
            .map_err(|e| terr(format!("download: {}", tipo_do_erro(&e)), None))?;
        if resp.status().as_u16() >= 400 {
            return Err(terr(format!("download: HTTP {}", resp.status().as_u16()), Some(resp.status().as_u16())));
        }
        if resp.content_length().is_some_and(|n| n > limite as u64) {
            return Err(terr("arquivo grande demais", None));
        }
        let mut stream = resp.bytes_stream();
        let mut data = Vec::new();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| terr("download interrompido", None))?;
            if data.len() + chunk.len() > limite {
                return Err(terr("arquivo grande demais", None));
            }
            data.extend_from_slice(&chunk);
        }
        Ok((data, caminho))
    }

    /// Envia em pedaços de até 4000 caracteres; o teclado vai no último.
    async fn enviar(&self, chat_id: i64, texto: &str, teclado: Option<Value>) -> R<()> {
        let mut pedacos = dividir(texto, LIMITE_MENSAGEM);
        if pedacos.is_empty() {
            pedacos.push("(vazio)".into());
        }
        let n = pedacos.len();
        for (i, p) in pedacos.into_iter().enumerate() {
            let mut payload = json!({"chat_id": chat_id, "text": p});
            if let (Some(t), true) = (&teclado, i == n - 1) {
                payload["reply_markup"] = json!({"inline_keyboard": t});
            }
            self.api("sendMessage", payload).await?;
        }
        Ok(())
    }

    async fn dizer(&self, chat_id: i64, texto: &str) {
        if let Err(e) = self.enviar(chat_id, texto, None).await {
            tracing::warn!("telegram: mensagem não enviada: {e}");
        }
    }

    // ── laço principal ────────────────────────────────────────────────────
    /// Roda até ser cancelado. Token inválido encerra (nada a tentar de novo).
    pub async fn run(self: Arc<Self>) {
        let eu = match self.api("getMe", json!({})).await {
            Ok(e) => e,
            Err(e) => {
                tracing::error!("telegram não iniciou: {e}");
                return;
            }
        };
        tracing::info!("telegram: bot @{}, {} usuário(s) permitido(s)", eu["username"].as_str().unwrap_or("?"), self.allowed.len());
        let me = self.clone();
        let avisos = tokio::spawn(async move {
            loop {
                tokio::time::sleep(me.notify_every).await;
                if let Err(e) = me.deliver_approvals().await {
                    tracing::warn!("telegram: cartões não enviados: {e}");
                }
                if let Err(e) = me.deliver_notifications().await {
                    tracing::warn!("telegram: entrega de avisos falhou: {e}");
                }
            }
        });
        let mut espera = 2.0f64;
        loop {
            match self.poll_once().await {
                Ok(_) => {
                    espera = 2.0;
                    tokio::task::yield_now().await;
                }
                Err(e) => {
                    match e.codigo {
                        Some(409) => tracing::error!("telegram: outro cliente usa este token (pare o bot do legado)"),
                        Some(401) => {
                            tracing::error!("telegram: token recusado; canal encerrado");
                            avisos.abort();
                            return;
                        }
                        _ => tracing::warn!("telegram: {e}; nova tentativa em {espera:.0}s"),
                    }
                    tokio::time::sleep(Duration::from_secs_f64(espera)).await;
                    espera = (espera * 2.0).min(60.0);
                }
            }
        }
    }

    /// Um `getUpdates`; cada atualização vira uma tarefa (um turno longo não trava o resto).
    pub async fn poll_once(self: &Arc<Self>) -> R<usize> {
        let off = *self.offset.lock();
        let r = self.api("getUpdates", json!({"offset": off, "timeout": self.poll_timeout, "allowed_updates": ["message", "callback_query"]})).await?;
        let lista = r.as_array().cloned().unwrap_or_default();
        for u in &lista {
            {
                let mut o = self.offset.lock();
                *o = (*o).max(u["update_id"].as_i64().unwrap_or(0) + 1);
            }
            let (me, u) = (self.clone(), u.clone());
            tokio::spawn(async move { me.handle_update(u).await });
        }
        Ok(lista.len())
    }

    /// Entrega a fila de avisos; só confirma o que foi enviado (falha: tenta na próxima).
    pub async fn deliver_notifications(&self) -> R<usize> {
        let ops = self.ops.clone();
        let pendentes = tokio::task::spawn_blocking(move || ops.pending_notifications(50, false)).await.ok().and_then(Result::ok).unwrap_or_default();
        let mut enviados = 0;
        let eventos = self.eventos.lock().clone();
        for n in pendentes {
            let id = n["id"].as_i64().unwrap_or(0);
            // fora da matriz evento × canal: marca como entregue sem mandar (o app continua mostrando)
            if !eventos.is_empty() && !eventos.iter().any(|e| n["kind"].as_str() == Some(e.as_str())) {
                let _ = self.ops.ack_notification(id);
                continue;
            }
            if let Err(e) = self.enviar(self.home, &format!("🔔 {}", n["text"].as_str().unwrap_or("")), teclado_lembrete(&n)).await {
                tracing::warn!("telegram: aviso {id} não enviado: {e}");
                break;
            }
            let _ = self.ops.ack_notification(id);
            enviados += 1;
        }
        Ok(enviados)
    }

    // ── atualizações ──────────────────────────────────────────────────────
    pub async fn handle_update(self: &Arc<Self>, u: Value) {
        if u.get("callback_query").is_some() {
            self.on_callback(&u["callback_query"]).await;
        } else if u.get("message").is_some() {
            self.on_message(&u["message"]).await;
        }
    }

    fn permitido(&self, user_id: Option<i64>, chat: &Value) -> bool {
        if chat["type"] == "private" && user_id.is_some_and(|u| self.allowed.contains(&u) && chat["id"].as_i64() == Some(u)) {
            return true;
        }
        if let Some(u) = user_id {
            if self.recusados.lock().insert(u) {
                // um aviso por pessoa, sem encher o log
                tracing::warn!("telegram: usuário {u} recusado (fora da lista ou fora de conversa privada)");
            }
        }
        false
    }

    async fn on_message(self: &Arc<Self>, m: &Value) {
        let chat = &m["chat"];
        if !self.permitido(m["from"]["id"].as_i64(), chat) {
            return;
        }
        let Some(chat_id) = chat["id"].as_i64() else { return };
        let texto = m["text"].as_str().unwrap_or("").trim().to_string();
        let comando = texto.split_whitespace().next().map(|c| c.split('@').next().unwrap_or("").to_lowercase()).unwrap_or_default();
        let arg = || texto.split_once(char::is_whitespace).map(|x| x.1.trim().to_string()).unwrap_or_default();
        if comando == "/capturar" {
            self.on_capturar(chat_id, &texto).await;
            return;
        }
        if texto.starts_with('/') {
            self.armados.lock().remove(&chat_id); // outro comando cancela a captura que esperava
        } else if self.armados.lock().remove(&chat_id).is_some_and(|ate| ate > Instant::now()) {
            self.capturar_mensagem(chat_id, m, &texto).await;
            return;
        }
        let tem = |k: &str| !m[k].is_null();
        if texto.is_empty() && (tem("voice") || tem("audio")) {
            self.on_voz(chat_id, m).await;
            return;
        }
        if texto.is_empty() && tem("photo") {
            self.on_foto(chat_id, m).await;
            return;
        }
        if texto.is_empty() && tem("document") {
            self.dizer(chat_id, "Conversão indisponível neste canal.").await;
            return;
        }
        if texto.is_empty() {
            self.dizer(chat_id, "Entendo texto, voz e foto por aqui.").await;
            return;
        }
        let vida = self.vida.lock().clone();
        if let Some(v) = vida {
            if let Some(r) = v.tratar_async(chat_id, &texto).await {
                // E10: é da vida pessoal; o resto segue para o agente
                self.dizer(chat_id, &r).await;
                return;
            }
        }
        match comando.as_str() {
            "/start" | "/ajuda" => self.dizer(chat_id, BOAS_VINDAS).await,
            "/painel" => {
                let f = self.painel.lock().clone();
                let r = match f {
                    Some(f) => tokio::task::spawn_blocking(move || f()).await.unwrap_or_default(),
                    None => "Painel indisponível neste canal.".into(),
                };
                self.dizer(chat_id, &r).await;
            }
            "/tela" => {
                let (f, a) = (self.tela.lock().clone(), arg());
                let r = match f {
                    Some(f) => tokio::task::spawn_blocking(move || f(&a)).await.unwrap_or_default(),
                    None => "Memória da tela indisponível neste canal.".into(),
                };
                self.dizer(chat_id, &r).await;
            }
            "/camera" => {
                let (f, a) = (self.camera.lock().clone(), arg());
                let r = match f {
                    Some(f) => tokio::task::spawn_blocking(move || f(&a)).await.unwrap_or_default(),
                    None => "Câmera desligada (ORION_CAMERA).".into(),
                };
                self.dizer(chat_id, &r).await;
            }
            "/panico" => {
                let r = self.panico(&arg());
                self.dizer(chat_id, &r).await;
            }
            "/desligar" => {
                let f = self.desligar.lock().clone();
                let r = match f {
                    Some(f) => tokio::task::spawn_blocking(move || f()).await.unwrap_or_default(),
                    None => "Conferência indisponível neste canal.".into(),
                };
                self.dizer(chat_id, &r).await;
            }
            "/ler" => {
                let r = self.ler_depois_cmd(&arg()).await;
                self.dizer(chat_id, &r).await;
            }
            "/rotina" => self.on_rotina(chat_id, &arg()).await,
            "/briefing" => {
                let (ops, ag) = (self.ops.clone(), self.agenda.lock().clone());
                let r = tokio::task::spawn_blocking(move || crate::briefing::build_briefing(&ops, crate::util::agora(), "Antônio", ag.as_deref()))
                    .await
                    .unwrap_or_default();
                self.dizer(chat_id, &r).await;
            }
            "/nova" => {
                if let Ok(atual) = self.memory.active_session(CANAL) {
                    let _ = self.memory.archive_session(&atual.id);
                }
                self.dizer(chat_id, "Conversa nova. A anterior ficou arquivada.").await;
            }
            _ => {
                let por_frase = self.rotinas.lock().clone().and_then(|r| r.por_frase(&texto));
                if let Some(c) = por_frase {
                    self.on_rotina(chat_id, &c.nome).await; // a frase é exatamente a de uma rotina sua
                } else {
                    let _ = self.api("sendChatAction", json!({"chat_id": chat_id, "action": "typing"})).await;
                    let t: String = texto.chars().take(LIMITE_ENTRADA).collect();
                    self.consumir(chat_id, self.agent.run(TurnRequest { channel: CANAL.into(), text: t, ..Default::default() })).await;
                }
            }
        }
    }

    /// `/rotina` lista as suas; `/rotina <nome>` roda uma concedida. Passo que pede aprovação vira o
    /// cartão de sempre aqui mesmo, e a rotina para nele.
    async fn on_rotina(&self, chat_id: i64, nome: &str) {
        let (rot, ex) = (self.rotinas.lock().clone(), self.executor_rotinas.lock().clone());
        let (Some(rot), Some(ex)) = (rot, ex) else {
            self.dizer(chat_id, "Rotinas indisponíveis neste canal.").await;
            return;
        };
        if nome.is_empty() {
            let linhas: Vec<String> = rot.listar().iter().map(|c| format!("• {} — {}", c.nome, c.estado.replace('_', " "))).collect();
            self.dizer(
                chat_id,
                &if linhas.is_empty() { "Nenhuma rotina em <dados>/rotinas.".to_string() } else { format!("Suas rotinas:\n{}", linhas.join("\n")) },
            )
            .await;
            return;
        }
        let r = ex.executar(nome, "telegram").await;
        let icone = match r.status.as_str() {
            "concluida" => "✅",
            "aguardando_aprovacao" => "⏸",
            "recusada" => "⛔",
            _ => "⚠️",
        };
        let resumo =
            format!("{icone} Rotina {nome}: {}{}", r.status.replace('_', " "), if r.motivo.is_empty() { String::new() } else { format!(" — {}", r.motivo) });
        self.dizer(chat_id, &resumo).await;
        if let Some(a) = r.aprovacao_id.as_ref().and_then(|id| self.approvals.get(id)) {
            let _ = self.send_approval(chat_id, &a.id).await;
        }
    }

    async fn ler_depois_cmd(&self, link: &str) -> String {
        let Some(ld) = self.ler_depois.lock().clone() else { return "Ler depois indisponível neste canal.".into() };
        if link.is_empty() {
            let lista = ld.listar(Some("pendente"), 10).unwrap_or_default();
            if lista.is_empty() {
                return "Nada na fila. Use /ler <link>.".into();
            }
            return format!(
                "Na fila para ler:\n{}",
                lista
                    .iter()
                    .map(|i| format!("• {}", i["title"].as_str().filter(|t| !t.is_empty()).or(i["url"].as_str()).unwrap_or("")))
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
        let item = match ld.adicionar(link) {
            Ok(i) => i,
            Err(e) => return e.0,
        };
        if item["status"] == "pendente" {
            ld.agendar(item["id"].as_i64().unwrap_or(0));
            return "📚 Guardei. Vou resumir e deixar a nota em 'Ler depois' no vault.".into();
        }
        format!("Esse link já estava na lista ({}).", item["status"].as_str().unwrap_or(""))
    }

    // ── captura rápida (REGRAS.md: captura rápida e briefing não passam pelo modelo) ──
    async fn on_capturar(&self, chat_id: i64, texto: &str) {
        if self.capture.is_none() {
            self.dizer(chat_id, "Captura desligada: defina ORION_VAULT_DIR (a pasta do seu vault).").await;
            return;
        }
        if let Some(resto) = texto.split_once(char::is_whitespace).map(|x| x.1.trim()).filter(|r| !r.is_empty()) {
            self.armados.lock().remove(&chat_id);
            self.guardar_texto(chat_id, resto, "").await;
            return;
        }
        self.armados.lock().insert(chat_id, Instant::now() + Duration::from_secs(CAPTURA_ESPERA_S));
        self.dizer(
            chat_id,
            &format!(
                "Pode mandar: texto, link, foto ou voz. A próxima mensagem vira nota no vault (vale por {} minutos; qualquer comando cancela).",
                CAPTURA_ESPERA_S / 60
            ),
        )
        .await;
    }

    async fn capturar_mensagem(&self, chat_id: i64, m: &Value, texto: &str) {
        if !texto.is_empty() {
            self.guardar_texto(chat_id, texto, "").await;
        } else if !m["voice"].is_null() || !m["audio"].is_null() {
            if let Some(e) = self.transcrever_voz(chat_id, m).await {
                self.dizer(chat_id, &format!("🎙️ Entendi: {e}")).await;
                self.guardar_texto(chat_id, &e, "voz").await;
            }
        } else if !m["photo"].is_null() {
            if let Some((imagem, caminho)) = self.baixar_foto(chat_id, m).await {
                let ext = caminho.rsplit_once('.').map(|x| format!(".{}", x.1.to_lowercase())).unwrap_or_else(|| ".jpg".into());
                let ext = if EXTENSOES_FOTO.contains(&ext.as_str()) { ext } else { ".jpg".into() };
                let legenda = m["caption"].as_str().unwrap_or("").trim().to_string();
                let Some(cap) = self.capture.clone() else { return };
                let r = tokio::task::spawn_blocking(move || cap.save_photo(&imagem, &ext, &legenda).map(|p| cap.relativo(&p))).await;
                self.resultado_captura(chat_id, r.map_err(|e| e.to_string()).and_then(|x| x.map_err(|e| e.0))).await;
            }
        } else {
            self.dizer(chat_id, "Só guardo texto, link, foto ou voz.").await;
        }
    }

    async fn guardar_texto(&self, chat_id: i64, texto: &str, origem: &str) {
        let origem = if !origem.is_empty() {
            origem
        } else if LINK.is_match(texto) {
            "link"
        } else {
            "texto"
        };
        let Some(cap) = self.capture.clone() else { return };
        let (t, o) = (texto.chars().take(LIMITE_ENTRADA * 2).collect::<String>(), origem.to_string());
        let r = tokio::task::spawn_blocking(move || cap.save_text(&t, &o, "telegram", crate::capture::MAX_TEXTO).map(|p| cap.relativo(&p))).await;
        self.resultado_captura(chat_id, r.map_err(|e| e.to_string()).and_then(|x| x.map_err(|e| e.0))).await;
    }

    async fn resultado_captura(&self, chat_id: i64, r: Result<String, String>) {
        match r {
            Ok(rel) => self.dizer(chat_id, &format!("📝 Guardado no vault: {rel}")).await,
            Err(e) => {
                tracing::warn!("telegram: captura não gravada");
                self.dizer(chat_id, &format!("⚠️ Não guardei: {e}")).await;
            }
        }
    }

    /// Baixa e transcreve a voz; em qualquer falha avisa o Antônio e devolve `None`.
    async fn transcrever_voz(&self, chat_id: i64, m: &Value) -> Option<String> {
        let Some(t) = self.transcriber.clone() else {
            self.dizer(chat_id, "Ainda não transcrevo áudio (falta ORION_TRANSCRIBE_API_KEY). Escreva, por favor.").await;
            return None;
        };
        let voz = if m["voice"].is_null() { &m["audio"] } else { &m["voice"] };
        if voz["duration"].as_i64().unwrap_or(0) > MAX_VOZ_S {
            self.dizer(chat_id, &format!("Áudio longo demais (máximo {} minutos).", MAX_VOZ_S / 60)).await;
            return None;
        }
        let _ = self.api("sendChatAction", json!({"chat_id": chat_id, "action": "typing"})).await;
        let r = async {
            let (audio, caminho) = self.baixar(voz["file_id"].as_str().unwrap_or(""), MAX_AUDIO).await.map_err(|e| e.mensagem)?;
            let nome = caminho.rsplit('/').next().filter(|n| !n.is_empty()).unwrap_or("voz.ogg").to_string();
            t.transcribe(audio, &nome, voz["mime_type"].as_str().unwrap_or("audio/ogg")).await.map_err(|e| e.to_string())
        }
        .await;
        match r {
            Ok(texto) => Some(texto.chars().take(LIMITE_ENTRADA).collect()),
            Err(e) => {
                tracing::warn!("telegram: voz não transcrita: {e}");
                self.dizer(chat_id, &format!("⚠️ Não consegui entender o áudio: {e}")).await;
                None
            }
        }
    }

    async fn on_voz(&self, chat_id: i64, m: &Value) {
        let Some(texto) = self.transcrever_voz(chat_id, m).await else { return };
        // mostra o que foi entendido: quem falou confere (e aprovação continua só por botão)
        self.dizer(chat_id, &format!("🎙️ Entendi: {texto}")).await;
        let resposta = self.consumir(chat_id, self.agent.run(TurnRequest { channel: CANAL.into(), text: texto, ..Default::default() })).await;
        if !resposta.is_empty() {
            if let Err(e) = self.send_audio_reply(chat_id, &resposta).await {
                tracing::warn!("telegram: {e}");
            }
        }
    }

    /// O maior tamanho da foto que couber no limite; falha avisa e devolve `None`.
    async fn baixar_foto(&self, chat_id: i64, m: &Value) -> Option<(Vec<u8>, String)> {
        let tamanhos: Vec<&Value> = m["photo"].as_array().into_iter().flatten().filter(|p| p["file_id"].is_string()).collect();
        let mut cabem: Vec<&Value> = tamanhos.iter().copied().filter(|p| p["file_size"].as_u64().unwrap_or(0) as usize <= MAX_FOTO).collect();
        if cabem.is_empty() {
            cabem = tamanhos.iter().take(1).copied().collect();
        }
        let Some(ultima) = cabem.last() else {
            self.dizer(chat_id, "Não consegui ler essa foto.").await;
            return None;
        };
        let _ = self.api("sendChatAction", json!({"chat_id": chat_id, "action": "typing"})).await;
        match self.baixar(ultima["file_id"].as_str().unwrap_or(""), MAX_FOTO).await {
            Ok(x) => Some(x),
            Err(e) => {
                tracing::warn!("telegram: foto não baixada: {e}");
                self.dizer(chat_id, &format!("⚠️ Não consegui baixar a foto: {e}")).await;
                None
            }
        }
    }

    async fn on_foto(&self, chat_id: i64, m: &Value) {
        let Some((imagem, caminho)) = self.baixar_foto(chat_id, m).await else { return };
        let ext = caminho.rsplit_once('.').map(|x| format!(".{}", x.1.to_lowercase())).unwrap_or_else(|| ".jpg".into());
        let mime = match ext.as_str() {
            ".png" => "image/png",
            ".webp" => "image/webp",
            _ => "image/jpeg",
        };
        let url = format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(&imagem));
        let legenda = m["caption"].as_str().unwrap_or("").trim();
        let legenda = if legenda.is_empty() { "O que você vê nesta imagem?" } else { legenda };
        self.consumir(
            chat_id,
            self.agent.run(TurnRequest {
                channel: CANAL.into(),
                text: legenda.chars().take(LIMITE_ENTRADA).collect(),
                images: vec![url],
                ..Default::default()
            }),
        )
        .await;
    }

    /// Junta o turno do agente e responde: texto, depois um cartão por aprovação pedida.
    async fn consumir(&self, chat_id: i64, mut eventos: EventStream) -> String {
        // F14.04: com outro turno rodando, avisa e espera a vez (o Mutex do tokio atende por ordem)
        let _vez = match self.turno.try_lock() {
            Ok(g) => g,
            Err(_) => {
                self.dizer(chat_id, "⏳ Estou terminando outra resposta. A sua está na fila e sai em seguida.").await;
                self.turno.lock().await
            }
        };
        let (mut texto, mut cartoes, mut erros) = (String::new(), vec![], vec![]);
        while let Some(ev) = eventos.next().await {
            match ev.kind {
                "text" => texto += ev.data["text"].as_str().unwrap_or(""),
                "approval" => cartoes.push(ev.data),
                "error" => erros.push(ev.data["message"].as_str().unwrap_or("erro").to_string()),
                _ => {}
            }
        }
        let resposta = texto.trim().to_string();
        if !resposta.is_empty() {
            self.dizer(chat_id, &resposta).await;
        }
        for c in &cartoes {
            let _ = self.send_approval(chat_id, c["id"].as_str().unwrap_or("")).await;
        }
        for e in &erros {
            self.dizer(chat_id, &format!("⚠️ {e}")).await;
        }
        if resposta.is_empty() && cartoes.is_empty() && erros.is_empty() {
            self.dizer(chat_id, "(sem resposta)").await;
        }
        resposta
    }

    async fn on_callback(self: &Arc<Self>, cb: &Value) {
        let msg = &cb["message"];
        let chat = &msg["chat"];
        let uid = cb["from"]["id"].as_i64();
        if !self.permitido(uid, chat) || cb["id"].is_null() {
            return;
        }
        let Some(chat_id) = chat["id"].as_i64() else { return };
        let data = cb["data"].as_str().unwrap_or("");
        if let Some(l) = LEMBRETE.captures(data) {
            let (rid, acao) = (l[1].parse::<i64>().unwrap_or(0), l[2].to_string());
            let texto = if acao == "f" {
                self.ops.complete_reminder(rid).map(|_| "Feito ✅").unwrap_or("Esse lembrete não existe mais.")
            } else {
                self.ops.snooze_reminder(rid, 60).map(|_| "Adiado 1 h ⏳").unwrap_or("Esse lembrete não existe mais.")
            };
            self.fechar_botoes(cb, chat_id, msg, texto).await;
            return;
        }
        let Some(c) = CALLBACK.captures(data) else {
            let _ = self.api("answerCallbackQuery", json!({"callback_query_id": cb["id"], "text": "Botão inválido."})).await;
            return;
        };
        let (aprovacao_id, resposta) = (c[1].to_string(), &c[2] == "y");
        // REGRAS.md: confirmação fora de banda (canal autenticado). Vincula o botão ao cartão enviado.
        if self.remote_cards.lock().get(&aprovacao_id).copied() != msg["message_id"].as_i64().map(|mid| (chat_id, mid)) {
            return;
        }
        let a = self.approvals.get(&aprovacao_id);
        let sessao = a.as_ref().and_then(|a| self.memory.get_session(&a.session_id).ok().flatten());
        let (Some(a), Some(sessao)) = (a, sessao) else {
            self.fechar_botoes(cb, chat_id, msg, "Aprovação inexistente ou de outro canal.").await;
            return;
        };
        let original_channel = sessao.channel.clone();
        if resposta && !revisavel(&a.args) {
            // o cartão nem oferece o botão; confere de novo aqui
            self.fechar_botoes(cb, chat_id, msg, "Argumentos grandes demais: só dá para negar.").await;
            return;
        }
        if self.approvals.decide(&aprovacao_id, resposta, CANAL, &format!("telegram:{}", uid.unwrap_or(0))).is_err() {
            self.fechar_botoes(cb, chat_id, msg, "Já decidida ou expirada.").await;
            return;
        }
        self.fechar_botoes(cb, chat_id, msg, if resposta { "Aprovado ✅" } else { "Negado ❌" }).await;
        if resposta {
            self.consumir(chat_id, self.agent.resume(&original_channel, &aprovacao_id)).await;
        } else {
            self.dizer(chat_id, &format!("Ação negada: {}. Não foi executada.", a.tool)).await;
        }
    }

    /// Responde ao clique e tira os botões (um cartão não se decide duas vezes).
    async fn fechar_botoes(&self, cb: &Value, chat_id: i64, msg: &Value, texto: &str) {
        let _ = self.api("answerCallbackQuery", json!({"callback_query_id": cb["id"], "text": texto})).await;
        if !msg["message_id"].is_null() {
            if let Err(e) =
                self.api("editMessageReplyMarkup", json!({"chat_id": chat_id, "message_id": msg["message_id"], "reply_markup": {"inline_keyboard": []}})).await
            {
                tracing::info!("telegram: botões não removidos: {e}"); // mensagem antiga demais: o clique já valeu
            }
        }
    }
}

fn tipo_do_erro(e: &reqwest::Error) -> &'static str {
    if e.is_timeout() {
        "Timeout"
    } else if e.is_connect() {
        "ConnectError"
    } else {
        "HTTPError"
    }
}

/// Botões só nos avisos de lembrete (`ref` = `reminder:<id>` ou `reminder:<id>:n<k>`).
fn teclado_lembrete(n: &serde_json::Map<String, Value>) -> Option<Value> {
    let c = REF_LEMBRETE.captures(n.get("ref").and_then(Value::as_str).unwrap_or(""))?;
    Some(json!([[{"text": "✅ Feito", "callback_data": format!("lem:{}:f", &c[1])}, {"text": "⏳ Adiar 1 h", "callback_data": format!("lem:{}:a", &c[1])}]]))
}

fn teclado(aprovacao_id: &str, aprovar: bool) -> Value {
    let negar = json!({"text": "❌ Negar", "callback_data": format!("ap:{aprovacao_id}:n")});
    if !aprovar {
        return json!([[negar]]);
    }
    json!([[{"text": "✅ Aprovar", "callback_data": format!("ap:{aprovacao_id}:y")}, negar]])
}

/// Dá para mostrar os argumentos inteiros no cartão (sem corte e dentro do limite)?
fn revisavel(args: &serde_json::Map<String, Value>) -> bool {
    let v = Value::Object(args.clone());
    let mostrados = orion_policy::redact(&v, 2000);
    mostrados == orion_policy::redact(&v, 1_000_000_000) && serde_json::to_string_pretty(&mostrados).is_ok_and(|s| s.chars().count() <= MAX_ARGS_REVISAVEL)
}

/// Texto do cartão e se dá para aprovar por aqui. Argumento cortado (ou grande demais para caber
/// inteiro) esconderia o fim do comando de quem decide: então só dá para negar.
fn cartao_de(c: &Value) -> (String, bool) {
    let mut args = serde_json::to_string_pretty(c.get("args").unwrap_or(&json!({}))).unwrap_or_default();
    let revisavel = c["args_truncated"] != json!(true) && args.chars().count() <= MAX_ARGS_REVISAVEL;
    if !revisavel {
        args = format!("{}…", args.chars().take(MAX_ARGS_REVISAVEL).collect::<String>());
    }
    let rodape = if revisavel {
        "Vale por 10 minutos e uma única execução."
    } else {
        "⛔ Os argumentos são grandes demais para revisar aqui, então só dá para negar. Se for legítimo, peça de novo em partes menores."
    };
    (
        format!(
            "⚠️ Aprovar esta ação?\n\nFerramenta: {}\nMotivo: {}\nArgumentos (segredos mascarados):\n{args}\n\n{rodape}",
            c["tool"].as_str().unwrap_or("None"),
            c["reason"].as_str().unwrap_or("None")
        ),
        revisavel,
    )
}

/// Quebra em pedaços de até `limite`, preferindo fim de linha.
pub fn dividir(texto: &str, limite: usize) -> Vec<String> {
    let mut resto: Vec<char> = texto.trim().chars().collect();
    let mut pedacos = vec![];
    while resto.len() > limite {
        let corte = resto[..limite].iter().rposition(|c| *c == '\n').filter(|c| *c > limite / 2).unwrap_or(limite);
        let p: String = resto[..corte].iter().collect();
        pedacos.push(p.trim_end().to_string());
        let mut r = resto[corte..].to_vec();
        while r.first() == Some(&'\n') {
            r.remove(0);
        }
        resto = r;
    }
    if !resto.is_empty() {
        pedacos.push(resto.into_iter().collect());
    }
    pedacos
}

#[cfg(test)]
mod tests {
    use super::*;
    fn channel() -> TelegramChannel {
        let memory = Arc::new(Store::open(":memory:").unwrap());
        let policy = Arc::new(orion_policy::PolicyEngine::new(orion_policy::PathGuard::default()));
        let gateway = Arc::new(orion_ai::ChatGateway::new(vec![orion_ai::Endpoint::new("fake", "http://127.0.0.1:1", "fake")]).unwrap());
        let tools = crate::tools::registry::ToolRegistry::new(vec![]).unwrap();
        let agent = Arc::new(Agent::new(gateway, tools, policy.clone(), memory.clone(), crate::agent::AgentConfig::default()));
        TelegramChannel::new("bot-falso", &[42], agent, memory.clone(), policy.approvals.clone(), Arc::new(Operations::new(memory)), None, None).unwrap()
    }
    #[test]
    fn so_dono_em_conversa_privada_e_cartao_completo() {
        let t = channel();
        assert!(t.permitido(Some(42), &json!({"type":"private","id":42})));
        assert!(!t.permitido(Some(42), &json!({"type":"group","id":42})));
        assert!(!t.permitido(Some(7), &json!({"type":"private","id":7})));
        assert!(!t.permitido(Some(42), &json!({"type":"private","id":7})));
        let (_, allow) = cartao_de(&json!({"args":{"cmd":"x"},"args_truncated":true}));
        assert!(!allow);
    }
    #[tokio::test]
    async fn botao_remoto_exige_cartao_enviado_e_nao_repete() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else { break };
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 16384];
                    let _ = socket.read(&mut buf).await;
                    let body = r#"{"ok":true,"result":{"message_id":7}}"#;
                    let reply =
                        format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                    let _ = socket.write_all(reply.as_bytes()).await;
                });
            }
        });
        let mut t = channel();
        t.base = format!("http://{addr}");
        t.client = reqwest::Client::builder().no_proxy().build().unwrap();
        let t = Arc::new(t);
        let session = t.memory.active_session("desktop").unwrap();
        let a = t.approvals.request(&session.id, "executar_comando", &serde_json::Map::new(), "teste", "binding");
        let mut cb = json!({"id":"cb","from":{"id":42},"data":format!("ap:{}:n",a.id),"message":{"message_id":7,"chat":{"id":42,"type":"private"}}});
        t.on_callback(&cb).await;
        assert_eq!(t.approvals.get(&a.id).unwrap().status, orion_policy::Status::Pending);
        t.deliver_approvals().await.unwrap();
        cb["message"]["message_id"] = json!(8);
        t.on_callback(&cb).await;
        assert_eq!(t.approvals.get(&a.id).unwrap().status, orion_policy::Status::Pending);
        cb["message"]["message_id"] = json!(7);
        t.on_callback(&cb).await;
        assert_eq!(t.approvals.get(&a.id).unwrap().status, orion_policy::Status::Denied);
        cb["data"] = json!(format!("ap:{}:y", a.id));
        t.on_callback(&cb).await;
        assert_eq!(t.approvals.get(&a.id).unwrap().status, orion_policy::Status::Denied);
        server.abort();
    }
}
