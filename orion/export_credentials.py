"""Credenciais de clientes MCP de entrada: hash, expiração e um único escopo."""

from __future__ import annotations

import hashlib
import json
import secrets
import uuid
from typing import Literal

from fastapi import APIRouter, Depends, HTTPException, Request, Response
from pydantic import BaseModel, ConfigDict, Field

from .projects import Projects

Permission = Literal["search", "facts", "sources", "artifacts"]


class Grant(BaseModel):
    model_config = ConfigDict(extra="forbid")
    name: str = Field(min_length=1, max_length=100)
    project_id: str | None = Field(default=None, pattern=r"^[a-f0-9]{32}$")
    permissions: list[Permission] = Field(min_length=1, max_length=4)
    lifetime_hours: int = Field(default=168, ge=1, le=720)


class ExportCredentials:
    def __init__(self, memory):
        self.memory = memory

    def issue(self, body):
        if body.project_id and Projects(self.memory).get(body.project_id)["archived"]:
            raise HTTPException(409, "project_archived")
        id_, token = uuid.uuid4().hex, "orion_client_" + secrets.token_urlsafe(32)
        expires = self.memory.clock() + body.lifetime_hours * 3600
        with self.memory.transaction() as c:
            if c.execute("SELECT count(*) FROM export_clients WHERE revoked=0").fetchone()[0] >= 50:
                raise HTTPException(409, "export_client_limit")
            c.execute(
                "INSERT INTO export_clients(id,name,project_id,permissions,token_hash,"
                "expires_at,revoked,created_at) VALUES (?,?,?,?,?,?,0,?)",
                (
                    id_,
                    body.name,
                    body.project_id,
                    json.dumps(sorted(set(body.permissions))),
                    hashlib.sha256(token.encode()).hexdigest(),
                    expires,
                    self.memory.clock(),
                ),
            )
        return {"id": id_, "token": token, "expires_at": expires}

    def listing(self):
        return [
            {**dict(row), "permissions": json.loads(row["permissions"])}
            for row in self.memory.query(
                "SELECT id,name,project_id,permissions,expires_at,revoked,created_at "
                "FROM export_clients ORDER BY created_at DESC LIMIT 100"
            )
        ]

    def revoke(self, id_):
        with self.memory.transaction() as c:
            if c.execute("UPDATE export_clients SET revoked=1 WHERE id=?", (id_,)).rowcount != 1:
                raise HTTPException(404, "export_client_not_found")
        return {"ok": True}

    def verify(self, token, permission=None):
        if not isinstance(token, str) or not token.startswith("orion_client_") or len(token) > 128:
            raise PermissionError("export_unauthorized")
        rows = self.memory.query(
            "SELECT id,project_id,permissions,expires_at,revoked FROM export_clients "
            "WHERE token_hash=?",
            (hashlib.sha256(token.encode()).hexdigest(),),
        )
        if not rows or rows[0]["revoked"] or rows[0]["expires_at"] <= self.memory.clock():
            raise PermissionError("export_unauthorized")
        row = dict(rows[0])
        row["permissions"] = json.loads(row["permissions"])
        if permission and permission not in row["permissions"]:
            raise PermissionError("export_permission_denied")
        if row["project_id"] and Projects(self.memory).get(row["project_id"])["archived"]:
            raise PermissionError("export_project_unavailable")
        return row


def router(require_admin):
    api = APIRouter(prefix="/mcp-export/clients", dependencies=[Depends(require_admin)])

    @api.get("")
    def listing(request: Request):
        return request.app.state.orion.export_credentials.listing()

    @api.post("")
    def issue(body: Grant, request: Request, response: Response):
        response.headers["Cache-Control"] = "no-store"
        return request.app.state.orion.export_credentials.issue(body)

    @api.post("/{id_}/revoke")
    def revoke(id_: str, request: Request):
        return request.app.state.orion.export_credentials.revoke(id_)

    return api
