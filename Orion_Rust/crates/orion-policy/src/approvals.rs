//! Aprovações fora de banda (REGRAS.md: confirmação fora de banda (canal autenticado)).
//!
//! A confirmação NÃO vem de uma frase no chat: o motor pede, e só um canal autenticado
//! (botão do Telegram, tela com login) chama `decide`. A aprovação fica ligada a
//! (sessão, ferramenta, hash dos argumentos), vale por um tempo e é consumida uma única vez.
//! O caminho das ferramentas nunca recebe `decide`, então o modelo não se auto-aprova.
//!
//! `pending()` também serve de fila: ações propostas enquanto o Antônio está offline ficam
//! esperando a decisão dele.

use crate::{pyjson, Clock};
use base64::Engine as _;
use rand::RngCore;
use serde::Serialize;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Pending,
    Approved,
    Denied,
    Consumed,
    Expired,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Pending => "pending",
            Status::Approved => "approved",
            Status::Denied => "denied",
            Status::Consumed => "consumed",
            Status::Expired => "expired",
        }
    }
}

/// Hash canônico (sha256 completo) de (ferramenta, argumentos), igual ao do Python.
pub fn hash_call(tool: &str, args: &Map<String, Value>) -> String {
    let payload = pyjson::dumps(&json!({"tool": tool, "args": Value::Object(args.clone())}), true);
    hex::encode(Sha256::digest(payload.as_bytes()))
}

#[derive(Debug, Clone, Serialize)]
pub struct Approval {
    pub id: String,
    pub session_id: String,
    pub tool: String,
    pub args_hash: String,
    pub reason: String,
    pub created_at: f64,
    pub expires_at: f64,
    /// O que o Antônio está aprovando.
    pub args: Map<String, Value>,
    pub status: Status,
    pub decided_by: Option<String>,
    pub channel: Option<String>,
    pub binding: String,
}

#[derive(Debug)]
pub enum DecideError {
    NotFound(String),
    NotPending(String, Status),
}

impl std::fmt::Display for DecideError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecideError::NotFound(id) => write!(f, "aprovação {id} não existe"),
            DecideError::NotPending(id, s) => {
                write!(f, "aprovação {id} não está pendente ({})", s.as_str())
            }
        }
    }
}

impl std::error::Error for DecideError {}

/// `secrets.token_urlsafe(n)`.
pub fn token_urlsafe(n: usize) -> String {
    let mut b = vec![0u8; n];
    rand::thread_rng().fill_bytes(&mut b);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)
}

pub struct ApprovalStore {
    ttl: f64,
    clock: Clock,
    items: Mutex<HashMap<String, Approval>>,
}

impl Default for ApprovalStore {
    fn default() -> Self {
        Self::new(600, crate::wall_clock())
    }
}

impl ApprovalStore {
    pub fn new(ttl_s: u64, clock: Clock) -> Self {
        ApprovalStore { ttl: ttl_s as f64, clock, items: Mutex::new(HashMap::new()) }
    }

    fn expire(&self, a: &mut Approval) {
        if matches!(a.status, Status::Pending | Status::Approved) && (self.clock)() >= a.expires_at {
            a.status = Status::Expired;
        }
    }

    /// Cria (ou devolve a pendente idêntica) uma solicitação de aprovação.
    pub fn request(
        &self,
        session_id: &str,
        tool: &str,
        args: &Map<String, Value>,
        reason: &str,
        binding: &str,
    ) -> Approval {
        let h = hash_call(tool, args);
        let mut items = self.items.lock().unwrap();
        for a in items.values_mut() {
            self.expire(a);
            if a.session_id == session_id
                && a.args_hash == h
                && a.binding == binding
                && a.status == Status::Pending
            {
                return a.clone();
            }
        }
        let agora = (self.clock)();
        let novo = Approval {
            id: token_urlsafe(8),
            session_id: session_id.into(),
            tool: tool.into(),
            args_hash: h,
            reason: reason.into(),
            created_at: agora,
            expires_at: agora + self.ttl,
            args: args.clone(),
            status: Status::Pending,
            decided_by: None,
            channel: None,
            binding: binding.into(),
        };
        items.insert(novo.id.clone(), novo.clone());
        novo
    }

    /// Registra a decisão. Chamado só por canais autenticados.
    pub fn decide(
        &self,
        approval_id: &str,
        approved: bool,
        channel: &str,
        actor: &str,
    ) -> Result<Approval, DecideError> {
        let mut items = self.items.lock().unwrap();
        let a = items
            .get_mut(approval_id)
            .ok_or_else(|| DecideError::NotFound(approval_id.into()))?;
        self.expire(a);
        if a.status != Status::Pending {
            return Err(DecideError::NotPending(approval_id.into(), a.status));
        }
        a.status = if approved { Status::Approved } else { Status::Denied };
        a.decided_by = Some(actor.into());
        a.channel = Some(channel.into());
        Ok(a.clone())
    }

    /// `true` uma única vez se há aprovação válida para exatamente esta chamada.
    pub fn consume(&self, session_id: &str, tool: &str, args: &Map<String, Value>, binding: &str) -> bool {
        let h = hash_call(tool, args);
        let mut items = self.items.lock().unwrap();
        for a in items.values_mut() {
            self.expire(a);
            if a.session_id == session_id
                && a.args_hash == h
                && a.binding == binding
                && a.status == Status::Approved
            {
                a.status = Status::Consumed;
                return true;
            }
        }
        false
    }

    pub fn pending(&self, session_id: Option<&str>, include_approved: bool) -> Vec<Approval> {
        let mut items = self.items.lock().unwrap();
        let mut out: Vec<Approval> = items
            .values_mut()
            .filter_map(|a| {
                self.expire(a);
                let vivo = a.status == Status::Pending || (include_approved && a.status == Status::Approved);
                (vivo && session_id.is_none_or(|s| a.session_id == s)).then(|| a.clone())
            })
            .collect();
        out.sort_by(|a, b| a.created_at.total_cmp(&b.created_at));
        out
    }

    /// Pendentes ou aprovadas ainda não consumidas não podem desaparecer do contexto.
    pub fn unresolved(&self, session_id: &str) -> bool {
        let mut items = self.items.lock().unwrap();
        items.values_mut().any(|a| {
            if a.session_id != session_id {
                return false;
            }
            self.expire(a);
            matches!(a.status, Status::Pending | Status::Approved)
        })
    }

    /// Editar ou esquecer um fato invalida decisões pendentes sobre o conteúdo anterior.
    pub fn invalidate_fact(&self, fact_id: i64) {
        let mut items = self.items.lock().unwrap();
        for a in items.values_mut() {
            if (a.tool == "editar_fato" || a.tool == "esquecer_fato")
                && a.args.get("id").and_then(Value::as_i64) == Some(fact_id)
                && matches!(a.status, Status::Pending | Status::Approved)
            {
                a.status = Status::Denied;
            }
        }
    }

    pub fn invalidate_tools(&self, names: &HashSet<String>) {
        let mut items = self.items.lock().unwrap();
        for a in items.values_mut() {
            if names.contains(&a.tool) && matches!(a.status, Status::Pending | Status::Approved) {
                a.status = Status::Denied;
                a.reason = "origem ou revisão revogada".into();
            }
        }
    }

    pub fn invalidate_binding(&self, binding: &str) {
        let mut items = self.items.lock().unwrap();
        for a in items.values_mut() {
            if a.binding.contains(binding) && matches!(a.status, Status::Pending | Status::Approved) {
                a.status = Status::Denied;
                a.reason = "escopo revogado".into();
            }
        }
    }

    pub fn get(&self, approval_id: &str) -> Option<Approval> {
        let mut items = self.items.lock().unwrap();
        let a = items.get_mut(approval_id)?;
        self.expire(a);
        Some(a.clone())
    }
}
