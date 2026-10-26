//! Ponto de entrada do executável `orion-setup` (`Orion Setup.exe`).
//!
//! Abre uma janela sem borda nativa de 960×600 px centralizada na tela,
//! com cantos arredondados nativos do Windows 11 (`DWMWA_WINDOW_CORNER_PREFERENCE = 2`),
//! ícone das 3 estrelas do Cinturão com blur e sem abrir console (`windows_subsystem = "windows"`).

#![cfg_attr(windows, windows_subsystem = "windows")]

use iced::window;
use iced::{Size, Task, Theme};
use orion_setup::app::{SetupApp, FONT_SANS};
use orion_setup::sky::generate_belt_icon_rgba;
use orion_setup::system::PcInfo;
use orion_setup::theme;

#[cfg(windows)]
fn apply_rounded_corners_to_process_windows() {
    #[link(name = "user32")]
    extern "system" {
        fn GetActiveWindow() -> isize;
        fn GetForegroundWindow() -> isize;
    }

    #[link(name = "dwmapi")]
    extern "system" {
        fn DwmSetWindowAttribute(
            hwnd: isize,
            dw_attribute: u32,
            pv_attribute: *const std::ffi::c_void,
            cb_attribute: u32,
        ) -> i32;
    }

    unsafe {
        let mut hwnd = GetActiveWindow();
        if hwnd == 0 {
            hwnd = GetForegroundWindow();
        }
        if hwnd != 0 {
            // DWMWA_WINDOW_CORNER_PREFERENCE = 33, DWMWCP_ROUND = 2 (cantos arredondados Windows 11 + sombra DWM, respeitando DPI/escala)
            let round_pref: u32 = 2;
            let _ = DwmSetWindowAttribute(
                hwnd,
                33,
                &round_pref as *const u32 as *const std::ffi::c_void,
                4,
            );
        }
    }
}

fn main() -> iced::Result {
    // Usa o renderizador `tiny-skia` por padrão para abertura instantânea (< 50 ms).
    if std::env::var_os("ICED_BACKEND").is_none() {
        std::env::set_var("ICED_BACKEND", "tiny-skia");
    }

    let args: Vec<String> = std::env::args().collect();
    match orion_setup::register::handle_uninstall_args(&args) {
        Some(orion_setup::register::UninstallAction::Exit(code)) => std::process::exit(code),
        Some(orion_setup::register::UninstallAction::Ui(dir)) => return orion_setup::uninstall_ui::run(dir),
        None => {}
    }
    if let Some(i) = args.iter().position(|a| a == "--whats-new") {
        let dir = args.get(i + 1).map(std::path::PathBuf::from).unwrap_or_default();
        let english = args.iter().any(|a| a == "--lang=en");
        return orion_setup::whats_new_ui::run(dir, english);
    }
    let real_install = !args.iter().any(|a| a == "--preview" || a == "--prototype-hw");
    let use_proto_hw = args.iter().any(|a| a == "--prototype-hw");

    let icon_rgba = generate_belt_icon_rgba(64);
    let win_icon = window::icon::from_rgba(icon_rgba, 64, 64).ok();

    let window_settings = window::Settings {
        size: Size::new(theme::WINDOW_W, theme::WINDOW_H),
        position: window::Position::Centered,
        min_size: Some(Size::new(theme::WINDOW_W, theme::WINDOW_H)),
        max_size: Some(Size::new(theme::WINDOW_W, theme::WINDOW_H)),
        resizable: false,
        decorations: false,
        transparent: true,
        visible: true,
        level: window::Level::Normal,
        icon: win_icon,
        ..Default::default()
    };

    iced::application(
        move || {
            let pc = if use_proto_hw {
                PcInfo::prototype_reference()
            } else {
                PcInfo::detect()
            };
            let focus_task = window::latest().and_then(|id| {
                Task::batch([
                    window::gain_focus(id),
                    window::run(id, |_w| {
                        #[cfg(windows)]
                        apply_rounded_corners_to_process_windows();
                    })
                    .discard(),
                ])
            });
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
