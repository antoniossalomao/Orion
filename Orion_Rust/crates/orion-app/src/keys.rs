//! Atalhos globais do teclado (§25), tratados por assinatura de eventos.
//! Os atalhos da janela são personalizáveis com detecção de conflito (F01.22); os atalhos do sistema
//! (Ctrl+Alt+Espaço, P, O e I), a escala e as teclas fixas (Enter, Esc, setas) aparecem na tela como reservados,
//! gerados desta mesma lista.

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

/// Todas as grafias dos atalhos de escala (`+` pode vir com ou sem Shift); `map` e `RESERVED` usam esta lista.
const ZOOM_IN: [&str; 4] = ["Ctrl+=", "Ctrl++", "Ctrl+Shift++", "Ctrl+Shift+="];
const ZOOM_OUT: [&str; 3] = ["Ctrl+-", "Ctrl+Shift+-", "Ctrl+Shift+_"];

/// Atalhos que nenhuma ação pode tomar: `(combinação, rótulo, texto na tela)`. Texto vazio esconde a linha
/// (grafia repetida do mesmo atalho); a tela de Atalhos é gerada desta lista.
pub const RESERVED: [(&str, &str, &str); 15] = [
    ("Ctrl+Alt+Space", "Trazer a janela de qualquer lugar (mini barra)", "Ctrl Alt Espaço"),
    ("Ctrl+Alt+P", "Pânico: corta tudo", "Ctrl Alt P"),
    ("Ctrl+Alt+O", "Captura rápida", "Ctrl Alt O"),
    ("Ctrl+Alt+I", "Isso? (capturar a tela)", "Ctrl Alt I"),
    ("Esc", "Fechar painel", "Esc"),
    ("Enter", "Enviar", "Enter"),
    ("ArrowUp", "Navegar na lista", "↑ ↓"),
    ("ArrowDown", "Navegar na lista", ""),
    (ZOOM_IN[0], "Escala da interface", "Ctrl + / Ctrl −"),
    (ZOOM_IN[1], "Escala da interface", ""),
    (ZOOM_IN[2], "Escala da interface", ""),
    (ZOOM_IN[3], "Escala da interface", ""),
    (ZOOM_OUT[0], "Escala da interface", ""),
    (ZOOM_OUT[1], "Escala da interface", ""),
    (ZOOM_OUT[2], "Escala da interface", ""),
];

/// O campo de texto trata estes atalhos antes da assinatura global; uma ação neles nunca rodaria com o campo focado.
const EDITING_COMBOS: [&str; 7] = ["Ctrl+A", "Ctrl+C", "Ctrl+V", "Ctrl+X", "Ctrl+Z", "Ctrl+Y", "Ctrl+Shift+Z"];
const EDITING_KEYS: [&str; 8] = ["ArrowLeft", "ArrowRight", "Home", "End", "Backspace", "Delete", "PageUp", "PageDown"];
pub const EDITING_LABEL: &str = "Edição de texto";
/// Texto da linha de edição na tela de Atalhos.
pub const EDITING_SHOWN: &str = "Ctrl A · C · V · X · Z · Y";

static OVERRIDES: RwLock<Option<HashMap<String, String>>> = RwLock::new(None);
/// Só o app real lê e grava `atalhos.txt`; testes e capturas ficam só na memória.
static PERSIST: AtomicBool = AtomicBool::new(false);

fn action(id: &str) -> Option<&'static Action> {
    ACTIONS.iter().find(|a| a.id == id)
}

/// Liga a persistência em disco e carrega os atalhos personalizados (chamado só pelo app real).
pub fn enable_persistence() {
    PERSIST.store(true, Ordering::Relaxed);
    load();
}

/// Atalho em vigor para a ação (personalizado ou padrão).
pub fn effective(id: &str) -> String {
    let custom = OVERRIDES.read().ok().and_then(|g| g.as_ref().and_then(|m| m.get(id).cloned()));
    custom.or_else(|| action(id).map(|a| a.default.to_string())).unwrap_or_default()
}

/// Separa `Ctrl+Shift+O` em modificadores e tecla; a própria tecla pode ser `+` (`Ctrl++`).
fn split(combo: &str) -> (Vec<&str>, &str) {
    if let Some(rest) = combo.strip_suffix('+') {
        let mods = rest.strip_suffix('+').unwrap_or(rest);
        return (mods.split('+').filter(|m| !m.is_empty()).collect(), "+");
    }
    match combo.rsplit_once('+') {
        Some((mods, key)) => (mods.split('+').filter(|m| !m.is_empty()).collect(), key),
        None => (vec![], combo),
    }
}

/// Texto para exibir (`Ctrl+Shift+O` vira `Ctrl Shift O`; `Ctrl++` vira `Ctrl +`).
pub fn display(combo: &str) -> String {
    let (mods, key) = split(combo);
    let key = if key == "Space" && !crate::i18n::english() { "Espaço" } else { key };
    mods.into_iter().chain(std::iter::once(key)).collect::<Vec<_>>().join(" ")
}

/// Motivo de uma recusa ao trocar ou restaurar um atalho.
#[derive(Debug, PartialEq, Eq)]
pub enum ShortcutError {
    Unknown,
    NeedsModifier(String),
    Taken(String, String),
    DefaultTaken(String, String),
    NotSaved(String),
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
            Self::NotSaved(e) => formatted("Não foi possível salvar: {e}", format!("Não foi possível salvar: {e}")),
        }
    }
}

/// Atalho aceito: precisa de Ctrl ou Alt (ou ser tecla de função) para não roubar a digitação.
fn typable(combo: &str) -> bool {
    let (mods, key) = split(combo);
    mods.iter().any(|m| *m == "Ctrl" || *m == "Alt") || (key.len() > 1 && key.starts_with('F') && key[1..].chars().all(|c| c.is_ascii_digit()))
}

/// Quem já tem a combinação por regra fixa (sistema, escala, edição de texto, Enter, Esc e setas), se houver.
fn reserved_by(combo: &str) -> Option<String> {
    if let Some((_, what, _)) = RESERVED.iter().find(|(k, _, _)| k.eq_ignore_ascii_case(combo)) {
        return Some((*what).to_string());
    }
    let (_, key) = split(combo);
    if EDITING_COMBOS.iter().any(|c| c.eq_ignore_ascii_case(combo)) || EDITING_KEYS.contains(&key) {
        return Some(EDITING_LABEL.to_string());
    }
    // Enter, Esc e setas valem em qualquer combinação: o campo de mensagem e as listas ignoram os modificadores.
    RESERVED.iter().find(|(k, _, _)| !k.contains('+') && k.eq_ignore_ascii_case(key)).map(|(_, what, _)| (*what).to_string())
}

/// Quem já usa o atalho, se houver, excluindo a própria ação.
pub fn conflict(id: &str, combo: &str) -> Option<String> {
    reserved_by(combo).or_else(|| ACTIONS.iter().filter(|a| a.id != id).find(|a| effective(a.id).eq_ignore_ascii_case(combo)).map(|a| a.label.to_string()))
}

fn put(id: &str, combo: Option<String>) -> Option<String> {
    let mut g = OVERRIDES.write().ok()?;
    let map = g.get_or_insert_with(HashMap::new);
    match combo {
        Some(c) => map.insert(id.to_string(), c),
        None => map.remove(id),
    }
}

/// Troca o atalho de uma ação. Recusa tecla sem Ctrl/Alt e conflito; se não conseguir gravar, desfaz a troca.
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
    let before = put(id, Some(combo.to_string()));
    save().map_err(|e| {
        put(id, before);
        ShortcutError::NotSaved(e.to_string())
    })
}

/// Volta ao atalho padrão. Se o padrão estiver tomado por outra ação, a restauração é recusada.
pub fn reset(id: &str) -> Result<(), ShortcutError> {
    let Some(a) = action(id) else { return Err(ShortcutError::Unknown) };
    if let Some(who) = conflict(id, a.default) {
        return Err(ShortcutError::DefaultTaken(a.default.to_string(), who));
    }
    let before = put(id, None);
    save().map_err(|e| {
        put(id, before);
        ShortcutError::NotSaved(e.to_string())
    })
}

pub fn custom(id: &str) -> bool {
    OVERRIDES.read().ok().is_some_and(|g| g.as_ref().is_some_and(|m| m.contains_key(id)))
}

fn file() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA").or_else(|| std::env::var_os("HOME"))?;
    Some(PathBuf::from(base).join("Orion").join("atalhos.txt"))
}

fn save() -> std::io::Result<()> {
    if !PERSIST.load(Ordering::Relaxed) {
        return Ok(());
    }
    let Some(path) = file() else { return Ok(()) };
    let body = OVERRIDES.read().ok().and_then(|g| g.as_ref().map(|m| {
        let mut rows: Vec<String> = m.iter().map(|(k, v)| format!("{k}={v}")).collect();
        rows.sort();
        rows.join("\n")
    })).unwrap_or_default();
    crate::uiprefs::write_atomic(&path, &body)
}

/// Lê os atalhos personalizados do disco com as mesmas regras de `set`. Carrega todos e só depois descarta os
/// duplicados, para que trocas entre ações sobrevivam à reabertura, qualquer que seja a ordem do arquivo.
fn load() {
    let Some(path) = file() else { return };
    let Ok(text) = std::fs::read_to_string(path) else { return };
    let mut loaded: HashMap<String, String> = HashMap::new();
    for line in text.lines() {
        if let Some((id, combo)) = line.split_once('=') {
            let (id, combo) = (id.trim(), combo.trim());
            if action(id).is_some() && typable(combo) && reserved_by(combo).is_none() {
                loaded.insert(id.to_string(), combo.to_string());
            }
        }
    }
    if let Ok(mut g) = OVERRIDES.write() {
        *g = Some(loaded);
    }
    for a in ACTIONS.iter() {
        if custom(a.id) && ACTIONS.iter().any(|o| o.id != a.id && effective(o.id).eq_ignore_ascii_case(&effective(a.id))) {
            put(a.id, None);
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

/// Assinatura usada só enquanto um atalho é capturado (`App.short_edit`): a próxima tecla vira o atalho; Esc cancela.
pub fn map_capture(ev: Event) -> Option<Msg> {
    let Event::KeyPressed { key, modifiers, .. } = ev else { return None };
    if matches!(key.as_ref(), Key::Named(Named::Shift | Named::Control | Named::Alt | Named::Super)) {
        return None;
    }
    Some(match combo_of(key.as_ref(), modifiers) {
        Some(c) if c != "Esc" => Msg::ShortcutCaptured(c),
        _ => Msg::ShortcutCancel,
    })
}

pub fn map(ev: Event) -> Option<Msg> {
    let Event::KeyPressed { key, modifiers, .. } = ev else { return None };
    let combo = combo_of(key.as_ref(), modifiers);
    if let Some(c) = combo.as_deref() {
        if let Some(a) = ACTIONS.iter().find(|a| effective(a.id).eq_ignore_ascii_case(c)) {
            return msg_of(a.id);
        }
    }
    // Ctrl + e Ctrl − andam um degrau na escala (§3); `=` é o `+` sem Shift.
    if let Some(c) = combo.as_deref() {
        if ZOOM_IN.contains(&c) {
            return Some(Msg::ZoomStep(1));
        }
        if ZOOM_OUT.contains(&c) {
            return Some(Msg::ZoomStep(-1));
        }
    }
    match key.as_ref() {
        Key::Named(Named::Escape) => Some(Msg::Esc),
        Key::Named(Named::ArrowDown) => Some(Msg::PalMove(1)),
        Key::Named(Named::ArrowUp) => Some(Msg::PalMove(-1)),
        _ => None,
    }
}
