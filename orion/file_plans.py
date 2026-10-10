"""Organização revisável de cópias; originais preservados, sem scripts/shell."""

from __future__ import annotations

import hashlib
import json
import os
import stat
import uuid
from pathlib import Path

from fastapi import APIRouter, Depends, HTTPException, Request
from pydantic import BaseModel, ConfigDict, Field

from .extensions.host import MCPError
from .memory.scope import current
from .policy import Action, Risk, Status, ToolCall, ToolSpec
from .tools.registry import Tool

GROUPS = {
    ".md": "Textos",
    ".txt": "Textos",
    ".csv": "Textos",
    ".pdf": "Documentos",
    ".png": "Imagens",
    ".jpg": "Imagens",
    ".jpeg": "Imagens",
    ".webp": "Imagens",
}
MAX_FILE = 8 * 1024 * 1024
MAX_PLAN = 32 * 1024 * 1024


class Plan(BaseModel):
    model_config = ConfigDict(extra="forbid")
    root: str = Field(min_length=1, max_length=4096)
    session_id: str = Field(pattern=r"^[a-f0-9]{32}$")


class Review(BaseModel):
    model_config = ConfigDict(extra="forbid")
    reviewed_digest: str = Field(pattern=r"^[a-f0-9]{64}$")


class Apply(BaseModel):
    model_config = ConfigDict(extra="forbid")
    approval_id: str = Field(min_length=1, max_length=128)


class FilePlans:
    def __init__(self, events):
        self.events = events
        self.memory, self.policy = events.memory, events.policy
        self.guard = self.policy.path_guard
        self.registry = events.calendar.registry
        self.policy.tools["aplicar_organizacao"] = ToolSpec(
            "aplicar_organizacao", Risk.WRITE, require_confirmation=True
        )
        self.policy.tools["propor_organizacao"] = ToolSpec("propor_organizacao", Risk.WRITE)

        def propose(root):
            scope = current.get()
            if not scope.session_id:
                raise MCPError("file_plan_session_required")
            row = self.create(Plan(root=root, session_id=scope.session_id), scope.project_id)
            return {
                "ok": True,
                "plan_id": row["id"],
                "files": len(row["copies"]),
                "action": "Revise as cópias e confirme em Fontes. Nenhum arquivo foi criado.",
            }

        self.registry.register(
            Tool(
                "propor_organizacao",
                "Preparar plano de cópias por tipo em Organizados, sem alterar arquivos. "
                "Exige pasta autorizada.",
                {
                    "type": "object",
                    "properties": {"root": {"type": "string", "maxLength": 4096}},
                    "required": ["root"],
                    "additionalProperties": False,
                },
                propose,
                validar=True,
            )
        )

    def root(self, value, ctx):
        root = Path(value).expanduser()
        if not root.is_absolute() or "\0" in value or not root.is_dir():
            raise MCPError("file_plan_root_invalid")
        if any(p.is_symlink() for p in (root, *root.parents)):
            raise MCPError("file_plan_symlink_denied")
        root = root.resolve()
        if ctx.project_id:
            project = self.memory.query("SELECT root FROM projects WHERE id=?", (ctx.project_id,))[
                0
            ][0]
            if not project or not root.is_relative_to(Path(project).resolve()):
                raise MCPError("file_plan_root_out_of_project")
        if self.guard.check_read(str(root)) or self.guard.check_write(str(root)):
            raise MCPError("file_plan_root_not_authorized")
        return root

    def path(self, root, relative):
        part = Path(relative)
        if part.is_absolute() or any(p in {"..", "."} for p in part.parts):
            raise MCPError("file_plan_path_invalid")
        path = root / part
        if not path.resolve().is_relative_to(root) or any(
            p.is_symlink() for p in (path, *list(path.parents)[: len(part.parts) - 1])
        ):
            raise MCPError("file_plan_symlink_denied")
        if self.guard.check_read(str(path)) or self.guard.check_write(str(path)):
            raise MCPError("file_plan_path_not_authorized")
        return path

    @staticmethod
    def read(path):
        fd = os.open(
            path,
            os.O_RDONLY | getattr(os, "O_NONBLOCK", 0) | getattr(os, "O_NOFOLLOW", 0),
        )
        with os.fdopen(fd, "rb") as file:
            info = os.fstat(file.fileno())
            if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1 or info.st_size > MAX_FILE:
                raise MCPError("file_plan_file_limit_or_link")
            data = file.read(MAX_FILE + 1)
            if len(data) > MAX_FILE:
                raise MCPError("file_plan_file_limit_or_link")
            return data

    def create(self, body, project_id):
        ctx = self.events.context(body.session_id, project_id)
        root = self.root(body.root, ctx)
        copies, skipped, total = [], [], 0
        entries = []
        for path in root.iterdir():
            if len(entries) >= 1000:
                raise MCPError("file_plan_listing_limit")
            entries.append(path)
        for path in sorted(entries, key=lambda p: p.name.casefold()):
            if not path.is_file() or path.suffix.lower() not in GROUPS:
                continue
            try:
                if len(path.name) > 160:
                    raise MCPError("file_plan_name_limit")
                source = self.path(root, path.name)
                raw = self.read(source)
                target = f"Organizados/{GROUPS[path.suffix.lower()]}/{path.name}"
                dest = self.path(root, target)
                if dest.exists():
                    raise MCPError("file_plan_target_exists")
                total += len(raw)
                if total > MAX_PLAN or len(copies) >= 100:
                    raise MCPError("file_plan_size_limit")
                copies.append(
                    {
                        "source": path.name,
                        "target": target,
                        "digest": hashlib.sha256(raw).hexdigest(),
                        "size": len(raw),
                    }
                )
            except (MCPError, OSError) as error:
                reason = error.code if isinstance(error, MCPError) else "file_plan_read_failed"
                skipped.append({"name": path.name, "reason": reason})
        if not copies:
            raise MCPError("file_plan_no_eligible_files")
        payload = json.dumps({"root": str(root), "copies": copies}, sort_keys=True)
        id_ = uuid.uuid4().hex
        with self.memory.transaction() as c:
            if c.execute("SELECT count(*) FROM file_plans").fetchone()[0] >= 500:
                raise MCPError("file_plan_limit")
            c.execute(
                "INSERT INTO file_plans(id,project_id,session_id,payload,digest,skipped,"
                "status,created_at) VALUES (?,?,?,?,?,?,?,?)",
                (
                    id_,
                    project_id,
                    body.session_id,
                    payload,
                    hashlib.sha256(payload.encode()).hexdigest(),
                    json.dumps(skipped),
                    "draft",
                    self.memory.clock(),
                ),
            )
        return self.get(id_, project_id)

    def get(self, id_, project_id):
        rows = self.memory.query(
            "SELECT * FROM file_plans WHERE id=? AND project_id IS ?", (id_, project_id)
        )
        if not rows:
            raise MCPError("file_plan_not_found")
        row = dict(rows[0])
        payload = json.loads(row.pop("payload"))
        row.update(payload)
        row["skipped"] = json.loads(row["skipped"])
        row["approvals"] = []
        if row["status"] in {"draft", "pending"}:
            try:
                ctx = self.context(row, project_id)
                binding = self.policy.binding("aplicar_organizacao", ctx)
                row["approvals"] = [
                    {"id": a.id, "status": a.status.value}
                    for a in self.policy.approvals.pending(include_approved=True)
                    if a.tool == "aplicar_organizacao"
                    and a.args.get("plan_id") == id_
                    and a.binding == binding
                    and a.session_id == row["session_id"]
                ]
            except MCPError:
                row["review_error"] = "file_plan_context_changed"
        return row

    def context(self, row, project_id):
        if row["status"] not in {"draft", "pending"}:
            raise MCPError("file_plan_already_submitted")
        ctx = self.events.context(row["session_id"], project_id)
        self.root(row["root"], ctx)
        ctx.authorities = (f"file-plan:{row['id']}:{row['digest']}",)
        return ctx

    def review(self, id_, project_id, digest):
        row = self.get(id_, project_id)
        if digest != row["digest"]:
            raise MCPError("file_plan_review_changed")
        ctx = self.context(row, project_id)
        decision = self.policy.evaluate(
            ToolCall("aplicar_organizacao", {"plan_id": id_, "digest": digest}),
            ctx,
            consume_approval=False,
        )
        if decision.action is not Action.CONFIRM:
            raise MCPError("file_plan_review_denied")
        with self.memory.transaction() as c:
            c.execute("UPDATE file_plans SET status='pending' WHERE id=?", (id_,))
        return {"approval_id": decision.approval_id}

    def apply(self, id_, project_id, approval_id):
        row = self.get(id_, project_id)
        ctx = self.context(row, project_id)
        approval = self.policy.approvals.get(approval_id)
        args = {"plan_id": id_, "digest": row["digest"]}
        if (
            not approval
            or approval.status is not Status.APPROVED
            or approval.tool != "aplicar_organizacao"
            or approval.args != args
            or approval.session_id != ctx.session_id
        ):
            raise MCPError("file_plan_approval_unavailable")
        root = self.root(row["root"], ctx)
        # Validate and freeze every reviewed byte before making the first directory/file.
        frozen = []
        for item in row["copies"]:
            source = self.path(root, item["source"])
            target = self.path(root, item["target"])
            try:
                raw = self.read(source)
            except OSError as error:
                raise MCPError("file_plan_snapshot_changed") from error
            if (
                hashlib.sha256(raw).hexdigest() != item["digest"]
                or len(raw) != item["size"]
                or target.exists()
            ):
                raise MCPError("file_plan_snapshot_changed")
            frozen.append((target, raw))
        if (
            self.policy.evaluate(ToolCall("aplicar_organizacao", args), ctx).action
            is not Action.ALLOW
        ):
            raise MCPError("file_plan_approval_unavailable")
        with self.memory.transaction() as c:
            if (
                c.execute(
                    "UPDATE file_plans SET status='applying' WHERE id=? "
                    "AND status IN ('draft','pending')",
                    (id_,),
                ).rowcount
                != 1
            ):
                raise MCPError("file_plan_already_submitted")
        status = "partial"
        try:
            for target, raw in frozen:
                self.path(root, str(target.relative_to(root)))
                target.parent.mkdir(parents=True, exist_ok=True)
                self.path(root, str(target.relative_to(root)))
                fd = os.open(
                    target,
                    os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, "O_NOFOLLOW", 0),
                    0o600,
                )
                with os.fdopen(fd, "wb") as file:
                    file.write(raw)
                with self.memory.transaction() as c:
                    c.execute("UPDATE file_plans SET completed=completed+1 WHERE id=?", (id_,))
            status = "complete"
            return {"ok": True, "status": status, "copies": len(frozen)}
        except OSError as error:
            raise MCPError("file_plan_partial_failure") from error
        finally:
            with self.memory.transaction() as c:
                c.execute("UPDATE file_plans SET status=? WHERE id=?", (status, id_))


def router(require_admin):
    api = APIRouter(prefix="/files/plans", dependencies=[Depends(require_admin)])

    @api.get("")
    def listing(request: Request, project_id: str | None = None):
        plans = request.app.state.orion.file_plans
        return [
            plans.get(r[0], project_id)
            for r in plans.memory.query(
                "SELECT id FROM file_plans WHERE project_id IS ? "
                "ORDER BY created_at DESC LIMIT 100",
                (project_id,),
            )
        ]

    @api.post("")
    def create(body: Plan, request: Request, project_id: str | None = None):
        s = request.app.state.orion
        if s.agent and s.agent.busy(body.session_id):
            raise HTTPException(409, "session_busy")
        return s.file_plans.create(body, project_id)

    @api.post("/{id_}/review")
    def review(id_: str, body: Review, request: Request, project_id: str | None = None):
        s = request.app.state.orion
        row = s.file_plans.get(id_, project_id)
        if s.agent and s.agent.busy(row["session_id"]):
            raise HTTPException(409, "session_busy")
        return s.file_plans.review(id_, project_id, body.reviewed_digest)

    @api.post("/{id_}/apply")
    def apply(id_: str, body: Apply, request: Request, project_id: str | None = None):
        s = request.app.state.orion
        row = s.file_plans.get(id_, project_id)
        if s.agent and s.agent.busy(row["session_id"]):
            raise HTTPException(409, "session_busy")
        return s.file_plans.apply(id_, project_id, body.approval_id)

    return api
