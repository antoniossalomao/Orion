use iced::{window, Size};
use orion_app::app::{App, Msg};
use orion_app::net::Api;
use orion_app::theme;

/// `orion-app` sobe o `orion serve` sozinho (só em 127.0.0.1) e o encerra ao sair; com `ORION_URL`
/// usa o servidor indicado.
/// `orion-app --demo` abre com os dados de exemplo do protótipo, sem servidor.
fn main() -> iced::Result {
    if std::env::var_os("ICED_BACKEND").is_none() {
        std::env::set_var("ICED_BACKEND", "tiny-skia");
    }
    let demo = std::env::args().any(|a| a == "--demo");
    let mut win = window::Settings {
        size: Size::new(1440.0, 900.0),
        min_size: Some(Size::new(760.0, 520.0)),
        position: window::Position::Centered,
        decorations: false,
        transparent: false,
        ..Default::default()
    };
    win.platform_specific = Default::default();
    // `--cena config|paleta|modelo|editor|conversa`: abre já num estado (para conferir a janela de verdade)
    let cena = std::env::args().skip_while(|a| a != "--cena").nth(1);
    let boot = move || {
        if !demo {
            if std::env::var_os("ORION_URL").is_none() && std::env::var_os("ORION_ADMIN_TOKEN").is_none() {
                match orion_app::host::iniciar() {
                    Ok((url, tk)) => return App::live(Api::new(&url, Some(tk))),
                    Err(e) => {
                        let (mut a, t) = App::live(Api::from_env());
                        a.gate_err = e;
                        return (a, t);
                    }
                }
            }
            return App::live(Api::from_env());
        }
        let (mut a, t) = App::boot();
        if cena.is_some() {
            a.logged = true;
            a.hour = Some(9);
            let _ = a.update(Msg::OpenConv(0));
        }
        match cena.as_deref() {
            Some("config") => drop(a.update(Msg::OpenSettings(None))),
            Some("paleta") => drop(a.update(Msg::PalOpen)),
            Some("modelo") => {
                let _ = a.update(Msg::NewChat);
                let _ = a.update(Msg::Pop(orion_app::app::Pop::Model));
            }
            Some("editor") => drop(a.update(Msg::EditorOpen("03 Recursos/Orion/Memória/Nota.md".into()))),
            _ => {}
        }
        (a, t)
    };
    let mut app = iced::application(boot, App::update, App::view).title(App::title).theme(App::theme).subscription(App::subscription).scale_factor(App::scale_factor).window(win).default_font(theme::UI).antialiasing(true);
    for f in theme::fonts() {
        app = app.font(f);
    }
    let r = app.run();
    orion_app::host::encerrar();
    r
}
