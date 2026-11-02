//! REGRAS.md: ponte de desktop só abre e cola o que você aprovou.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesktopAction {
    MiniBar,
    Budget,
    Panic,
    /// Ctrl+Alt+O: captura rápida.
    QuickCapture,
    /// Ctrl+Alt+I: "Isso?".
    Isso,
    Exit,
}
pub mod despertar;
#[cfg(windows)]
mod janela;
#[cfg(windows)]
pub use janela::{allow_foreground, main_window, set_mica, style_main_window, Estilo};
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::NativeShell;

/// Fora do Windows não há DWM: tudo é no-op.
#[cfg(not(windows))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Estilo {
    pub aplicado: bool,
    pub mica: bool,
}
#[cfg(not(windows))]
pub fn style_main_window(_fundo: (u8, u8, u8), _mica: bool) -> Estilo {
    Estilo::default()
}
#[cfg(not(windows))]
pub fn set_mica(_ligado: bool) -> bool {
    false
}
#[cfg(not(windows))]
pub fn allow_foreground() {}
#[cfg(not(windows))]
pub fn main_window() -> Option<isize> {
    None
}

/// A sessão está bloqueada (Win+L)? `None` se o Windows não soube dizer.
///
/// `WTSSessionInfoEx` é a resposta oficial: `SessionFlags` vale 0 com a estação bloqueada e 1 com ela
/// desbloqueada. `OpenInputDesktop` sozinho erra: também falha no prompt do UAC e em Ctrl+Alt+Del,
/// que não são bloqueio.
#[cfg(windows)]
fn wts_locked() -> Option<bool> {
    #[link(name = "wtsapi32")]
    extern "system" {
        fn WTSQuerySessionInformationW(server: isize, session: u32, classe: i32, buffer: *mut *mut u16, bytes: *mut u32) -> i32;
        fn WTSFreeMemory(memoria: *mut std::ffi::c_void);
    }
    const WTS_CURRENT_SESSION: u32 = 0xFFFF_FFFF;
    const WTS_SESSION_INFO_EX: i32 = 25;
    unsafe {
        let mut buffer: *mut u16 = std::ptr::null_mut();
        let mut bytes = 0u32;
        if WTSQuerySessionInformationW(0, WTS_CURRENT_SESSION, WTS_SESSION_INFO_EX, &mut buffer, &mut bytes) == 0 || buffer.is_null() {
            return None;
        }
        // WTSINFOEXW: `Level` em 0, e o nível 1 começa em 8 (alinhamento de 8 por causa dos LARGE_INTEGER);
        // `SessionFlags` é o terceiro campo do nível 1, em 8 + 8 = 16. Só lê o que o Windows devolveu.
        let base = buffer as *const u8;
        let resposta = if (bytes as usize) >= 20 && std::ptr::read_unaligned(base as *const u32) == 1 {
            Some(std::ptr::read_unaligned(base.add(16) as *const i32))
        } else {
            None
        };
        WTSFreeMemory(buffer as *mut std::ffi::c_void);
        match resposta {
            Some(0) => Some(true),
            Some(1) => Some(false),
            _ => None,
        }
    }
}

/// O Windows está bloqueado (Win+L ou tela de login)? Usa o WTS; se ele não responder, cai no
/// desktop de entrada (no bloqueio é o do Winlogon e `OpenInputDesktop` falha para a sessão do usuário).
#[cfg(windows)]
pub fn session_locked() -> bool {
    if let Some(travado) = wts_locked() {
        return travado;
    }
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

/// F01.11: o ponto (coordenadas da área de trabalho) cai em algum monitor conectado agora?
/// Janela salva num monitor que foi desconectado não deve reabrir fora da tela.
#[cfg(windows)]
pub fn point_on_monitor(x: i32, y: i32) -> bool {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::Graphics::Gdi::{MonitorFromPoint, MONITOR_DEFAULTTONULL};
    // SAFETY: MonitorFromPoint só lê o ponto passado por valor; não há ponteiro envolvido.
    !unsafe { MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONULL) }.is_null()
}
/// Fora do Windows não dá para saber: aceita e deixa o gerenciador de janelas ajustar.
#[cfg(not(windows))]
pub fn point_on_monitor(_x: i32, _y: i32) -> bool {
    true
}
