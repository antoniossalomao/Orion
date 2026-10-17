//! Modo pânico e não perturbe (REGRAS.md: pânico é corte; não perturbe só segura).
//!
//! O estado mora no `meta` do banco: vale para o servidor e para a CLI e sobrevive a reinício.
//! Pânico é corte, não pausa: só sai por comando explícito, e a saída sem audit é recusada. Não
//! perturbe só segura avisos não urgentes.

use chrono::{Local, NaiveTime, TimeZone, Timelike};
use orion_store::Store;
use serde_json::{json, Value};
use std::sync::Arc;

pub const CHAVE_PANICO: &str = "modo:panico";
pub const CHAVE_DND: &str = "modo:nao_perturbe_ate";

#[derive(Debug, Clone, thiserror::Error)]
#[error("{0}")]
pub struct ModoError(pub String);

fn hm(texto: &str) -> Result<(u32, u32), ModoError> {
    let erro = || ModoError(format!("horário inválido: '{texto}' (use HH:MM)"));
    let (h, m) = texto.trim().split_once(':').ok_or_else(erro)?;
    let (h, m): (u32, u32) = (h.trim().parse().map_err(|_| erro())?, m.trim().parse().map_err(|_| erro())?);
    if h >= 24 || m >= 60 {
        return Err(erro());
    }
    Ok((h, m))
}

fn local(agora: f64) -> chrono::DateTime<Local> {
    chrono::DateTime::from_timestamp(agora as i64, 0).unwrap_or_default().with_timezone(&Local)
}

/// Epoch da próxima vez que o relógio marca `hhmm` (hoje, ou amanhã se já passou).
pub fn proxima(hhmm: &str, agora: f64) -> Result<f64, ModoError> {
    let (h, m) = hm(hhmm)?;
    let dt = local(agora);
    let mut alvo = dt.date_naive().and_time(NaiveTime::from_hms_opt(h, m, 0).unwrap());
    if Local.from_local_datetime(&alvo).earliest().map(|d| d.timestamp() as f64).unwrap_or(0.0) <= agora {
        alvo += chrono::Duration::days(1);
    }
    Ok(Local.from_local_datetime(&alvo).earliest().map(|d| d.timestamp() as f64).unwrap_or(agora))
}

/// `22:30-07:00` (atravessa a meia-noite) ou `13:00-14:00`; vazia nunca está.
pub fn na_janela(janela: &str, agora: f64) -> bool {
    let Some((a, b)) = janela.split_once('-') else { return false };
    let (Ok(ini), Ok(fim)) = (hm(a), hm(b)) else { return false };
    let dt = local(agora);
    let atual = (dt.hour(), dt.minute());
    if ini <= fim {
        ini <= atual && atual < fim
    } else {
        atual >= ini || atual < fim
    }
}

pub type AuditFn = Arc<dyn Fn(&Value) -> anyhow::Result<()> + Send + Sync>;

pub struct Modos {
    pub memory: Arc<Store>,
    janela: String,
    audit: Option<AuditFn>,
    emergency_cut: std::sync::atomic::AtomicBool,
}

impl Modos {
    pub fn new(memory: Arc<Store>, dnd_at: &str, audit: Option<AuditFn>) -> Self {
        Modos { memory, janela: dnd_at.into(), audit, emergency_cut: std::sync::atomic::AtomicBool::new(false) }
    }

    fn estado_panico(&self) -> Option<Value> {
        let bruto = match self.memory.meta_get(CHAVE_PANICO) {
            Some(v) => v,
            None if self.emergency_cut.load(std::sync::atomic::Ordering::SeqCst) => return Some(json!({"desde":null,"origem":"corte_local"})),
            None => return None,
        };
        if bruto.is_empty() {
            return None;
        }
        // corrompido: continua em pânico (falha fechada)
        match serde_json::from_str::<Value>(&bruto) {
            Ok(v @ Value::Object(_)) => Some(v),
            _ => Some(json!({"desde": null, "origem": "?"})),
        }
    }

    pub fn panico(&self) -> bool {
        self.estado_panico().is_some()
    }

    /// Liga o pânico (`false` se já estava). Entrar nunca é barrado: falha do audit só vai ao
    /// log, porque cortar é sempre mais seguro que continuar.
    pub fn entrar_panico(&self, origem: &str) -> anyhow::Result<bool> {
        if self.panico() {
            return Ok(false);
        }
        // REGRAS.md: pânico é corte; não perturbe só segura. Nem falha do banco impede o corte local.
        self.emergency_cut.store(true, std::sync::atomic::Ordering::SeqCst);
        let o: String = origem.chars().take(30).collect();
        let v = orion_policy::pyjson::dumps_ascii(&json!({"desde": self.memory.now(), "origem": o}), false);
        self.memory.meta_set(CHAVE_PANICO, Some(&v))?;
        if let Err(e) = self.registrar("entrar", origem) {
            tracing::error!("audit da entrada no pânico falhou (o pânico está ligado): {e}");
        }
        tracing::warn!("MODO PÂNICO ligado ({origem})");
        Ok(true)
    }

    /// Desliga o pânico (`false` se não estava). Sem o audit da saída, não sai
    /// (REGRAS.md: falha de auditoria nega a ação).
    pub fn sair_panico(&self, origem: &str) -> Result<bool, ModoError> {
        if !self.panico() {
            return Ok(false);
        }
        self.registrar("sair", origem).map_err(|_| ModoError("não saí do pânico: o audit falhou".into()))?;
        self.memory.meta_set(CHAVE_PANICO, None).map_err(|e| ModoError(e.to_string()))?;
        self.emergency_cut.store(false, std::sync::atomic::Ordering::SeqCst);
        tracing::warn!("modo pânico desligado ({origem})");
        Ok(true)
    }

    fn registrar(&self, acao: &str, origem: &str) -> anyhow::Result<()> {
        match &self.audit {
            Some(a) => a(&json!({"tool": "modo_panico", "action": acao, "risk": null, "reason": format!("origem: {origem}")})),
            None => Ok(()),
        }
    }

    /// Até quando vale o não perturbe ligado à mão.
    pub fn nao_perturbe_ate(&self) -> Option<f64> {
        let ate: f64 = self.memory.meta_get(CHAVE_DND)?.parse().ok()?;
        (ate > self.memory.now()).then_some(ate)
    }

    pub fn nao_perturbe(&self) -> bool {
        self.nao_perturbe_ate().is_some() || na_janela(&self.janela, self.memory.now())
    }

    /// `ate`: "HH:MM" (a próxima vez que o relógio marcar) ou epoch. Devolve o epoch.
    pub fn ligar_nao_perturbe(&self, ate: &str) -> Result<f64, ModoError> {
        let fim = match ate.trim().parse::<f64>() {
            Ok(v) => v,
            Err(_) => proxima(ate, self.memory.now())?,
        };
        if fim <= self.memory.now() {
            return Err(ModoError("o fim do não perturbe precisa estar no futuro".into()));
        }
        self.memory.meta_set(CHAVE_DND, Some(&orion_policy::pyjson::float_repr(fim))).map_err(|e| ModoError(e.to_string()))?;
        Ok(fim)
    }

    pub fn desligar_nao_perturbe(&self) -> anyhow::Result<()> {
        Ok(self.memory.meta_set(CHAVE_DND, None)?)
    }

    pub fn estado(&self) -> Value {
        let p = self.estado_panico();
        json!({
            "panico": p.is_some(),
            "panico_desde": p.as_ref().and_then(|p| p.get("desde").cloned()),
            "panico_origem": p.as_ref().and_then(|p| p.get("origem").cloned()),
            "nao_perturbe": self.nao_perturbe(),
            "nao_perturbe_ate": self.nao_perturbe_ate(),
            "nao_perturbe_horario": if self.janela.is_empty() { None } else { Some(&self.janela) },
        })
    }
}

#[cfg(test)]
mod panic_tests {
    use super::*;
    #[test]
    fn corte_local_continua_se_banco_falha() {
        let memory = Arc::new(Store::open(":memory:").unwrap());
        let modes = Modos::new(memory.clone(), "", None);
        memory.conn().execute_batch("DROP TABLE meta").unwrap();
        assert!(modes.entrar_panico("teste").is_err());
        assert!(modes.panico());
        assert!(modes.sair_panico("teste").is_err());
        assert!(modes.panico());
    }
}
