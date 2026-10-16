//! Classificação MCP local: a identidade inclui origem e revisão, nunca `annotations` remotas.
//! A classe de risco vem da configuração revisada por você; ferramenta sem classe não entra.

use super::host::{Connection, ConnectionConfig, MCPError, MCPHost};
use crate::tools::registry::{Args, Tool, ToolError, ToolRegistry};
use orion_mcp::ToolInfo;
use orion_policy::{PolicyEngine, Risk, ToolSpec};
use parking_lot::RwLock;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub connection: String,
    pub remote_name: String,
    pub revision: String,
    pub canonical_id: String,
}

fn sha(s: &str) -> String {
    hex::encode(Sha256::digest(s.as_bytes()))
}

pub fn identity(connection: &Connection, remote: &ToolInfo) -> Entry {
    let payload = json!({
        "config": connection.config.identidade_json(),
        "tool": {"name": remote.name, "description": remote.description, "inputSchema": remote.input_schema},
        "generation": connection.generation(),
    });
    let revision = sha(&payload.to_string());
    let canonical = format!("mcp:{}:{}:{}", connection.config.id(), remote.name, revision);
    // Nome ASCII estável, ≤64; não depende de nome remoto bem-formado/sem colisão.
    let short = format!("mcp_{}", &sha(&canonical)[..32]);
    Entry { name: short, connection: connection.config.id().into(), remote_name: remote.name.clone(), revision, canonical_id: canonical }
}

pub struct Catalog {
    pub host: Arc<MCPHost>,
    registry: Arc<RwLock<ToolRegistry>>,
    policy: Arc<PolicyEngine>,
    pub entries: Arc<RwLock<HashMap<String, Entry>>>,
    lock: tokio::sync::Mutex<()>,
}

impl Catalog {
    pub fn new(host: Arc<MCPHost>, registry: Arc<RwLock<ToolRegistry>>, policy: Arc<PolicyEngine>) -> Self {
        Catalog { host, registry, policy, entries: Default::default(), lock: tokio::sync::Mutex::new(()) }
    }

    pub async fn reconnect(&self, connection_id: &str) -> Result<(), MCPError> {
        let c = self.host.connections.get(connection_id).ok_or_else(|| MCPError::new("connection_not_found"))?.clone();
        // revogar antes do reconnect; nunca retomar chamadas pendentes de execução
        c.close().await;
        c.start().await;
        self.refresh().await
    }

    pub async fn refresh(&self) -> Result<(), MCPError> {
        let _g = self.lock.lock().await;
        self.refresh_locked().await
    }

    fn prepare(&self, connection: &Arc<Connection>, remotes: Vec<ToolInfo>) -> Result<Vec<(Entry, Tool, ToolSpec)>, MCPError> {
        let nomes: HashSet<&str> = remotes.iter().map(|t| t.name.as_str()).collect();
        if remotes.len() > 2048 || nomes.len() != remotes.len() {
            return Err(MCPError::new("catalog_invalid"));
        }
        let mut pending = vec![];
        for remote in &remotes {
            let Some(&risk) = connection.config.classifications().get(&remote.name) else { continue };
            if remote.input_schema.to_string().len() > 64000 {
                return Err(MCPError::new("schema_too_large"));
            }
            let entry = identity(connection, remote);
            let esquema = if remote.input_schema.get("type") == Some(&json!("object")) { remote.input_schema.clone() } else { json!({"type": "object", "properties": {}}) };
            let (conn, nome, atual, entradas) = (connection.clone(), remote.name.clone(), entry.clone(), self.entries.clone());
            let descricao = format!("[{}/{}] {}", connection.label, remote.name, remote.description.clone().unwrap_or_default().chars().take(1000).collect::<String>());
            let tool = Tool::new(&entry.name, &descricao, esquema, move |args: Args| {
                let (conn, nome, atual, entradas) = (conn.clone(), nome.clone(), atual.clone(), entradas.clone());
                async move {
                    let revogada = conn.authorized.lock().clone().is_some_and(|f| !f()) || entradas.read().get(&atual.name) != Some(&atual);
                    if revogada {
                        return Err(ToolError::exec("MCPError", "origin_revoked"));
                    }
                    match conn.call(&nome, args.0).await {
                        Err(e) => Ok(json!({"ok": false, "codigo": e.code, "possibly_active": e.possibly_active, "origin": conn.config.id()})),
                        Ok(r) => Ok(json!({"ok": !r.is_error, "origin": conn.config.id(), "structured": r.structured, "content": r.content})),
                    }
                }
            })
            .validando()
            .com_origem(&entry.canonical_id);
            let spec = ToolSpec {
                external: true,
                origin: Some(entry.canonical_id.clone()),
                revision: Some(entry.revision.clone()),
                require_confirmation: risk != Risk::Read,
                display_name: Some(remote.name.clone()),
                origin_label: Some(connection.label.clone()),
                scope: Some(connection.config.scope().into()),
                ..ToolSpec::new(entry.name.clone(), risk)
            };
            pending.push((entry, tool, spec));
        }
        Ok(pending)
    }

    async fn refresh_locked(&self) -> Result<(), MCPError> {
        let mut pending: Vec<(Entry, Tool, ToolSpec)> = vec![];
        for c in self.host.connections.values() {
            if c.state() != "connected" {
                continue;
            }
            let r = async { self.prepare(c, c.list_tools().await?) }.await;
            match r {
                Ok(p) => pending.extend(p),
                // revogar conexão cujo catálogo não é confiável
                Err(_) => c.set_error("catalog_unavailable"),
            }
        }
        {
            let reg = self.registry.read();
            let ent = self.entries.read();
            if pending.iter().any(|(e, _, _)| reg.get(&e.name).is_some() && !ent.contains_key(&e.name)) {
                return Err(MCPError::new("catalog_collision"));
            }
        }
        let novos: HashSet<String> = pending.iter().map(|(e, _, _)| e.name.clone()).collect();
        let antigos: HashSet<String> = self.entries.read().keys().cloned().collect();
        let removidos: HashSet<String> = antigos.difference(&novos).cloned().collect();
        self.policy.approvals.invalidate_tools(&removidos);
        {
            let mut reg = self.registry.write();
            let mut ent = self.entries.write();
            for n in &antigos {
                reg.unregister(n);
                self.policy.unregister_tool(n);
            }
            ent.clear();
            for (entry, tool, spec) in pending {
                if reg.register(tool).is_err() {
                    continue;
                }
                let _ = self.policy.register_tool(spec, Some((20, 60)));
                ent.insert(entry.name.clone(), entry);
            }
        }
        Ok(())
    }

    /// Ferramentas atualmente expostas por uma conexão (id remoto → nome local, classe).
    pub fn de(&self, connection_id: &str) -> Vec<Value> {
        let ent = self.entries.read();
        let mut v: Vec<_> = ent.values().filter(|e| e.connection == connection_id).map(|e| json!({"name": e.name, "remote_name": e.remote_name})).collect();
        v.sort_by_key(|x| x["remote_name"].as_str().unwrap_or("").to_string());
        v
    }
}

pub fn config_com_risco(c: &ConnectionConfig, nome: &str) -> Option<Risk> {
    c.classifications().get(nome).copied()
}
