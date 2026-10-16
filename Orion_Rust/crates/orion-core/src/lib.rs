//! Núcleo do Orion: configuração, persona, agente, ferramentas, extensões e rotinas.

pub mod agent;
pub mod agenda;
pub mod auth;
pub mod briefing;
pub mod capture;
pub mod captura_rapida;
pub mod config;
pub mod extensions;
pub mod grafo;
pub mod mcp_indice;
pub mod jobs;
pub mod ler_depois;
pub mod memory;
pub mod modos;
pub mod painel;
pub mod persona;
pub mod projects;
pub mod resultados;
pub mod skills;
pub mod tools;
pub mod util;

pub use config::Settings;
