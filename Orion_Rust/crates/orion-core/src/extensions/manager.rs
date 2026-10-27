//! Lifecycle administrativo de pacotes. Instalar nunca executa o pacote.

use super::catalog::Catalog;
use super::catalogo::Catalogos;
use super::grants::Grants;
use super::host::{parse_config, ConnectionConfig, MCPError, MCPHost};
use super::installer::Installer;
use super::plugins::{perr, PluginError, PluginManifest, PluginState, PluginStore};
use super::skill_runtime::SkillRuntime;
use super::skills::{metadata, SkillError};
use orion_policy::{PolicyEngine, Risk};
use parking_lot::{Mutex, RwLock};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

type R<T> = Result<T, PluginError>;

impl From<MCPError> for PluginError {
    fn from(e: MCPError) -> Self {
        PluginError(e.code)
    }
}

impl From<SkillError> for PluginError {
    fn from(e: SkillError) -> Self {
        PluginError(e.0)
    }
}

pub fn public_plugin(row: &Map<String, Value>) -> Value {
    let m = &row["manifest"];
    let origem = match row["origin"].as_str() {
        Some(o @ ("local" | "orion" | "catalogo")) => o,
        _ => "imported",
    };
    let conexoes: Vec<Value> = m["mcp"].as_array().map(|a| a.iter().map(|c| json!({"id": c["id"], "transport": c["transport"], "tools": c["tools"]})).collect()).unwrap_or_default();
    json!({"id": row["id"], "state": row["state"], "selected_digest": row["selected_digest"], "previous_digest": row["previous_digest"],
           "version": row["version"], "error": row["error"], "name": m["name"], "description": m["description"], "license": m["license"],
           "capabilities": m["capabilities"], "skills": m["skills"], "connections": conexoes, "origin": origem})
}

fn python() -> PathBuf {
    std::env::var_os("PATH")
        .and_then(|p| std::env::split_paths(&p).map(|d| d.join("python3")).find(|c| c.is_file()))
        .unwrap_or_else(|| PathBuf::from("/usr/bin/python3"))
}

/// Revisão já concedida nesta sessão; só usada para substituir o núcleo em processo.
#[derive(Clone)]
pub struct RuntimeReview {pub id:String,pub digest:String,pub capabilities:HashSet<String>,pub trusted_local:bool,pub authorized_remote:bool,pub classifications:Classificacoes,pub scope:String}
pub struct PluginManager {
    runtime_reviews:Mutex<HashMap<String,RuntimeReview>>,
    pub store: Arc<PluginStore>,
    pub installer: Installer,
    pub grants: Grants,
    pub skills: Arc<SkillRuntime>,
    pub host: Arc<MCPHost>,
    pub catalog: Arc<Catalog>,
    pub policy: Arc<PolicyEngine>,
    pub connections: Mutex<HashMap<String, HashSet<String>>>,
    owned_skills: Mutex<HashMap<String, HashSet<String>>>,
    active_scopes: Mutex<HashMap<String, String>>,
    pub lock: tokio::sync::Mutex<()>,
    pub catalogos: RwLock<Option<Arc<Catalogos>>>,
}

/// classificações revistas: conexão → ferramenta → classe
pub type Classificacoes = HashMap<String, HashMap<String, Risk>>;

impl PluginManager {
    pub fn new(root: &Path, skills: Arc<SkillRuntime>, host: Arc<MCPHost>, catalog: Arc<Catalog>, policy: Arc<PolicyEngine>) -> R<Self> {
        Self::with_runtime_state(root,skills,host,catalog,policy,false)
    }
    pub fn with_runtime_state(root:&Path,skills:Arc<SkillRuntime>,host:Arc<MCPHost>,catalog:Arc<Catalog>,policy:Arc<PolicyEngine>,preserve:bool)->R<Self>{
        let store = Arc::new(PluginStore::open(&root.join("plugins.db"))?);
        let installer = Installer::new(root, store.clone())?;
        let grants = Grants::new(store.clone());
        let salvas: Vec<String> = store.with_db(|c| {
            let mut st = c.prepare("SELECT config FROM extension_connections")?;
            let r = st.query_map([], |r| r.get::<_, String>(0))?.flatten().collect();
            Ok(r)
        })?;
        for raw in salvas {
            // configuração inválida não executa código no startup
            let Ok(mut v) = serde_json::from_str::<Value>(&raw) else { continue };
            v["enabled"] = json!(false);
            if let Ok(c) = parse_config(&v) {
                if !host.contains(c.id()) && host.len() < 32 {
                    host.insert(host.connection(c));
                }
            }
        }
        // reinício não autoriza executar código/conectar conta silenciosamente
        if !preserve{for row in store.list()? {
            store.transition(row["id"].as_str().unwrap_or(""), PluginState::Disabled, None)?;
        }}
        Ok(PluginManager { runtime_reviews:Default::default(), store, installer, grants, skills, host, catalog, policy, connections: Default::default(), owned_skills: Default::default(), active_scopes: Default::default(), lock: tokio::sync::Mutex::new(()), catalogos: RwLock::new(None) })
    }

    pub fn save_connection(&self, config: &ConnectionConfig) -> R<()> {
        let (id, cfg) = (config.id().to_string(), config.to_json().to_string());
        self.store.with_db(|c| c.execute("INSERT OR REPLACE INTO extension_connections VALUES(?,?)", rusqlite::params![id, cfg]))?;
        Ok(())
    }

    pub fn forget_connection(&self, id: &str) -> R<()> {
        self.store.with_db(|c| c.execute("DELETE FROM extension_connections WHERE id=?", [id]))?;
        Ok(())
    }

    pub fn list(&self) -> R<Vec<Value>> {
        let escopos = self.active_scopes.lock().clone();
        Ok(self
            .store
            .list()?
            .iter()
            .map(|r| {
                let mut p = public_plugin(r);
                p["scope"] = json!(escopos.get(r["id"].as_str().unwrap_or("")));
                p
            })
            .collect())
    }

    pub fn runtime_reviews(&self)->Vec<RuntimeReview>{self.runtime_reviews.lock().values().cloned().collect()}
    pub fn retain_runtime_review(&self,review:RuntimeReview){self.runtime_reviews.lock().insert(review.id.clone(),review);}
    pub fn dona_da_conexao(&self, cid: &str) -> bool {
        self.connections.lock().values().any(|s| s.contains(cid))
    }

    pub async fn deactivate(&self, id: &str, revoke: bool) -> R<Value> {
        self.store.get(id)?;
        self.runtime_reviews.lock().remove(id);
        self.store.transition(id, PluginState::Disabled, None)?;
        self.active_scopes.lock().remove(id);
        let donas = self.owned_skills.lock().remove(id).unwrap_or_default();
        {
            let mut g = self.skills.inner.write();
            for sid in donas {
                if let Some(a) = g.authorities.remove(&sid) {
                    self.policy.approvals.invalidate_binding(&a);
                }
                g.enabled.remove(&sid);
                g.scopes.remove(&sid);
                g.contexts.remove(&sid);
                g.script_reviews.remove(&sid);
                g.index.skills.remove(&sid);
            }
        }
        // scripts compartilham o runner: cancelamento conservador não deixa execução órfã
        self.skills.runner.close();
        self.skills.runner.reabrir();
        let cids = self.connections.lock().remove(id).unwrap_or_default();
        for cid in cids {
            if let Some(c) = self.host.remove(&cid) {
                c.close().await;
            }
        }
        let _ = self.catalog.refresh().await;
        if revoke {
            self.grants.revoke(id)?;
        }
        self.store.transition(id, PluginState::Disabled, None)?;
        Ok(public_plugin(&self.store.get(id)?))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn activate(&self, id: &str, digest: &str, capabilities: &HashSet<String>, trusted_local: bool, authorized_remote: bool, classifications: &Classificacoes, scope: &str) -> R<Value> {
        match self.activate_inner(id, digest, capabilities, trusted_local, authorized_remote, classifications, scope).await {
            Ok(v) => {
                if v["state"].as_str()==Some("active"){self.retain_runtime_review(RuntimeReview{id:id.into(),digest:digest.into(),capabilities:capabilities.clone(),trusted_local,authorized_remote,classifications:classifications.clone(),scope:scope.into()});}
                Ok(v)
            },
            Err(e) => {
                if e.0 != "plugin_review_required" && e.0 != "plugin_dependency_unavailable" && e.0 != "plugin_not_found" && e.0 != "bundle_path_invalid" && e.0 != "plugin_version_not_found" {
                    let _ = self.deactivate(id, true).await;
                    let _ = self.store.transition(id, PluginState::Error, Some("plugin_activation_failed"));
                }
                Err(e)
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn activate_inner(&self, id: &str, digest: &str, capabilities: &HashSet<String>, trusted_local: bool, authorized_remote: bool, classifications: &Classificacoes, scope: &str) -> R<Value> {
        let row = self.store.get(id)?;
        if row["selected_digest"].as_str() != Some(digest) {
            return Err(perr("plugin_review_required"));
        }
        let (inst_root, i_id, i_digest) = (&self.installer, id.to_string(), digest.to_string());
        let root = tokio::task::block_in_place(|| inst_root.bundle(&i_id, &i_digest))?;
        let manifest = PluginManifest::from_value(&row["manifest"])?;
        for d in &manifest.dependencies {
            let inst = self.store.get(&d.id)?;
            if inst["state"].as_str() != Some("active") || inst["version"].as_str() != Some(&d.version) {
                return Err(perr("plugin_dependency_unavailable"));
            }
        }
        self.deactivate(id, true).await?;
        let generation = self.grants.review(id, digest, capabilities, scope)?;
        let effective = self.grants.effective(id, digest, scope, None)?;
        let nativa_ausente = effective.iter().any(|c| c.strip_prefix("native:").is_some_and(|n| self.policy.spec(n).is_none() || self.catalog.registry.read().get(n).is_none()));
        if nativa_ausente {
            self.store.transition(id, PluginState::Waiting, Some("capability_unavailable"))?;
            return Ok(public_plugin(&self.store.get(id)?));
        }
        self.connections.lock().insert(id.into(), HashSet::new());
        for remote in &manifest.mcp {
            let escolhidas: Vec<&String> = remote.tools.keys().filter(|n| effective.contains(&format!("mcp:{}:{n}", remote.id))).collect();
            if escolhidas.is_empty() && !effective.contains(&format!("resource:{}", remote.id)) && !effective.contains(&format!("prompt:{}", remote.id)) {
                continue;
            }
            let vazio = HashMap::new();
            let revisoes = classifications.get(&remote.id).unwrap_or(&vazio);
            if !escolhidas.iter().all(|n| revisoes.contains_key(*n)) || (remote.transport == "stdio" && !trusted_local) || (remote.transport == "http" && !authorized_remote) {
                self.deactivate(id, false).await?;
                self.store.transition(id, PluginState::Waiting, None)?;
                return Ok(public_plugin(&self.store.get(id)?));
            }
            let cid = format!("p_{}", &hex::encode(Sha256::digest(format!("{id}:{}", remote.id).as_bytes()))[..40]);
            if self.host.contains(&cid) {
                return Err(perr("plugin_connection_collision"));
            }
            let cls: Map<String, Value> = escolhidas.iter().map(|n| ((*n).clone(), json!(revisoes[*n].as_str()))).collect();
            let recursos = if effective.contains(&format!("resource:{}", remote.id)) { remote.resources.clone() } else { vec![] };
            let prompts = if effective.contains(&format!("prompt:{}", remote.id)) { remote.prompts.clone() } else { vec![] };
            let mut cfg = json!({"id": cid, "scope": scope, "enabled": true, "classifications": cls, "resources": recursos, "prompts": prompts});
            if remote.transport == "stdio" {
                cfg["transport"] = json!("stdio");
                cfg["command"] = json!(python().to_string_lossy());
                cfg["args"] = json!(["-I", root.join(remote.entrypoint.as_deref().unwrap_or("")).to_string_lossy()]);
                cfg["cwd"] = json!(root.to_string_lossy());
                cfg["trusted"] = json!(true);
            } else {
                cfg["transport"] = json!("http");
                cfg["url"] = json!(remote.url);
                cfg["secret_ref"] = json!(remote.secret_ref);
                cfg["authorized"] = json!(true);
            }
            let config = parse_config(&cfg).map_err(|_| perr("manifest_invalid"))?;
            let conn = self.host.connection(config);
            *conn.label.write() = format!("{}/{}", manifest.name, remote.id);
            let (store, pid, pdig) = (self.store.clone(), id.to_string(), digest.to_string());
            *conn.authorized.lock() = Some(Arc::new(move || store.get(&pid).is_ok_and(|r| r["state"].as_str() == Some("active") && r["selected_digest"].as_str() == Some(&pdig))));
            self.host.insert(conn.clone());
            self.connections.lock().entry(id.into()).or_default().insert(cid);
            conn.start().await;
            if conn.state() != "connected" {
                return Err(perr("plugin_connection_failed"));
            }
        }
        self.catalog.refresh().await?;
        let mut allowed: HashSet<String> = effective.iter().filter_map(|c| c.strip_prefix("native:")).filter(|n| self.policy.spec(n).is_some()).map(String::from).collect();
        {
            let donas = self.connections.lock().get(id).cloned().unwrap_or_default();
            for (name, e) in self.catalog.entries.read().iter() {
                if donas.contains(&e.connection) {
                    allowed.insert(name.clone());
                }
            }
        }
        // scripts de pacotes aguardam runner próprio por revisão; instalação não basta
        if effective.contains("scripts") {
            return Err(perr("plugin_scripts_require_local_source"));
        }
        let mut preparadas = vec![];
        for rel in &manifest.skills {
            let (raiz, ns, origem) = (root.join(rel), id.to_string(), format!("plugin:{id}"));
            let skill = tokio::task::spawn_blocking(move || metadata(&raiz, &ns, &origem).and_then(|s| s.load().map(|_| s))).await.map_err(|_| perr("plugin_activation_failed"))??;
            if self.skills.inner.read().index.skills.contains_key(&skill.id()) {
                return Err(perr("plugin_skill_collision"));
            }
            preparadas.push(skill);
        }
        let mut donas = HashSet::new();
        {
            let mut g = self.skills.inner.write();
            for skill in preparadas {
                let sid = skill.id();
                g.index.add(skill)?;
                donas.insert(sid.clone());
                g.enabled.insert(sid.clone());
                g.scopes.insert(sid.clone(), allowed.clone());
                g.contexts.insert(sid.clone(), scope.into());
                g.authorities.insert(sid, format!("{id}:{digest}:{generation}:{scope}"));
            }
        }
        self.owned_skills.lock().insert(id.into(), donas);
        self.store.transition(id, PluginState::Active, None)?;
        self.active_scopes.lock().insert(id.into(), scope.into());
        Ok(public_plugin(&self.store.get(id)?))
    }

    pub async fn close(&self) {
        let ids: HashSet<String> = self.connections.lock().keys().cloned().chain(self.owned_skills.lock().keys().cloned()).collect();
        for id in ids {
            let _ = self.deactivate(&id, true).await;
        }
    }
}

