//! Biblioteca do instalador nativo `Orion Setup` (`iced 0.14`).
//!
//! Reúne:
//! - [`theme`]: tokens de cor, tipografia, dimensões e curvas de animação
//! - [`i18n`]: textos em português do Brasil e inglês
//! - [`audio`]: sintetizador ambiente opcional (`cpal`)
//! - [`register`]: registro em Apps instalados, Menu Iniciar, início automático e desinstalação
//! - [`sky`]: constelação de Orion procedural com PRNG Park–Miller (`seed = 7`)
//! - [`system`]: detecção de hardware, recomendação de modelos e plano transacional
//! - [`app`]: máquina de estados e interface visual de 5 etapas

pub mod app;
pub mod audio;
pub mod i18n;
pub mod identity;
pub mod register;
pub use orion_sky as sky;
pub mod system;
pub mod theme;
pub mod uninstall_ui;
pub mod whats_new_ui;

pub use app::{Message, SetupApp};
