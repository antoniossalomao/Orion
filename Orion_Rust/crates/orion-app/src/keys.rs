//! Atalhos globais do teclado (§25), tratados por assinatura de eventos.
//! Os atalhos da janela são personalizáveis com detecção de conflito (F01.22); os atalhos do sistema
//! (Ctrl+Alt+Espaço, P, O e I) e as teclas fixas (Enter, Esc, setas) só aparecem na lista como reservados.

use crate::app::Msg;
use iced::keyboard::{key::Named, Event, Key, Modifiers};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;

/// Ação personalizável: id estável, rótulo e atalho padrão.
pub struct Action {
    pub id: &'static str,
    pub label: &'static str,
    pub default: &'static str,
}

pub const ACTIONS: [Action; 4] = [
    Action { id: "palette", label: "Buscar conversas e notas", default: "Ctrl+K" },
    Action { id: "new_chat", label: "Nova conversa", default: "Ctrl+Shift+O" },
    Action { id: "sidebar", label: "Recolher barra lateral", default: "Ctrl+B" },
    Action { id: "settings", label: "Configurações", default: "Ctrl+," },
];

/// Atalhos que nenhuma ação pode tomar: sistema e teclas fixas.
pub const RESERVED: [(&str, &str); 8] = [
    ("Ctrl+Alt+Space", "Trazer a janela de qualquer lugar (mini barra)"),
    ("Ctrl+Alt+P", "Pânico: corta tudo"),
    ("Ctrl+Alt+O", "Captura rápida"),
    ("Ctrl+Alt+I", "Isso? (capturar a tela)"),
    ("Esc", "Fechar painel"),
    ("Enter", "Enviar"),
    ("ArrowUp", "Navegar na lista"),
    ("ArrowDown", "Navegar na lista"),
];

static OVERRIDES: RwLock<Option<HashMap<String, String>>> = RwLock::new(None);
static CAPTURING: AtomicBool = AtomicBool::new(false);

fn action(id: &str) -> Option<&'static Action> {
    ACTIONS.iter().find(|a| a.id == id)
}

/// Atalho em vigor para a ação (personalizado ou padrão).
pub fn effective(id: &str) -> String {
    let custom = OVERRIDES.read().ok().and_then(|g| g.as_ref().and_then(|m| m.get(id).cloned()));
    custom.or_else(|| action(id).map(|a| a.default.to_string())).unwrap_or_default()
}

/// Texto para exibir (`Ctrl+Shift+O` vira `Ctrl Shift O`).
pub fn display(combo: &str) -> String {
    let combo = if crate::i18n::english() { combo.to_string() } else { combo.replace("Space", "Espaço") };
    combo.replace('+', " ")
}

/// Motivo de uma recusa ao trocar ou restaurar um atalho.
#[derive(Debug, PartialEq, Eq)]
pub enum ShortcutError {
    Unknown,
    NeedsModifier(String),
    Taken(String, String),
    DefaultTaken(String, String),
}

impl ShortcutError {
    /// Frase para a interface, traduzida pelo catálogo Fluent.
    pub fn message(&self) -> String {
        use crate::i18n::{formatted, status};
        match self {
            Self::Unknown => String::new(),
            Self::NeedsModifier(c) => formatted("{} precisa de Ctrl ou Alt junto com a tecla", format!("{} precisa de Ctrl ou Alt junto com a tecla", display(c))),
            Self::Taken(c, who) => formatted("{} já é usado por: {}", format!("{} já é usado por: {}", display(c), status(who))),
            Self::DefaultTaken(c, who) => formatted("O atalho padrão ({}) já é usado por: {}", format!("O atalho padrão ({}) já é usado por: {}", display(c), status(who))),
        }
    }
}

/// Atalho aceito: precisa de Ctrl ou Alt (ou ser tecla de função) para não roubar a digitação.
fn typable(combo: &str) -> bool {
    combo.contains("Ctrl+") || combo.contains("Alt+") || (combo.starts_with('F') && combo[1..].chars().all(|c| c.is_ascii_digit()) && combo.len() > 1)
}

/// Quem já usa o atalho, se houver, excluindo a própria ação.
pub fn conflict(id: &str, combo: &str) -> Option<String> {
    if let Some((_, what)) = RESERVED.iter().find(|(k, _)| k.eq_ignore_ascii_case(combo)) {
        return Some((*what).to_string());
    }
    // Enter, Esc e setas valem em qualquer combinação: o campo de mensagem e as listas ignoram os modificadores.
    let key = combo.rsplit('+').next().unwrap_or("");
    if let Some((_, what)) = RESERVED.iter().find(|(k, _)| !k.contains('+') && k.eq_ignore_ascii_case(key)) {
        return Some((*what).to_string());
    }
    ACTIONS.iter().filter(|a| a.id != id).find(|a| effective(a.id).eq_ignore_ascii_case(combo)).map(|a| a.label.to_string())
}

/// Troca o atalho de uma ação. Recusa tecla sem Ctrl/Alt e conflito.
pub fn set(id: &str, combo: &str) -> Result<(), ShortcutError> {
    if action(id).is_none() {
        return Err(ShortcutError::Unknown);
    }
    if !typable(combo) {
        return Err(ShortcutError::NeedsModifier(combo.to_string()));
    }
    if let Some(who) = conflict(id, combo) {
        return Err(ShortcutError::Taken(combo.to_string(), who));
    }
    if let Ok(mut g) = OVERRIDES.write() {
        g.get_or_insert_with(HashMap::new).insert(id.to_string(), combo.to_string());
    }
    save();
    Ok(())
}

/// Volta ao atalho padrão. Se o padrão estiver tomado por outra ação, a restauração é recusada.
pub fn reset(id: &str) -> Result<(), ShortcutError> {
    let Some(a) = action(id) else { return Err(ShortcutError::Unknown) };
    if let Some(who) = conflict(id, a.default) {
        return Err(ShortcutError::DefaultTaken(a.default.to_string(), who));
    }
    if let Ok(mut g) = OVERRIDES.write() {
        if let Some(m) = g.as_mut() {
            m.remove(id);
        }
    }
    save();
    Ok(())
}

pub fn custom(id: &str) -> bool {
    OVERRIDES.read().ok().is_some_and(|g| g.as_ref().is_some_and(|m| m.contains_key(id)))
}

pub fn capturing() -> bool {
    CAPTURING.load(Ordering::Relaxed)
}

pub fn set_capturing(on: bool) {
    CAPTURING.store(on, Ordering::Relaxed);
}

fn file() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA").or_else(|| std::env::var_os("HOME"))?;
    Some(PathBuf::from(base).join("Orion").join("atalhos.txt"))
}

fn save() {
    let Some(path) = file() else { return };
    let body = OVERRIDES.read().ok().and_then(|g| g.as_ref().map(|m| {
        let mut rows: Vec<String> = m.iter().map(|(k, v)| format!("{k}={v}")).collect();
        rows.sort();
        rows.join("\n")
    })).unwrap_or_default();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, body);
}

/// Lê os atalhos personalizados do disco. Carrega todos e só depois descarta os inválidos ou duplicados,
/// para que trocas de atalho entre ações sobrevivam à reabertura, qualquer que seja a ordem do arquivo.
pub fn load() {
    let Some(path) = file() else { return };
    let Ok(text) = std::fs::read_to_string(path) else { return };
    let mut loaded: HashMap<String, String> = HashMap::new();
    for line in text.lines() {
        if let Some((id, combo)) = line.split_once('=') {
            let (id, combo) = (id.trim(), combo.trim());
            let reserved = RESERVED.iter().any(|(k, _)| k.eq_ignore_ascii_case(combo));
            if action(id).is_some() && typable(combo) && !reserved {
                loaded.insert(id.to_string(), combo.to_string());
            }
        }
    }
    if let Ok(mut g) = OVERRIDES.write() {
        *g = Some(loaded);
    }
    for a in ACTIONS.iter() {
        if custom(a.id) && ACTIONS.iter().any(|o| o.id != a.id && effective(o.id).eq_ignore_ascii_case(&effective(a.id))) {
            if let Ok(mut g) = OVERRIDES.write() {
                if let Some(m) = g.as_mut() {
                    m.remove(a.id);
                }
            }
        }
    }
}

/// Combinação no formato `Ctrl+Shift+O`; `None` para teclas soltas de modificador ou sem Ctrl/Alt (evita roubar a digitação).
pub fn combo_of(key: Key<&str>, m: Modifiers) -> Option<String> {
    let name = match key {
        Key::Character(c) => c.to_uppercase(),
        Key::Named(Named::Space) => "Space".into(),
        Key::Named(Named::Escape) => "Esc".into(),
        Key::Named(Named::Enter) => "Enter".into(),
        Key::Named(Named::ArrowUp) => "ArrowUp".into(),
        Key::Named(Named::ArrowDown) => "ArrowDown".into(),
        Key::Named(n) => format!("{n:?}"),
        _ => return None,
    };
    let mut out = String::new();
    if m.command() {
        out.push_str("Ctrl+");
    }
    if m.alt() {
        out.push_str("Alt+");
    }
    if m.shift() {
        out.push_str("Shift+");
    }
    out.push_str(&name);
    Some(out)
}

fn msg_of(id: &str) -> Option<Msg> {
    Some(match id {
        "palette" => Msg::PalOpen,
        "new_chat" => Msg::NewChat,
        "sidebar" => Msg::Collapse,
        "settings" => Msg::OpenSettings(None),
        _ => return None,
    })
}

pub fn map(ev: Event) -> Option<Msg> {
    let Event::KeyPressed { key, modifiers, .. } = ev else { return None };
    let combo = combo_of(key.as_ref(), modifiers);
    if capturing() {
        // Durante a captura, a próxima tecla vira o atalho; Esc cancela.
        if matches!(key.as_ref(), Key::Named(Named::Shift | Named::Control | Named::Alt | Named::Super)) {
            return None;
        }
        return Some(match combo {
            Some(c) if c != "Esc" => Msg::ShortcutCaptured(c),
            _ => Msg::ShortcutCancel,
        });
    }
    if let Some(c) = combo.as_deref() {
        if let Some(a) = ACTIONS.iter().find(|a| effective(a.id).eq_ignore_ascii_case(c)) {
            return msg_of(a.id);
        }
    }
    match key.as_ref() {
        Key::Named(Named::Escape) => Some(Msg::Esc),
        Key::Named(Named::ArrowDown) => Some(Msg::PalMove(1)),
        Key::Named(Named::ArrowUp) => Some(Msg::PalMove(-1)),
        _ => None,
    }
}
