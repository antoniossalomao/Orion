"""Propostas de Agenda: o modelo só prepara; a criação exige revisão de produto."""

from __future__ import annotations

import hashlib
import json
import uuid
from datetime import datetime

from fastapi import APIRouter, Depends, HTTPException, Request
from pydantic import BaseModel, ConfigDict, Field, model_validator

from .extensions.host import MCPError
from .memory.scope import current
from .policy import Action, Context, Risk, Status, ToolCall, ToolSpec
from .projects import Projects
from .tools.registry import Tool


class Event(BaseModel):
    model_config = ConfigDict(extra="forbid")
    title: str = Field(min_length=1, max_length=200)
    start: str = Field(max_length=100)
    end: str = Field(max_length=100)
    description: str = Field(default="", max_length=8000)
    location: str = Field(default="", max_length=500)

    @model_validator(mode="after")
    def period(self):
        first, last = datetime.fromisoformat(self.start), datetime.fromisoformat(self.end)
        if (
            not first.tzinfo
            or not last.tzinfo
            or not 0 < (last - first).total_seconds() <= 7 * 86400
        ):
            raise ValueError("event_period_invalid")
        return self


class Proposal(Event):
    session_id: str = Field(pattern=r"^[a-f0-9]{32}$")


class Review(BaseModel):
    model_config = ConfigDict(extra="forbid")
    reviewed_digest: str = Field(pattern=r"^[a-f0-9]{64}$")


class Resume(BaseModel):
    model_config = ConfigDict(extra="forbid")
    approval_id: str = Field(min_length=1, max_length=128)


class Events:
    def __init__(self, calendar):
        self.calendar = calendar
        self.memory, self.policy = calendar.memory, calendar.policy
        self.policy.tools["criar_evento_agenda"] = ToolSpec(
            "criar_evento_agenda", Risk.WRITE, require_confirmation=True
        )

        # The actual creation operation is absent from the model registry.
        async def propose(**args):
            scope = current.get()
            if not scope.session_id:
                return {"ok": False, "code": "proposal_session_required"}
            row = self.create(Proposal(session_id=scope.session_id, **args), scope.project_id)
            return {
                "ok": True,
                "proposal": row,
                "action": "Abra Agenda em Integrações para revisar. Nenhum evento foi criado.",
            }

        schema = Event.model_json_schema()
        calendar.registry.register(
            Tool(
                "propor_evento_agenda",
                "Preparar proposta de evento com título, horários e descrição para "
                "revisão; sem enviar ou criar.",
                schema,
                propose,
            )
        )
        self.policy.tools["propor_evento_agenda"] = ToolSpec("propor_evento_agenda", Risk.WRITE)

    def context(self, session_id, project_id):
        session = self.memory.get_session(session_id)
        if (
            not session
            or session.channel != "web"
            or session.project_id != project_id
            or session.read_only
            or session.archived
        ):
            raise MCPError("event_session_unavailable")
        ctx = Context(
            session.id,
            project_id=project_id,
            tainted=self.memory.counter_get(f"taint:{session.id}") > 0,
        )
        if project_id:
            project = Projects(self.memory).get(project_id)
            if project["archived"]:
                raise MCPError("project_archived")
            ctx.project_revision = project["updated_at"]
        return ctx

    def get(self, id_, project_id):
        rows = self.memory.query(
            "SELECT * FROM event_proposals WHERE id=? AND project_id IS ?", (id_, project_id)
        )
        if not rows:
            raise MCPError("event_proposal_not_found")
        row = dict(rows[0])
        row["payload"] = json.loads(row["payload"])
        row["approvals"] = []
        if row["status"] in {"draft", "pending"}:
            try:
                ctx, _ = self.validate(row, project_id)
                binding = self.policy.binding("criar_evento_agenda", ctx)
                row["approvals"] = [
                    {"id": a.id, "status": a.status.value}
                    for a in self.policy.approvals.pending(include_approved=True)
                    if a.tool == "criar_evento_agenda"
                    and a.args.get("proposal_id") == id_
                    and a.session_id == row["session_id"]
                    and a.binding == binding
                ]
            except MCPError as error:
                row["review_error"] = error.code
        return row

    def create(self, body, project_id):
        self.context(body.session_id, project_id)
        row, conn = self.calendar.binding(f"project:{project_id}" if project_id else "personal")
        event = body.model_dump(exclude={"session_id"})
        id_ = uuid.uuid4().hex
        payload = {
            "eventId": id_,
            "account": row["account"],
            "calendarId": row["calendar_id"],
            "timeZone": row["timezone"],
            "summary": event["title"],
            "start": event["start"],
            "end": event["end"],
            "description": event["description"],
            "location": event["location"],
            "sendUpdates": "none",
        }
        text = json.dumps(payload, sort_keys=True, ensure_ascii=False)
        digest = hashlib.sha256(text.encode()).hexdigest()
        with self.memory.transaction() as c:
            if c.execute("SELECT count(*) FROM event_proposals").fetchone()[0] >= 1000:
                raise MCPError("event_proposal_limit")
            c.execute(
                "INSERT INTO event_proposals(id,project_id,session_id,payload,digest,"
                "binding_revision,generation,status,created_at) VALUES (?,?,?,?,?,?,?,?,?)",
                (
                    id_,
                    project_id,
                    body.session_id,
                    text,
                    digest,
                    row["revision"],
                    conn.generation,
                    "draft",
                    self.memory.clock(),
                ),
            )
        return self.get(id_, project_id)

    def validate(self, proposal, project_id):
        binding, conn = self.calendar.binding(f"project:{project_id}" if project_id else "personal")
        if (
            proposal["binding_revision"] != binding["revision"]
            or proposal["generation"] != conn.generation
        ):
            raise MCPError("event_origin_review_changed")
        if proposal["status"] not in {"draft", "pending"}:
            raise MCPError("event_already_submitted")
        ctx = self.context(proposal["session_id"], project_id)
        ctx.authorities = (
            f"calendar:{proposal['id']}:{binding['revision']}:{conn.generation}:{proposal['digest']}:"
            + str(proposal.get("creation_revision")),
        )
        return ctx, conn

    async def review(self, id_, project_id, digest):
        proposal = self.get(id_, project_id)
        if proposal["digest"] != digest:
            raise MCPError("event_review_changed")
        ctx, conn = self.validate(proposal, project_id)
        remotes = {t.name: t for t in await conn.list_tools()}
        if "create-event" not in remotes or Tool(
            "remote", "", remotes["create-event"].input_schema, lambda: None
        )._invalid(proposal["payload"]):
            raise MCPError("event_creation_not_supported")
        creation_revision = hashlib.sha256(
            remotes["create-event"].model_dump_json().encode()
        ).hexdigest()
        with self.memory.transaction() as c:
            c.execute(
                "UPDATE event_proposals SET creation_revision=? WHERE id=?",
                (creation_revision, id_),
            )
        ctx, conn = self.validate(self.get(id_, project_id), project_id)
        decision = self.policy.evaluate(
            ToolCall("criar_evento_agenda", {"proposal_id": id_, "digest": digest}),
            ctx,
            consume_approval=False,
        )
        if decision.action is not Action.CONFIRM:
            raise MCPError("event_review_denied")
        with self.memory.transaction() as c:
            c.execute("UPDATE event_proposals SET status='pending' WHERE id=?", (id_,))
        return {"approval_id": decision.approval_id, "proposal_id": id_}

    async def resume(self, id_, project_id, approval_id):
        proposal = self.get(id_, project_id)
        ctx, conn = self.validate(proposal, project_id)
        remotes = {t.name: t for t in await conn.list_tools()}
        if (
            "create-event" not in remotes
            or hashlib.sha256(remotes["create-event"].model_dump_json().encode()).hexdigest()
            != proposal["creation_revision"]
        ):
            raise MCPError("event_creation_schema_changed")
        approval = self.policy.approvals.get(approval_id)
        args = {"proposal_id": id_, "digest": proposal["digest"]}
        if (
            not approval
            or approval.status is not Status.APPROVED
            or approval.tool != "criar_evento_agenda"
            or approval.session_id != ctx.session_id
            or approval.args != args
        ):
            raise MCPError("event_approval_unavailable")
        decision = self.policy.evaluate(ToolCall("criar_evento_agenda", args), ctx)
        if decision.action is not Action.ALLOW:
            raise MCPError("event_approval_unavailable")
        with self.memory.transaction() as c:
            changed = c.execute(
                "UPDATE event_proposals SET status='sending' WHERE id=? AND status IN "
                "('draft','pending')",
                (id_,),
            ).rowcount
        if changed != 1:
            raise MCPError("event_already_submitted")
        status = "unknown"
        try:
            result = await conn.call("create-event", proposal["payload"])
            status = "failed" if result.is_error else "created"
            return {"ok": not result.is_error, "status": status}
        finally:
            # Persist even cancellation/timeout: never silently replay a possibly applied event.
            with self.memory.transaction() as c:
                c.execute("UPDATE event_proposals SET status=? WHERE id=?", (status, id_))


def router(require_admin):
    api = APIRouter(prefix="/calendar/proposals", dependencies=[Depends(require_admin)])

    @api.get("")
    def listing(request: Request, project_id: str | None = None):
        e = request.app.state.orion.events
        return [
            e.get(r[0], project_id)
            for r in e.memory.query(
                "SELECT id FROM event_proposals WHERE project_id IS ? ORDER BY "
                "created_at DESC LIMIT 100",
                (project_id,),
            )
        ]

    @api.post("")
    def create(body: Proposal, request: Request, project_id: str | None = None):
        return request.app.state.orion.events.create(body, project_id)

    @api.post("/{id_}/review")
    async def review(id_: str, body: Review, request: Request, project_id: str | None = None):
        s = request.app.state.orion
        if s.agent and s.agent.busy(s.events.get(id_, project_id)["session_id"]):
            raise HTTPException(409, "session_busy")
        return await s.events.review(id_, project_id, body.reviewed_digest)

    @api.post("/{id_}/resume")
    async def resume(id_: str, body: Resume, request: Request, project_id: str | None = None):
        s = request.app.state.orion
        if s.agent and s.agent.busy(s.events.get(id_, project_id)["session_id"]):
            raise HTTPException(409, "session_busy")
        return await s.events.resume(id_, project_id, body.approval_id)

    return api
