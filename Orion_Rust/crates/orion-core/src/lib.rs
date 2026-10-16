//! Núcleo do Orion: configuração, persona, agente, ferramentas, extensões e rotinas.

pub mod agent;
pub mod auth;
pub mod config;
pub mod extensions;
pub mod mcp_indice;
pub mod memory;
pub mod modos;
pub mod persona;
pub mod projects;
pub mod resultados;
pub mod skills;
pub mod tools;
pub mod util;

pub use config::Settings;
