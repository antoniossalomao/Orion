//! Casos de `tests/policy/test_approvals.py` (Python) portados.

use orion_policy::{ApprovalStore, Status};
use serde_json::{json, Map, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

struct Relogio(Arc<AtomicU64>);

impl Relogio {
    fn novo() -> (Self, orion_policy::Clock) {
        let t = Arc::new(AtomicU64::new(1000));
        let c = t.clone();
        (Relogio(t), Arc::new(move || c.load(Ordering::SeqCst) as f64))
    }
    fn avancar(&self, s: u64) {
        self.0.fetch_add(s, Ordering::SeqCst);
    }
}

fn args(cmd: &str) -> Map<String, Value> {
    json!({"cmd": cmd}).as_object().unwrap().clone()
}

fn store() -> (ApprovalStore, Relogio) {
    let (r, clock) = Relogio::novo();
    (ApprovalStore::new(600, clock), r)
}

const CMD: &str = "Remove-Item C:\\x -Recurse";

#[test]
fn fluxo_pedido_decisao_consumo_unico() {
    let (s, _) = store();
    let a = s.request("s1", "executar_comando", &args(CMD), "destrutivo", "");
    assert!(!s.consume("s1", "executar_comando", &args(CMD), ""), "ainda pendente");
    s.decide(&a.id, true, "telegram", "antonio").unwrap();
    assert!(s.consume("s1", "executar_comando", &args(CMD), ""));
    assert!(!s.consume("s1", "executar_comando", &args(CMD), ""), "uso único");
}

#[test]
fn pedido_idempotente_enquanto_pendente() {
    let (s, _) = store();
    let a = s.request("s1", "executar_comando", &args(CMD), "x", "");
    let b = s.request("s1", "executar_comando", &args(CMD), "x", "");
    assert_eq!(a.id, b.id);
    assert_eq!(s.pending(Some("s1"), false).len(), 1);
}

#[test]
fn aprovacao_e_da_chamada_exata_e_da_sessao() {
    let (s, _) = store();
    let a = s.request("s1", "executar_comando", &args(CMD), "x", "");
    s.decide(&a.id, true, "web", "antonio").unwrap();
    assert!(!s.consume("s2", "executar_comando", &args(CMD), ""), "outra sessão");
    assert!(!s.consume("s1", "executar_comando", &args("Remove-Item D:\\"), ""), "outro argumento");
    assert!(!s.consume("s1", "escrever_arquivo", &args(CMD), ""), "outra ferramenta");
    assert!(s.consume("s1", "executar_comando", &args(CMD), ""));
}

#[test]
fn negada_nunca_consome() {
    let (s, _) = store();
    let a = s.request("s1", "executar_comando", &args(CMD), "x", "");
    s.decide(&a.id, false, "web", "antonio").unwrap();
    assert!(!s.consume("s1", "executar_comando", &args(CMD), ""));
    assert_eq!(s.get(&a.id).unwrap().status, Status::Denied);
}

#[test]
fn expira() {
    let (s, r) = store();
    let a = s.request("s1", "executar_comando", &args(CMD), "x", "");
    s.decide(&a.id, true, "web", "antonio").unwrap();
    r.avancar(601);
    assert!(!s.consume("s1", "executar_comando", &args(CMD), ""));
    assert_eq!(s.get(&a.id).unwrap().status, Status::Expired);
}

#[test]
fn nao_decide_duas_vezes_nem_expirada_nem_inexistente() {
    let (s, r) = store();
    let a = s.request("s1", "executar_comando", &args(CMD), "x", "");
    s.decide(&a.id, true, "web", "antonio").unwrap();
    assert!(s.decide(&a.id, false, "web", "antonio").is_err());
    let b = s.request("s1", "executar_comando", &args("outro"), "x", "");
    r.avancar(700);
    assert!(s.decide(&b.id, true, "web", "antonio").is_err());
    assert!(matches!(s.decide("inexistente", true, "web", "antonio"), Err(orion_policy::approvals::DecideError::NotFound(_))));
}

#[test]
fn fila_de_pendentes_ordenada() {
    let (s, r) = store();
    s.request("s1", "executar_comando", &args("a"), "x", "");
    r.avancar(1);
    s.request("s2", "executar_comando", &args("b"), "x", "");
    let todas: Vec<_> = s.pending(None, false).into_iter().map(|p| p.session_id).collect();
    assert_eq!(todas, ["s1", "s2"]);
    let s2: Vec<_> = s.pending(Some("s2"), false).into_iter().map(|p| p.session_id).collect();
    assert_eq!(s2, ["s2"]);
}
