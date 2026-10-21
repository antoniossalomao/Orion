//! Fala com um servidor de verdade. Só roda com `ORION_TEST_URL` (e `ORION_TEST_SENHA`):
//! `ORION_TEST_URL=http://127.0.0.1:8765 ORION_TEST_SENHA=... cargo test -p orion-app --test live`
use futures::StreamExt;
use orion_app::live::{map_history, map_sessions};
use orion_app::net::{Api, Chat, LoginResult};
use serde_json::json;

fn api() -> Option<(Api, String)> {
    let url = std::env::var("ORION_TEST_URL").ok()?;
    Some((Api::new(&url, None), std::env::var("ORION_TEST_SENHA").unwrap_or_default()))
}

#[tokio::test]
async fn login_chat_historico_e_listas() {
    let Some((api, senha)) = api() else { return };
    api.health().await.expect("servidor no ar");
    assert!(!api.auth_ok().await, "sem login não autentica");
    let usuario = std::env::var("ORION_TEST_USER").unwrap_or_default();
    let err = api.login(&usuario, "senha-errada").await.unwrap_err();
    assert_eq!(err, "Usuário ou senha incorretos.");
    assert!(matches!(api.login(&usuario, &senha).await, Ok(LoginResult::Ok)));
    assert!(api.auth_ok().await);

    // nova conversa + chat por SSE
    let v = api.post("/sessoes", json!({})).await.unwrap();
    let sid = v["sessao_id"].as_str().unwrap().to_string();
    let mut texto = String::new();
    let mut fim = false;
    let mut s = Box::pin(api.stream("/chat", json!({"texto": "oi, Orion", "canal": "web"})));
    while let Some(ev) = s.next().await {
        match ev {
            Chat::Text(t) => texto.push_str(&t),
            Chat::Done { .. } => fim = true,
            Chat::Error(e) => panic!("erro no chat: {e}"),
            _ => {}
        }
    }
    assert!(fim && texto.contains("modelo de teste"), "texto: {texto:?}");

    // histórico e lista de conversas
    let h = api.get(&format!("/historico?sessao={sid}&limite=50")).await.unwrap();
    let msgs = map_history(&h);
    assert_eq!(msgs.len(), 2, "{h}");
    assert!(matches!(&msgs[0], orion_app::data::ChatMsg::User(t) if t == "oi, Orion"));
    let lista = api.get("/sessoes?limite=100").await.unwrap();
    let (convs, ativa) = map_sessions(&lista, &[], &[], 1.8e9);
    assert!(convs.iter().any(|c| c.id == sid));
    assert_eq!(ativa.as_deref(), Some(sid.as_str()));

    // listas que o app lê
    for p in ["/approvals", "/rotinas", "/projects", "/facts", "/resultados", "/painel", "/budget", "/modo", "/grafo/completo"] {
        api.get(p).await.unwrap_or_else(|e| panic!("{p}: {e}"));
    }

    // sair derruba a sessão
    api.logout().await;
    assert!(!api.auth_ok().await);
}
