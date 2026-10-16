//! Manifesto Orion e versões persistentes. Declarar capability não concede autorização.

use orion_policy::Risk;
use parking_lot::Mutex;
use regex::Regex;
use rusqlite::Connection;
use serde::de::{Deserializer, MapAccess, SeqAccess, Visitor};
use serde::Deserialize;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;
use std::sync::LazyLock;

static ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-z0-9][a-z0-9_-]{0,47}$").unwrap());
static VERSAO: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[0-9]+\.[0-9]+\.[0-9]+(?:-[a-z0-9.-]+)?$").unwrap());
static SEGREDO: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^ORION_MCP_[A-Z0-9_]{1,80}$").unwrap());
static CAPACIDADE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:native:[a-z0-9_]{1,64}|mcp:[a-z0-9_-]{1,48}:[A-Za-z0-9_.-]{1,128}|resource:[a-z0-9_-]{1,48}|prompt:[a-z0-9_-]{1,48}|scripts)$").unwrap()
});

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct PluginError(pub String);

pub fn perr(c: &str) -> PluginError {
    PluginError(c.into())
}

type R<T> = Result<T, PluginError>;

pub fn relative_path(v: &str) -> R<&str> {
    if v.is_empty() || v.contains('\\') || v.contains(':') || v.starts_with('/') {
        return Err(perr("manifest_path_invalid"));
    }
    if v.split('/').any(|p| p.is_empty() || p == "." || p == "..") {
        return Err(perr("manifest_path_invalid"));
    }
    Ok(v)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
    pub id: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginConnection {
    pub id: String,
    pub transport: String,
    pub entrypoint: Option<String>,
    pub url: Option<String>,
    pub secret_ref: Option<String>,
    pub tools: BTreeMap<String, Risk>,
    pub resources: Vec<String>,
    pub prompts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginManifest {
    pub schema_version: i64,
    pub orion_api: i64,
    pub id: String,
    pub version: String,
    pub name: String,
    pub description: String,
    pub license: String,
    pub skills: Vec<String>,
    pub mcp: Vec<PluginConnection>,
    pub capabilities: Vec<String>,
    pub dependencies: Vec<Dependency>,
}

impl PluginManifest {
    pub fn to_json(&self) -> Value {
        let mcp: Vec<Value> = self
            .mcp
            .iter()
            .map(|c| {
                let tools: Map<String, Value> = c.tools.iter().map(|(k, v)| (k.clone(), json!(v.as_str()))).collect();
                json!({"id": c.id, "transport": c.transport, "entrypoint": c.entrypoint, "url": c.url, "secret_ref": c.secret_ref, "tools": tools, "resources": c.resources, "prompts": c.prompts})
            })
            .collect();
        json!({"schema_version": self.schema_version, "orion_api": self.orion_api, "id": self.id, "version": self.version, "name": self.name, "description": self.description, "license": self.license, "skills": self.skills, "mcp": mcp, "capabilities": self.capabilities, "dependencies": self.dependencies.iter().map(|d| json!({"id": d.id, "version": d.version})).collect::<Vec<_>>()})
    }

    /// A partir do JSON já gravado (ou do corpo de uma requisição).
    pub fn from_value(v: &Value) -> R<Self> {
        validar(v)
    }
}

/// JSON que rejeita chaves repetidas em qualquer nível.
struct Unico(Value);

impl<'de> Deserialize<'de> for Unico {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Value;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("JSON")
            }
            fn visit_bool<E>(self, v: bool) -> Result<Value, E> {
                Ok(Value::Bool(v))
            }
            fn visit_i64<E>(self, v: i64) -> Result<Value, E> {
                Ok(json!(v))
            }
            fn visit_u64<E>(self, v: u64) -> Result<Value, E> {
                Ok(json!(v))
            }
            fn visit_f64<E>(self, v: f64) -> Result<Value, E> {
                Ok(json!(v))
            }
            fn visit_str<E>(self, v: &str) -> Result<Value, E> {
                Ok(Value::String(v.into()))
            }
            fn visit_none<E>(self) -> Result<Value, E> {
                Ok(Value::Null)
            }
            fn visit_unit<E>(self) -> Result<Value, E> {
                Ok(Value::Null)
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Value, A::Error> {
                let mut out = vec![];
                while let Some(Unico(v)) = a.next_element()? {
                    out.push(v);
                }
                Ok(Value::Array(out))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Value, A::Error> {
                let mut m = Map::new();
                while let Some((k, Unico(v))) = a.next_entry::<String, Unico>()? {
                    if m.contains_key(&k) {
                        return Err(serde::de::Error::custom("manifest_duplicate_key"));
                    }
                    m.insert(k, v);
                }
                Ok(Value::Object(m))
            }
        }
        d.deserialize_any(V).map(Unico)
    }
}

fn so_chaves(o: &Map<String, Value>, ok: &[&str]) -> R<()> {
    if o.keys().any(|k| !ok.contains(&k.as_str())) {
        return Err(perr("manifest_invalid"));
    }
    Ok(())
}

fn texto(v: &Value, k: &str, min: usize, max: usize) -> R<String> {
    match v.get(k) {
        Some(Value::String(s)) if s.chars().count() >= min && s.chars().count() <= max => Ok(s.clone()),
        _ => Err(perr("manifest_invalid")),
    }
}

fn lista_texto(v: &Value, k: &str, max: usize) -> R<Vec<String>> {
    match v.get(k) {
        None => Ok(vec![]),
        Some(Value::Array(a)) if a.len() <= max => a.iter().map(|x| x.as_str().map(String::from).ok_or_else(|| perr("manifest_invalid"))).collect(),
        _ => Err(perr("manifest_invalid")),
    }
}

fn validar(v: &Value) -> R<PluginManifest> {
    let o = v.as_object().ok_or_else(|| perr("manifest_invalid"))?;
    so_chaves(o, &["schema_version", "orion_api", "id", "version", "name", "description", "license", "skills", "mcp", "capabilities", "dependencies"])?;
    let um = |k: &str| match v.get(k) {
        None => Ok(1),
        Some(x) if x.as_i64() == Some(1) => Ok(1),
        _ => Err(perr("manifest_invalid")),
    };
    let (schema_version, orion_api) = (um("schema_version")?, um("orion_api")?);
    let id = texto(v, "id", 1, 48)?;
    let version = texto(v, "version", 1, 80)?;
    if !ID.is_match(&id) || !VERSAO.is_match(&version) {
        return Err(perr("manifest_invalid"));
    }
    let skills = lista_texto(v, "skills", 128)?;
    for s in &skills {
        relative_path(s)?;
    }
    if skills.iter().collect::<HashSet<_>>().len() != skills.len() {
        return Err(perr("manifest_duplicate_skill"));
    }
    let capabilities = lista_texto(v, "capabilities", 256)?;
    if capabilities.iter().collect::<HashSet<_>>().len() != capabilities.len() || capabilities.iter().any(|c| !CAPACIDADE.is_match(c)) {
        return Err(perr("manifest_capability_invalid"));
    }
    let mut mcp = vec![];
    match v.get("mcp") {
        None => {}
        Some(Value::Array(a)) if a.len() <= 32 => {
            for c in a {
                mcp.push(conexao(c)?);
            }
        }
        _ => return Err(perr("manifest_invalid")),
    }
    let mut dependencies = vec![];
    match v.get("dependencies") {
        None => {}
        Some(Value::Array(a)) if a.len() <= 16 => {
            for d in a {
                let o = d.as_object().ok_or_else(|| perr("manifest_invalid"))?;
                so_chaves(o, &["id", "version"])?;
                let (did, dv) = (texto(d, "id", 1, 48)?, texto(d, "version", 1, 80)?);
                if !ID.is_match(&did) || !VERSAO.is_match(&dv) {
                    return Err(perr("manifest_invalid"));
                }
                dependencies.push(Dependency { id: did, version: dv });
            }
        }
        _ => return Err(perr("manifest_invalid")),
    }
    if skills.is_empty() && mcp.is_empty() {
        return Err(perr("manifest_empty"));
    }
    if mcp.iter().map(|c| &c.id).collect::<HashSet<_>>().len() != mcp.len() {
        return Err(perr("manifest_connection_collision"));
    }
    if dependencies.iter().any(|d| d.id == id) {
        return Err(perr("manifest_dependency_cycle"));
    }
    Ok(PluginManifest { schema_version, orion_api, name: texto(v, "name", 1, 80)?, description: texto(v, "description", 1, 1024)?, license: texto(v, "license", 1, 128)?, id, version, skills, mcp, capabilities, dependencies })
}

fn conexao(c: &Value) -> R<PluginConnection> {
    let o = c.as_object().ok_or_else(|| perr("manifest_invalid"))?;
    so_chaves(o, &["id", "transport", "entrypoint", "url", "secret_ref", "tools", "resources", "prompts"])?;
    let id = texto(c, "id", 1, 48)?;
    if !ID.is_match(&id) {
        return Err(perr("manifest_invalid"));
    }
    let transport = texto(c, "transport", 1, 5)?;
    let opt = |k: &str, max: usize| -> R<Option<String>> {
        match c.get(k) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) if s.len() <= max => Ok(Some(s.clone())),
            _ => Err(perr("manifest_invalid")),
        }
    };
    let (entrypoint, url, secret_ref) = (opt("entrypoint", 1024)?, opt("url", 4096)?, opt("secret_ref", 200)?);
    if secret_ref.as_ref().is_some_and(|s| !SEGREDO.is_match(s)) {
        return Err(perr("manifest_invalid"));
    }
    let mut tools = BTreeMap::new();
    match c.get("tools") {
        None => {}
        Some(Value::Object(m)) if m.len() <= 128 => {
            for (k, x) in m {
                tools.insert(k.clone(), x.as_str().and_then(Risk::parse).ok_or_else(|| perr("manifest_invalid"))?);
            }
        }
        _ => return Err(perr("manifest_invalid")),
    }
    match transport.as_str() {
        "stdio" => {
            let e = entrypoint.as_deref().filter(|e| e.ends_with(".py")).ok_or_else(|| perr("manifest_transport_invalid"))?;
            if url.is_some() {
                return Err(perr("manifest_transport_invalid"));
            }
            relative_path(e)?;
        }
        "http" => {
            let u = url.as_deref().filter(|_| entrypoint.is_none()).ok_or_else(|| perr("manifest_transport_invalid"))?;
            let p = url::Url::parse(u).map_err(|_| perr("manifest_endpoint_invalid"))?;
            let loopback = ["127.0.0.1", "localhost", "::1", "[::1]"].contains(&p.host_str().unwrap_or(""));
            if !(p.scheme() == "https" || (p.scheme() == "http" && loopback)) || p.host_str().is_none() || !p.username().is_empty() || p.password().is_some() || p.query().is_some() || p.fragment().is_some() {
                return Err(perr("manifest_endpoint_invalid"));
            }
        }
        _ => return Err(perr("manifest_invalid")),
    }
    Ok(PluginConnection { id, transport, entrypoint, url, secret_ref, tools, resources: lista_texto(c, "resources", 128)?, prompts: lista_texto(c, "prompts", 128)? })
}

pub fn parse_manifest(data: &[u8]) -> R<PluginManifest> {
    if data.len() > 64000 {
        return Err(perr("manifest_too_large"));
    }
    let Unico(v) = serde_json::from_slice(data).map_err(|e| if e.to_string().contains("manifest_duplicate_key") { perr("manifest_duplicate_key") } else { perr("manifest_invalid") })?;
    validar(&v).map_err(|e| if e.0.starts_with("manifest_") && e.0 != "manifest_invalid" { e } else { perr("manifest_invalid") })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginState {
    Installed,
    Disabled,
    Waiting,
    Active,
    Error,
}

impl PluginState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Installed => "installed",
            Self::Disabled => "disabled",
            Self::Waiting => "waiting_connection",
            Self::Active => "active",
            Self::Error => "error",
        }
    }
}

pub struct PluginStore {
    db: Mutex<Connection>,
}

fn sq(e: rusqlite::Error) -> PluginError {
    tracing::warn!("plugin_store: {e}");
    perr("plugin_store_error")
}

impl PluginStore {
    pub fn open(path: &Path) -> R<Self> {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p).map_err(|_| perr("plugin_store_unavailable"))?;
        }
        let db = Connection::open(path).map_err(sq)?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;").map_err(sq)?;
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS plugins (
                id TEXT PRIMARY KEY, state TEXT NOT NULL, selected_digest TEXT,
                active_digest TEXT, previous_digest TEXT, error TEXT);
             CREATE TABLE IF NOT EXISTS plugin_versions (
                plugin_id TEXT NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
                version TEXT NOT NULL, digest TEXT NOT NULL, origin TEXT NOT NULL,
                manifest TEXT NOT NULL, bundle TEXT,
                PRIMARY KEY(plugin_id,digest), UNIQUE(plugin_id,version));
             CREATE TABLE IF NOT EXISTS plugin_grants (
                plugin_id TEXT NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
                digest TEXT NOT NULL, scope TEXT NOT NULL, capabilities TEXT NOT NULL,
                generation TEXT NOT NULL, PRIMARY KEY(plugin_id,digest,scope));
             CREATE TABLE IF NOT EXISTS extension_connections (id TEXT PRIMARY KEY, config TEXT NOT NULL);",
        )
        .map_err(sq)?;
        Ok(PluginStore { db: Mutex::new(db) })
    }

    /// Acesso direto à conexão (grants e conexões salvas).
    pub fn with_db<T>(&self, f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> R<T> {
        let c = self.db.lock();
        f(&c).map_err(sq)
    }

    pub fn record(&self, m: &PluginManifest, digest: &str, origin: &str, bundle: Option<&str>, update: bool) -> R<()> {
        if digest.len() != 64 || !digest.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()) || origin.len() > 1024 {
            return Err(perr("plugin_identity_invalid"));
        }
        let mut c = self.db.lock();
        let tx = c.transaction().map_err(sq)?;
        let existe: bool = tx.query_row("SELECT 1 FROM plugins WHERE id=?", [&m.id], |_| Ok(true)).unwrap_or(false);
        let igual: Option<String> = tx.query_row("SELECT digest FROM plugin_versions WHERE plugin_id=? AND version=?", [&m.id, &m.version], |r| r.get(0)).ok();
        if let Some(d) = igual {
            return if d != digest { Err(perr("plugin_version_conflict")) } else { Ok(()) };
        }
        if existe && !update {
            return Err(perr("plugin_id_conflict"));
        }
        if !existe {
            tx.execute("INSERT INTO plugins(id,state,selected_digest) VALUES(?,?,?)", rusqlite::params![m.id, PluginState::Disabled.as_str(), digest]).map_err(sq)?;
        }
        tx.execute("INSERT INTO plugin_versions VALUES(?,?,?,?,?,?)", rusqlite::params![m.id, m.version, digest, origin, m.to_json().to_string(), bundle]).map_err(sq)?;
        tx.commit().map_err(sq)
    }

    fn linhas(&self, sql: &str, params: &[&dyn rusqlite::ToSql]) -> R<Vec<Map<String, Value>>> {
        let c = self.db.lock();
        let mut rows = orion_store::query_maps(&c, sql, params).map_err(|_| perr("plugin_store_unavailable"))?;
        for r in &mut rows {
            if let Some(Value::String(s)) = r.get("manifest").cloned() {
                r.insert("manifest".into(), serde_json::from_str(&s).unwrap_or(Value::Null));
            }
        }
        Ok(rows)
    }

    pub fn list(&self) -> R<Vec<Map<String, Value>>> {
        self.linhas("SELECT p.*,v.version,v.origin,v.manifest,v.bundle FROM plugins p JOIN plugin_versions v ON v.plugin_id=p.id AND v.digest=p.selected_digest ORDER BY p.id", &[])
    }

    pub fn get(&self, id: &str) -> R<Map<String, Value>> {
        self.list()?.into_iter().find(|r| r["id"].as_str() == Some(id)).ok_or_else(|| perr("plugin_not_found"))
    }

    pub fn versions(&self, id: &str) -> R<Vec<Map<String, Value>>> {
        self.linhas("SELECT * FROM plugin_versions WHERE plugin_id=? ORDER BY rowid DESC", &[&id])
    }

    pub fn version(&self, id: &str, digest: &str) -> R<Map<String, Value>> {
        self.versions(id)?.into_iter().find(|r| r["digest"].as_str() == Some(digest)).ok_or_else(|| perr("plugin_version_not_found"))
    }

    pub fn change_version(&self, id: &str, digest: &str, reviewed_digest: &str, reviewed_capabilities: &HashSet<String>) -> R<Map<String, Value>> {
        let current = self.get(id)?;
        let target = self.version(id, digest)?;
        if current["state"].as_str() == Some("active") || !current["active_digest"].is_null() {
            return Err(perr("plugin_disable_first"));
        }
        let caps: HashSet<String> = target["manifest"]["capabilities"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
        if reviewed_digest != digest || *reviewed_capabilities != caps {
            return Err(perr("plugin_review_required"));
        }
        if current["selected_digest"].as_str() == Some(digest) {
            return Ok(current);
        }
        // um único UPDATE publica a revisão inteira; grants não atravessam update
        self.with_db(|c| c.execute("UPDATE plugins SET previous_digest=selected_digest, selected_digest=?,active_digest=NULL,state=?,error=NULL WHERE id=?", rusqlite::params![digest, PluginState::Disabled.as_str(), id]))?;
        self.get(id)
    }

    pub fn remove(&self, id: &str) -> R<Vec<Map<String, Value>>> {
        let row = self.get(id)?;
        if row["state"].as_str() == Some("active") || !row["active_digest"].is_null() {
            return Err(perr("plugin_disable_first"));
        }
        let versions = self.versions(id)?;
        self.with_db(|c| c.execute("DELETE FROM plugins WHERE id=?", [id]))?;
        Ok(versions)
    }

    pub fn transition(&self, id: &str, state: PluginState, error: Option<&str>) -> R<()> {
        if let Some(e) = error {
            if e.is_empty() || e.len() > 80 || !e.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
                return Err(perr("plugin_error_code_invalid"));
            }
        }
        let existe = self.with_db(|c| c.query_row("SELECT 1 FROM plugins WHERE id=?", [id], |_| Ok(true)).or(Ok(false)))?;
        if !existe {
            return Err(perr("plugin_not_found"));
        }
        let s = state.as_str();
        self.with_db(|c| c.execute("UPDATE plugins SET state=?,error=?,active_digest=CASE WHEN ?='active' THEN selected_digest ELSE NULL END WHERE id=?", rusqlite::params![s, error, s, id]))?;
        Ok(())
    }
}
