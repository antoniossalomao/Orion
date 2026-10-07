"""Caixa persistente de avisos e ponte para decisões revisáveis por contexto."""

from fastapi import APIRouter, Depends, Request
from pydantic import BaseModel, ConfigDict

from .memory.scope import data_scope


class Preferences(BaseModel):
    model_config = ConfigDict(extra="forbid")
    completion: bool = True
    question: bool = True
    approval: bool = True


def router(require_admin):
    api = APIRouter(prefix="/activity", dependencies=[Depends(require_admin)])

    def preferences(s, project_id):
        key = project_id or "personal"
        rows = s.memory.query(
            "SELECT completion, question, approval FROM activity_preferences WHERE scope=?", (key,)
        )
        return (
            {k: bool(v) for k, v in dict(rows[0]).items()} if rows else Preferences().model_dump()
        )

    @api.get("")
    def list_activity(request: Request, project_id: str | None = None, unread: bool = False):
        s = request.app.state.orion
        with data_scope(project_id, include_personal=False):
            rows = s.ops._query(
                "SELECT * FROM notifications WHERE (?=0 OR delivered_at IS NULL) ORDER "
                "BY id DESC LIMIT 200",
                (int(unread),),
            )
        approvals = []
        for a in s.policy.approvals.pending(include_approved=True):
            session = s.memory.get_session(a.session_id)
            if session and session.channel == "web" and session.project_id == project_id:
                approvals.append(
                    {
                        "id": a.id,
                        "tool": a.tool,
                        "reason": a.reason,
                        "session_id": session.id,
                        "title": session.title or "Conversa",
                        "status": a.status.value,
                        "target": "memoria"
                        if a.tool in {"editar_fato", "esquecer_fato"}
                        else "integracoes"
                        if a.tool == "criar_evento_agenda"
                        else "chat",
                    }
                )
        return {
            "notifications": [dict(r) for r in rows],
            "approvals": approvals,
            "preferences": preferences(s, project_id),
        }

    @api.put("/preferences")
    def save_preferences(body: Preferences, request: Request, project_id: str | None = None):
        s = request.app.state.orion
        with s.memory.transaction() as c:
            c.execute(
                "INSERT OR REPLACE INTO "
                "activity_preferences(scope,completion,question,approval) VALUES (?,?,?,?)",
                (
                    project_id or "personal",
                    int(body.completion),
                    int(body.question),
                    int(body.approval),
                ),
            )
        return body.model_dump()

    return api
