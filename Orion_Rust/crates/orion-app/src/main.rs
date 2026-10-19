use iced::{window, Size};
use orion_app::app::App;
use orion_app::theme;

fn main() -> iced::Result {
    if std::env::var_os("ICED_BACKEND").is_none() {
        std::env::set_var("ICED_BACKEND", "tiny-skia");
    }
    let mut win = window::Settings {
        size: Size::new(1440.0, 900.0),
        min_size: Some(Size::new(760.0, 520.0)),
        position: window::Position::Centered,
        decorations: false,
        transparent: false,
        ..Default::default()
    };
    win.platform_specific = Default::default();
    let mut app = iced::application(App::boot, App::update, App::view).title(App::title).theme(App::theme).subscription(App::subscription).scale_factor(App::scale_factor).window(win).default_font(theme::UI).antialiasing(true);
    for f in theme::fonts() {
        app = app.font(f);
    }
    app.run()
}
