//! Jobs da memória que dependem de modelo: consolidação, auto-compact e ciclo de sono.

pub mod compact;
pub mod consolidate;
pub mod sleep;

use futures::future::BoxFuture;
use serde_json::Value;

/// Quem completa um pedido inteiro em texto (o gateway, ou um modelo de mentira).
pub trait Completer: Send + Sync {
    fn complete(&self, messages: Vec<Value>) -> BoxFuture<'_, anyhow::Result<String>>;
}

impl Completer for orion_ai::ChatGateway {
    fn complete(&self, messages: Vec<Value>) -> BoxFuture<'_, anyhow::Result<String>> {
        Box::pin(async move { Ok(orion_ai::ChatGateway::complete(self, messages).await?) })
    }
}

/// Só o endpoint local (para o auto-compact: o texto da conversa não sai do computador).
pub struct SoLocal(pub std::sync::Arc<orion_ai::ChatGateway>);

impl Completer for SoLocal {
    fn complete(&self, messages: Vec<Value>) -> BoxFuture<'_, anyhow::Result<String>> {
        Box::pin(async move { Ok(self.0.complete_tier(messages, Some(orion_ai::gateway::CAMADA_LOCAL)).await?) })
    }
}
