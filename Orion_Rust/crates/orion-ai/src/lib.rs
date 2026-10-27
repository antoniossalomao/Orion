//! Modelos do Orion: provedores diretos, gateway com fallback e quarentena, roteamento por tipo
//! de tarefa, cota gratuita, registro de saídas, barreira de rede, CLIs delegadas, embeddings e
//! cofre de segredos.

pub mod costs;
pub mod delegate;
pub mod embedders;
pub mod gateway;
pub mod injecao;
pub mod leitor;
pub mod netguard;
pub mod provedores;
pub mod router;
pub mod saidas;
pub mod secrets;
pub mod transcribe;
pub mod vision;

pub use gateway::{ChatGateway, Endpoint, Event, GatewayError};

pub mod budget;
