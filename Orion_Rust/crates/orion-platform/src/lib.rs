//! REGRAS.md: ponte de desktop só abre e cola o que você aprovou.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesktopAction {
    MiniBar,
    Budget,
    Panic,
    Exit,
}
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::NativeShell;

/// O Windows está bloqueado (Win+L ou tela de login)? Com a estação bloqueada, o desktop de entrada
/// é o do Winlogon e `OpenInputDesktop` falha para quem roda na sessão do usuário.
#[cfg(windows)]
pub fn session_locked() -> bool {
    #[link(name = "user32")]
    extern "system" {
        fn OpenInputDesktop(flags: u32, inherit: i32, access: u32) -> isize;
        fn CloseDesktop(desktop: isize) -> i32;
    }
    const DESKTOP_SWITCHDESKTOP: u32 = 0x0100;
    unsafe {
        let h = OpenInputDesktop(0, 0, DESKTOP_SWITCHDESKTOP);
        if h == 0 {
            return true;
        }
        CloseDesktop(h);
        false
    }
}

#[cfg(not(windows))]
pub fn session_locked() -> bool {
    false
}
