use crate::DesktopAction;
use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
};
use tray_icon::{
    menu::{Menu, MenuEvent, MenuId, MenuItem},
    Icon, TrayIcon, TrayIconBuilder,
};

/// Vive na thread da UI; iced fornece o laço de mensagens do Windows.
pub struct NativeShell {
    _tray: TrayIcon,
    keys: GlobalHotKeyManager,
    mini: HotKey,
    panic: HotKey,
    captura: HotKey,
    isso: HotKey,
    menu_ids: Vec<(MenuId, DesktopAction)>,
}
impl NativeShell {
    pub fn new() -> anyhow::Result<Self> {
        let keys = GlobalHotKeyManager::new()?;
        let mini = HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::Space);
        let panic = HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyP);
        keys.register(mini).map_err(|_| anyhow::anyhow!("Ctrl+Alt+Espaço já está em uso"))?;
        if keys.register(panic).is_err() {
            let _ = keys.unregister(mini);
            anyhow::bail!("Ctrl+Alt+P já está em uso")
        }
        let ca = Some(Modifiers::CONTROL | Modifiers::ALT);
        let (captura, isso) = (HotKey::new(ca, Code::KeyO), HotKey::new(ca, Code::KeyI));
        // captura rápida e "Isso?" são extras: se a combinação estiver ocupada, o resto segue funcionando
        let _ = keys.register(captura);
        let _ = keys.register(isso);
        let menu = Menu::new();
        let mut menu_ids = Vec::new();
        for (label, action) in [
            ("Abrir mini barra", DesktopAction::MiniBar),
            ("Captura rápida (Ctrl+Alt+O)", DesktopAction::QuickCapture),
            ("Isso? (Ctrl+Alt+I)", DesktopAction::Isso),
            ("Orçamento e rotinas", DesktopAction::Budget),
            ("Pânico — cortar tudo", DesktopAction::Panic),
            ("Sair do Orion", DesktopAction::Exit),
        ] {
            let item = MenuItem::new(label, true, None);
            menu.append(&item)?;
            menu_ids.push((item.id().clone(), action));
        }
        let mut rgba = vec![0u8; 32 * 32 * 4];
        for (x, y) in [(8, 9), (16, 16), (24, 23)] {
            for dx in -2i32..=2 {
                for dy in -2i32..=2 {
                    if dx.abs() + dy.abs() <= 2 {
                        let i = ((y + dy) * 32 + x + dx) as usize * 4;
                        rgba[i..i + 4].copy_from_slice(&[170, 195, 255, 255]);
                    }
                }
            }
        }
        let tray = TrayIconBuilder::new()
            .with_tooltip("Orion — Ctrl+Alt+Espaço · Ctrl+Alt+O · Ctrl+Alt+I · Ctrl+Alt+P")
            .with_icon(Icon::from_rgba(rgba, 32, 32)?)
            .with_menu(Box::new(menu))
            .build()?;
        Ok(Self { _tray: tray, keys, mini, panic, captura, isso, menu_ids })
    }
    pub fn poll(&self) -> Vec<DesktopAction> {
        let mut actions = Vec::new();
        while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
            if event.state != HotKeyState::Pressed {
                continue;
            }
            if event.id == self.mini.id() {
                actions.push(DesktopAction::MiniBar)
            }
            if event.id == self.panic.id() {
                actions.push(DesktopAction::Panic)
            }
            if event.id == self.captura.id() {
                actions.push(DesktopAction::QuickCapture)
            }
            if event.id == self.isso.id() {
                actions.push(DesktopAction::Isso)
            }
        }
        while let Ok(event) = tray_icon::TrayIconEvent::receiver().try_recv() {
            if matches!(event, tray_icon::TrayIconEvent::Click { button: tray_icon::MouseButton::Left, button_state: tray_icon::MouseButtonState::Up, .. }) {
                actions.push(DesktopAction::MiniBar)
            }
        }
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if let Some((_, a)) = self.menu_ids.iter().find(|(id, _)| *id == event.id) {
                actions.push(*a)
            }
        }
        actions
    }
}
impl Drop for NativeShell {
    fn drop(&mut self) {
        let _ = self.keys.unregister(self.mini);
        let _ = self.keys.unregister(self.panic);
        let _ = self.keys.unregister(self.captura);
        let _ = self.keys.unregister(self.isso);
    }
}
