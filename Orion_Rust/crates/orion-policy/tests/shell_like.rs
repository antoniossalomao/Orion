//! F02.23: argv livre (`abrir_app` com argumentos) e digitação (`enviar_teclas`) valem como shell:
//! com pasta bloqueada, nem a autorização persistente dispensa o cartão.

use orion_policy::{Action, Context, PathGuard, PolicyEngine, ToolCall};
use serde_json::json;
use std::collections::HashSet;

fn motor(bloqueada: bool) -> PolicyEngine {
    let tmp = std::env::temp_dir().join("orion-shell-like");
    let segredo = tmp.join("segredo");
    std::fs::create_dir_all(&segredo).unwrap();
    let mut guard = PathGuard::new(vec![tmp.join("proj")]);
    if bloqueada {
        guard.blocked_roots = vec![segredo];
    }
    let p = PolicyEngine::new(guard);
    p.configure_access(true, HashSet::new());
    p
}

fn decide(p: &PolicyEngine, nome: &str, args: serde_json::Value) -> Action {
    p.evaluate(&ToolCall::new(nome, args), &Context::new("s")).action
}

#[test]
fn sem_pasta_bloqueada_autorizacao_persistente_vale() {
    let p = motor(false);
    assert_eq!(decide(&p, "abrir_app", json!({"nome": "powershell", "argumentos": ["-c", "dir"]})), Action::Allow);
}

#[test]
fn argumentos_com_pasta_bloqueada_confirmam() {
    let p = motor(true);
    assert_eq!(decide(&p, "abrir_app", json!({"nome": "code"})), Action::Allow);
    assert_eq!(decide(&p, "abrir_app", json!({"nome": "powershell", "argumentos": ["-c", "dir"]})), Action::Confirm);
    assert_eq!(decide(&p, "enviar_teclas", json!({"titulo_janela": "Terminal", "texto": "dir"})), Action::Confirm);
}

#[test]
fn argumento_que_cita_pasta_bloqueada_e_negado() {
    let p = motor(true);
    let alvo = std::env::temp_dir().join("orion-shell-like").join("segredo").join("a.txt");
    let alvo = alvo.to_string_lossy();
    assert_eq!(decide(&p, "abrir_app", json!({"nome": "notepad", "argumentos": [alvo]})), Action::Deny);
    assert_eq!(decide(&p, "enviar_teclas", json!({"titulo_janela": "Terminal", "texto": format!("cat {alvo}")})), Action::Deny);
}
