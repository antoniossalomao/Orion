"""Concessões vinculadas ao bundle e ao escopo, sem confiar no manifesto remoto."""

from __future__ import annotations

import json
import secrets

from .plugins import PluginError, PluginStore


class Grants:
    def __init__(self, store: PluginStore):
        self.store = store
        with store._lock, store._db:
            store._db.execute("""CREATE TABLE IF NOT EXISTS plugin_grants (
                plugin_id TEXT NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
                digest TEXT NOT NULL, scope TEXT NOT NULL, capabilities TEXT NOT NULL,
                generation TEXT NOT NULL, PRIMARY KEY(plugin_id,digest,scope))""")

    def review(
        self, id_: str, digest: str, capabilities: set[str], *, scope: str = "personal"
    ) -> str:
        manifest = self.store.version(id_, digest)["manifest"]
        if not scope or len(scope) > 128 or not capabilities <= set(manifest["capabilities"]):
            raise PluginError("plugin_grant_invalid")
        generation = secrets.token_hex(16)
        with self.store._lock, self.store._db:
            self.store._db.execute(
                "INSERT OR REPLACE INTO plugin_grants VALUES(?,?,?,?,?)",
                (id_, digest, scope, json.dumps(sorted(capabilities)), generation),
            )
        return generation

    def effective(
        self, id_: str, digest: str, *, scope: str = "personal", allowed: set[str] | None = None
    ) -> set[str]:
        manifest = self.store.version(id_, digest)["manifest"]
        with self.store._lock:
            row = self.store._db.execute(
                "SELECT capabilities FROM plugin_grants WHERE plugin_id=? AND digest=? AND scope=?",
                (id_, digest, scope),
            ).fetchone()
        granted = set(json.loads(row[0])) if row else set()
        effective = granted & set(manifest["capabilities"])
        return effective if allowed is None else effective & allowed

    def revoke(self, id_: str) -> None:
        with self.store._lock, self.store._db:
            self.store._db.execute("DELETE FROM plugin_grants WHERE plugin_id=?", (id_,))
