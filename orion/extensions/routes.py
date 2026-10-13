"""API de administração fora do catálogo de ferramentas do modelo."""

from __future__ import annotations

import asyncio
from pathlib import Path

from fastapi import APIRouter, Depends, HTTPException, Request
from pydantic import BaseModel, ConfigDict, Field, TypeAdapter

from ..policy import Risk
from .archives import MAX_ARCHIVE, archive_files, archive_snapshot
from .catalogo import LOCAL, versao_maior
from .host import ConnectionConfig
from .importar import converter
from .manager import PluginManager, public_plugin
from .plugins import PluginError
from .profiles import profile, profiles


class Install(BaseModel):
    model_config = ConfigDict(extra="forbid")
    folder: str = Field(min_length=1, max_length=4096)
    update: bool = False


class CatalogoInstalar(BaseModel):
    model_config = ConfigDict(extra="forbid")
    fonte: str = Field(min_length=1, max_length=2048)
    id: str = Field(pattern=r"^[a-z0-9][a-z0-9_-]{0,47}$")
    version: str | None = Field(default=None, max_length=80)


class Review(BaseModel):
    model_config = ConfigDict(extra="forbid")
    digest: str = Field(pattern=r"^[a-f0-9]{64}$")
    capabilities: list[str] = Field(default_factory=list, max_length=256)
    trusted_local: bool = False
    authorized_remote: bool = False
    classifications: dict[str, dict[str, Risk]] = Field(default_factory=dict, max_length=32)
    scope: str = Field(default="personal", pattern=r"^(personal|project:[a-f0-9]{32})$")


def manager(request: Request) -> PluginManager:
    value = request.app.state.orion.extensions
    if not isinstance(value, PluginManager):
        raise HTTPException(503, "extensions_unavailable")
    return value


def validate_scope(request: Request, scope: str) -> None:
    if scope.startswith("project:"):
        from ..projects import Projects

        if Projects(request.app.state.orion.memory).get(scope[8:])["archived"]:
            raise HTTPException(409, "project_archived")


def router(require_admin) -> APIRouter:
    api = APIRouter(dependencies=[Depends(require_admin)])

    @api.get("/plugins")
    def catalog(request: Request):
        return manager(request).list()

    @api.get("/plugins/available")
    async def available():
        return await asyncio.to_thread(profiles)

    @api.post("/plugins/builtin/{id_}")
    async def install_profile(id_: str, request: Request):
        package = await asyncio.to_thread(profile, id_)
        m = manager(request)
        async with m.lock:
            row = await asyncio.to_thread(
                m.installer.install_snapshot, package, origin="orion", update=True
            )
        return public_plugin(row)

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

    async def _zip(request: Request) -> bytes:
        if request.headers.get("content-type", "").split(";")[0] != "application/zip":
            raise HTTPException(415, "plugin_zip_required")
        data = bytearray()
        async for chunk in request.stream():
            data.extend(chunk)
            if len(data) > MAX_ARCHIVE:
                raise HTTPException(413, "archive_size_limit")
        return bytes(data)

    @api.post("/plugins/importar/previa")
    async def import_preview(request: Request):
        """Lê um ZIP no formato do Claude Code, do Codex ou do Orion e diz o que viraria um plugin
        e o que ficaria de fora. Não instala nada."""
        files = await asyncio.to_thread(archive_files, await _zip(request), "plugin.zip")
        return (await asyncio.to_thread(converter, files)).resumo()

    @api.post("/plugins/importar")
    async def import_foreign(request: Request, update: bool = False):
        """Converte (skills apenas) e instala como qualquer pacote: inativo e sem concessão."""
        files = await asyncio.to_thread(archive_files, await _zip(request), "plugin.zip")
        conversao = await asyncio.to_thread(converter, files)
        m = manager(request)
        async with m.lock:
            row = await asyncio.to_thread(
                m.installer.install_snapshot,
                conversao.package,
                origin=f"import:{conversao.formato}",
                update=update,
            )
        return {**public_plugin(row), "ignorado": conversao.ignorado}

    def catalogos(request: Request):
        c = manager(request).catalogos
        if c is None:
            raise HTTPException(503, "catalog_unavailable")
        return c

    @api.get("/plugins/catalogo")
    async def catalog_list(request: Request):
        """Plugins dos catálogos (local e cadastrados). Versão nova é só aviso: nada atualiza."""
        entradas, fontes = await asyncio.to_thread(catalogos(request).listar)
        instalados = {r["id"]: r["version"] for r in manager(request).store.list()}
        return {
            "fontes": fontes,
            "plugins": [
                {
                    **e.publico(),
                    "instalado": instalados.get(e.id),
                    "atualizacao": e.id in instalados and versao_maior(e.version, instalados[e.id]),
                    "local": e.fonte == LOCAL,
                }
                for e in entradas
            ],
        }

    @api.post("/plugins/catalogo/instalar")
    async def catalog_install(body: CatalogoInstalar, request: Request):
        """Baixa um pacote do catálogo, confere o SHA-256 do índice e instala INATIVO e sem
        concessão. Quem ativa é você, depois de revisar."""
        c = catalogos(request)
        entrada, dados = await asyncio.to_thread(c.baixar, body.fonte, body.id, body.version)
        package = await asyncio.to_thread(archive_snapshot, dados, f"{entrada.id}.zip")
        if package.manifest.id != entrada.id or package.manifest.version != entrada.version:
            raise PluginError("catalog_manifest_mismatch")
        m = manager(request)
        async with m.lock:
            ja = any(r["id"] == entrada.id for r in m.store.list())
            row = await asyncio.to_thread(
                m.installer.install_snapshot, package, origin="catalogo", update=ja
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
        validate_scope(request, body.scope)
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
        validate_scope(request, config.scope)
        m = manager(request)
        async with m.lock:
            if config.id in m.host.connections or len(m.host.connections) >= 32:
                raise PluginError("connection_id_conflict")
            m.save_connection(config)
            m.host.connections[config.id] = m.host.connection(config)
        return {"id": config.id, "state": "configured"}

    @api.put("/mcp/connections/{id_}")
    async def reconfigure(id_: str, body: dict, request: Request):
        try:
            config = TypeAdapter(ConnectionConfig).validate_python(body)
        except ValueError:
            raise HTTPException(422, "connection_config_invalid") from None
        validate_scope(request, config.scope)
        if config.id != id_:
            raise PluginError("connection_id_conflict")
        m = manager(request)
        async with m.lock:
            if any(id_ in owned for owned in m.connections.values()):
                raise PluginError("plugin_disable_first")
            old = m.host.connections.pop(id_, None)
            if old is None:
                raise PluginError("connection_not_found")
            await old.close()
            await m.catalog.refresh()
            m.save_connection(config)
            m.host.connections[id_] = m.host.connection(config)
        return {"id": id_, "state": "configured"}

    @api.post("/mcp/connections/{id_}/disable")
    async def disable_connection(id_: str, request: Request):
        m = manager(request)
        async with m.lock:
            if any(id_ in owned for owned in m.connections.values()):
                raise PluginError("plugin_disable_first")
            old = m.host.connections.get(id_)
            if old is None:
                raise PluginError("connection_not_found")
            await old.close()
            await m.catalog.refresh()
            config = old.config.model_copy(update={"enabled": False})
            m.save_connection(config)
            m.host.connections[id_] = m.host.connection(config)
        return {"id": id_, "state": "disabled"}

    @api.post("/mcp/connections/{id_}/test")
    async def test(id_: str, request: Request):
        m = manager(request)
        async with m.lock:
            connection = m.host.connections.get(id_)
            if connection is None:
                raise PluginError("connection_not_found")
            if connection.state == "failed":
                await connection.close()
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
            m.forget_connection(id_)
        return {"ok": True}

    return api
