"""Lifecycle administrativo de pacotes. Instalar nunca executa o pacote."""

from __future__ import annotations

import asyncio
import hashlib
import json
import sys
from pathlib import Path

from pydantic import TypeAdapter

from ..policy import PolicyEngine, Risk
from .catalog import Catalog
from .grants import Grants
from .host import Connection, ConnectionConfig, HTTPConfig, MCPHost, StdioConfig
from .installer import Installer
from .plugins import PluginError, PluginManifest, PluginState, PluginStore
from .skill_runtime import SkillRuntime
from .skills import metadata


def public_plugin(row: dict) -> dict:
    manifest = row["manifest"]
    return {
        key: row[key]
        for key in ("id", "state", "selected_digest", "previous_digest", "version", "error")
    } | {
        "name": manifest["name"],
        "description": manifest["description"],
        "license": manifest["license"],
        "capabilities": manifest["capabilities"],
        "skills": manifest["skills"],
        "connections": [
            {"id": c["id"], "transport": c["transport"], "tools": c["tools"]}
            for c in manifest["mcp"]
        ],
        "origin": "local" if row["origin"] == "local" else "imported",
    }


class PluginManager:
    def __init__(
        self,
        root: Path,
        skills: SkillRuntime,
        host: MCPHost,
        catalog: Catalog,
        policy: PolicyEngine,
    ):
        self.store = PluginStore(root / "plugins.db")
        self.installer = Installer(root, self.store)
        self.grants = Grants(self.store)
        self.skills, self.host, self.catalog, self.policy = skills, host, catalog, policy
        self.connections: dict[str, set[str]] = {}
        self.owned_skills: dict[str, set[str]] = {}
        self.lock = asyncio.Lock()
        with self.store._lock, self.store._db:
            self.store._db.execute(
                "CREATE TABLE IF NOT EXISTS extension_connections "
                "(id TEXT PRIMARY KEY, config TEXT NOT NULL)"
            )

            rows = self.store._db.execute("SELECT config FROM extension_connections").fetchall()
            for record in rows:
                try:
                    value = json.loads(record[0])
                    value["enabled"] = False
                    config = TypeAdapter(ConnectionConfig).validate_python(value)
                    if config.id not in host.connections and len(host.connections) < 32:
                        host.connections[config.id] = Connection(config)
                except ValueError:
                    pass  # Configuração inválida não executa código no startup.
        # Reinício não autoriza executar código/conectar conta silenciosamente.
        for row in self.store.list():
            self.store.transition(row["id"], PluginState.DISABLED)

    def save_connection(self, config: StdioConfig | HTTPConfig) -> None:
        with self.store._lock, self.store._db:
            self.store._db.execute(
                "INSERT OR REPLACE INTO extension_connections VALUES(?,?)",
                (config.id, config.model_dump_json()),
            )

    def forget_connection(self, id_: str) -> None:
        with self.store._lock, self.store._db:
            self.store._db.execute("DELETE FROM extension_connections WHERE id=?", (id_,))

    def list(self) -> list[dict]:
        return [public_plugin(row) for row in self.store.list()]

    async def deactivate(self, id_: str, *, revoke: bool = True) -> dict:
        self.store.get(id_)
        self.store.transition(id_, PluginState.DISABLED)
        for skill_id in self.owned_skills.pop(id_, set()):
            authority = self.skills.authorities.pop(skill_id, None)
            if authority:
                self.policy.approvals.invalidate_binding(authority)
            self.skills.enabled.discard(skill_id)
            self.skills.scopes.pop(skill_id, None)
            self.skills.script_reviews.pop(skill_id, None)
            self.skills.index.skills.pop(skill_id, None)
        # Scripts compartilham runner; cancelamento conservador não deixa execução órfã.
        await self.skills.runner.close()
        for connection_id in self.connections.pop(id_, set()):
            connection = self.host.connections.pop(connection_id, None)
            if connection is not None:
                await connection.close()
        await self.catalog.refresh()
        if revoke:
            self.grants.revoke(id_)
        self.store.transition(id_, PluginState.DISABLED)
        return public_plugin(self.store.get(id_))

    async def activate(
        self,
        id_: str,
        digest: str,
        capabilities: set[str],
        *,
        trusted_local: bool = False,
        authorized_remote: bool = False,
        classifications: dict[str, dict[str, Risk]] | None = None,
        scope: str = "personal",
    ) -> dict:
        row = self.store.get(id_)
        if row["selected_digest"] != digest:
            raise PluginError("plugin_review_required")
        root = await asyncio.to_thread(self.installer.bundle, id_, digest)
        manifest = PluginManifest.model_validate(row["manifest"])
        for dependency in manifest.dependencies:
            installed = self.store.get(dependency.id)
            if (
                installed["state"] != PluginState.ACTIVE
                or installed["version"] != dependency.version
            ):
                raise PluginError("plugin_dependency_unavailable")
        await self.deactivate(id_)
        generation = self.grants.review(id_, digest, capabilities, scope=scope)
        effective = self.grants.effective(id_, digest, scope=scope)
        self.connections[id_] = set()
        try:
            for remote in manifest.mcp:
                chosen = {name for name in remote.tools if f"mcp:{remote.id}:{name}" in effective}
                if not chosen and not (
                    {f"resource:{remote.id}", f"prompt:{remote.id}"} & effective
                ):
                    continue
                reviews = (classifications or {}).get(remote.id, {})
                if (
                    not chosen <= reviews.keys()
                    or (remote.transport == "stdio" and not trusted_local)
                    or (remote.transport == "http" and not authorized_remote)
                ):
                    await self.deactivate(id_, revoke=False)
                    self.store.transition(id_, PluginState.WAITING)
                    return public_plugin(self.store.get(id_))
                cid = "p_" + hashlib.sha256(f"{id_}:{remote.id}".encode()).hexdigest()[:40]
                if cid in self.host.connections:
                    raise PluginError("plugin_connection_collision")
                common = dict(
                    id=cid,
                    enabled=True,
                    classifications={name: reviews[name] for name in chosen},
                    resources=remote.resources if f"resource:{remote.id}" in effective else [],
                    prompts=remote.prompts if f"prompt:{remote.id}" in effective else [],
                )
                if remote.transport == "stdio":
                    config = StdioConfig.model_validate(
                        dict(
                            **common,
                            command=sys.executable,
                            args=["-I", str(root / str(remote.entrypoint))],
                            cwd=root,
                            trusted=True,
                        )
                    )
                else:
                    config = HTTPConfig.model_validate(
                        dict(
                            **common,
                            url=str(remote.url),
                            secret_ref=remote.secret_ref,
                            authorized=True,
                        )
                    )
                connection = Connection(config)
                connection.authorized = lambda: (
                    self.store.get(id_)["state"] == PluginState.ACTIVE
                    and self.store.get(id_)["selected_digest"] == digest
                )
                self.host.connections[cid] = connection
                self.connections[id_].add(cid)
                await connection.start()
                if connection.state != "connected":
                    raise PluginError("plugin_connection_failed")
            await self.catalog.refresh()
            allowed = {
                c.removeprefix("native:")
                for c in effective
                if c.startswith("native:") and c.removeprefix("native:") in self.policy.tools
            }
            allowed.update(
                name
                for name, entry in self.catalog.entries.items()
                if entry.connection in self.connections[id_]
            )
            # Scripts de pacotes aguardam runner próprio por revisão; instalação não basta.
            if "scripts" in effective:
                raise PluginError("plugin_scripts_require_local_source")
            prepared = []
            for relative in manifest.skills:
                skill = await asyncio.to_thread(
                    metadata, root / relative, namespace=id_, origin=f"plugin:{id_}"
                )
                await asyncio.to_thread(skill.load)
                if skill.id in self.skills.index.skills:
                    raise PluginError("plugin_skill_collision")
                prepared.append(skill)
            owned = set()
            self.owned_skills[id_] = owned
            for skill in prepared:
                self.skills.index.add(skill)
                owned.add(skill.id)
                self.skills.enabled.add(skill.id)
                self.skills.scopes[skill.id] = frozenset(allowed)
                self.skills.authorities[skill.id] = f"{id_}:{digest}:{generation}:{scope}"
            self.store.transition(id_, PluginState.ACTIVE)
            return public_plugin(self.store.get(id_))
        except BaseException:
            await self.deactivate(id_)
            self.store.transition(id_, PluginState.ERROR, error="plugin_activation_failed")
            raise

    async def close(self) -> None:
        for id_ in list(self.connections.keys() | self.owned_skills.keys()):
            await self.deactivate(id_)
        self.store.close()
