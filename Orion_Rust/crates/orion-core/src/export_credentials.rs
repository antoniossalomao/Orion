//! Credenciais de clientes MCP de entrada: hash, expiração e um único escopo.

use crate::projects::Projects;
use orion_store::Store;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::Arc;

pub const PERMISSOES: [&str; 4] = ["search", "facts", "sources", "artifacts"];

#[derive(Debug, Clone, thiserror::Error)]
#[error("{0}")]
pub struct ExportError(pub String);

pub struct ExportCredentials {
    memory: Arc<Store>,
}

impl ExportCredentials {
    pub fn new(memory: Arc<Store>) -> Self {
        ExportCredentials { memory }
    }

    pub fn emitir(&self, name: &str, project_id: Option<&str>, permissions: &[String], lifetime_hours: i64) -> Result<Value, (u16, String)> {
        let bad = |c: &str| (422, c.to_string());
        if name.is_empty() || name.chars().count() > 100 || permissions.is_empty() || permissions.len() > 4 || !permissions.iter().all(|p| PERMISSOES.contains(&p.as_str())) || !(1..=720).contains(&lifetime_hours) {
            return Err(bad("pedido inválido"));
        }
        if let Some(p) = project_id {
            let arq = Projects::new(self.memory.clone()).get(p).map_err(|e| (404, e.0))?["archived"] == json!(true);
            if arq {
                return Err((409, "project_archived".into()));
            }
        }
        let id = orion_store::uuid_hex();
        let token = format!("orion_client_{}", orion_policy::approvals::token_urlsafe(32));
        let agora = self.memory.now();
        let expira = agora + lifetime_hours as f64 * 3600.0;
        let mut perms: Vec<String> = permissions.to_vec();
        perms.sort();
        perms.dedup();
        let hash = hex::encode(Sha256::digest(token.as_bytes()));
        let r: Result<(), (u16, String)> = self
            .memory
            .tx(|c| {
                let n: i64 = c.query_row("SELECT count(*) FROM export_clients WHERE revoked=0", [], |r| r.get(0))?;
                if n >= 50 {
                    return Ok(Err((409, "export_client_limit".to_string())));
                }
                c.execute(
                    "INSERT INTO export_clients(id,name,project_id,permissions,token_hash,expires_at,revoked,created_at) VALUES (?,?,?,?,?,?,0,?)",
                    orion_store::rusqlite::params![id, name, project_id, json!(perms).to_string(), hash, expira, agora],
                )?;
                Ok(Ok(()))
            })
            .map_err(|e| (500, e.to_string()))?;
        r?;
        Ok(json!({"id": id, "token": token, "expires_at": expira}))
    }

    pub fn listar(&self) -> Result<Vec<Value>, orion_store::StoreError> {
        Ok(self
            .memory
            .query("SELECT id,name,project_id,permissions,expires_at,revoked,created_at FROM export_clients ORDER BY created_at DESC LIMIT 100", &[])?
            .into_iter()
            .map(|mut r| {
                let p = r.get("permissions").and_then(Value::as_str).and_then(|s| serde_json::from_str::<Value>(s).ok()).unwrap_or(json!([]));
                r.insert("permissions".into(), p);
                Value::Object(r)
            })
            .collect())
    }

    pub fn revogar(&self, id: &str) -> Result<bool, orion_store::StoreError> {
        self.memory.tx(|c| Ok(c.execute("UPDATE export_clients SET revoked=1 WHERE id=?", [id])? == 1))
    }

    /// A linha do cliente (`id`, `project_id`, `permissions`...) ou `PermissionError` (como texto).
    pub fn verificar(&self, token: &str, permissao: Option<&str>) -> Result<serde_json::Map<String, Value>, &'static str> {
        if !token.starts_with("orion_client_") || token.len() > 128 {
            return Err("export_unauthorized");
        }
        let rows = self
            .memory
            .query("SELECT id,project_id,permissions,expires_at,revoked FROM export_clients WHERE token_hash=?", &[&hex::encode(Sha256::digest(token.as_bytes()))])
            .map_err(|_| "export_unauthorized")?;
        let Some(mut row) = rows.into_iter().next() else { return Err("export_unauthorized") };
        if row["revoked"].as_i64() != Some(0) || row["expires_at"].as_f64().unwrap_or(0.0) <= self.memory.now() {
            return Err("export_unauthorized");
        }
        let perms: Vec<String> = row["permissions"].as_str().and_then(|s| serde_json::from_str(s).ok()).unwrap_or_default();
        if permissao.is_some_and(|p| !perms.iter().any(|x| x == p)) {
            return Err("export_permission_denied");
        }
        if let Some(p) = row["project_id"].as_str() {
            if Projects::new(self.memory.clone()).get(p).map(|x| x["archived"] == json!(true)).unwrap_or(true) {
                return Err("export_project_unavailable");
            }
        }
        row.insert("permissions".into(), json!(perms));
        Ok(row)
    }
}
