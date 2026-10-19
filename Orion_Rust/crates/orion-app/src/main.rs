use orion_app::net::Api;

/// `orion-app` (desenvolvimento): sobe o `orion serve` sozinho e o encerra ao sair; com `ORION_URL`
/// usa o servidor indicado; `--demo` abre com os dados de exemplo. O produto é o `orion.exe` (um só
/// executável, com o servidor dentro): `orion app`.
fn main() -> iced::Result {
    let demo = std::env::args().any(|a| a == "--demo");
    let cena = std::env::args().skip_while(|a| a != "--cena").nth(1);
    let api = if demo {
        None
    } else if std::env::var_os("ORION_URL").is_none() && std::env::var_os("ORION_ADMIN_TOKEN").is_none() {
        match orion_app::host::iniciar() {
            Ok((url, tk)) => Some(Api::new(&url, Some(tk))),
            Err(_) => Some(Api::from_env()),
        }
    } else {
        Some(Api::from_env())
    };
    let r = orion_app::run::run_ui(api, cena);
    orion_app::host::encerrar();
    r
}
