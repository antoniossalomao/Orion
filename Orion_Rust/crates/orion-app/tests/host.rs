//! O app sobe o `orion serve` sozinho e o encerra. Só roda com `ORION_BIN` apontando para o binário.
use orion_app::{host, net::Api};

#[tokio::test]
async fn sobe_e_encerra_o_servidor() {
    if std::env::var_os("ORION_BIN").is_none() {
        eprintln!("ORION_BIN ausente: teste ignorado");
        return;
    }
    let dir = std::env::temp_dir().join(format!("orion-host-{}", std::process::id()));
    std::env::set_var("ORION_DATA_DIR", &dir);
    let (url, tk) = host::iniciar().expect("inicia");
    assert!(url.starts_with("http://127.0.0.1:"));
    let api = Api::new(&url, Some(tk));
    let mut ok = false;
    for _ in 0..60 {
        if api.health().await.is_ok() {
            ok = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    assert!(ok, "servidor não respondeu: {:?}", host::morreu());
    assert!(api.auth_ok().await, "o token aleatório vale como login");
    host::encerrar();
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert!(api.health().await.is_err(), "servidor ainda no ar depois de encerrar");
    let _ = std::fs::remove_dir_all(dir);
}
