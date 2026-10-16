//! Concessões vinculadas ao bundle e ao escopo, sem confiar no manifesto remoto.

use super::plugins::{perr, PluginError, PluginStore};
use std::collections::HashSet;
use std::sync::Arc;

type R<T> = Result<T, PluginError>;

pub struct Grants {
    store: Arc<PluginStore>,
}

fn caps_do_manifesto(v: &serde_json::Value) -> HashSet<String> {
    v["capabilities"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default()
}

impl Grants {
    pub fn new(store: Arc<PluginStore>) -> Self {
        Grants { store }
    }

    pub fn review(&self, id: &str, digest: &str, capabilities: &HashSet<String>, scope: &str) -> R<String> {
        let manifest = self.store.version(id, digest)?["manifest"].clone();
        let permitidas = caps_do_manifesto(&manifest);
        if scope.is_empty() || scope.len() > 128 || !capabilities.is_subset(&permitidas) {
            return Err(perr("plugin_grant_invalid"));
        }
        let generation = orion_store::uuid_hex();
        let mut ord: Vec<&String> = capabilities.iter().collect();
        ord.sort();
        let caps = serde_json::json!(ord).to_string();
        self.store.with_db(|c| c.execute("INSERT OR REPLACE INTO plugin_grants VALUES(?,?,?,?,?)", rusqlite::params![id, digest, scope, caps, generation]))?;
        Ok(generation)
    }

    pub fn effective(&self, id: &str, digest: &str, scope: &str, allowed: Option<&HashSet<String>>) -> R<HashSet<String>> {
        let manifest = self.store.version(id, digest)?["manifest"].clone();
        let row: Option<String> = self.store.with_db(|c| c.query_row("SELECT capabilities FROM plugin_grants WHERE plugin_id=? AND digest=? AND scope=?", rusqlite::params![id, digest, scope], |r| r.get(0)).map(Some).or(Ok(None)))?;
        let granted: HashSet<String> = row.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        let efetivas: HashSet<String> = granted.intersection(&caps_do_manifesto(&manifest)).cloned().collect();
        Ok(match allowed {
            None => efetivas,
            Some(a) => efetivas.intersection(a).cloned().collect(),
        })
    }

    pub fn revoke(&self, id: &str) -> R<()> {
        self.store.with_db(|c| c.execute("DELETE FROM plugin_grants WHERE plugin_id=?", [id]))?;
        Ok(())
    }
}
