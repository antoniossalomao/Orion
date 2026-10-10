"""Ramificações explícitas: histórico original imutável, sem replay de decisões."""

from __future__ import annotations

from fastapi import APIRouter, Depends, HTTPException, Request
from pydantic import BaseModel, ConfigDict, Field


class Edit(BaseModel):
    model_config = ConfigDict(extra="forbid")
    text: str = Field(min_length=1, max_length=32000)
    session_id: str = Field(pattern=r"^[a-f0-9]{32}$")


def router(require_admin):
    api = APIRouter(prefix="/branches", dependencies=[Depends(require_admin)])

    @api.get("/{session_id}")
    def list_paths(session_id: str, request: Request):
        s = request.app.state.orion
        session = s.memory.get_session(session_id)
        if not session or session.channel != "web":
            raise HTTPException(404, "session_not_found")
        row = s.memory.query(
            "SELECT root_session FROM message_branches WHERE session_id=?", (session_id,)
        )
        root = row[0][0] if row else session_id
        paths = s.memory.query(
            "SELECT session_id,source_message,edited_text,prefix_count FROM message_branches "
            "WHERE root_session=? ORDER BY created_at LIMIT 100",
            (root,),
        )
        result = []
        for id_, message, text, prefix_count in [(root, None, None, 0), *paths]:
            candidate = s.memory.get_session(id_)
            if candidate and candidate.project_id == session.project_id:
                count = s.memory.query("SELECT count(*) FROM artifacts WHERE session_id=?", (id_,))[
                    0
                ][0]
                count_messages = s.memory.query(
                    "SELECT count(*) FROM messages WHERE session_id=?", (id_,)
                )[0][0]
                result.append(
                    {
                        "session_id": id_,
                        "title": candidate.title or "Caminho original",
                        "message_id": message,
                        "edited_text": text if count_messages <= prefix_count else None,
                        "artifacts": count,
                    }
                )
        return result

    @api.post("/messages/{message_id}")
    def branch(message_id: int, body: Edit, request: Request):
        s = request.app.state.orion
        session = s.memory.get_session(body.session_id)
        if not session or session.channel != "web":
            raise HTTPException(404, "session_not_found")
        if session.archived or session.read_only or (s.agent and s.agent.busy(session.id)):
            raise HTTPException(409, "session_busy_or_read_only")
        if session.project_id:
            rows = s.memory.query("SELECT archived FROM projects WHERE id=?", (session.project_id,))
            if not rows or rows[0][0]:
                raise HTTPException(409, "project_archived")
        floor = s.memory.counter_get(f"history_after:{session.id}")
        rows = s.memory.query(
            "SELECT id FROM messages WHERE id=? AND session_id=? AND role='user' AND id>?",
            (message_id, session.id, floor),
        )
        if not rows:
            raise HTTPException(404, "editable_message_not_found")
        root = s.memory.query(
            "SELECT root_session FROM message_branches WHERE session_id=?", (session.id,)
        )
        root_id = root[0][0] if root else session.id
        if (
            s.memory.query(
                "SELECT count(*) FROM message_branches WHERE root_session=?", (root_id,)
            )[0][0]
            >= 99
        ):
            raise HTTPException(409, "branch_limit")
        prefix = s.memory.query(
            "SELECT role,text,created_at,provenance FROM messages WHERE "
            "session_id=? AND id>? AND id<? AND role IN ('user','assistant') ORDER BY id",
            (session.id, floor, message_id),
        )
        if len(prefix) > 200 or sum(len(r["text"]) for r in prefix) > 128000:
            raise HTTPException(413, "branch_context_limit")
        # Atomic creation and prefix copy. Tools/system instructions and decisions are not copied.
        with s.memory.transaction() as c:
            target = s.memory._new_session(
                c, "web", f"Versão · {(session.title or 'Conversa')[:100]}", session.project_id
            )
            for r in prefix:
                c.execute(
                    "INSERT INTO messages(session_id,role,text,created_at,provenance) "
                    "VALUES (?,?,?,?,?)",
                    (target.id, r["role"], r["text"], r["created_at"], r["provenance"]),
                )
            c.execute(
                "INSERT INTO message_branches(session_id,root_session,parent_session,"
                "source_message,edited_text,prefix_count,created_at) VALUES (?,?,?,?,?,?,?)",
                (
                    target.id,
                    root_id,
                    session.id,
                    message_id,
                    body.text,
                    len(prefix),
                    s.memory.clock(),
                ),
            )
        if s.memory.counter_get(f"taint:{session.id}"):
            s.memory.counter_set(f"taint:{target.id}", 1)
        return {"session_id": target.id, "edited_text": body.text, "requires_send": True}

    return api
