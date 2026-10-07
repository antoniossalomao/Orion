"""API de administração fora do catálogo de ferramentas do modelo."""

from __future__ import annotations

import asyncio
from pathlib import Path

from fastapi import APIRouter, Depends, HTTPException, Request
from pydantic import BaseModel, ConfigDict, Field, TypeAdapter

from ..policy import Risk
from .archives import MAX_ARCHIVE, archive_snapshot
from .host import Connection, ConnectionConfig
from .manager import PluginManager, public_plugin
from .plugins import PluginError


class Install(BaseModel):
    model_config = ConfigDict(extra="forbid")
    folder: str = Field(min_length=1, max_length=4096)
    update: bool = False


class Review(BaseModel):
    model_config = ConfigDict(extra="forbid")
    digest: str = Field(pattern=r"^[a-f0-9]{64}$")
    capabilities: list[str] = Field(default_factory=list, max_length=256)
    trusted_local: bool = False
    authorized_remote: bool = False
    classifications: dict[str, dict[str, Risk]] = Field(default_factory=dict, max_length=32)
    scope: str = Field(default="personal", min_length=1, max_length=128)


def manager(request: Request) -> PluginManager:
    value = request.app.state.orion.extensions
    if not isinstance(value, PluginManager):
        raise HTTPException(503, "extensions_unavailable")
    return value


def router(require_admin) -> APIRouter:
    api = APIRouter(dependencies=[Depends(require_admin)])

    @api.get("/plugins")
    def catalog(request: Request):
        return manager(request).list()

    @api.post("/plugins/install")
    async def install(body: Install, request: Request):
        if not Path(body.folder).is_absolute():
            raise PluginError("bundle_source_invalid")
        m = manager(request)
        async with m.lock:
            row = await asyncio.to_thread(
                m.installer.install_folder, Path(body.folder), update=body.update
            )
        return public_plugin(row)

    @api.post("/plugins/import")
    async def import_zip(request: Request, update: bool = False):
        if request.headers.get("content-type", "").split(";")[0] != "application/zip":
            raise HTTPException(415, "plugin_zip_required")
        data = bytearray()
        async for chunk in request.stream():
            data.extend(chunk)
            if len(data) > MAX_ARCHIVE:
                raise HTTPException(413, "archive_size_limit")
        package = await asyncio.to_thread(archive_snapshot, bytes(data), "plugin.zip")
        m = manager(request)
        async with m.lock:
            row = await asyncio.to_thread(
                m.installer.install_snapshot, package, origin="zip", update=update
            )
        return public_plugin(row)

    @api.get("/plugins/{id_}/versions")
    def versions(id_: str, request: Request):
        return [
            {
                "digest": row["digest"],
                "version": row["version"],
                "capabilities": row["manifest"]["capabilities"],
            }
            for row in manager(request).store.versions(id_)
        ]

    @api.get("/plugins/{id_}/plan/{digest}")
    async def plan(id_: str, digest: str, request: Request):
        return await asyncio.to_thread(manager(request).installer.plan_update, id_, digest)

    @api.post("/plugins/{id_}/activate")
    async def activate(id_: str, body: Review, request: Request):
        m = manager(request)
        async with m.lock:
            return await m.activate(
                id_,
                body.digest,
                set(body.capabilities),
                trusted_local=body.trusted_local,
                authorized_remote=body.authorized_remote,
                classifications=body.classifications,
                scope=body.scope,
            )

    @api.post("/plugins/{id_}/deactivate")
    async def deactivate(id_: str, request: Request):
        m = manager(request)
        async with m.lock:
            return await m.deactivate(id_)

    @api.post("/plugins/{id_}/version")
    async def select_version(id_: str, body: Review, request: Request):
        m = manager(request)
        async with m.lock:
            # Antes de revogar a revisão atual, validar a revisão apresentada.
            await asyncio.to_thread(m.installer.bundle, id_, body.digest)
            target = m.store.version(id_, body.digest)
            if set(body.capabilities) != set(target["manifest"]["capabilities"]):
                raise PluginError("plugin_review_required")
            await m.deactivate(id_)
            row = await asyncio.to_thread(
                m.installer.change_version,
                id_,
                body.digest,
                reviewed_digest=body.digest,
                reviewed_capabilities=set(body.capabilities),
            )
        return public_plugin(row)

    @api.delete("/plugins/{id_}")
    async def remove(id_: str, request: Request):
        m = manager(request)
        async with m.lock:
            await m.deactivate(id_)
            await asyncio.to_thread(m.installer.uninstall, id_)
        return {"ok": True}

    @api.get("/plugins/{id_}/diagnostics")
    def diagnostics(id_: str, request: Request):
        m = manager(request)
        row = m.store.get(id_)
        return {
            "id": id_,
            "state": row["state"],
            "code": row["error"],
            "connections": [
                m.host.connections[cid].status() for cid in m.connections.get(id_, set())
            ],
        }

    @api.get("/mcp/connections")
    def connections(request: Request):
        return manager(request).host.statuses()

    @api.post("/mcp/connections")
    async def configure(body: dict, request: Request):
        try:
            config = TypeAdapter(ConnectionConfig).validate_python(body)
        except ValueError:
            raise HTTPException(422, "connection_config_invalid") from None
        m = manager(request)
        async with m.lock:
            if config.id in m.host.connections or len(m.host.connections) >= 32:
                raise PluginError("connection_id_conflict")
            m.host.connections[config.id] = Connection(config)
        return {"id": config.id, "state": "configured"}

    @api.post("/mcp/connections/{id_}/test")
    async def test(id_: str, request: Request):
        m = manager(request)
        async with m.lock:
            connection = m.host.connections.get(id_)
            if connection is None:
                raise PluginError("connection_not_found")
            await connection.start()
            # Só handshake/discovery. Nunca call_tool nem read_resource/prompt.
            tools = await connection.list_tools() if connection.state == "connected" else []
            await m.catalog.refresh()
            return {**connection.status(), "tools": [{"name": tool.name} for tool in tools[:128]]}

    @api.delete("/mcp/connections/{id_}")
    async def disconnect(id_: str, request: Request):
        m = manager(request)
        async with m.lock:
            if any(id_ in owned for owned in m.connections.values()):
                raise PluginError("plugin_disable_first")
            connection = m.host.connections.pop(id_, None)
            if connection is None:
                raise PluginError("connection_not_found")
            await connection.close()
            await m.catalog.refresh()
        return {"ok": True}

    return api
