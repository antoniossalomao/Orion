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
