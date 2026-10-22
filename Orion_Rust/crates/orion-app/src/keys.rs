//! Atalhos globais do teclado (§25), tratados por assinatura de eventos.

use crate::app::Msg;
use iced::keyboard::{key::Named, Event, Key};

pub fn map(ev: Event) -> Option<Msg> {
    let Event::KeyPressed { key, modifiers, .. } = ev else { return None };
    let ctrl = modifiers.command();
    match key.as_ref() {
        Key::Character("k") if ctrl => Some(Msg::PalOpen),
        Key::Character(",") if ctrl => Some(Msg::OpenSettings(None)),
        Key::Character("b") if ctrl => Some(Msg::Collapse),
        Key::Character("o") | Key::Character("O") if ctrl && modifiers.shift() => Some(Msg::NewChat),
        Key::Named(Named::Escape) => Some(Msg::Esc),
        Key::Named(Named::ArrowDown) => Some(Msg::PalMove(1)),
        Key::Named(Named::ArrowUp) => Some(Msg::PalMove(-1)),
        _ => None,
    }
}
