"""Classificação MCP local: identidade inclui origem e revisão, nunca annotations remotas."""

from __future__ import annotations

import asyncio
import hashlib
import json
from dataclasses import dataclass

from ..policy import PolicyEngine, Risk, ToolSpec
from ..tools.registry import Tool, ToolRegistry
from .host import Connection, MCPError, MCPHost


@dataclass(frozen=True)
class Entry:
    name: str
    connection: str
    remote_name: str
    revision: str
    canonical_id: str


def identity(connection: Connection, remote) -> Entry:
    config = connection.config.model_dump(mode="json", exclude={"enabled", "trusted", "authorized"})
    payload = {
        "config": config,
        "tool": remote.model_dump(mode="json"),
        "generation": connection.generation,
    }
    revision = hashlib.sha256(json.dumps(payload, sort_keys=True).encode()).hexdigest()
    canonical = f"mcp:{connection.config.id}:{remote.name}:{revision}"
    # Nome ASCII estável, <=64; não depende de nome remoto bem-formado/sem colisão.
    short = "mcp_" + hashlib.sha256(canonical.encode()).hexdigest()[:32]
    return Entry(short, connection.config.id, remote.name, revision, canonical)


class Catalog:
    def __init__(self, host: MCPHost, registry: ToolRegistry, policy: PolicyEngine):
        self.host, self.registry, self.policy = host, registry, policy
        self.entries: dict[str, Entry] = {}
        self._lock = asyncio.Lock()

    async def reconnect(self, connection_id: str) -> None:
        connection = self.host.connections[connection_id]
        # Revogar antes do reconnect; nunca retomar chamadas pendentes de execução.
        await connection.close()
        await connection.start()
        await self.refresh()

    async def refresh(self) -> None:
        async with self._lock:
            await self._refresh()

    def _prepare(self, connection: Connection, remotes) -> list[tuple[Entry, Tool, ToolSpec]]:
        if len(remotes) > 2048 or len({t.name for t in remotes}) != len(remotes):
            raise MCPError("catalog_invalid")
        pending = []
        for remote in remotes:
            risk = connection.config.classifications.get(remote.name)
            if risk is None:
                continue
            if len(json.dumps(remote.input_schema).encode()) > 64000:
                raise MCPError("schema_too_large")
            entry = identity(connection, remote)

            def bind(conn: Connection, name: str, current: Entry):
                async def execute(**arguments):
                    if (conn.authorized is not None and not conn.authorized()) or self.entries.get(
                        current.name
                    ) != current:
                        raise MCPError("origin_revoked")
                    try:
                        result = await conn.call(name, arguments)
                    except MCPError as error:
                        return {
                            "ok": False,
                            "codigo": error.code,
                            "possibly_active": error.possibly_active,
                            "origin": conn.config.id,
                        }
                    return {
                        "ok": not result.is_error,
                        "origin": conn.config.id,
                        "structured": result.structured_content,
                        "content": [c.model_dump(mode="json") for c in result.content],
                    }

                return execute

            tool = Tool(
                entry.name,
                f"[{connection.config.id}/{remote.name}] " + (remote.description or "")[:1000],
                remote.input_schema,
                bind(connection, remote.name, entry),
                origin=entry.canonical_id,
                validar=True,
            )
            _ = tool.validator
            spec = ToolSpec(
                entry.name,
                risk,
                external=True,
                origin=entry.canonical_id,
                revision=entry.revision,
                require_confirmation=risk is not Risk.READ,
                display_name=remote.name,
                origin_label=connection.label,
                scope=connection.config.scope,
            )
            pending.append((entry, tool, spec))
        return pending

    async def _refresh(self) -> None:
        pending: list[tuple[Entry, Tool, ToolSpec]] = []
        for connection in self.host.connections.values():
            if connection.state != "connected":
                continue
            try:
                pending.extend(self._prepare(connection, await connection.list_tools()))
            except Exception:  # noqa: BLE001 — revogar conexão cujo catálogo não é confiável
                connection.error = "catalog_unavailable"
        for entry, _, _ in pending:
            if self.registry.get(entry.name) is not None and entry.name not in self.entries:
                raise MCPError("catalog_collision")
        removed = set(self.entries) - {e.name for e, _, _ in pending}
        self.policy.approvals.invalidate_tools(removed)
        for name in self.entries:
            self.registry.unregister(name)
            self.policy.tools.pop(name, None)
        self.entries.clear()
        for entry, tool, spec in pending:
            self.registry.register(tool)
            self.policy.tools[entry.name] = spec
            self.policy.rate.set_limit(entry.name, (20, 60))
            self.entries[entry.name] = entry
