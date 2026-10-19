use orion_app::net::Api;

/// `orion-app` (desenvolvimento): `--demo` abre com os dados de exemplo; com `ORION_URL` fala com um
/// `orion serve` (compilado com `--features http`). O produto é o `orion.exe`: um só executável, com o
/// núcleo dentro e sem rede.
fn main() -> iced::Result {
    let demo = std::env::args().any(|a| a == "--demo");
    let cena = std::env::args().skip_while(|a| a != "--cena").nth(1);
    let api = if demo { None } else { Some(Api::from_env()) };
    orion_app::run::run_ui(api, cena, false)
}
