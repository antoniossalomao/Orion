//! Organização revisável de cópias; originais preservados, sem scripts/shell.

use crate::calendar_events::Events;
use crate::extensions::host::MCPError;
use crate::tools::registry::{Args, Tool, ToolError};
use orion_policy::{Action, Context, Risk, Status, ToolCall, ToolSpec};
use orion_store::Store;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

const MAX_FILE: u64 = 8 * 1024 * 1024;
const MAX_PLAN: u64 = 32 * 1024 * 1024;

fn grupo(ext: &str) -> Option<&'static str> {
    match ext {
        "md" | "txt" | "csv" => Some("Textos"),
        "pdf" => Some("Documentos"),
        "png" | "jpg" | "jpeg" | "webp" => Some("Imagens"),
        _ => None,
    }
}

type R<T> = Result<T, MCPError>;

fn mcp(e: impl std::fmt::Display) -> MCPError {
    MCPError::new(e.to_string())
}

fn sha(b: &[u8]) -> String {
    hex::encode(Sha256::digest(b))
}

pub struct FilePlans {
    pub events: Arc<Events>,
    memory: Arc<Store>,
}

impl FilePlans {
    pub fn new(events: Arc<Events>) -> Arc<Self> {
        let fp = Arc::new(FilePlans { memory: events.calendar.memory.clone(), events: events.clone() });
        let policy = events.calendar.policy.clone();
        let _ = policy.register_tool(ToolSpec { require_confirmation: true, ..ToolSpec::new("aplicar_organizacao", Risk::Write) }, None);
        let _ = policy.register_tool(ToolSpec::new("propor_organizacao", Risk::Write), None);
        let f = fp.clone();
        let t = Tool::sync(
            "propor_organizacao",
            "Preparar plano de cópias por tipo em Organizados, sem alterar arquivos. Exige pasta autorizada.",
            json!({"type": "object", "properties": {"root": {"type": "string", "maxLength": 4096}}, "required": ["root"], "additionalProperties": false}),
            move |a: Args| {
                let escopo = orion_store::current_scope();
                let root = a.s("root")?;
                let Some(sessao) = escopo.session_id.clone() else { return Err(ToolError::exec("MCPError", "file_plan_session_required")) };
                let row = f.criar(&root, &sessao, escopo.project_id.as_deref()).map_err(|e| ToolError::exec("MCPError", e.code))?;
                Ok(json!({"ok": true, "plan_id": row["id"], "files": row["copies"].as_array().map_or(0, Vec::len), "action": "Revise as cópias e confirme em Fontes. Nenhum arquivo foi criado."}))
            },
        )
        .validando();
        let _ = events.calendar.registry.write().register(t);
        fp
    }

    fn raiz(&self, valor: &str, ctx: &Context) -> R<PathBuf> {
        let expandido = if let Some(r) = valor.strip_prefix("~/") { format!("{}/{r}", crate::downloads::dirs_home().display()) } else { valor.to_string() };
        let root = PathBuf::from(&expandido);
        if !root.is_absolute() || valor.contains('\0') || !root.is_dir() {
            return Err(MCPError::new("file_plan_root_invalid"));
        }
        let mut atual = Some(root.as_path());
        while let Some(p) = atual {
            if fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_symlink()) {
                return Err(MCPError::new("file_plan_symlink_denied"));
            }
            atual = p.parent();
        }
        let root = root.canonicalize().map_err(|_| MCPError::new("file_plan_root_invalid"))?;
        if let Some(pid) = &ctx.project_id {
            let rows = self.memory.query("SELECT root FROM projects WHERE id=?", &[pid]).map_err(mcp)?;
            let proj = rows.first().and_then(|r| r["root"].as_str().map(String::from)).filter(|s| !s.is_empty());
            let ok = proj.and_then(|p| Path::new(&p).canonicalize().ok()).is_some_and(|p| root.starts_with(p));
            if !ok {
                return Err(MCPError::new("file_plan_root_out_of_project"));
            }
        }
        let guard = &self.events.calendar.policy.path_guard;
        let s = root.to_string_lossy().to_string();
        if guard.check_read(&s).is_some() || guard.check_write(&s).is_some() {
            return Err(MCPError::new("file_plan_root_not_authorized"));
        }
        Ok(root)
    }

    fn caminho(&self, root: &Path, relativo: &str) -> R<PathBuf> {
        let parte = Path::new(relativo);
        if parte.is_absolute() || parte.components().any(|c| matches!(c, Component::ParentDir | Component::CurDir)) {
            return Err(MCPError::new("file_plan_path_invalid"));
        }
        let path = root.join(parte);
        // o que existe de `path` precisa continuar dentro de `root`, sem elos simbólicos no caminho
        let mut cursor = path.clone();
        let mut existente = None;
        loop {
            if fs::symlink_metadata(&cursor).is_ok() {
                existente = Some(cursor.clone());
                break;
            }
            if !cursor.pop() || cursor == root {
                break;
            }
        }
        if let Some(e) = existente {
            let mut p = Some(e.as_path());
            while let Some(x) = p {
                if x == root {
                    break;
                }
                if fs::symlink_metadata(x).is_ok_and(|m| m.file_type().is_symlink()) {
                    return Err(MCPError::new("file_plan_symlink_denied"));
                }
                p = x.parent();
            }
            if !e.canonicalize().map_or(false, |c| c.starts_with(root)) {
                return Err(MCPError::new("file_plan_symlink_denied"));
            }
        }
        let guard = &self.events.calendar.policy.path_guard;
        let s = path.to_string_lossy().to_string();
        if guard.check_read(&s).is_some() || guard.check_write(&s).is_some() {
            return Err(MCPError::new("file_plan_path_not_authorized"));
        }
        Ok(path)
    }

    fn ler(path: &Path) -> R<Vec<u8>> {
        let mut opts = fs::OpenOptions::new();
        opts.read(true);
        #[cfg(unix)]
        opts.custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW);
        let mut f = opts.open(path).map_err(|_| MCPError::new("file_plan_read_failed"))?;
        let m = f.metadata().map_err(|_| MCPError::new("file_plan_read_failed"))?;
        #[cfg(unix)]
        let links_ok = m.nlink() == 1;
        #[cfg(not(unix))]
        let links_ok = !fs::symlink_metadata(path).map_or(true, |sm| sm.file_type().is_symlink());
        if !m.is_file() || !links_ok || m.len() > MAX_FILE {
            return Err(MCPError::new("file_plan_file_limit_or_link"));
        }
        let mut data = vec![];
        (&mut f).take(MAX_FILE + 1).read_to_end(&mut data).map_err(|_| MCPError::new("file_plan_read_failed"))?;
        if data.len() as u64 > MAX_FILE {
            return Err(MCPError::new("file_plan_file_limit_or_link"));
        }
        Ok(data)
    }

    pub fn criar(&self, root_in: &str, session_id: &str, project_id: Option<&str>) -> R<Value> {
        let ctx = self.events.context(session_id, project_id)?;
        let root = self.raiz(root_in, &ctx)?;
        let mut entradas = vec![];
        for e in fs::read_dir(&root).map_err(|_| MCPError::new("file_plan_root_invalid"))?.flatten() {
            if entradas.len() >= 1000 {
                return Err(MCPError::new("file_plan_listing_limit"));
            }
            entradas.push(e.path());
        }
        entradas.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default());
        let (mut copies, mut skipped, mut total) = (vec![], vec![], 0u64);
        for path in entradas {
            let nome = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            let Some(g) = grupo(&ext) else { continue };
            if !path.is_file() {
                continue;
            }
            let r: R<Value> = (|| {
                if nome.chars().count() > 160 {
                    return Err(MCPError::new("file_plan_name_limit"));
                }
                let fonte = self.caminho(&root, &nome)?;
                let raw = Self::ler(&fonte).map_err(|e| if e.code == "file_plan_read_failed" { MCPError::new("file_plan_read_failed") } else { e })?;
                let alvo = format!("Organizados/{g}/{nome}");
                let dest = self.caminho(&root, &alvo)?;
                if dest.exists() {
                    return Err(MCPError::new("file_plan_target_exists"));
                }
                total += raw.len() as u64;
                if total > MAX_PLAN || copies.len() >= 100 {
                    return Err(MCPError::new("file_plan_size_limit"));
                }
                Ok(json!({"source": nome, "target": alvo, "digest": sha(&raw), "size": raw.len()}))
            })();
            match r {
                Ok(c) => copies.push(c),
                Err(e) => skipped.push(json!({"name": nome, "reason": e.code})),
            }
        }
        if copies.is_empty() {
            return Err(MCPError::new("file_plan_no_eligible_files"));
        }
        let payload = orion_policy::pyjson::dumps(&json!({"root": root.to_string_lossy(), "copies": copies}), true);
        let id = orion_store::uuid_hex();
        let (digest, sk, agora) = (sha(payload.as_bytes()), json!(skipped).to_string(), (self.memory.clock())());
        let ok = self
            .memory
            .tx(|c| {
                let n: i64 = c.query_row("SELECT count(*) FROM file_plans", [], |r| r.get(0))?;
                if n >= 500 {
                    return Ok(false);
                }
                c.execute("INSERT INTO file_plans(id,project_id,session_id,payload,digest,skipped,status,created_at) VALUES (?,?,?,?,?,?,?,?)", rusqlite::params![id, project_id, session_id, payload, digest, sk, "draft", agora])?;
                Ok(true)
            })
            .map_err(mcp)?;
        if !ok {
            return Err(MCPError::new("file_plan_limit"));
        }
        self.get(&id, project_id)
    }

    pub fn get(&self, id: &str, project_id: Option<&str>) -> R<Value> {
        let rows = self.memory.query("SELECT * FROM file_plans WHERE id=? AND project_id IS ?", &[&id, &project_id]).map_err(mcp)?;
        let mut row = rows.into_iter().next().ok_or_else(|| MCPError::new("file_plan_not_found"))?;
        let payload: Map<String, Value> = serde_json::from_str(row.remove("payload").and_then(|v| v.as_str().map(String::from)).unwrap_or_default().as_str()).unwrap_or_default();
        row.extend(payload);
        let sk: Value = serde_json::from_str(row["skipped"].as_str().unwrap_or("[]")).unwrap_or(json!([]));
        row.insert("skipped".into(), sk);
        row.insert("approvals".into(), json!([]));
        if matches!(row["status"].as_str(), Some("draft" | "pending")) {
            match self.contexto(&row, project_id) {
                Ok(ctx) => {
                    let binding = self.events.calendar.policy.binding("aplicar_organizacao", &ctx);
                    let sid = row["session_id"].as_str().unwrap_or("").to_string();
                    let a: Vec<Value> = self
                        .events
                        .calendar
                        .policy
                        .approvals
                        .pending(None, true)
                        .into_iter()
                        .filter(|a| a.tool == "aplicar_organizacao" && a.args.get("plan_id") == Some(&json!(id)) && a.binding == binding && a.session_id == sid)
                        .map(|a| json!({"id": a.id, "status": a.status.as_str()}))
                        .collect();
                    row.insert("approvals".into(), json!(a));
                }
                Err(_) => {
                    row.insert("review_error".into(), json!("file_plan_context_changed"));
                }
            }
        }
        Ok(Value::Object(row))
    }

    pub fn listar(&self, project_id: Option<&str>) -> R<Vec<Value>> {
        let ids = self.memory.query("SELECT id FROM file_plans WHERE project_id IS ? ORDER BY created_at DESC LIMIT 100", &[&project_id]).map_err(mcp)?;
        ids.iter().map(|r| self.get(r["id"].as_str().unwrap_or(""), project_id)).collect()
    }

    fn contexto(&self, row: &Map<String, Value>, project_id: Option<&str>) -> R<Context> {
        if !matches!(row["status"].as_str(), Some("draft" | "pending")) {
            return Err(MCPError::new("file_plan_already_submitted"));
        }
        let mut ctx = self.events.context(row["session_id"].as_str().unwrap_or(""), project_id)?;
        self.raiz(row["root"].as_str().unwrap_or(""), &ctx)?;
        ctx.authorities = vec![format!("file-plan:{}:{}", row["id"].as_str().unwrap_or(""), row["digest"].as_str().unwrap_or(""))];
        Ok(ctx)
    }

    pub fn revisar(&self, id: &str, project_id: Option<&str>, digest: &str) -> R<Value> {
        let row = self.get(id, project_id)?;
        let row = row.as_object().cloned().unwrap_or_default();
        if row["digest"].as_str() != Some(digest) {
            return Err(MCPError::new("file_plan_review_changed"));
        }
        let ctx = self.contexto(&row, project_id)?;
        let d = self.events.calendar.policy.evaluate_opts(&ToolCall::new("aplicar_organizacao", json!({"plan_id": id, "digest": digest})), &ctx, false);
        if d.action != Action::Confirm {
            return Err(MCPError::new("file_plan_review_denied"));
        }
        self.memory.tx(|c| Ok(c.execute("UPDATE file_plans SET status='pending' WHERE id=?", [id])?)).map_err(mcp)?;
        Ok(json!({"approval_id": d.approval_id}))
    }

    pub fn aplicar(&self, id: &str, project_id: Option<&str>, approval_id: &str) -> R<Value> {
        let row = self.get(id, project_id)?;
        let row = row.as_object().cloned().unwrap_or_default();
        let ctx = self.contexto(&row, project_id)?;
        let policy = &self.events.calendar.policy;
        let args = json!({"plan_id": id, "digest": row["digest"]});
        let aprovada = policy.approvals.get(approval_id).is_some_and(|a| a.status == Status::Approved && a.tool == "aplicar_organizacao" && Value::Object(a.args.clone()) == args && a.session_id == ctx.session_id);
        if !aprovada {
            return Err(MCPError::new("file_plan_approval_unavailable"));
        }
        let root = self.raiz(row["root"].as_str().unwrap_or(""), &ctx)?;
        // valida e congela cada byte revisado antes de criar a primeira pasta/arquivo
        let mut frozen: Vec<(PathBuf, Vec<u8>)> = vec![];
        for item in row["copies"].as_array().cloned().unwrap_or_default() {
            let fonte = self.caminho(&root, item["source"].as_str().unwrap_or(""))?;
            let alvo = self.caminho(&root, item["target"].as_str().unwrap_or(""))?;
            let raw = Self::ler(&fonte).map_err(|_| MCPError::new("file_plan_snapshot_changed"))?;
            if Some(sha(&raw).as_str()) != item["digest"].as_str() || Some(raw.len() as u64) != item["size"].as_u64() || alvo.exists() {
                return Err(MCPError::new("file_plan_snapshot_changed"));
            }
            frozen.push((alvo, raw));
        }
        if policy.evaluate(&ToolCall::new("aplicar_organizacao", args), &ctx).action != Action::Allow {
            return Err(MCPError::new("file_plan_approval_unavailable"));
        }
        let n = self.memory.tx(|c| Ok(c.execute("UPDATE file_plans SET status='applying' WHERE id=? AND status IN ('draft','pending')", [id])?)).map_err(mcp)?;
        if n != 1 {
            return Err(MCPError::new("file_plan_already_submitted"));
        }
        let mut status = "partial";
        let r = (|| -> R<()> {
            for (alvo, raw) in &frozen {
                let rel = alvo.strip_prefix(&root).map_err(|_| MCPError::new("file_plan_path_invalid"))?.to_string_lossy().to_string();
                self.caminho(&root, &rel)?;
                fs::create_dir_all(alvo.parent().unwrap_or(&root)).map_err(|_| MCPError::new("file_plan_partial_failure"))?;
                self.caminho(&root, &rel)?;
                let mut opts = fs::OpenOptions::new();
                opts.write(true).create_new(true);
                #[cfg(unix)]
                opts.mode(0o600).custom_flags(libc::O_NOFOLLOW);
                let mut f = opts.open(alvo).map_err(|_| MCPError::new("file_plan_partial_failure"))?;
                f.write_all(raw).map_err(|_| MCPError::new("file_plan_partial_failure"))?;
                self.memory.tx(|c| Ok(c.execute("UPDATE file_plans SET completed=completed+1 WHERE id=?", [id])?)).map_err(mcp)?;
            }
            Ok(())
        })();
        if r.is_ok() {
            status = "complete";
        }
        let _ = self.memory.tx(|c| Ok(c.execute("UPDATE file_plans SET status=? WHERE id=?", rusqlite::params![status, id])?));
        r.map(|_| json!({"ok": true, "status": status, "copies": frozen.len()}))
    }
}
