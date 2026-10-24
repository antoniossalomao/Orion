//! Projetos persistentes; arquivar nunca remove conversas ou fontes.

use orion_store::sessions::Session;
use orion_store::{query_maps, Store, StoreError};
use serde_json::{Map, Value};
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Clone, thiserror::Error)]
#[error("{0}")]
pub struct ProjectError(pub String);

impl From<StoreError> for ProjectError {
    fn from(e: StoreError) -> Self {
        ProjectError(e.to_string())
    }
}

type R<T> = Result<T, ProjectError>;

#[derive(Clone)]
pub struct Projects {
    pub memory: Arc<Store>,
}

impl Projects {
    pub fn new(memory: Arc<Store>) -> Self {
        Projects { memory }
    }

    pub fn get(&self, id: &str) -> R<Map<String, Value>> {
        let mut row = self
            .memory
            .query("SELECT * FROM projects WHERE id=?", &[&id])?
            .into_iter()
            .next()
            .ok_or_else(|| ProjectError("project_not_found".into()))?;
        for k in ["archived", "share_personal"] {
            let b = row.get(k).and_then(Value::as_i64).unwrap_or(0) != 0;
            row.insert(k.into(), Value::Bool(b));
        }
        Ok(row)
    }

    pub fn archived(&self, id: &str) -> bool {
        self.get(id).map(|p| p["archived"] == Value::Bool(true)).unwrap_or(true)
    }

    pub fn list(&self, archived: bool) -> R<Vec<Map<String, Value>>> {
        let ids = self.memory.query("SELECT id FROM projects WHERE archived=? ORDER BY updated_at DESC,id", &[&(archived as i64)])?;
        ids.iter().map(|r| self.get(r["id"].as_str().unwrap_or(""))).collect()
    }

    pub fn validate_root(root: Option<&str>) -> R<Option<String>> {
        let Some(root) = root else { return Ok(None) };
        let p = Path::new(root);
        if !p.is_absolute() || !p.is_dir() {
            return Err(ProjectError("project_root_invalid".into()));
        }
        Ok(Some(orion_policy::paths::realpath(p).to_string_lossy().into_owned()))
    }

    pub fn create(&self, name: &str, instructions: &str, share_personal: bool, root: Option<&str>) -> R<Map<String, Value>> {
        Self::validate_text(name, instructions)?;
        let (id, now) = (orion_store::uuid_hex(), self.memory.now());
        let root = Self::validate_root(root)?;
        self.memory.tx(|c| {
            c.execute(
                "INSERT INTO projects(id,name,instructions,share_personal,archived,created_at,updated_at,root) VALUES(?,?,?,?,?,?,?,?)",
                rusqlite_params(&[&id, &name.trim(), &instructions, &(share_personal as i64), &0i64, &now, &now, &root]),
            )?;
            Ok(())
        })?;
        self.get(&id)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &self,
        id: &str,
        name: Option<&str>,
        instructions: Option<&str>,
        share_personal: Option<bool>,
        archived: Option<bool>,
        root: Option<&str>,
        root_set: bool,
    ) -> R<Map<String, Value>> {
        let old = self.get(id)?;
        let root = if root_set { Self::validate_root(root)? } else { old["root"].as_str().map(String::from) };
        let b = |k: &str| old[k] == Value::Bool(true);
        let nome = name.map(|n| n.trim().to_string()).unwrap_or_else(|| old["name"].as_str().unwrap_or("").into());
        let instr = instructions.map(String::from).unwrap_or_else(|| old["instructions"].as_str().unwrap_or("").into());
        Self::validate_text(&nome, &instr)?;
        let share = share_personal.unwrap_or(b("share_personal")) as i64;
        let arq = archived.unwrap_or(b("archived")) as i64;
        let now = self.memory.now();
        self.memory.tx(|c| {
            if archived == Some(true) && !b("archived") {
                c.execute("INSERT OR IGNORE INTO project_session_archives SELECT id,project_id FROM sessions WHERE project_id=? AND archived=0", [id])?;
                c.execute("UPDATE sessions SET archived=1 WHERE project_id=?", [id])?;
                c.execute("UPDATE active_sessions SET session_id=NULL WHERE session_id IN (SELECT id FROM sessions WHERE project_id=?)", [id])?;
            } else if archived == Some(false) && b("archived") {
                c.execute("UPDATE sessions SET archived=0 WHERE id IN (SELECT session_id FROM project_session_archives WHERE project_id=?)", [id])?;
                c.execute("DELETE FROM project_session_archives WHERE project_id=?", [id])?;
            }
            c.execute(
                "UPDATE projects SET name=?,instructions=?,share_personal=?,archived=?,updated_at=?,root=? WHERE id=?",
                rusqlite_params(&[&nome, &instr, &share, &arq, &now, &root, &id]),
            )?;
            Ok(())
        })?;
        self.get(id)
    }

    fn validate_text(name: &str, instructions: &str) -> R<()> {
        if name.trim().is_empty() || name.chars().count() > 120 || name.chars().any(char::is_control) || instructions.chars().count() > 16000 {
            return Err(ProjectError("project_text_invalid".into()));
        }
        Ok(())
    }

    pub fn associate(&self, channel: &str, session_id: &str, project_id: Option<&str>) -> R<Session> {
        let s = self.memory.get_session(session_id)?.filter(|s|s.channel==channel)
            .ok_or_else(||ProjectError("session_not_found".into()))?;
        if s.archived || s.read_only { return Err(ProjectError("session_read_only".into())); }
        if let Some(p) = project_id {
            if self.get(p)?["archived"] == Value::Bool(true) {
                return Err(ProjectError("project_archived".into()));
            }
        }
        self.memory.tx(|c| {
            // Mover só a conversa não pode deixar dados no escopo antigo
            // (REGRAS.md: o que é de um projeto fica no projeto).
            if s.project_id.as_deref() != project_id {
                for table in ["artifacts", "file_plans", "event_proposals", "message_branches"] {
                    let count: i64 = c.query_row(&format!("SELECT count(*) FROM {table} WHERE session_id=?"), [session_id], |r| r.get(0))?;
                    if count > 0 { return orion_store::invalid("session_has_scoped_data"); }
                }
                let linked: i64 = c.query_row("SELECT count(*) FROM message_branches WHERE root_session=? OR parent_session=?", [session_id,session_id], |r|r.get(0))?;
                if linked>0 { return orion_store::invalid("session_has_scoped_data"); }
                c.execute("DELETE FROM session_summaries WHERE session_id=?",[session_id])?;
                c.execute("DELETE FROM project_session_archives WHERE session_id=?",[session_id])?;
            }
            c.execute("UPDATE sessions SET project_id=? WHERE id=?", rusqlite_params(&[&project_id, &session_id]))?;
            c.execute("UPDATE sessions SET last_active_at=? WHERE id=?", rusqlite_params(&[&self.memory.now(), &session_id]))?;
            Ok(())
        })?;
        Ok(self.memory.get_session(session_id)?.ok_or_else(|| ProjectError("session_not_found".into()))?)
    }

    pub fn activate(&self, channel: &str, project_id: Option<&str>) -> R<Session> {
        if let Some(p) = project_id {
            if self.get(p)?["archived"] == Value::Bool(true) {
                return Err(ProjectError("project_archived".into()));
            }
        }
        let c = self.memory.conn();
        let candidatos = query_maps(
            &c,
            "SELECT s.id FROM sessions s WHERE s.channel=? AND s.project_id IS ? AND s.archived=0
             AND NOT EXISTS(SELECT 1 FROM imported WHERE kind='sessao' AND ref=s.id)
             ORDER BY s.last_active_at DESC,s.created_at DESC,s.id DESC LIMIT 1",
            &[&channel, &project_id],
        )?;
        drop(c);
        if let Some(r) = candidatos.first() {
            return Ok(self.memory.activate_session(channel, r["id"].as_str().unwrap_or(""))?);
        }
        Ok(self.memory.new_session(channel, None, project_id)?)
    }
}

fn rusqlite_params<'a>(p: &'a [&'a dyn orion_store::rusqlite::ToSql]) -> &'a [&'a dyn orion_store::rusqlite::ToSql] {
    p
}
