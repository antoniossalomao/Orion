//! Sessões e mensagens (episódico), uma sessão ativa por canal.

use crate::{invalid, not_found, py_json, text, uuid_hex, Result, Store};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::Serialize;
use serde_json::Value;

/// Filtro de `list_sessions`: só as conversas sem projeto.
pub const SEM_PROJETO: &str = "nenhum";

/// Tabelas que ainda apontam para uma conversa (sem ON DELETE): apagar a conversa as deixaria
/// órfãs.
const REFERENCIAS_DE_SESSAO: &[(&str, &str, &str)] = &[
    ("message_branches", "session_id", "ramos"),
    ("message_branches", "root_session", "ramos"),
    ("message_branches", "parent_session", "ramos"),
    ("artifacts", "session_id", "artefatos"),
    ("event_proposals", "session_id", "propostas de evento"),
    ("file_plans", "session_id", "planos de arquivos"),
];

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Session {
    pub id: String,
    pub channel: String,
    pub title: Option<String>,
    pub created_at: f64,
    pub last_active_at: f64,
    pub archived: bool,
    pub read_only: bool,
    pub favorite: bool,
    pub project_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Message {
    pub id: i64,
    pub session_id: String,
    pub role: String,
    pub text: String,
    pub created_at: f64,
    pub provenance: Option<Value>,
    /// E3.5: primeira versão do pedido que esta mensagem reescreve.
    pub version_of: Option<i64>,
}

pub(crate) fn session_row(r: &Row) -> rusqlite::Result<Session> {
    Ok(Session {
        id: r.get("id")?,
        channel: r.get("channel")?,
        title: r.get("title")?,
        created_at: r.get("created_at")?,
        last_active_at: r.get("last_active_at")?,
        archived: r.get::<_, i64>("archived")? != 0,
        read_only: r.get::<_, i64>("read_only")? != 0,
        favorite: r.get::<_, i64>("favorite")? != 0,
        project_id: r.get("project_id")?,
    })
}

pub(crate) fn message_row(r: &Row) -> rusqlite::Result<Message> {
    let prov: Option<String> = r.get("provenance")?;
    Ok(Message {
        id: r.get("id")?,
        session_id: r.get("session_id")?,
        role: r.get("role")?,
        text: r.get("text")?,
        created_at: r.get("created_at")?,
        provenance: prov.and_then(|p| serde_json::from_str(&p).ok()),
        version_of: r.get("version_of")?,
    })
}

const SESSAO_COM_READ_ONLY: &str =
    "SELECT s.*, EXISTS(SELECT 1 FROM imported WHERE kind='sessao' AND ref=s.id) AS read_only FROM sessions s";

impl Store {
    fn new_session_in(&self, c: &Connection, channel: &str, title: Option<&str>, project_id: Option<&str>) -> Result<Session> {
        if let Some(p) = project_id {
            let arq: Option<i64> =
                c.query_row("SELECT archived FROM projects WHERE id=?", [p], |r| r.get(0)).optional()?;
            if arq.is_none_or(|a| a != 0) {
                return invalid("project_unavailable");
            }
        }
        let (agora, sid) = (self.now(), uuid_hex());
        c.execute(
            "INSERT INTO sessions(id, channel, title, created_at, last_active_at, project_id) VALUES (?,?,?,?,?,?)",
            params![sid, channel, title, agora, agora, project_id],
        )?;
        c.execute(
            "INSERT INTO active_sessions(channel, session_id) VALUES (?,?)
             ON CONFLICT(channel) DO UPDATE SET session_id=excluded.session_id",
            params![channel, sid],
        )?;
        Ok(Session {
            id: sid,
            channel: channel.into(),
            title: title.map(Into::into),
            created_at: agora,
            last_active_at: agora,
            archived: false,
            read_only: false,
            favorite: false,
            project_id: project_id.map(Into::into),
        })
    }

    /// Cria e seleciona no mesmo commit, preservando as outras conversas.
    pub fn new_session(&self, channel: &str, title: Option<&str>, project_id: Option<&str>) -> Result<Session> {
        self.tx(|c| self.new_session_in(c, channel, title, project_id))
    }

    /// Consulta sem criar sessão; só o ponteiro persistido determina a ativa.
    pub fn selected_session(&self, channel: &str) -> Result<Option<Session>> {
        let c = self.conn();
        Ok(c.query_row(
            "SELECT s.*, 0 AS read_only FROM active_sessions a JOIN sessions s ON s.id=a.session_id
             WHERE a.channel=? AND s.channel=? AND s.archived=0
             AND NOT EXISTS(SELECT 1 FROM imported WHERE kind='sessao' AND ref=s.id)",
            [channel, channel],
            session_row,
        )
        .optional()?)
    }

    /// Sessão selecionada do canal; cria uma vazia quando ainda não há seleção.
    pub fn active_session(&self, channel: &str) -> Result<Session> {
        self.tx(|c| match self.selected_session(channel)? {
            Some(s) => Ok(s),
            None => self.new_session_in(c, channel, None, None),
        })
    }

    /// Nunca muda canal nem reabre importada ou arquivada; a escolha é persistente.
    pub fn activate_session(&self, channel: &str, session_id: &str) -> Result<Session> {
        self.tx(|c| {
            let s = match self.get_session(session_id)? {
                Some(s) if s.channel == channel => s,
                _ => return not_found(session_id),
            };
            if s.archived || s.read_only {
                return invalid("sessão arquivada ou importada é somente leitura");
            }
            if let Some(p) = &s.project_id {
                let arq: i64 = c.query_row("SELECT archived FROM projects WHERE id=?", [p], |r| r.get(0))?;
                if arq != 0 {
                    return invalid("project_archived");
                }
            }
            c.execute("UPDATE sessions SET last_active_at=? WHERE id=?", params![self.now(), s.id])?;
            c.execute(
                "INSERT INTO active_sessions(channel, session_id) VALUES (?,?)
                 ON CONFLICT(channel) DO UPDATE SET session_id=excluded.session_id",
                params![channel, s.id],
            )?;
            self.get_session(&s.id)?.ok_or_else(|| crate::StoreError::NotFound(s.id.clone()))
        })
    }

    // ── importação (export do legado) ─────────────────────────────────────
    /// Sessão arquivada para um registro externo: (id interno, criada agora?).
    pub fn import_session(&self, external_id: &str, channel: &str, title: Option<&str>, created_at: f64) -> Result<(String, bool)> {
        self.tx(|c| {
            let r: Option<String> = c
                .query_row("SELECT ref FROM imported WHERE kind='sessao' AND external_id=?", [external_id], |r| r.get(0))
                .optional()?;
            if let Some(r) = r {
                return Ok((r, false));
            }
            let sid = uuid_hex();
            c.execute(
                "INSERT INTO sessions(id, channel, title, created_at, last_active_at, archived) VALUES (?,?,?,?,?,1)",
                params![sid, channel, title, created_at, created_at],
            )?;
            c.execute("INSERT INTO imported VALUES ('sessao', ?, ?)", params![external_id, sid])?;
            Ok((sid, true))
        })
    }

    /// Insere uma mensagem com a data original. `false` se já foi importada.
    pub fn import_message(
        &self,
        session_id: &str,
        external_id: &str,
        role: &str,
        text: &str,
        created_at: f64,
        provenance: Option<&Value>,
    ) -> Result<bool> {
        self.tx(|c| {
            let novo = c.execute("INSERT OR IGNORE INTO imported(kind, external_id) VALUES ('mensagem', ?)", [external_id])?;
            if novo == 0 {
                return Ok(false);
            }
            c.execute(
                "INSERT INTO messages(session_id, role, text, created_at, provenance) VALUES (?,?,?,?,?)",
                params![session_id, role, text, created_at, provenance.map(py_json)],
            )?;
            let mid = c.last_insert_rowid();
            c.execute(
                "UPDATE imported SET ref=? WHERE kind='mensagem' AND external_id=?",
                params![mid.to_string(), external_id],
            )?;
            c.execute(
                "UPDATE sessions SET last_active_at = MAX(last_active_at, ?) WHERE id=?",
                params![created_at, session_id],
            )?;
            Ok(true)
        })
    }

    pub fn get_session(&self, session_id: &str) -> Result<Option<Session>> {
        let c = self.conn();
        Ok(c.query_row(&format!("{SESSAO_COM_READ_ONLY} WHERE s.id=?"), [session_id], session_row).optional()?)
    }

    pub fn archive_session(&self, session_id: &str) -> Result<()> {
        self.tx(|c| {
            c.execute("UPDATE sessions SET archived=1 WHERE id=?", [session_id])?;
            c.execute("UPDATE active_sessions SET session_id=NULL WHERE session_id=?", [session_id])?;
            Ok(())
        })
    }

    /// Metadados persistentes; arquivar preserva mensagens e libera a seleção.
    pub fn edit_session(
        &self,
        channel: &str,
        session_id: &str,
        title: Option<&str>,
        favorite: Option<bool>,
        archived: Option<bool>,
    ) -> Result<Session> {
        self.tx(|c| {
            let s = match self.get_session(session_id)? {
                Some(s) if s.channel == channel => s,
                _ => return not_found(session_id),
            };
            if s.read_only {
                return invalid("sessão importada é somente leitura");
            }
            if let Some(t) = title {
                let t = t.trim();
                if t.is_empty() || t.chars().count() > 120 {
                    return invalid("título deve ter entre 1 e 120 caracteres");
                }
                c.execute("UPDATE sessions SET title=? WHERE id=?", params![t, session_id])?;
            }
            if let Some(f) = favorite {
                c.execute("UPDATE sessions SET favorite=? WHERE id=?", params![f as i64, session_id])?;
            }
            if let Some(a) = archived {
                c.execute("UPDATE sessions SET archived=? WHERE id=?", params![a as i64, session_id])?;
                if a {
                    c.execute("UPDATE active_sessions SET session_id=NULL WHERE session_id=?", [session_id])?;
                }
            }
            self.get_session(session_id)?.ok_or_else(|| crate::StoreError::NotFound(session_id.into()))
        })
    }

    /// `archived`: `None` = todas. `project`: `None` = qualquer, `SEM_PROJETO` = só as sem
    /// projeto, senão o id do projeto.
    pub fn list_sessions(&self, channel: Option<&str>, limit: i64, archived: Option<bool>, project: Option<&str>) -> Result<Vec<Session>> {
        let c = self.conn();
        let arq = archived.map(|a| a as i64);
        let mut st = c.prepare(&format!(
            "{SESSAO_COM_READ_ONLY} WHERE (?1 IS NULL OR channel=?1) AND (?2 IS NULL OR archived=?2)
             AND (?3 IS NULL OR (?3 = ?4 AND project_id IS NULL) OR project_id = ?3)
             ORDER BY favorite DESC, last_active_at DESC, created_at DESC, id DESC LIMIT ?5"
        ))?;
        let rows = st.query_map(params![channel, arq, project, SEM_PROJETO, limit], session_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Apaga uma conversa **arquivada** com as mensagens. Recusa a que ainda é referência de
    /// outra coisa: ramos, artefatos, propostas e planos apontam para ela.
    pub fn delete_session(&self, channel: &str, session_id: &str) -> Result<()> {
        self.tx(|c| {
            let s = match self.get_session(session_id)? {
                Some(s) if s.channel == channel => s,
                _ => return not_found(session_id),
            };
            if s.read_only {
                return invalid("sessão importada é somente leitura");
            }
            if !s.archived {
                return invalid("arquive a conversa antes de apagar");
            }
            for (tabela, coluna, rotulo) in REFERENCIAS_DE_SESSAO {
                let tem: Option<i64> = c
                    .query_row(&format!("SELECT 1 FROM {tabela} WHERE {coluna}=? LIMIT 1"), [session_id], |r| r.get(0))
                    .optional()?;
                if tem.is_some() {
                    return invalid(format!("a conversa tem {rotulo}: mantenha-a arquivada"));
                }
            }
            c.execute("DELETE FROM active_sessions WHERE session_id=?", [session_id])?;
            c.execute("DELETE FROM meta WHERE key=?", [format!("counter:history_after:{session_id}")])?;
            c.execute("DELETE FROM sessions WHERE id=?", [session_id])?;
            Ok(())
        })
    }

    /// Busca nas conversas do canal (título e mensagens). Devolve (sessão, trecho) e o total.
    pub fn search_sessions(
        &self,
        channel: &str,
        texto: &str,
        limit: i64,
        offset: i64,
        archived: Option<bool>,
    ) -> Result<(Vec<(Session, String)>, i64)> {
        let Some(q) = text::fts_query(texto) else { return Ok((vec![], 0)) };
        let hits = "WITH hits AS (SELECT s.id FROM sessions_fts JOIN sessions s ON s.rowid=sessions_fts.rowid
            WHERE sessions_fts MATCH ?1 UNION SELECT m.session_id FROM messages_fts
            JOIN messages m ON m.id=messages_fts.rowid WHERE messages_fts MATCH ?1
            AND m.role IN ('user','assistant')) ";
        let arq = archived.map(|a| a as i64);
        let c = self.conn();
        let total: i64 = c.query_row(
            &format!("{hits}SELECT count(*) FROM sessions s WHERE channel=?2 AND id IN hits AND (?3 IS NULL OR archived=?3)"),
            params![q, channel, arq],
            |r| r.get(0),
        )?;
        let mut st = c.prepare(&format!(
            "{hits}SELECT s.*, EXISTS(SELECT 1 FROM imported WHERE kind='sessao' AND ref=s.id) AS read_only,
             (SELECT snippet(messages_fts,0,'','','…',8) FROM messages_fts
              JOIN messages m ON m.id=messages_fts.rowid WHERE m.session_id=s.id
              AND m.role IN ('user','assistant') AND messages_fts MATCH ?1 LIMIT 1) AS trecho
             FROM sessions s WHERE channel=?2 AND id IN hits AND (?3 IS NULL OR archived=?3)
             ORDER BY favorite DESC, last_active_at DESC, created_at DESC, id DESC LIMIT ?4 OFFSET ?5"
        ))?;
        let rows = st.query_map(params![q, channel, arq, limit, offset], |r| {
            Ok((session_row(r)?, r.get::<_, Option<String>>("trecho")?.unwrap_or_default()))
        })?;
        Ok((rows.collect::<rusqlite::Result<_>>()?, total))
    }

    pub fn add_message(
        &self,
        session_id: &str,
        role: &str,
        texto: &str,
        provenance: Option<&Value>,
        version_of: Option<i64>,
    ) -> Result<Message> {
        let agora = self.now();
        let mid = self.tx(|c| {
            c.execute(
                "INSERT INTO messages(session_id, role, text, created_at, provenance, version_of) VALUES (?,?,?,?,?,?)",
                params![session_id, role, texto, agora, provenance.map(py_json), version_of],
            )?;
            let mid = c.last_insert_rowid();
            c.execute("UPDATE sessions SET last_active_at=? WHERE id=?", params![agora, session_id])?;
            Ok(mid)
        })?;
        Ok(Message {
            id: mid,
            session_id: session_id.into(),
            role: role.into(),
            text: texto.into(),
            created_at: agora,
            provenance: provenance.cloned(),
            version_of,
        })
    }

    /// Substitui o pedido `message_id` (e tudo que veio depois) por uma nova versão: as
    /// mensagens ficam no banco, mas somem do contexto e do histórico. Devolve o id da PRIMEIRA
    /// versão do pedido, a que a nova mensagem deve citar em `version_of`.
    pub fn supersede_from(&self, session_id: &str, message_id: i64) -> Result<i64> {
        self.tx(|c| {
            let s = match self.get_session(session_id)? {
                Some(s) => s,
                None => return not_found(session_id),
            };
            if s.archived || s.read_only {
                return invalid("sessão somente leitura");
            }
            let piso = self.counter_get(&format!("history_after:{session_id}"));
            let r: Option<(i64, Option<i64>)> = c
                .query_row(
                    "SELECT id, version_of FROM messages WHERE id=? AND session_id=? AND role='user'
                     AND superseded=0 AND id>? AND text NOT LIKE '[CONTEXTO EXTERNO%'",
                    params![message_id, session_id, piso],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            let Some((rid, version_of)) = r else { return not_found(message_id) };
            let mut inicio = message_id;
            loop {
                // os marcadores de contexto externo do mesmo turno vão junto
                let anterior: Option<(i64, String)> = c
                    .query_row(
                        "SELECT id, text FROM messages WHERE session_id=? AND id<? AND superseded=0
                         AND id>? ORDER BY id DESC LIMIT 1",
                        params![session_id, inicio, piso],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .optional()?;
                match anterior {
                    Some((id, t)) if t.starts_with("[CONTEXTO EXTERNO") => inicio = id,
                    _ => break,
                }
            }
            c.execute("UPDATE messages SET superseded=1 WHERE session_id=? AND id>=?", params![session_id, inicio])?;
            // um resumo que já cobre o trecho reescrito ficaria falando do que não vale mais
            c.execute(
                "DELETE FROM session_summaries WHERE session_id=? AND upto_message_id>=?",
                params![session_id, inicio],
            )?;
            Ok(version_of.unwrap_or(rid))
        })
    }

    /// Todas as versões de um pedido, da mais antiga à mais nova (inclui a primeira).
    pub fn message_versions(&self, session_id: &str, root_id: i64) -> Result<Vec<Message>> {
        let c = self.conn();
        let mut st = c.prepare("SELECT * FROM messages WHERE session_id=?1 AND (id=?2 OR version_of=?2) ORDER BY id")?;
        let rows = st.query_map(params![session_id, root_id], message_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn get_message(&self, id: i64) -> Result<Option<Message>> {
        let c = self.conn();
        Ok(c.query_row("SELECT * FROM messages WHERE id=?", [id], message_row).optional()?)
    }

    pub fn history(&self, session_id: &str, limit: i64, after: i64) -> Result<Vec<Message>> {
        let c = self.conn();
        let mut st = c.prepare(
            "SELECT * FROM (SELECT * FROM messages WHERE session_id=? AND id>? AND superseded=0
             ORDER BY id DESC LIMIT ?) ORDER BY id",
        )?;
        let rows = st.query_map(params![session_id, after, limit], message_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn context_history(&self, session_id: &str, limit: i64) -> Result<Vec<Message>> {
        self.history(session_id, limit, self.counter_get(&format!("history_after:{session_id}")))
    }

    // ── resumo da conversa (REGRAS.md: resumo da conversa nunca substitui o registro) ──
    pub fn get_summary(&self, session_id: &str) -> Result<Option<serde_json::Map<String, Value>>> {
        crate::query_map_one(&self.conn(), "SELECT * FROM session_summaries WHERE session_id=?", &[&session_id])
    }

    pub fn set_summary(&self, session_id: &str, upto_message_id: i64, texto: &str, model: &str) -> Result<()> {
        self.tx(|c| {
            c.execute(
                "INSERT INTO session_summaries(session_id, upto_message_id, text, model, created_at)
                 VALUES (?,?,?,?,?) ON CONFLICT(session_id) DO UPDATE SET
                 upto_message_id=excluded.upto_message_id, text=excluded.text,
                 model=excluded.model, created_at=excluded.created_at",
                params![session_id, upto_message_id, texto, model, self.now()],
            )?;
            Ok(())
        })
    }

    pub fn drop_summary(&self, session_id: &str) -> Result<bool> {
        self.tx(|c| Ok(c.execute("DELETE FROM session_summaries WHERE session_id=?", [session_id])? > 0))
    }

    /// Página por id: timestamps repetidos ou importados não duplicam nem pulam mensagens.
    /// Devolve (mensagens, total, próximo `before`).
    pub fn history_page(&self, session_id: &str, limit: i64, before: Option<i64>, complete: bool) -> Result<(Vec<Message>, i64, Option<i64>)> {
        let c = self.conn();
        let after = if complete { 0 } else { self.counter_get(&format!("history_after:{session_id}")) };
        let total: i64 = c.query_row(
            "SELECT count(*) FROM messages WHERE session_id=? AND id>? AND superseded=0 AND role IN ('user','assistant')",
            params![session_id, after],
            |r| r.get(0),
        )?;
        let mut st = c.prepare(
            "SELECT * FROM messages WHERE session_id=?1 AND id>?2 AND (?3 IS NULL OR id<?3)
             AND superseded=0 AND role IN ('user','assistant') ORDER BY id DESC LIMIT ?4",
        )?;
        let mut rows: Vec<Message> = st
            .query_map(params![session_id, after, before, limit + 1], message_row)?
            .collect::<rusqlite::Result<_>>()?;
        let mais = rows.len() as i64 > limit;
        rows.truncate(limit.max(0) as usize);
        let proximo = if mais { rows.last().map(|m| m.id) } else { None };
        rows.reverse();
        Ok((rows, total, proximo))
    }

    /// Avança o limite de contexto; não apaga mensagens, fatos, documentos ou vetores.
    pub fn clear_context(&self, session_id: &str) -> Result<i64> {
        self.tx(|c| {
            let s = match self.get_session(session_id)? {
                Some(s) => s,
                None => return not_found(session_id),
            };
            if s.archived || s.read_only {
                return invalid("sessão somente leitura");
            }
            let last: i64 =
                c.query_row("SELECT coalesce(max(id),0) FROM messages WHERE session_id=?", [session_id], |r| r.get(0))?;
            c.execute(
                "INSERT INTO meta(key,value) VALUES (?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                params![format!("counter:history_after:{session_id}"), last.to_string()],
            )?;
            c.execute("DELETE FROM session_summaries WHERE session_id=?", [session_id])?;
            Ok(last)
        })
    }
}
