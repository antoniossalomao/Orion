//! Propostas de Agenda: o modelo só prepara; a criação exige revisão de produto.

use crate::calendar::{iso_com_fuso, Calendar};
use crate::extensions::host::{Connection, MCPError};
use crate::projects::Projects;
use crate::tools::registry::{Args, Tool, ToolError};
use orion_policy::{Action, Context, Risk, Status, ToolCall, ToolSpec};
use orion_store::Store;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::sync::Arc;

pub struct Event {
    pub title: String,
    pub start: String,
    pub end: String,
    pub description: String,
    pub location: String,
}

impl Event {
    /// Mesmas regras do modelo pydantic (`extra=forbid`, limites, período ≤ 7 dias com fuso).
    pub fn parse(v: &Value, permitir_sessao: bool) -> Result<Self, String> {
        let o = v.as_object().ok_or("esperava objeto")?;
        let ok = ["title", "start", "end", "description", "location"];
        if let Some(k) = o.keys().find(|k| !ok.contains(&k.as_str()) && !(permitir_sessao && *k == "session_id")) {
            return Err(format!("campo desconhecido: {k}"));
        }
        let s = |k: &str| v.get(k).and_then(Value::as_str).map(String::from);
        let (title, start, end) = (s("title").ok_or("title obrigatório")?, s("start").ok_or("start obrigatório")?, s("end").ok_or("end obrigatório")?);
        let (description, location) = (s("description").unwrap_or_default(), s("location").unwrap_or_default());
        if title.is_empty() || title.chars().count() > 200 || start.len() > 100 || end.len() > 100 || description.chars().count() > 8000 || location.chars().count() > 500 {
            return Err("campo fora dos limites".into());
        }
        let (Some(a), Some(b)) = (iso_com_fuso(&start), iso_com_fuso(&end)) else { return Err("event_period_invalid".into()) };
        let secs = (b - a).num_seconds();
        if secs <= 0 || secs > 7 * 86400 {
            return Err("event_period_invalid".into());
        }
        Ok(Event { title, start, end, description, location })
    }
}

pub fn esquema_evento() -> Value {
    json!({"type": "object", "additionalProperties": false, "required": ["title", "start", "end"], "properties": {
        "title": {"type": "string", "minLength": 1, "maxLength": 200},
        "start": {"type": "string", "maxLength": 100},
        "end": {"type": "string", "maxLength": 100},
        "description": {"type": "string", "maxLength": 8000, "default": ""},
        "location": {"type": "string", "maxLength": 500, "default": ""}}})
}

fn mcp(e: impl std::fmt::Display) -> MCPError {
    MCPError::new(e.to_string())
}

pub struct Events {
    pub calendar: Arc<Calendar>,
    pub memory: Arc<Store>,
}

type R<T> = Result<T, MCPError>;

impl Events {
    pub fn new(calendar: Arc<Calendar>) -> Arc<Self> {
        let ev = Arc::new(Events { memory: calendar.memory.clone(), calendar: calendar.clone() });
        let _ = calendar.policy.register_tool(ToolSpec { require_confirmation: true, ..ToolSpec::new("criar_evento_agenda", Risk::Write) }, None);
        // a operação real de criação não está no registro do modelo
        let e = ev.clone();
        let t = Tool::new("propor_evento_agenda", "Preparar proposta de evento com título, horários e descrição para revisão; sem enviar ou criar.", esquema_evento(), move |a: Args| {
            let e = e.clone();
            async move {
                let escopo = orion_store::current_scope();
                let Some(sessao) = escopo.session_id.clone() else { return Ok(json!({"ok": false, "code": "proposal_session_required"})) };
                let ev = Event::parse(&Value::Object(a.0), false).map_err(|m| ToolError::valor(m))?;
                let row = e.criar(&sessao, &ev, escopo.project_id.as_deref());
                Ok(match row {
                    Ok(r) => json!({"ok": true, "proposal": r, "action": "Abra Agenda em Integrações para revisar. Nenhum evento foi criado."}),
                    Err(m) => json!({"ok": false, "code": m.code}),
                })
            }
        })
        .validando();
        let _ = calendar.registry.write().register(t);
        let _ = calendar.policy.register_tool(ToolSpec::new("propor_evento_agenda", Risk::Write), None);
        ev
    }

    pub fn context(&self, session_id: &str, project_id: Option<&str>) -> R<Context> {
        let s = self.memory.get_session(session_id).ok().flatten();
        let Some(s) = s.filter(|s| s.channel == "web" && s.project_id.as_deref() == project_id && !s.read_only && !s.archived) else { return Err(MCPError::new("event_session_unavailable")) };
        let mut ctx = Context::new(s.id.clone());
        ctx.project_id = project_id.map(String::from);
        ctx.tainted = self.memory.counter_get(&format!("taint:{}", s.id)) > 0;
        if let Some(p) = project_id {
            let proj = Projects::new(self.memory.clone()).get(p).map_err(mcp)?;
            if proj["archived"] == Value::Bool(true) {
                return Err(MCPError::new("project_archived"));
            }
            ctx.project_revision = proj["updated_at"].as_f64();
        }
        Ok(ctx)
    }

    fn escopo(project_id: Option<&str>) -> String {
        project_id.map(|p| format!("project:{p}")).unwrap_or_else(|| "personal".into())
    }

    pub fn get(&self, id: &str, project_id: Option<&str>) -> R<Value> {
        let rows = self.memory.query("SELECT * FROM event_proposals WHERE id=? AND project_id IS ?", &[&id, &project_id]).map_err(mcp)?;
        let mut row = rows.into_iter().next().ok_or_else(|| MCPError::new("event_proposal_not_found"))?;
        let payload: Value = serde_json::from_str(row["payload"].as_str().unwrap_or("{}")).unwrap_or(Value::Null);
        row.insert("payload".into(), payload);
        row.insert("approvals".into(), json!([]));
        if matches!(row["status"].as_str(), Some("draft" | "pending")) {
            match self.validar(&row, project_id) {
                Ok((ctx, _)) => {
                    let binding = self.calendar.policy.binding("criar_evento_agenda", &ctx);
                    let sid = row["session_id"].as_str().unwrap_or("").to_string();
                    let aprovs: Vec<Value> = self
                        .calendar
                        .policy
                        .approvals
                        .pending(None, true)
                        .into_iter()
                        .filter(|a| a.tool == "criar_evento_agenda" && a.args.get("proposal_id") == Some(&json!(id)) && a.session_id == sid && a.binding == binding)
                        .map(|a| json!({"id": a.id, "status": a.status.as_str()}))
                        .collect();
                    row.insert("approvals".into(), json!(aprovs));
                }
                Err(e) => {
                    row.insert("review_error".into(), json!(e.code));
                }
            }
        }
        Ok(Value::Object(row))
    }

    pub fn listar(&self, project_id: Option<&str>) -> R<Vec<Value>> {
        let ids = self.memory.query("SELECT id FROM event_proposals WHERE project_id IS ? ORDER BY created_at DESC LIMIT 100", &[&project_id]).map_err(mcp)?;
        ids.iter().map(|r| self.get(r["id"].as_str().unwrap_or(""), project_id)).collect()
    }

    pub fn criar(&self, session_id: &str, ev: &Event, project_id: Option<&str>) -> R<Value> {
        self.context(session_id, project_id)?;
        let (row, conn) = self.calendar.binding(Some(&Self::escopo(project_id)))?;
        let id = orion_store::uuid_hex();
        let g = |k: &str| row[k].as_str().unwrap_or("").to_string();
        let payload = json!({"eventId": id, "account": g("account"), "calendarId": g("calendar_id"), "timeZone": g("timezone"), "summary": ev.title, "start": ev.start, "end": ev.end, "description": ev.description, "location": ev.location, "sendUpdates": "none"});
        let text = orion_policy::pyjson::dumps(&payload, true);
        let digest = hex::encode(Sha256::digest(text.as_bytes()));
        let agora = (self.memory.clock())();
        let gen = conn.generation();
        let ok = self
            .memory
            .tx(|c| {
                let n: i64 = c.query_row("SELECT count(*) FROM event_proposals", [], |r| r.get(0))?;
                if n >= 1000 {
                    return Ok(false);
                }
                c.execute(
                    "INSERT INTO event_proposals(id,project_id,session_id,payload,digest,binding_revision,generation,status,created_at) VALUES (?,?,?,?,?,?,?,?,?)",
                    rusqlite::params![id, project_id, session_id, text, digest, g("revision"), gen, "draft", agora],
                )?;
                Ok(true)
            })
            .map_err(mcp)?;
        if !ok {
            return Err(MCPError::new("event_proposal_limit"));
        }
        self.get(&id, project_id)
    }

    fn validar(&self, proposal: &Map<String, Value>, project_id: Option<&str>) -> R<(Context, Arc<Connection>)> {
        let (binding, conn) = self.calendar.binding(Some(&Self::escopo(project_id)))?;
        if proposal["binding_revision"] != binding["revision"] || proposal["generation"].as_str() != Some(&conn.generation()) {
            return Err(MCPError::new("event_origin_review_changed"));
        }
        if !matches!(proposal["status"].as_str(), Some("draft" | "pending")) {
            return Err(MCPError::new("event_already_submitted"));
        }
        let mut ctx = self.context(proposal["session_id"].as_str().unwrap_or(""), project_id)?;
        let cr = match proposal.get("creation_revision") {
            Some(Value::String(s)) => s.clone(),
            _ => "None".into(),
        };
        ctx.authorities = vec![format!(
            "calendar:{}:{}:{}:{}:{}",
            proposal["id"].as_str().unwrap_or(""),
            binding["revision"].as_str().unwrap_or(""),
            conn.generation(),
            proposal["digest"].as_str().unwrap_or(""),
            cr
        )];
        Ok((ctx, conn))
    }

    fn linha(&self, id: &str, project_id: Option<&str>) -> R<Map<String, Value>> {
        let v = self.get(id, project_id)?;
        Ok(v.as_object().cloned().unwrap_or_default())
    }

    fn rev_criacao(t: &orion_mcp::ToolInfo) -> String {
        hex::encode(Sha256::digest(json!({"name": t.name, "description": t.description, "inputSchema": t.input_schema}).to_string().as_bytes()))
    }

    pub async fn revisar(&self, id: &str, project_id: Option<&str>, digest: &str) -> R<Value> {
        let proposal = self.linha(id, project_id)?;
        if proposal["digest"].as_str() != Some(digest) {
            return Err(MCPError::new("event_review_changed"));
        }
        let (_, conn) = self.validar(&proposal, project_id)?;
        let remotos = conn.list_tools().await?;
        let Some(criar) = remotos.iter().find(|t| t.name == "create-event") else { return Err(MCPError::new("event_creation_not_supported")) };
        let v = Tool::new("remote", "", criar.input_schema.clone(), |_a: Args| async { Ok(Value::Null) }).validando();
        let payload = proposal["payload"].as_object().cloned().unwrap_or_default();
        if v.invalido(&payload).is_some() {
            return Err(MCPError::new("event_creation_not_supported"));
        }
        let rev = Self::rev_criacao(criar);
        self.memory.tx(|c| Ok(c.execute("UPDATE event_proposals SET creation_revision=? WHERE id=?", rusqlite::params![rev, id])?)).map_err(mcp)?;
        let proposal = self.linha(id, project_id)?;
        let (ctx, _) = self.validar(&proposal, project_id)?;
        let d = self.calendar.policy.evaluate_opts(&ToolCall::new("criar_evento_agenda", json!({"proposal_id": id, "digest": digest})), &ctx, false);
        if d.action != Action::Confirm {
            return Err(MCPError::new("event_review_denied"));
        }
        self.memory.tx(|c| Ok(c.execute("UPDATE event_proposals SET status='pending' WHERE id=?", [id])?)).map_err(mcp)?;
        Ok(json!({"approval_id": d.approval_id, "proposal_id": id}))
    }

    pub async fn retomar(&self, id: &str, project_id: Option<&str>, approval_id: &str) -> R<Value> {
        let proposal = self.linha(id, project_id)?;
        let (ctx, conn) = self.validar(&proposal, project_id)?;
        let remotos = conn.list_tools().await?;
        let ok = remotos.iter().find(|t| t.name == "create-event").is_some_and(|t| Some(Self::rev_criacao(t)).as_deref() == proposal.get("creation_revision").and_then(Value::as_str));
        if !ok {
            return Err(MCPError::new("event_creation_schema_changed"));
        }
        let args = json!({"proposal_id": id, "digest": proposal["digest"]});
        let aprovada = self.calendar.policy.approvals.get(approval_id).is_some_and(|a| a.status == Status::Approved && a.tool == "criar_evento_agenda" && a.session_id == ctx.session_id && Value::Object(a.args.clone()) == args);
        if !aprovada {
            return Err(MCPError::new("event_approval_unavailable"));
        }
        let d = self.calendar.policy.evaluate(&ToolCall::new("criar_evento_agenda", args), &ctx);
        if d.action != Action::Allow {
            return Err(MCPError::new("event_approval_unavailable"));
        }
        let n = self.memory.tx(|c| Ok(c.execute("UPDATE event_proposals SET status='sending' WHERE id=? AND status IN ('draft','pending')", [id])?)).map_err(mcp)?;
        if n != 1 {
            return Err(MCPError::new("event_already_submitted"));
        }
        let mut status = "unknown";
        let r = conn.call("create-event", proposal["payload"].as_object().cloned().unwrap_or_default()).await;
        let out = match &r {
            Ok(res) => {
                status = if res.is_error { "failed" } else { "created" };
                Ok(json!({"ok": !res.is_error, "status": status}))
            }
            Err(e) => Err(e.clone()),
        };
        // grava até em timeout/cancelamento: nunca reenviar em silêncio um evento talvez aplicado
        let _ = self.memory.tx(|c| Ok(c.execute("UPDATE event_proposals SET status=? WHERE id=?", rusqlite::params![status, id])?));
        out
    }
}
