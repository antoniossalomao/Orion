//! Ponto de entrada do executável `orion-setup` (`Orion Setup.exe`).
//!
//! Abre uma janela sem borda nativa de 960×600 px centralizada na tela,
//! sem abrir console (`windows_subsystem = "windows"`).

#![cfg_attr(windows, windows_subsystem = "windows")]

use iced::window;
use iced::{Size, Theme};
use orion_setup::app::{SetupApp, FONT_SANS};
use orion_setup::system::PcInfo;
use orion_setup::theme;

fn main() -> iced::Result {
    // Usa o renderizador `tiny-skia` por padrão para abertura instantânea (< 50 ms).
    if std::env::var_os("ICED_BACKEND").is_none() {
        std::env::set_var("ICED_BACKEND", "tiny-skia");
    }

    let args: Vec<String> = std::env::args().collect();
    let real_install = args.iter().any(|a| a == "--real-install");
    let use_proto_hw = args.iter().any(|a| a == "--prototype-hw");

    let window_settings = window::Settings {
        size: Size::new(theme::WINDOW_W, theme::WINDOW_H),
        position: window::Position::Centered,
        min_size: Some(Size::new(theme::WINDOW_W, theme::WINDOW_H)),
        max_size: Some(Size::new(theme::WINDOW_W, theme::WINDOW_H)),
        resizable: false,
        decorations: false,
        transparent: false,
        visible: true,
        level: window::Level::Normal,
        ..Default::default()
    };

    iced::application(
        move || {
            let pc = if use_proto_hw {
                PcInfo::prototype_reference()
            } else {
                PcInfo::detect()
            };
            let focus_task = window::latest().and_then(window::gain_focus);
            (SetupApp::new_with_sound(pc, !real_install), focus_task)
        },
        SetupApp::update,
        SetupApp::view,
    )
    .title(SetupApp::title)
    .theme(SetupApp::theme)
    .subscription(SetupApp::subscription)
    .window(window_settings)
    .default_font(FONT_SANS)
    .antialiasing(true)
    .run()
}

const _: fn(&SetupApp) -> Theme = SetupApp::theme;
