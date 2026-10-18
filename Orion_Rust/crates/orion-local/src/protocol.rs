//! Protocolo privado por stdin/stdout, sem servidor HTTP ou herança de segredos.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const MAX_REQUEST: u64 = 256 * 1024;
pub const MAX_RESPONSE: usize = 64 * 1024;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub model: PathBuf,
    pub messages: Vec<Message>,
    pub gpu: bool,
    pub context: u32,
    pub max_tokens: u32,
}
impl Request {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(self.model.is_absolute() && self.model.is_file(), "modelo local ausente");
        anyhow::ensure!((256..=4096).contains(&self.context), "contexto fora do teto");
        anyhow::ensure!(
            self.max_tokens > 0 && self.max_tokens <= 1024 && self.max_tokens < self.context,
            "limite de resposta inválido"
        );
        anyhow::ensure!(!self.messages.is_empty() && self.messages.len() <= 128, "mensagens inválidas");
        for m in &self.messages {
            anyhow::ensure!(
                ["system", "user", "assistant"].contains(&m.role.as_str()),
                "modelo local não recebe ferramentas"
            );
            anyhow::ensure!(!m.content.contains('\0'), "mensagem inválida");
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Response {
    Text { text: String },
    Done,
    Error { message: String },
}
