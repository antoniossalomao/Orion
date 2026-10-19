//! `orion app`: um só executável. O servidor roda neste processo (só em 127.0.0.1, porta livre, token
//! aleatório que nunca sai da memória) e a janela do app fala com ele.
use orion_core::config::Settings;
use std::time::Duration;

fn token_aleatorio() -> String {
    format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple())
}

pub fn run(mut s: Settings) -> anyhow::Result<i32> {
    let escuta = std::net::TcpListener::bind(("127.0.0.1", 0))?;
    escuta.set_nonblocking(true)?;
    s.host = "127.0.0.1".into();
    s.port = escuta.local_addr()?.port();
    if s.admin_token.is_empty() {
        s.admin_token = token_aleatorio();
    }
    let (url, token) = (format!("http://127.0.0.1:{}", s.port), s.admin_token.clone());
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    let state = runtime.block_on(crate::estado::montar(s))?;
    let listener = {
        let _g = runtime.enter();
        tokio::net::TcpListener::from_std(escuta)?
    };
    let parada = state.clone();
    let (fim_tx, fim_rx) = tokio::sync::oneshot::channel::<()>();
    let mut tarefa = runtime.spawn(crate::serve_state(state, listener, async {
        let _ = fim_rx.await;
    }));
    let r = orion_app::run::run_ui(Some(orion_app::net::Api::new(&url, Some(token))), None);
    if let Some(e) = &parada.escuta {
        e.stop()
    }
    let _ = fim_tx.send(());
    runtime.block_on(async {
        if tokio::time::timeout(Duration::from_secs(10), &mut tarefa).await.is_err() {
            tarefa.abort();
        }
    });
    r?;
    Ok(0)
}
