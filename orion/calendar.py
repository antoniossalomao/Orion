"""Ponte em leitura para Google Calendar MCP 2.7.0; conta nunca inferida/mesclada."""

from __future__ import annotations

import hashlib
import json
import uuid
from datetime import datetime
from zoneinfo import ZoneInfo, ZoneInfoNotFoundError

from fastapi import APIRouter, Depends, HTTPException, Request
from pydantic import BaseModel, ConfigDict, Field

from .extensions.host import HTTPConfig, MCPError, MCPHost
from .memory import MemoryStore
from .memory.scope import current
from .policy import PolicyEngine, Risk, ToolSpec
from .tools.registry import Tool, ToolRegistry

READ_TOOLS = {"list-events", "get-freebusy", "get-current-time"}


class Binding(BaseModel):
    model_config = ConfigDict(extra="forbid")
    scope: str = Field(default="personal", pattern=r"^(personal|project:[a-f0-9]{32})$")
    connection_id: str = Field(pattern=r"^[a-z0-9][a-z0-9_-]{0,47}$")
    account: str = Field(pattern=r"^[a-z0-9_-]{1,64}$")
    calendar_id: str = Field(default="primary", min_length=1, max_length=256)
    timezone: str = Field(default="America/Sao_Paulo", min_length=1, max_length=100)
    reviewed_read_only: bool = False


class Calendar:
    def __init__(
        self, memory: MemoryStore, host: MCPHost, registry: ToolRegistry, policy: PolicyEngine
    ):
        self.memory, self.host, self.registry, self.policy = memory, host, registry, policy
        parameters = {
            "type": "object",
            "properties": {
                "start": {"type": "string", "format": "date-time"},
                "end": {"type": "string", "format": "date-time"},
            },
            "required": ["start", "end"],
            "additionalProperties": False,
        }
        for name, remote, description in [
            (
                "consultar_agenda",
                "list-events",
                "Consultar compromissos no período e fuso revisados.",
            ),
            (
                "consultar_disponibilidade",
                "get-freebusy",
                "Consultar disponibilidade no período sem criar eventos.",
            ),
        ]:

            async def execute(start, end, remote=remote):
                return await self.read(remote, start, end)

            registry.register(Tool(name, description, parameters, execute))
            policy.tools[name] = ToolSpec(
                name, Risk.READ, external=True, origin="google-calendar-mcp:2.7.0"
            )

    def binding(self, scope=None):
        if scope is None:
            project = current.get().project_id
            scope = f"project:{project}" if project else "personal"
        rows = self.memory.query("SELECT * FROM calendar_bindings WHERE scope=?", (scope,))
        if not rows:
            raise MCPError("calendar_not_configured")
        row = dict(rows[0])
        conn = self.host.connections.get(row["connection_id"])
        if not conn or conn.config.scope != scope or conn.state != "connected":
            raise MCPError("calendar_connection_unavailable")
        if conn.authorized and not conn.authorized():
            raise MCPError("calendar_account_revoked")
        if row["connection_revision"] != self.connection_revision(conn):
            raise MCPError("calendar_connection_review_changed")
        return row, conn

    @staticmethod
    def connection_revision(conn):
        config = conn.config.model_dump(mode="json", exclude={"enabled", "authorized", "trusted"})
        return hashlib.sha256(json.dumps(config, sort_keys=True).encode()).hexdigest()

    async def bind(self, body):
        if not body.reviewed_read_only:
            raise MCPError("calendar_read_review_required")
        try:
            ZoneInfo(body.timezone)
        except (ZoneInfoNotFoundError, ValueError):
            raise MCPError("calendar_timezone_invalid") from None
        conn = self.host.connections.get(body.connection_id)
        if not conn or conn.config.scope != body.scope or conn.state != "connected":
            raise MCPError("calendar_connection_unavailable")
        if (
            isinstance(conn.config, HTTPConfig)
            and not conn.config.secret_ref
            and not conn.config.account_id
        ):
            from urllib.parse import urlsplit

            if urlsplit(conn.config.url).hostname not in {"localhost", "127.0.0.1", "::1"}:
                raise MCPError("calendar_remote_auth_required")
        if any(
            name not in READ_TOOLS or risk is not Risk.READ
            for name, risk in conn.config.classifications.items()
        ):
            raise MCPError("calendar_read_only_catalog_required")
        tools = {t.name: t for t in await conn.list_tools()}
        if not {"list-events", "get-freebusy"}.issubset(tools):
            raise MCPError("calendar_connector_incompatible")
        # Account management advertised upstream is excluded.
        schema = json.dumps(
            {n: tools[n].input_schema for n in ("list-events", "get-freebusy")}, sort_keys=True
        )
        revision = hashlib.sha256((schema + uuid.uuid4().hex).encode()).hexdigest()
        with self.memory.transaction() as c:
            c.execute(
                "INSERT OR REPLACE INTO calendar_bindings(scope,connection_id,account,calendar_id,"
                "timezone,revision,connection_revision) VALUES (?,?,?,?,?,?,?)",
                (
                    body.scope,
                    body.connection_id,
                    body.account,
                    body.calendar_id,
                    body.timezone,
                    revision,
                    self.connection_revision(conn),
                ),
            )
        return {**body.model_dump(), "revision": revision}

    async def read(self, remote, start, end):
        try:
            row, conn = self.binding()
            first, last = datetime.fromisoformat(start), datetime.fromisoformat(end)
            if (
                not first.tzinfo
                or not last.tzinfo
                or not 0 < (last - first).total_seconds() <= 31 * 86400
            ):
                raise MCPError("calendar_period_invalid")
            args = {"account": row["account"], "timeMin": start, "timeMax": end}
            if remote == "list-events":
                args.update(
                    calendarId=row["calendar_id"],
                    timeZone=row["timezone"],
                )
            else:
                args.update(calendars=[{"id": row["calendar_id"]}], timeZone=row["timezone"])
            tools = {t.name: t for t in await conn.list_tools()}
            if remote not in READ_TOOLS or remote not in tools:
                raise MCPError("calendar_connector_incompatible")
            validated = Tool(
                "calendar_remote", "", tools[remote].input_schema, lambda: None
            )._invalid(args)
            if validated:
                raise MCPError("calendar_connector_incompatible")
            result = await conn.call(remote, args)
            data = {
                "structured": result.structured_content,
                "content": [c.model_dump(mode="json") for c in result.content],
            }
            if len(json.dumps(data).encode()) > 32000:
                raise MCPError("calendar_result_limit")
            error_code = None
            if result.is_error:
                detail = json.dumps(data).casefold()
                error_code = (
                    "calendar_quota"
                    if any(x in detail for x in ("quota", "ratelimit", "429"))
                    else "calendar_provider_error"
                )
            return {
                "ok": not result.is_error,
                "code": error_code,
                "source": {
                    "connector": "@cocal/google-calendar-mcp@2.7.0",
                    "connection": row["connection_id"],
                    "account": row["account"],
                    "calendar": row["calendar_id"],
                    "timezone": row["timezone"],
                    "revision": row["revision"],
                    "generation": conn.generation,
                },
                "data": data,
            }
        except MCPError as error:
            return {
                "ok": False,
                "code": error.code,
                "action": "Revise a conta e a conexão da Agenda ou reduza o período da consulta.",
            }


def router(require_admin):
    api = APIRouter(prefix="/calendar", dependencies=[Depends(require_admin)])

    @api.get("")
    def bindings(request: Request):
        return [
            dict(r) for r in request.app.state.orion.memory.query("SELECT * FROM calendar_bindings")
        ]

    @api.post("/bind")
    async def bind(body: Binding, request: Request):
        if body.scope.startswith("project:"):
            rows = request.app.state.orion.memory.query(
                "SELECT archived FROM projects WHERE id=?", (body.scope[8:],)
            )
            if not rows or rows[0][0]:
                raise HTTPException(409, "project_unavailable")
        return await request.app.state.orion.calendar.bind(body)

    return api
