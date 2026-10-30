//! Orion — aplicativo principal em iced, reproduzindo o protótipo `orion-ui-mockup.html`
//! (ORION - UI (ESPECIFICAÇÃO)).

pub mod anim;
pub mod app;
pub mod camera_ui;
pub mod chart;
pub mod data;
pub mod graph;
pub mod run;
pub mod html;
pub mod imgcache;
pub mod island;
pub mod icons;
pub mod keys;
pub mod live;
pub mod math;
pub mod md;
pub mod mermaid;
pub mod net;
pub mod novidades;
pub mod uiprefs;
pub mod popover;
pub mod sky;
pub mod theme;
pub mod tray;
pub mod ui;
pub mod views;

#[cfg(feature = "snap")]
pub mod snap;

pub mod onboarding;

pub mod i18n;
pub mod resources;
