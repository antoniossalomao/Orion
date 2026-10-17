//! Segundo fator de uso único (REGRAS.md: login com senha e sessão revogável).
use parking_lot::Mutex;
use rand::{rngs::OsRng, Rng};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, VecDeque},
    time::{Duration, Instant},
};
use subtle::ConstantTimeEq;

struct Pending {
    client: String,
    hash: [u8; 32],
    until: Instant,
    attempts: u8,
}
#[derive(Default)]
pub struct TwoFactor {
    pending: Mutex<HashMap<String, Pending>>,
    issued: Mutex<VecDeque<(String, Instant)>>,
}
impl TwoFactor {
    pub fn issue(&self, client: &str) -> Result<(String, String), &'static str> {
        self.issue_at(client, Instant::now())
    }
    fn issue_at(&self, client: &str, now: Instant) -> Result<(String, String), &'static str> {
        let mut pending = self.pending.lock();
        pending.retain(|_, p| p.until > now);
        if pending.values().any(|p| p.client == client) {
            return Err("aguarde o código anterior expirar");
        }
        if pending.len() >= 256 {
            return Err("muitos logins pendentes");
        }
        let mut issued = self.issued.lock();
        while issued.front().is_some_and(|(_, at)| now.duration_since(*at) >= Duration::from_secs(900)) {
            issued.pop_front();
        }
        if issued.len() >= 256 || issued.iter().filter(|(c, _)| c == client).count() >= 3 {
            return Err("limite de códigos; aguarde 15 minutos");
        }
        issued.push_back((client.into(), now));
        let id = orion_policy::approvals::token_urlsafe(32);
        let code = format!("{:06}", OsRng.gen_range(0..1_000_000u32));
        let hash = Sha256::digest(format!("{id}:{code}")).into();
        pending.insert(id.clone(), Pending { client: client.into(), hash, until: now + Duration::from_secs(300), attempts: 0 });
        Ok((id, code))
    }
    pub fn cancel(&self, id: &str) {
        self.pending.lock().remove(id);
    }
    pub fn verify(&self, id: &str, code: &str, client: &str) -> bool {
        self.verify_at(id, code, client, Instant::now())
    }
    fn verify_at(&self, id: &str, code: &str, client: &str, now: Instant) -> bool {
        let mut pending = self.pending.lock();
        let Some(p) = pending.get_mut(id) else { return false };
        if p.client != client {
            return false;
        }
        let hash: [u8; 32] = Sha256::digest(format!("{id}:{code}")).into();
        p.attempts += 1;
        let ok = p.until > now && code.len() == 6 && bool::from(p.hash.ct_eq(&hash));
        if ok || p.until <= now || p.attempts >= 5 {
            pending.remove(id);
        }
        ok
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fator_vinculado_expira_e_nao_repete() {
        let t = TwoFactor::default();
        let now = Instant::now();
        let (id, code) = t.issue_at("pc", now).unwrap();
        assert!(!t.verify_at(&id, &code, "outro", now));
        assert!(t.verify_at(&id, &code, "pc", now));
        assert!(!t.verify_at(&id, &code, "pc", now));
        let (id, code) = t.issue_at("pc", now).unwrap();
        assert!(!t.verify_at(&id, &code, "pc", now + Duration::from_secs(301)));
    }
    #[test]
    fn limita_tentativas_e_reenvios() {
        let t = TwoFactor::default();
        let (id, code) = t.issue("pc").unwrap();
        assert!(t.issue("pc").is_err());
        for _ in 0..5 {
            assert!(!t.verify(&id, "errado", "pc"));
        }
        assert!(!t.verify(&id, &code, "pc"));
    }
    #[test]
    fn limita_emissao_mesmo_com_senha_valida() {
        let t = TwoFactor::default();
        let now = Instant::now();
        for _ in 0..3 {
            let (id, _) = t.issue_at("pc", now).unwrap();
            t.cancel(&id);
        }
        assert!(t.issue_at("pc", now).is_err());
        assert!(t.issue_at("pc", now + Duration::from_secs(901)).is_ok());
    }
}
