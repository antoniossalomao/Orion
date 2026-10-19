//! `orion app`: um só executável, um só processo e **nenhuma rede**. O núcleo roda aqui dentro e a
//! janela chama as rotas direto (sem socket, porta ou HTTP de verdade); o token é aleatório, só vive
//! na memória e serve apenas para a mesma checagem de login das rotas.
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{header, Method, Request};
use futures::{future::BoxFuture, StreamExt};
use orion_app::net::{Corpo, Local, Resposta};
use orion_core::config::Settings;
use std::time::Duration;
use tower::ServiceExt;

struct Interno {
    router: axum::Router,
    st: crate::estado::Estado,
    mostrar: std::path::PathBuf,
}

impl Local for Interno {
    fn chamar(&self, metodo: &str, caminho: &str, corpo: Option<Vec<u8>>, bearer: Option<String>) -> BoxFuture<'static, Result<Resposta, String>> {
        let router = self.router.clone();
        let (metodo, caminho) = (metodo.to_string(), caminho.to_string());
        Box::pin(async move {
            let mut req = Request::builder().method(Method::from_bytes(metodo.as_bytes()).map_err(|e| e.to_string())?).uri(&caminho).header(header::HOST, "127.0.0.1").header(header::ORIGIN, "http://127.0.0.1");
            if let Some(b) = bearer {
                req = req.header(header::AUTHORIZATION, format!("Bearer {b}"));
            }
            let tem_corpo = corpo.is_some();
            if tem_corpo {
                req = req.header(header::CONTENT_TYPE, "application/json");
            }
            let mut req = req.body(corpo.map(Body::from).unwrap_or_else(Body::empty)).map_err(|e| e.to_string())?;
            req.extensions_mut().insert(ConnectInfo(std::net::SocketAddr::from(([127, 0, 0, 1], 0))));
            let resp = router.oneshot(req).await.map_err(|e| e.to_string())?;
            let status = resp.status().as_u16();
            let set_cookie = resp.headers().get_all(header::SET_COOKIE).iter().filter_map(|v| v.to_str().ok()).find(|c| c.starts_with(orion_app::net::COOKIE)).map(String::from);
            let corpo: Corpo = resp.into_body().into_data_stream().map(|c| c.map(|b| b.to_vec()).map_err(|e| e.to_string())).boxed();
            Ok(Resposta { status, set_cookie, corpo })
        })
    }

    fn acordar(&self) -> bool {
        let voz = self.st.desktop_wake.swap(false, std::sync::atomic::Ordering::SeqCst);
        // outra instância pediu para mostrar a janela: deixa um arquivo, sem socket nem porta
        let outra = self.mostrar.exists() && std::fs::remove_file(&self.mostrar).is_ok();
        voz || outra
    }

    fn panico(&self) -> Result<String, String> {
        match self.st.modos.entrar_panico("atalho_desktop") {
            Ok(_) => Ok("Pânico ligado. Entre na conta e retome nas configurações.".into()),
            Err(e) => Err(e.to_string()),
        }
    }
}

fn token_aleatorio() -> String {
    format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple())
}

/// Só uma cópia do Orion por pasta de dados: a segunda pede (por arquivo) que a primeira mostre a
/// janela e sai. A trava solta sozinha quando o processo termina.
struct Trava(#[allow(dead_code)] std::fs::File);

fn instancia_unica(dados: &std::path::Path) -> anyhow::Result<Option<Trava>> {
    std::fs::create_dir_all(dados)?;
    let f = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(dados.join("orion.lock"))?;
    match f.try_lock() {
        Ok(()) => Ok(Some(Trava(f))),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(std::fs::TryLockError::Error(e)) => Err(e.into()),
    }
}

/// O `orion.exe` é de console (a CLI precisa); ao abrir a janela solta o console para não ficar
/// uma tela preta junto do app.
#[cfg(windows)]
fn soltar_console() {
    #[link(name = "kernel32")]
    extern "system" {
        fn FreeConsole() -> i32;
    }
    unsafe {
        FreeConsole();
    }
}

/// Modo único do Orion: janela nativa + núcleo no mesmo processo, sem rede. `fundo` abre escondido
/// na bandeja (início com o Windows); clicar na bandeja ou abrir de novo mostra a janela.
pub fn run(mut s: Settings, fundo: bool) -> anyhow::Result<i32> {
    let dados = s.data_dir.clone();
    let mostrar = dados.join("orion.mostrar");
    let Some(_trava) = instancia_unica(&dados)? else {
        let _ = std::fs::write(&mostrar, b"1");
        return Ok(0);
    };
    #[cfg(windows)]
    soltar_console();
    if s.admin_token.is_empty() {
        s.admin_token = token_aleatorio();
    }
    let token = s.admin_token.clone();
    #[cfg(windows)]
    let vigiar = s.clone();
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    let state = runtime.block_on(crate::estado::montar(s))?;
    #[cfg(windows)]
    crate::cli_db::watch_screen_lock(&vigiar, state.auth.clone());
    let telegram = {
        let _g = runtime.enter();
        crate::iniciar_fundo(&state)
    };
    let api = orion_app::net::Api::local(std::sync::Arc::new(Interno { router: crate::rotas::router(state.clone()), st: state.clone(), mostrar: mostrar.clone() }), token);
    let _ = std::fs::remove_file(&mostrar);
    let r = orion_app::run::run_ui(Some(api), None, fundo);
    runtime.block_on(async {
        if tokio::time::timeout(Duration::from_secs(10), crate::encerrar_fundo(&state, telegram)).await.is_err() {
            tracing::warn!("encerramento do núcleo passou de 10 s");
        }
    });
    r?;
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use orion_app::net::{Api, Chat};
    use serde_json::json;

    /// Chat de ponta a ponta pelo caminho sem rede. Só roda com um LLM de teste em `ORION_TEST_GATEWAY`
    /// (ex.: http://127.0.0.1:9099/v1, modelo `mock`).
    #[tokio::test(flavor = "multi_thread")]
    async fn chat_sem_rede_passa_pelas_rotas() {
        let Ok(gw) = std::env::var("ORION_TEST_GATEWAY") else { return };
        let dir = std::env::temp_dir().join(format!("orion-appmode-{}", std::process::id()));
        std::env::set_var("ORION_DATA_DIR", &dir);
        std::env::set_var("ORION_GATEWAY_URL", gw);
        std::env::set_var("ORION_GATEWAY_MODEL", "mock");
        let mut s = Settings::load().expect("config");
        s.admin_token = token_aleatorio();
        let token = s.admin_token.clone();
        let st = crate::estado::montar(s).await.expect("estado");
        let api = Api::local(std::sync::Arc::new(Interno { router: crate::rotas::router(st.clone()), st, mostrar: dir.join("mostrar") }), token);
        api.health().await.expect("health");
        assert!(api.auth_ok().await, "o token vale como login");
        let sessao = api.post("/sessoes", json!({})).await.unwrap();
        assert!(sessao["sessao_id"].as_str().is_some());
        let (mut texto, mut fim) = (String::new(), false);
        let mut ev = Box::pin(api.stream("/chat", json!({"texto": "oi, Orion", "canal": "web"})));
        while let Some(e) = ev.next().await {
            match e {
                Chat::Text(t) => texto.push_str(&t),
                Chat::Done { .. } => fim = true,
                Chat::Error(e) => panic!("erro no chat: {e}"),
                _ => {}
            }
        }
        assert!(fim && texto.contains("modelo de teste"), "texto: {texto:?}");
        let sem_token = Api::local({ let st2 = crate::estado::montar(Settings::load().unwrap()).await.unwrap(); std::sync::Arc::new(Interno { router: crate::rotas::router(st2.clone()), st: st2, mostrar: dir.join("mostrar2") }) }, "token-errado-0123456789".into());
        assert!(!sem_token.auth_ok().await, "token errado não autentica");
        let _ = std::fs::remove_dir_all(dir);
    }
}
