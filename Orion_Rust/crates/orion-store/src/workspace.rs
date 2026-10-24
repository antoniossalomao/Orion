//! Preferências e contexto explícitos, sempre no escopo da conversa/projeto
//! (REGRAS.md: o que é de um projeto fica no projeto).

use crate::{invalid, not_found, Result, Store};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectPreferences {
    pub model: Option<String>,
    pub max_steps: Option<usize>,
}

impl Store {
    pub fn project_preferences(&self, project_id: &str) -> Result<ProjectPreferences> {
        self.conn()
            .query_row(
                "SELECT model,max_steps FROM project_preferences WHERE project_id=?",
                [project_id],
                |r| {
                    Ok(ProjectPreferences {
                        model: r.get(0)?,
                        max_steps: r.get(1)?,
                    })
                },
            )
            .optional()
            .map(|p| p.unwrap_or_default())
            .map_err(Into::into)
    }

    pub fn set_project_preferences(&self, project_id: &str, p: &ProjectPreferences) -> Result<()> {
        if p.model.as_ref().is_some_and(|m| {
            m.trim().is_empty() || m.chars().count() > 120 || m.chars().any(char::is_control)
        }) || p.max_steps.is_some_and(|n| !(1..=100).contains(&n))
        {
            return invalid("project_preferences_invalid");
        }
        self.tx(|c| {
            self.writable_project(project_id)?;
            c.execute("INSERT INTO project_preferences(project_id,model,max_steps) VALUES(?,?,?) ON CONFLICT(project_id) DO UPDATE SET model=excluded.model,max_steps=excluded.max_steps",
                params![project_id, p.model, p.max_steps])?;
            c.execute("UPDATE projects SET updated_at=? WHERE id=?", params![self.now(), project_id])?;
            Ok(())
        })
    }

    pub fn writable_project(&self, project_id: &str) -> Result<()> {
        match self
            .conn()
            .query_row(
                "SELECT archived FROM projects WHERE id=?",
                [project_id],
                |r| r.get::<_, i64>(0),
            )
            .optional()?
        {
            Some(0) => Ok(()),
            Some(_) => invalid("project_archived"),
            None => not_found("project_not_found"),
        }
    }

    pub fn project_references(&self, project_id: &str) -> Result<Vec<Map<String, Value>>> {
        self.query("SELECT d.id,d.title,d.source,d.indexed_at FROM project_references r JOIN documents d ON d.id=r.document_id WHERE r.project_id=? AND d.project_id=r.project_id ORDER BY d.id", &[&project_id])
    }

    /// Substitui a seleção inteira de forma atômica; documentos de outro projeto são recusados.
    pub fn set_project_references(&self, project_id: &str, ids: &[i64]) -> Result<()> {
        if ids.len() > 8 || ids.iter().any(|id| *id <= 0) {
            return invalid("project_reference_limit");
        }
        self.tx(|c| {
            self.writable_project(project_id)?;
            for id in ids {
                if !c.query_row(
                    "SELECT EXISTS(SELECT 1 FROM documents WHERE id=? AND project_id=?)",
                    params![id, project_id],
                    |r| r.get::<_, bool>(0),
                )? {
                    return invalid("project_reference_out_of_scope");
                }
            }
            c.execute(
                "DELETE FROM project_references WHERE project_id=?",
                [project_id],
            )?;
            for id in ids {
                c.execute(
                    "INSERT OR IGNORE INTO project_references VALUES(?,?)",
                    params![project_id, id],
                )?;
            }
            c.execute(
                "UPDATE projects SET updated_at=? WHERE id=?",
                params![self.now(), project_id],
            )?;
            Ok(())
        })
    }

    pub fn reference_text(&self, project_id: &str, document_id: i64) -> Result<String> {
        let rows = self.query("SELECT substr(c.text,1,8000) AS text FROM chunks c JOIN documents d ON d.id=c.document_id JOIN project_references r ON r.document_id=d.id AND r.project_id=d.project_id WHERE r.project_id=? AND d.id=? ORDER BY c.ord LIMIT 8", &[&project_id, &document_id])?;
        Ok(rows
            .iter()
            .filter_map(|r| r["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n\n")
            .chars()
            .take(12000)
            .collect())
    }

    fn writable_message(
        &self,
        channel: &str,
        session_id: &str,
        message_id: i64,
    ) -> Result<crate::sessions::Message> {
        let s = self
            .get_session(session_id)?
            .filter(|s| s.channel == channel)
            .ok_or_else(|| crate::StoreError::NotFound("session_not_found".into()))?;
        if s.archived || s.read_only {
            return invalid("session_read_only");
        }
        if let Some(pid) = &s.project_id {
            self.writable_project(pid)?;
        }
        self.get_message(message_id)?
            .filter(|m| m.session_id == session_id)
            .ok_or_else(|| crate::StoreError::NotFound("message_not_found".into()))
    }

    /// Avaliação é só local; não contém texto e não treina modelo (REGRAS.md: telemetria só local).
    pub fn set_message_feedback(
        &self,
        channel: &str,
        session_id: &str,
        message_id: i64,
        rating: Option<i8>,
    ) -> Result<()> {
        if rating.is_some_and(|n| n != -1 && n != 1) {
            return invalid("feedback_rating_invalid");
        }
        self.tx(|c| {
            if self.writable_message(channel, session_id, message_id)?.role != "assistant" { return invalid("feedback_assistant_only"); }
            match rating {
                Some(n) => { c.execute("INSERT INTO message_feedback VALUES(?,?,?) ON CONFLICT(message_id) DO UPDATE SET rating=excluded.rating,updated_at=excluded.updated_at", params![message_id, n, self.now()])?; }
                None => { c.execute("DELETE FROM message_feedback WHERE message_id=?", [message_id])?; }
            }
            Ok(())
        })
    }

    pub fn set_message_pinned(
        &self,
        channel: &str,
        session_id: &str,
        message_id: i64,
        pinned: bool,
    ) -> Result<()> {
        self.tx(|c| {
            let message = self.writable_message(channel, session_id, message_id)?;
            if !["user", "assistant"].contains(&message.role.as_str()) { return invalid("pin_message_role_invalid"); }
            if pinned {
                let (count, chars): (usize, usize) = c.query_row("SELECT count(*),coalesce(sum(length(m.text)),0) FROM pinned_messages p JOIN messages m ON m.id=p.message_id WHERE m.session_id=? AND m.id<>?", params![session_id, message_id], |r| Ok((r.get(0)?,r.get(1)?)))?;
                if count >= 20 || chars + message.text.chars().count() > 8000 { return invalid("pinned_context_limit"); }
                c.execute("INSERT OR IGNORE INTO pinned_messages VALUES(?,?)", params![message_id, self.now()])?;
            } else { c.execute("DELETE FROM pinned_messages WHERE message_id=?", [message_id])?; }
            Ok(())
        })
    }

    pub fn pinned_context(&self, session_id: &str) -> Result<Vec<Map<String, Value>>> {
        self.query("SELECT m.id,m.role,m.text,m.provenance FROM pinned_messages p JOIN messages m ON m.id=p.message_id WHERE m.session_id=? AND m.superseded=0 ORDER BY m.id LIMIT 20", &[&session_id])
    }

    pub fn message_annotations(&self, session_id: &str) -> Result<Vec<Map<String, Value>>> {
        self.query("SELECT m.id,coalesce(p.message_id IS NOT NULL,0) AS pinned,f.rating FROM messages m LEFT JOIN pinned_messages p ON p.message_id=m.id LEFT JOIN message_feedback f ON f.message_id=m.id WHERE m.session_id=? AND (p.message_id IS NOT NULL OR f.rating IS NOT NULL)", &[&session_id])
    }

    pub fn prompt_commands(&self, project_id: Option<&str>) -> Result<Vec<Map<String, Value>>> {
        self.query("SELECT id,name,prompt,project_id,updated_at FROM prompt_commands WHERE project_id IS ? ORDER BY name", &[&project_id])
    }

    pub fn save_prompt_command(
        &self,
        project_id: Option<&str>,
        name: &str,
        prompt: &str,
    ) -> Result<()> {
        const RESERVED: &[&str] = &[
            "nota", "vault", "rotina", "modelo", "projeto", "autonomo", "limpar", "resumo",
            "aprovar", "voz", "imagem", "ajuda", "config", "mcp", "skill", "nuvem",
        ];
        if name.is_empty()
            || name.len() > 32
            || !name
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
            || RESERVED.contains(&name)
            || prompt.trim().is_empty()
            || prompt.chars().count() > 6000
        {
            return invalid("prompt_command_invalid");
        }
        self.tx(|c| {
            if let Some(pid) = project_id {
                self.writable_project(pid)?;
            }
            let count: i64 = c.query_row(
                "SELECT count(*) FROM prompt_commands WHERE project_id IS ?",
                [project_id],
                |r| r.get(0),
            )?;
            let existing: Option<String> = c
                .query_row(
                    "SELECT id FROM prompt_commands WHERE project_id IS ? AND name=?",
                    params![project_id, name],
                    |r| r.get(0),
                )
                .optional()?;
            if existing.is_none() && count >= 100 {
                return invalid("prompt_command_limit");
            }
            match existing {
                Some(id) => {
                    c.execute(
                        "UPDATE prompt_commands SET prompt=?,updated_at=? WHERE id=?",
                        params![prompt, self.now(), id],
                    )?;
                }
                None => {
                    c.execute(
                        "INSERT INTO prompt_commands VALUES(?,?,?,?,?)",
                        params![crate::uuid_hex(), project_id, name, prompt, self.now()],
                    )?;
                }
            }
            Ok(())
        })
    }

    pub fn delete_prompt_command(&self, project_id: Option<&str>, name: &str) -> Result<bool> {
        self.tx(|c| {
            if let Some(pid) = project_id {
                self.writable_project(pid)?;
            }
            Ok(c.execute(
                "DELETE FROM prompt_commands WHERE project_id IS ? AND name=?",
                params![project_id, name],
            )? > 0)
        })
    }

    /// Expansão de prompt escolhida pelo usuário; nunca executa scripts ou ferramentas.
    pub fn expand_prompt_command(&self, project_id: Option<&str>, text: &str) -> Result<String> {
        let Some(command) = text.strip_prefix('/') else {
            return Ok(text.into());
        };
        let (name, args) = command
            .split_once(char::is_whitespace)
            .unwrap_or((command, ""));
        let prompt: Option<String> = self
            .conn()
            .query_row(
                "SELECT prompt FROM prompt_commands WHERE name=? AND project_id IS ?",
                params![name, project_id],
                |r| r.get(0),
            )
            .optional()?;
        let Some(prompt) = prompt else {
            return Ok(text.into());
        };
        let result = if prompt.contains("{{args}}") {
            prompt.replace("{{args}}", args.trim())
        } else {
            format!("{prompt}\n\n{}", args.trim())
        };
        if result.chars().count() > 8000 {
            return invalid("expanded_prompt_limit");
        }
        Ok(result)
    }

    pub fn daily_activity(&self, project_id: Option<&str>, start: f64, end: f64) -> Result<Value> {
        if !start.is_finite() || !end.is_finite() || end <= start || end - start > 172800.0 {
            return invalid("activity_period_invalid");
        }
        let c = self.conn();
        let count =
            |sql: &str| c.query_row(sql, params![project_id, start, end], |r| r.get::<_, i64>(0));
        let conversations = count("SELECT count(DISTINCT s.id) FROM sessions s JOIN messages m ON m.session_id=s.id WHERE s.project_id IS ? AND m.created_at>=? AND m.created_at<?")?;
        let messages = count("SELECT count(*) FROM messages m JOIN sessions s ON s.id=m.session_id WHERE s.project_id IS ? AND m.role='user' AND m.created_at>=? AND m.created_at<?")?;
        let actions = count("SELECT count(*) FROM audit a JOIN sessions s ON s.id=a.session_id WHERE s.project_id IS ? AND a.ts>=? AND a.ts<? AND a.action='allow' AND a.risk<>'read'")?;
        let approvals = count("SELECT count(*) FROM audit a JOIN sessions s ON s.id=a.session_id WHERE s.project_id IS ? AND a.ts>=? AND a.ts<? AND a.action='confirm'")?;
        let tasks = count("SELECT count(*) FROM tasks WHERE project_id IS ? AND updated_at>=? AND updated_at<? AND status='concluida'")?;
        let notes = count("SELECT count(*) FROM audit a JOIN sessions s ON s.id=a.session_id WHERE s.project_id IS ? AND a.ts>=? AND a.ts<? AND a.action='allow' AND a.tool IN ('gravar_nota','salvar_nota','guardar_nota')")?;
        Ok(
            json!({"start":start,"end":end,"project_id":project_id,"conversations":conversations,"messages":messages,"actions":actions,"approval_requests":approvals,"completed_tasks":tasks,"note_write_authorizations":notes}),
        )
    }

    /// Reconstitui FTS a partir das tabelas primárias, sem apagar mensagens ou fatos.
    pub fn rebuild_search_indexes(&self) -> Result<()> {
        self.tx(|c| {
            for table in ["sessions_fts", "messages_fts", "facts_fts", "chunks_fts"] {
                c.execute(
                    &format!("INSERT INTO {table}({table}) VALUES('rebuild')"),
                    [],
                )?;
            }
            Ok(())
        })
    }

    /// VACUUM é explícito/ocioso e fora de transação (REGRAS.md: resumo nunca substitui registro).
    pub fn vacuum(&self) -> Result<()> {
        let c = self.conn();
        if !c.is_autocommit() {
            return invalid("maintenance_transaction_active");
        }
        c.execute_batch("PRAGMA optimize; VACUUM;")?;
        Ok(())
    }
}
