//! Ponte em leitura para o Google Calendar MCP 2.7.0; conta nunca inferida/mesclada.

use crate::extensions::host::{ConnectionConfig, MCPError, MCPHost};
use crate::tools::registry::{Args, Tool, ToolRegistry};
use chrono::{DateTime, FixedOffset, NaiveDateTime};
use orion_policy::{PolicyEngine, Risk, ToolSpec};
use orion_store::Store;
use parking_lot::RwLock;
use regex::Regex;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use std::sync::{Arc, LazyLock};

pub const READ_TOOLS: [&str; 3] = ["list-events", "get-freebusy", "get-current-time"];
static ESCOPO: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(personal|project:[a-f0-9]{32})$").unwrap());
static CONEXAO: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-z0-9][a-z0-9_-]{0,47}$").unwrap());
static CONTA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-z0-9_-]{1,64}$").unwrap());

/// `datetime.fromisoformat` com fuso obrigatório (None quando falta o fuso ou o texto é inválido).
pub fn iso_com_fuso(s: &str) -> Option<DateTime<FixedOffset>> {
    let s = s.trim();
    DateTime::parse_from_rfc3339(s)
        .or_else(|_| DateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%:z"))
        .or_else(|_| DateTime::parse_from_str(s, "%Y-%m-%dT%H:%M%:z"))
        .or_else(|_| DateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f%:z"))
        .ok()
}

/// A data/hora é válida mas SEM fuso (para distinguir de texto inválido).
pub fn iso_sem_fuso(s: &str) -> bool {
    ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M", "%Y-%m-%dT%H:%M:%S%.f"].iter().any(|f| NaiveDateTime::parse_from_str(s.trim(), f).is_ok())
}

pub struct Binding {
    pub scope: String,
    pub connection_id: String,
    pub account: String,
    pub calendar_id: String,
    pub timezone: String,
    pub reviewed_read_only: bool,
}

impl Binding {
    pub fn parse(v: &Value) -> Result<Self, String> {
        let o = v.as_object().ok_or("esperava objeto")?;
        let permitidos = ["scope", "connection_id", "account", "calendar_id", "timezone", "reviewed_read_only"];
        if let Some(k) = o.keys().find(|k| !permitidos.contains(&k.as_str())) {
            return Err(format!("campo desconhecido: {k}"));
        }
        let s = |k: &str, d: &str| v.get(k).and_then(Value::as_str).unwrap_or(d).to_string();
        let b = Binding {
            scope: s("scope", "personal"),
            connection_id: s("connection_id", ""),
            account: s("account", ""),
            calendar_id: s("calendar_id", "primary"),
            timezone: s("timezone", "America/Sao_Paulo"),
            reviewed_read_only: v.get("reviewed_read_only").and_then(Value::as_bool).unwrap_or(false),
        };
        if !ESCOPO.is_match(&b.scope) || !CONEXAO.is_match(&b.connection_id) || !CONTA.is_match(&b.account) {
            return Err("scope, connection_id ou account inválido".into());
        }
        if b.calendar_id.is_empty() || b.calendar_id.len() > 256 || b.timezone.is_empty() || b.timezone.len() > 100 {
            return Err("calendar_id ou timezone inválido".into());
        }
        Ok(b)
    }
}

pub struct Calendar {
    pub memory: Arc<Store>,
    pub host: Arc<MCPHost>,
    pub registry: Arc<RwLock<ToolRegistry>>,
    pub policy: Arc<PolicyEngine>,
}

fn erro(code: &str) -> Value {
    json!({"ok": false, "code": code, "action": "Revise a conta e a conexão da Agenda ou reduza o período da consulta."})
}

impl Calendar {
    pub fn new(memory: Arc<Store>, host: Arc<MCPHost>, registry: Arc<RwLock<ToolRegistry>>, policy: Arc<PolicyEngine>) -> Arc<Self> {
        let cal = Arc::new(Calendar { memory, host, registry: registry.clone(), policy: policy.clone() });
        let params = json!({"type": "object", "properties": {"start": {"type": "string", "format": "date-time"}, "end": {"type": "string", "format": "date-time"}}, "required": ["start", "end"], "additionalProperties": false});
        for (nome, remoto, desc) in [("consultar_agenda", "list-events", "Consultar compromissos no período e fuso revisados."), ("consultar_disponibilidade", "get-freebusy", "Consultar disponibilidade no período sem criar eventos.")] {
            let c = cal.clone();
            let t = Tool::new(nome, desc, params.clone(), move |a: Args| {
                let c = c.clone();
                async move {
                    let start = a.s("start")?;
                    let end = a.s("end")?;
                    Ok(c.read(remoto, &start, &end).await)
                }
            })
            .validando();
            let _ = registry.write().register(t);
            let _ = policy.register_tool(ToolSpec { external: true, origin: Some("google-calendar-mcp:2.7.0".into()), ..ToolSpec::new(nome, Risk::Read) }, None);
        }
        cal
    }

    pub fn connection_revision(c: &ConnectionConfig) -> String {
        hex::encode(Sha256::digest(c.identidade_json().to_string().as_bytes()))
    }

    pub fn binding(&self, scope: Option<&str>) -> Result<(Map<String, Value>, Arc<crate::extensions::host::Connection>), MCPError> {
        let scope = match scope {
            Some(s) => s.to_string(),
            None => orion_store::current_scope().project_id.map(|p| format!("project:{p}")).unwrap_or_else(|| "personal".into()),
        };
        let rows = self.memory.query("SELECT * FROM calendar_bindings WHERE scope=?", &[&scope]).map_err(|_| MCPError::new("calendar_not_configured"))?;
        let row = rows.into_iter().next().ok_or_else(|| MCPError::new("calendar_not_configured"))?;
        let conn = self.host.connections.get(row["connection_id"].as_str().unwrap_or("")).cloned();
        let Some(conn) = conn.filter(|c| c.config.scope() == scope && c.state() == "connected") else { return Err(MCPError::new("calendar_connection_unavailable")) };
        if conn.authorized.lock().clone().is_some_and(|f| !f()) {
            return Err(MCPError::new("calendar_account_revoked"));
        }
        if row["connection_revision"].as_str() != Some(&Self::connection_revision(&conn.config)) {
            return Err(MCPError::new("calendar_connection_review_changed"));
        }
        Ok((row, conn))
    }

    pub async fn bind(&self, b: &Binding) -> Result<Value, MCPError> {
        if !b.reviewed_read_only {
            return Err(MCPError::new("calendar_read_review_required"));
        }
        if b.timezone.parse::<chrono_tz::Tz>().is_err() {
            return Err(MCPError::new("calendar_timezone_invalid"));
        }
        let conn = self.host.connections.get(&b.connection_id).cloned();
        let Some(conn) = conn.filter(|c| c.config.scope() == b.scope && c.state() == "connected") else { return Err(MCPError::new("calendar_connection_unavailable")) };
        if let ConnectionConfig::Http(h) = &conn.config {
            if h.secret_ref.is_none() && h.account_id.is_none() {
                let host = url::Url::parse(&h.url).ok().and_then(|u| u.host_str().map(String::from)).unwrap_or_default();
                if !["localhost", "127.0.0.1", "::1", "[::1]"].contains(&host.as_str()) {
                    return Err(MCPError::new("calendar_remote_auth_required"));
                }
            }
        }
        if conn.config.classifications().iter().any(|(n, r)| !READ_TOOLS.contains(&n.as_str()) || *r != Risk::Read) {
            return Err(MCPError::new("calendar_read_only_catalog_required"));
        }
        let tools = conn.list_tools().await?;
        let by: std::collections::HashMap<_, _> = tools.iter().map(|t| (t.name.as_str(), t)).collect();
        if !["list-events", "get-freebusy"].iter().all(|n| by.contains_key(n)) {
            return Err(MCPError::new("calendar_connector_incompatible"));
        }
        // gestão de conta anunciada pelo servidor fica de fora
        let schema = json!({"get-freebusy": by["get-freebusy"].input_schema, "list-events": by["list-events"].input_schema}).to_string();
        let revision = hex::encode(Sha256::digest(format!("{schema}{}", orion_store::uuid_hex()).as_bytes()));
        let crev = Self::connection_revision(&conn.config);
        self.memory
            .tx(|c| {
                c.execute(
                    "INSERT OR REPLACE INTO calendar_bindings(scope,connection_id,account,calendar_id,timezone,revision,connection_revision) VALUES (?,?,?,?,?,?,?)",
                    rusqlite::params![b.scope, b.connection_id, b.account, b.calendar_id, b.timezone, revision, crev],
                )?;
                Ok(())
            })
            .map_err(|_| MCPError::new("calendar_not_configured"))?;
        Ok(json!({"scope": b.scope, "connection_id": b.connection_id, "account": b.account, "calendar_id": b.calendar_id, "timezone": b.timezone, "reviewed_read_only": b.reviewed_read_only, "revision": revision}))
    }

    pub async fn read(&self, remoto: &str, start: &str, end: &str) -> Value {
        match self.read_inner(remoto, start, end).await {
            Ok(v) => v,
            Err(e) => erro(&e.code),
        }
    }

    async fn read_inner(&self, remoto: &str, start: &str, end: &str) -> Result<Value, MCPError> {
        let (row, conn) = self.binding(None)?;
        let (Some(a), Some(b)) = (iso_com_fuso(start), iso_com_fuso(end)) else { return Err(MCPError::new("calendar_period_invalid")) };
        let secs = (b - a).num_seconds();
        if secs <= 0 || secs > 31 * 86400 {
            return Err(MCPError::new("calendar_period_invalid"));
        }
        let g = |k: &str| row[k].as_str().unwrap_or("").to_string();
        let mut args = Map::new();
        args.insert("account".into(), json!(g("account")));
        args.insert("timeMin".into(), json!(start));
        args.insert("timeMax".into(), json!(end));
        args.insert("timeZone".into(), json!(g("timezone")));
        if remoto == "list-events" {
            args.insert("calendarId".into(), json!(g("calendar_id")));
        } else {
            args.insert("calendars".into(), json!([{"id": g("calendar_id")}]));
        }
        let tools = conn.list_tools().await?;
        let Some(t) = tools.iter().find(|t| t.name == remoto).filter(|_| READ_TOOLS.contains(&remoto)) else { return Err(MCPError::new("calendar_connector_incompatible")) };
        let validador = Tool::new("calendar_remote", "", t.input_schema.clone(), |_a: Args| async { Ok(Value::Null) }).validando();
        if validador.invalido(&args).is_some() {
            return Err(MCPError::new("calendar_connector_incompatible"));
        }
        let r = conn.call(remoto, args).await?;
        let data = json!({"structured": r.structured, "content": r.content});
        let ds = data.to_string();
        if ds.len() > 32000 {
            return Err(MCPError::new("calendar_result_limit"));
        }
        let code = r.is_error.then(|| {
            let d = ds.to_lowercase();
            if ["quota", "ratelimit", "429"].iter().any(|x| d.contains(x)) { "calendar_quota" } else { "calendar_provider_error" }
        });
        Ok(json!({"ok": !r.is_error, "code": code, "source": {"connector": "@cocal/google-calendar-mcp@2.7.0", "connection": g("connection_id"), "account": g("account"), "calendar": g("calendar_id"), "timezone": g("timezone"), "revision": g("revision"), "generation": conn.generation()}, "data": data}))
    }
}

