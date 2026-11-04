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
    /// Na ordem de `GLOBAL_ORDER`: mini barra, captura rápida, "Isso?" e pânico. Troca em tempo de execução.
    hot: std::sync::Arc<std::sync::Mutex<[HotKey; 4]>>,
    menu_ids: Vec<(MenuId, DesktopAction)>,
}
/// Ordem dos atalhos globais em `NativeShell::rebind`.
pub const GLOBAL_ORDER: [&str; 4] = ["mini", "captura", "isso", "panico"];

/// Converte `Ctrl+Alt+Space` em atalho do sistema. Aceita letras, dígitos, F1–F12 e Espaço, com Ctrl e/ou Alt (e Shift).
pub fn parse_combo(combo: &str) -> Option<HotKey> {
    let mut mods = Modifiers::empty();
    let mut code = None;
    for part in combo.split('+') {
        match part.trim() {
            "Ctrl" => mods |= Modifiers::CONTROL,
            "Alt" => mods |= Modifiers::ALT,
            "Shift" => mods |= Modifiers::SHIFT,
            "Space" => code = Some(Code::Space),
            key if key.len() == 1 && key.as_bytes()[0].is_ascii_alphabetic() => {
                code = Some(letter(key.as_bytes()[0].to_ascii_uppercase())?)
            }
            key if key.len() == 1 && key.as_bytes()[0].is_ascii_digit() => code = Some(digit(key.as_bytes()[0])?),
            key if key.starts_with('F') => code = Some(function(key[1..].parse().ok()?)?),
            _ => return None,
        }
    }
    // Sem Ctrl ou Alt (exceto tecla de função) o atalho roubaria a digitação.
    if !mods.intersects(Modifiers::CONTROL | Modifiers::ALT) && !matches!(code?, Code::F1 | Code::F2 | Code::F3 | Code::F4 | Code::F5 | Code::F6 | Code::F7 | Code::F8 | Code::F9 | Code::F10 | Code::F11 | Code::F12) {
        return None;
    }
    Some(HotKey::new(Some(mods).filter(|m| !m.is_empty()), code?))
}
fn letter(c: u8) -> Option<Code> {
    use Code::*;
    const L: [Code; 26] = [KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM, KeyN, KeyO, KeyP, KeyQ, KeyR, KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ];
    L.get((c - b'A') as usize).copied()
}
fn digit(c: u8) -> Option<Code> {
    use Code::*;
    const D: [Code; 10] = [Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9];
    D.get((c - b'0') as usize).copied()
}
fn function(n: u8) -> Option<Code> {
    use Code::*;
    const F: [Code; 12] = [F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12];
    F.get((n as usize).checked_sub(1)?).copied()
}

impl NativeShell {
    pub fn new() -> anyhow::Result<Self> {
        Self::new_with_labels(["Abrir mini barra", "Captura rápida", "Isso?", "Orçamento e rotinas", "Pânico — cortar tudo", "Sair do Orion"])
    }
    pub fn new_with_labels(labels: [&str; 6]) -> anyhow::Result<Self> {
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
            (labels[0], DesktopAction::MiniBar),
            (labels[1], DesktopAction::QuickCapture),
            (labels[2], DesktopAction::Isso),
            (labels[3], DesktopAction::Budget),
            (labels[4], DesktopAction::Panic),
            (labels[5], DesktopAction::Exit),
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
            .with_tooltip(format!("Orion — {}", labels[0]))
            .with_icon(Icon::from_rgba(rgba, 32, 32)?)
            .with_menu(Box::new(menu))
            .build()?;
        let hot = std::sync::Arc::new(std::sync::Mutex::new([mini, captura, isso, panic]));
        Ok(Self { _tray: tray, keys, hot, menu_ids })
    }
    /// Troca um atalho global (índice de `GLOBAL_ORDER`). Se a combinação estiver em uso por outro app, volta ao anterior.
    pub fn rebind(&self, index: usize, combo: &str) -> anyhow::Result<()> {
        let novo = parse_combo(combo).ok_or_else(|| anyhow::anyhow!("combinação não suportada"))?;
        let mut hot = self.hot.lock().unwrap_or_else(|e| e.into_inner());
        let antigo = *hot.get(index).ok_or_else(|| anyhow::anyhow!("atalho desconhecido"))?;
        if antigo.id() == novo.id() {
            return Ok(());
        }
        let _ = self.keys.unregister(antigo);
        if self.keys.register(novo).is_err() {
            let _ = self.keys.register(antigo);
            anyhow::bail!("combinação em uso por outro aplicativo");
        }
        hot[index] = novo;
        Ok(())
    }
    pub fn set_shortcut_enabled(&self,on:bool)->anyhow::Result<()>{
        let mini = self.hot.lock().unwrap_or_else(|e| e.into_inner())[0];
        if on{self.keys.register(mini)?;}else{self.keys.unregister(mini)?;}Ok(())
    }
    pub fn poller(&self) -> std::sync::Arc<dyn Fn() -> Vec<DesktopAction> + Send + Sync> {
        let hot = self.hot.clone();
        let menu_ids = self.menu_ids.clone();
        std::sync::Arc::new(move || {
        let [mini, captura, isso, panic] = hot.lock().unwrap_or_else(|e| e.into_inner()).map(|k| k.id());
        let mut actions = Vec::new();
        while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
            if event.state != HotKeyState::Pressed {
                continue;
            }
            if event.id == mini {
                actions.push(DesktopAction::MiniBar)
            }
            if event.id == panic {
                actions.push(DesktopAction::Panic)
            }
            if event.id == captura {
                actions.push(DesktopAction::QuickCapture)
            }
            if event.id == isso {
                actions.push(DesktopAction::Isso)
            }
        }
        while let Ok(event) = tray_icon::TrayIconEvent::receiver().try_recv() {
            if matches!(event, tray_icon::TrayIconEvent::Click { button: tray_icon::MouseButton::Left, button_state: tray_icon::MouseButtonState::Up, .. }) {
                actions.push(DesktopAction::MiniBar)
            }
        }
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if let Some((_, a)) = menu_ids.iter().find(|(id, _)| *id == event.id) {
                actions.push(*a)
            }
        }
        actions
        })
    }
    pub fn poll(&self) -> Vec<DesktopAction> { self.poller()() }
}
impl Drop for NativeShell {
    fn drop(&mut self) {
        for key in *self.hot.lock().unwrap_or_else(|e| e.into_inner()) {
            let _ = self.keys.unregister(key);
        }
    }
}
