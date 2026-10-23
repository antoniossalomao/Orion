//! Orion — aplicativo principal em iced, reproduzindo o protótipo `orion-ui-mockup.html`
//! (ORION - UI (ESPECIFICAÇÃO)).

pub mod anim;
pub mod app;
pub mod chart;
pub mod data;
pub mod graph;
pub mod run;
pub mod icons;
pub mod keys;
pub mod live;
pub mod md;
pub mod mermaid;
pub mod net;
pub mod novidades;
pub mod popover;
pub mod sky;
pub mod theme;
pub mod tray;
pub mod ui;
pub mod views;

#[cfg(feature = "snap")]
pub mod snap;

pub mod onboarding;
