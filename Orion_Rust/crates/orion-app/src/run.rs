//! Abre a janela do app: com `Api` (servidor local) ou com os dados de exemplo (`demo`).
use crate::app::{App, Msg};
use crate::net::Api;
use crate::theme;
use iced::{window, Size, Task};

/// `fundo`: começa com a janela escondida (início com o Windows); a bandeja a reabre.
pub fn run_ui(api: Option<Api>, cena: Option<String>, fundo: bool) -> iced::Result {
    let hardware = if let Ok(backend) = std::env::var("ICED_BACKEND") {
        backend != "tiny-skia"
    } else {
        let instance = iced::wgpu::Instance::new(&iced::wgpu::InstanceDescriptor::from_env_or_default());
        let hardware = instance.enumerate_adapters(iced::wgpu::Backends::all()).iter().any(|adapter| {
            let info = adapter.get_info();
            matches!(info.device_type, iced::wgpu::DeviceType::DiscreteGpu | iced::wgpu::DeviceType::IntegratedGpu | iced::wgpu::DeviceType::VirtualGpu)
                && !info.name.contains("Microsoft Basic Render Driver")
        });
        std::env::set_var("ICED_BACKEND", if hardware { "wgpu" } else { "tiny-skia" });
        hardware
    };
    let mut win = window::Settings {
        size: Size::new(1440.0, 900.0),
        min_size: Some(Size::new(760.0, 520.0)),
        position: window::Position::Centered,
        decorations: false,
        transparent: false,
        visible: !fundo,
        ..Default::default()
    };
    win.platform_specific = Default::default();
    let boot = move || {
        let id = window::oldest().map(Msg::Win);
        if let Some(api) = api.clone() {
            let (mut a, t) = App::live(api);
            a.fundo = fundo;
            a.window_active = !fundo;
            return (a, Task::batch([t, id]));
        }
        let (mut a, t) = App::boot();
        a.fundo = fundo;
        a.window_active = !fundo;
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
                let _ = a.update(Msg::Pop(crate::app::Pop::Model));
            }
            Some("editor") => drop(a.update(Msg::EditorOpen("03 Recursos/Orion/Memória/Nota.md".into()))),
            _ => {}
        }
        (a, Task::batch([t, id]))
    };
    let mut app = iced::application(boot, App::update, App::view).title(App::title).theme(App::theme).subscription(App::subscription).scale_factor(App::scale_factor).window(win).default_font(theme::UI).antialiasing(hardware).exit_on_close_request(false);
    for f in theme::fonts() {
        app = app.font(f);
    }
    app.run()
}
