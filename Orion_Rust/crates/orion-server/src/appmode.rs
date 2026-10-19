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

struct Interno(axum::Router);

impl Local for Interno {
    fn chamar(&self, metodo: &str, caminho: &str, corpo: Option<Vec<u8>>, bearer: Option<String>) -> BoxFuture<'static, Result<Resposta, String>> {
        let router = self.0.clone();
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
}

fn token_aleatorio() -> String {
    format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple())
}

pub fn run(mut s: Settings) -> anyhow::Result<i32> {
    if s.admin_token.is_empty() {
        s.admin_token = token_aleatorio();
    }
    let token = s.admin_token.clone();
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    let state = runtime.block_on(crate::estado::montar(s))?;
    let telegram = {
        let _g = runtime.enter();
        crate::iniciar_fundo(&state)
    };
    let api = orion_app::net::Api::local(std::sync::Arc::new(Interno(crate::rotas::router(state.clone()))), token);
    let r = orion_app::run::run_ui(Some(api), None);
    runtime.block_on(async {
        if tokio::time::timeout(Duration::from_secs(10), crate::encerrar_fundo(&state, telegram)).await.is_err() {
            tracing::warn!("encerramento do núcleo passou de 10 s");
        }
    });
    r?;
    Ok(0)
}
