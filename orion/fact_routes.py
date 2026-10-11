"""Fatos auditáveis: leitura escopada e mutações revisadas pela política existente."""

from __future__ import annotations

import asyncio
from dataclasses import asdict
from typing import Literal

from fastapi import APIRouter, Depends, HTTPException, Request
from pydantic import BaseModel, ConfigDict, Field, model_validator

from .memory.scope import data_scope
from .policy import Action, Context, Status, ToolCall
from .projects import Projects


class Review(BaseModel):
    model_config = ConfigDict(extra="forbid")
    action: Literal["edit", "forget"]
    text: str | None = Field(default=None, min_length=1, max_length=16000)
    source: str | None = Field(default=None, max_length=1000)
    session_id: str | None = Field(default=None, pattern=r"^[a-f0-9]{32}$")

    @model_validator(mode="after")
    def editing(self):
        if self.action == "edit" and (self.text is None or not self.text.strip()):
            raise ValueError("fact_text_required")
        return self


class Mover(BaseModel):
    """`project_id` None = memória pessoal. Obrigatório no corpo: omitir não vale como pessoal."""

    model_config = ConfigDict(extra="forbid")
    project_id: str | None = Field(pattern=r"^[a-f0-9]{32}$")


class Validade(BaseModel):
    """`valid_until` em epoch (segundos); None tira o prazo."""

    model_config = ConfigDict(extra="forbid")
    valid_until: float | None = Field(ge=0)


class Substituir(BaseModel):
    model_config = ConfigDict(extra="forbid")
    by: int = Field(ge=1)


class Decisao(BaseModel):
    model_config = ConfigDict(extra="forbid")
    older_id: int = Field(ge=1)
    newer_id: int = Field(ge=1)
    decision: Literal["novo", "ambos"]


class Resume(BaseModel):
    model_config = ConfigDict(extra="forbid")
    approval_id: str = Field(min_length=1, max_length=128)


def router(require_admin) -> APIRouter:
    api = APIRouter(prefix="/facts", dependencies=[Depends(require_admin)])

    def state(request: Request):
        return request.app.state.orion

    def fact(s, id_: int, project_id: str | None):
        rows = s.memory.query(
            "SELECT * FROM facts WHERE id=? AND project_id IS ?", (id_, project_id)
        )
        if not rows:
            raise HTTPException(404, "fact_not_found")
        return {k: v for k, v in dict(rows[0]).items() if k != "embedded"}

    def context(s, session_id: str | None, project_id: str | None):
        session = s.memory.get_session(session_id) if session_id else s.memory.active_session("web")
        if session is None or session.channel != "web" or session.project_id != project_id:
            raise HTTPException(409, "fact_session_out_of_scope")
        if session.archived or session.read_only or (s.agent and s.agent.busy(session.id)):
            raise HTTPException(409, "session_busy_or_read_only")
        ctx = Context(
            session.id,
            project_id=project_id,
            tainted=s.memory.counter_get(f"taint:{session.id}") > 0,
        )
        if project_id:
            project = Projects(s.memory).get(project_id)
            if project["archived"]:
                raise HTTPException(409, "project_archived")
            ctx.project_revision = project["updated_at"]
        return ctx

    @api.get("")
    def list_facts(request: Request, project_id: str | None = None, query: str = ""):
        s = state(request)
        with data_scope(project_id, include_personal=False):
            return [
                asdict(f) for f in s.memory.facts() if query[:200].casefold() in f.text.casefold()
            ][:200]

    @api.get("/conflicts")
    def conflicts(request: Request, project_id: str | None = None):
        """Pares de fatos que o ciclo de sono achou contraditórios e esperam a sua decisão."""
        s = state(request)
        with data_scope(project_id, include_personal=False):
            return [
                {"older": asdict(a), "newer": asdict(b)} for a, b in s.memory.pending_conflicts()
            ]

    @api.post("/conflicts/resolve")
    def resolve_conflict(body: Decisao, request: Request, project_id: str | None = None):
        """ "o novo vale" marca o antigo como substituído (pesa menos na busca); "os dois valem"
        encerra o aviso. Nenhum dos dois apaga nada, e só o Antônio decide (regra 54)."""
        s = state(request)
        with data_scope(project_id, include_personal=False):
            fact(s, body.older_id, project_id)
            fact(s, body.newer_id, project_id)
            try:
                s.memory.resolve_conflict(body.older_id, body.newer_id, body.decision)
            except KeyError as erro:
                raise HTTPException(404, "conflict_not_found") from erro
        return {"ok": True}

    @api.get("/pending")
    def pending(request: Request, project_id: str | None = None):
        s = state(request)
        result = []
        for approval in s.policy.approvals.pending(include_approved=True):
            session = s.memory.get_session(approval.session_id)
            if (
                session
                and session.project_id == project_id
                and approval.tool in {"editar_fato", "esquecer_fato"}
            ):
                original = s.memory.query(
                    "SELECT text FROM facts WHERE id=? AND project_id IS ?",
                    (approval.args.get("id"), project_id),
                )
                if not original:
                    continue
                result.append(
                    {
                        "id": approval.id,
                        "tool": approval.tool,
                        "fact_id": approval.args.get("id"),
                        "session_id": session.id,
                        "reason": approval.reason,
                        "status": approval.status.value,
                        "text": approval.args.get("texto") or original[0][0],
                        "source": approval.args.get("fonte"),
                    }
                )
        return result

    @api.get("/{id_}")
    def detail(id_: int, request: Request, project_id: str | None = None):
        return fact(state(request), id_, project_id)

    @api.delete("/{id_}")
    def forget(id_: int, request: Request, project_id: str | None = None):
        """Apaga o fato por um clique do próprio Antônio (ex.: "apagar o repetido"). O que o
        modelo pede continua passando por `/review` e pela aprovação (regra 2)."""
        s = state(request)
        fact(s, id_, project_id)
        s.policy.approvals.invalidate_fact(id_)
        with data_scope(project_id, include_personal=False):
            return {"ok": s.memory.forget_fact(id_)}

    @api.post("/{id_}/move")
    def move(id_: int, body: Mover, request: Request, project_id: str | None = None):
        """ "Projeto" do fato: muda onde ele vale (None = pessoal). Fato de projeto nunca aparece
        em outro projeto nem sem projeto (regra 52)."""
        s = state(request)
        fact(s, id_, project_id)
        if body.project_id:
            rows = s.memory.query("SELECT archived FROM projects WHERE id=?", (body.project_id,))
            if not rows or rows[0][0]:
                raise HTTPException(409, "project_missing_or_archived")
        s.policy.approvals.invalidate_fact(id_)
        return asdict(s.memory.move_fact(id_, body.project_id))

    @api.post("/{id_}/validity")
    def validity(id_: int, body: Validade, request: Request, project_id: str | None = None):
        s = state(request)
        fact(s, id_, project_id)
        with data_scope(project_id, include_personal=False):
            return asdict(s.memory.set_fact_validity(id_, body.valid_until))

    @api.post("/{id_}/supersede")
    def supersede(id_: int, body: Substituir, request: Request, project_id: str | None = None):
        """O fato `by` vale no lugar de `id_` (os dois ficam; o antigo pesa menos na busca)."""
        s = state(request)
        fact(s, id_, project_id)
        fact(s, body.by, project_id)
        with data_scope(project_id, include_personal=False):
            try:
                return asdict(s.memory.supersede_fact(id_, body.by))
            except ValueError as erro:
                raise HTTPException(422, str(erro)) from erro

    @api.post("/{id_}/review")
    def review(id_: int, body: Review, request: Request, project_id: str | None = None):
        s = state(request)
        fact(s, id_, project_id)
        ctx = context(s, body.session_id, project_id)
        name = "editar_fato" if body.action == "edit" else "esquecer_fato"
        args: dict[str, object] = {"id": id_}
        if body.action == "edit":
            args.update(texto=body.text, fonte=body.source)
        decision = s.policy.evaluate(ToolCall(name, args), ctx, consume_approval=False)
        if decision.action is Action.DENY:
            raise HTTPException(403, decision.reason)
        if decision.action is not Action.CONFIRM:
            raise HTTPException(409, "fact_review_requires_new_approval")
        return {
            "approval_id": decision.approval_id,
            "session_id": ctx.session_id,
            "status": "pending",
        }

    @api.post("/{id_}/resume")
    async def resume(id_: int, body: Resume, request: Request, project_id: str | None = None):
        s = state(request)
        fact(s, id_, project_id)
        approval = s.policy.approvals.get(body.approval_id)
        if (
            approval is None
            or approval.status is not Status.APPROVED
            or approval.tool not in {"editar_fato", "esquecer_fato"}
            or approval.args.get("id") != id_
        ):
            raise HTTPException(409, "fact_approval_unavailable")
        ctx = context(s, approval.session_id, project_id)
        decision = s.policy.evaluate(ToolCall(approval.tool, approval.args), ctx)
        if decision.action is not Action.ALLOW:
            raise HTTPException(409, "fact_approval_out_of_scope")
        s.policy.approvals.invalidate_fact(id_)
        with data_scope(project_id, include_personal=False):
            if approval.tool == "esquecer_fato":
                await asyncio.to_thread(s.memory.forget_fact, id_)
                return {"ok": True, "forgotten": True}
            result = await asyncio.to_thread(
                s.memory.update_fact, id_, approval.args["texto"], approval.args.get("fonte")
            )
            return {"ok": True, "fact": asdict(result)}

    return api
