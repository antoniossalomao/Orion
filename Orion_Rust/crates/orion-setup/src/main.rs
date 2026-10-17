//! Ponto de entrada do executável `orion-setup` (`Orion Setup.exe`).
//!
//! Abre uma janela sem borda nativa de 960×600 px centralizada na tela.
//! Suporta `--real-install` e `--prototype-hw`.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use iced::window;
use iced::{Size, Theme};
use orion_setup::app::SetupApp;
use orion_setup::system::PcInfo;
use orion_setup::theme;

fn main() -> iced::Result {
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
        ..Default::default()
    };

    iced::application(
        move || {
            let pc = if use_proto_hw {
                PcInfo::prototype_reference()
            } else {
                PcInfo::detect()
            };
            (SetupApp::new_with_sound(pc, !real_install), iced::Task::none())
        },
        SetupApp::update,
        SetupApp::view,
    )
    .title(SetupApp::title)
    .theme(SetupApp::theme)
    .subscription(SetupApp::subscription)
    .window(window_settings)
    .default_font(iced::Font::with_name("Segoe UI"))
    .antialiasing(true)
    .run()
}

const _: fn(&SetupApp) -> Theme = SetupApp::theme;
