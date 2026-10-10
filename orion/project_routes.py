"""Administração explícita de projetos, fora das ferramentas do modelo."""

from __future__ import annotations

from fastapi import APIRouter, Depends, HTTPException, Request
from pydantic import BaseModel, ConfigDict, Field, field_validator

from .projects import Projects


class Create(BaseModel):
    model_config = ConfigDict(extra="forbid")
    name: str = Field(min_length=1, max_length=120)
    instructions: str = Field(default="", max_length=16000)
    share_personal: bool = False
    root: str | None = Field(default=None, max_length=4096)

    @field_validator("name")
    @classmethod
    def nonempty(cls, name: str) -> str:
        if not name.strip():
            raise ValueError("project_name_required")
        return name.strip()


class Update(Create):
    name: str = Field(default="", max_length=120)
    instructions: str = Field(default="", max_length=16000)
    archived: bool = False


class Select(BaseModel):
    model_config = ConfigDict(extra="forbid")
    project_id: str | None = Field(default=None, pattern=r"^[a-f0-9]{32}$")
    channel: str = Field(default="web", pattern=r"^[a-z0-9_-]{1,48}$")


def guard(state, session_id: str) -> None:
    if (
        state.agent is not None and state.agent.busy(session_id)
    ) or state.policy.approvals.unresolved(session_id):
        raise HTTPException(409, "session_busy_or_approval_pending")


def router(require_admin) -> APIRouter:
    api = APIRouter(prefix="/projects", dependencies=[Depends(require_admin)])

    @api.get("")
    def list_projects(request: Request, archived: bool = False):
        return Projects(request.app.state.orion.memory).list(archived=archived)

    @api.post("")
    def create_project(body: Create, request: Request):
        return Projects(request.app.state.orion.memory).create(**body.model_dump())

    @api.post("/activate")
    def activate(body: Select, request: Request):
        state = request.app.state.orion
        old = state.memory.selected_session(body.channel)
        if old:
            guard(state, old.id)
        session = Projects(state.memory).activate(body.channel, body.project_id)
        return {"session_id": session.id, "project_id": session.project_id}

    @api.get("/{id_}")
    def detail(id_: str, request: Request):
        state = request.app.state.orion
        row = Projects(state.memory).get(id_)
        sessions = state.memory.query(
            "SELECT id,title,channel,archived FROM sessions WHERE project_id=? "
            "ORDER BY last_active_at DESC",
            (id_,),
        )
        documents = state.memory.query(
            "SELECT id,title,indexed_at FROM documents WHERE project_id=? ORDER BY indexed_at DESC",
            (id_,),
        )
        scope = "project:" + id_
        extensions = (
            [r for r in state.extensions.list() if r.get("scope") == scope]
            if state.extensions
            else []
        )
        return {
            **row,
            "sessions": [dict(s) for s in sessions],
            "documents": [dict(d) for d in documents],
            "extensions": extensions,
        }

    @api.patch("/{id_}")
    def update(id_: str, body: Update, request: Request):
        state = request.app.state.orion
        if body.archived:
            for session in state.memory.query("SELECT id FROM sessions WHERE project_id=?", (id_,)):
                guard(state, session["id"])
        for session in state.memory.query("SELECT id FROM sessions WHERE project_id=?", (id_,)):
            if state.agent and state.agent.busy(session["id"]):
                raise HTTPException(409, "session_busy")
        state.policy.approvals.invalidate_binding(id_)
        return Projects(state.memory).update(
            id_, **body.model_dump(exclude_unset=True), root_set="root" in body.model_fields_set
        )

    @api.put("/sessions/{session_id}")
    def associate(session_id: str, body: Select, request: Request):
        state = request.app.state.orion
        guard(state, session_id)
        session = Projects(state.memory).associate(body.channel, session_id, body.project_id)
        return {"session_id": session.id, "project_id": session.project_id}

    return api
