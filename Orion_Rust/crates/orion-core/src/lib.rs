//! Núcleo do Orion: configuração, persona, agente, ferramentas, extensões e rotinas.

pub mod accounts;
pub mod agenda;
pub mod agent;
pub mod artifacts;
pub mod auth;
pub mod automacao;
pub mod autostart;
pub mod briefing;
pub mod calendar;
pub mod calendar_events;
pub mod camera;
pub mod dbkey;
pub mod dbopen;
pub mod logcrypt;
pub mod captura_rapida;
pub mod capture;
pub mod channels;
pub mod config;
pub mod conversor;
pub mod desligar;
pub mod development;
pub mod doctor;
pub mod document_extract;
pub mod documents;
pub mod downloads;
pub mod estilo;
pub mod export_credentials;
pub mod extensions;
pub mod file_plans;
pub mod grafo;
pub mod historicos;
pub mod importer;
pub mod jobs;
pub mod legenda;
pub mod ler_depois;
pub mod mcp_client;
pub mod mcp_export;
pub mod mcp_indice;
pub mod mcp_servidor;
pub mod media_transcribe;
pub mod memory;
pub mod migration;
pub mod modos;
pub mod painel;
pub mod palmas;
pub mod persona;
pub mod plugins_locais;
pub mod ponte;
pub mod presenca;
pub mod projects;
pub mod propostas;
pub mod research;
pub mod resultados;
pub mod rosto;
pub mod rosto_lbph;
pub mod faceid;
pub mod relatorio;
pub mod rotinas;
pub mod screen_memory;
pub mod skills;
pub mod nuvem;
pub mod tarefas_bg;
pub mod tools;
pub mod util;
pub mod vida;
pub mod vigia;
pub mod visao_cli;
pub mod visao_local;
pub mod voice;
pub mod wake;

pub use config::Settings;

pub mod two_factor;

#[cfg(windows)]
pub mod native_audio;

pub mod onboarding;
pub mod tool_permissions;
pub mod runtime_preferences;

pub mod onboarding_backup;
