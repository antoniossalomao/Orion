"""Manifesto Orion e versões persistentes. Declarar capability não concede autorização."""

from __future__ import annotations

import json
import re
import sqlite3
import threading
from enum import StrEnum
from pathlib import Path
from urllib.parse import urlsplit

from pydantic import BaseModel, ConfigDict, Field, ValidationError, field_validator, model_validator

from ..policy import Risk

_ID = r"^[a-z0-9][a-z0-9_-]{0,47}$"
_VERSION = r"^[0-9]+\.[0-9]+\.[0-9]+(?:-[a-z0-9.-]+)?$"


class PluginError(ValueError):
    pass


def relative_path(value: str) -> str:
    if not value or "\\" in value or ":" in value or value.startswith("/"):
        raise PluginError("manifest_path_invalid")
    if any(part in {"", ".", ".."} for part in value.split("/")):
        raise PluginError("manifest_path_invalid")
    return value


class Dependency(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True)
    id: str = Field(pattern=_ID)
    version: str = Field(pattern=_VERSION, max_length=80)


class PluginConnection(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True)
    id: str = Field(pattern=_ID)
    transport: str = Field(pattern=r"^(stdio|http)$")
    entrypoint: str | None = Field(default=None, max_length=1024)
    url: str | None = Field(default=None, max_length=4096)
    secret_ref: str | None = Field(default=None, pattern=r"^ORION_MCP_[A-Z0-9_]{1,80}$")
    tools: dict[str, Risk] = Field(default_factory=dict, max_length=128)
    resources: list[str] = Field(default_factory=list, max_length=128)
    prompts: list[str] = Field(default_factory=list, max_length=128)

    @model_validator(mode="after")
    def valid_transport(self) -> PluginConnection:
        if self.transport == "stdio":
            if self.url or not self.entrypoint or not self.entrypoint.endswith(".py"):
                raise PluginError("manifest_transport_invalid")
            relative_path(self.entrypoint)
        else:
            if self.entrypoint or not self.url:
                raise PluginError("manifest_transport_invalid")
            url = urlsplit(self.url)
            if (
                (
                    url.scheme != "https"
                    and not (
                        url.scheme == "http" and url.hostname in {"127.0.0.1", "localhost", "::1"}
                    )
                )
                or not url.hostname
                or url.username
                or url.password
                or url.query
                or url.fragment
            ):
                raise PluginError("manifest_endpoint_invalid")
        return self


class PluginManifest(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True)
    schema_version: int = Field(default=1, ge=1, le=1)
    orion_api: int = Field(default=1, ge=1, le=1)
    id: str = Field(pattern=_ID)
    version: str = Field(pattern=_VERSION, max_length=80)
    name: str = Field(min_length=1, max_length=80)
    description: str = Field(min_length=1, max_length=1024)
    license: str = Field(min_length=1, max_length=128)
    skills: list[str] = Field(default_factory=list, max_length=128)
    mcp: list[PluginConnection] = Field(default_factory=list, max_length=32)
    capabilities: list[str] = Field(default_factory=list, max_length=256)
    dependencies: list[Dependency] = Field(default_factory=list, max_length=16)

    @field_validator("skills")
    @classmethod
    def valid_skills(cls, paths: list[str]) -> list[str]:
        for path in paths:
            relative_path(path)
        if len(set(paths)) != len(paths):
            raise PluginError("manifest_duplicate_skill")
        return paths

    @field_validator("capabilities")
    @classmethod
    def valid_capabilities(cls, values: list[str]) -> list[str]:
        if len(set(values)) != len(values) or any(
            not re.fullmatch(
                r"(?:native:[a-z0-9_]{1,64}|mcp:[a-z0-9_-]{1,48}:[A-Za-z0-9_.-]{1,128}|resource:[a-z0-9_-]{1,48}|prompt:[a-z0-9_-]{1,48}|scripts)",
                v,
            )
            for v in values
        ):
            raise PluginError("manifest_capability_invalid")
        return values

    @model_validator(mode="after")
    def valid_namespaces(self) -> PluginManifest:
        if not self.skills and not self.mcp:
            raise PluginError("manifest_empty")
        if len({c.id for c in self.mcp}) != len(self.mcp):
            raise PluginError("manifest_connection_collision")
        if any(d.id == self.id for d in self.dependencies):
            raise PluginError("manifest_dependency_cycle")
        return self


def parse_manifest(data: bytes) -> PluginManifest:
    if len(data) > 64000:
        raise PluginError("manifest_too_large")

    def unique(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise PluginError("manifest_duplicate_key")
            result[key] = value
        return result

    try:
        value = json.loads(data, object_pairs_hook=unique)
        return PluginManifest.model_validate(value)
    except (UnicodeError, json.JSONDecodeError, ValidationError) as error:
        raise PluginError("manifest_invalid") from error


class PluginState(StrEnum):
    INSTALLED = "installed"
    DISABLED = "disabled"
    WAITING = "waiting_connection"
    ACTIVE = "active"
    ERROR = "error"


class PluginStore:
    def __init__(self, path: Path):
        path.parent.mkdir(parents=True, exist_ok=True)
        self._db = sqlite3.connect(path, check_same_thread=False)
        self._db.row_factory = sqlite3.Row
        self._db.execute("PRAGMA journal_mode=WAL")
        self._db.execute("PRAGMA foreign_keys=ON")
        self._lock = threading.RLock()
        self._db.executescript("""
            CREATE TABLE IF NOT EXISTS plugins (
                id TEXT PRIMARY KEY, state TEXT NOT NULL, selected_digest TEXT,
                active_digest TEXT, previous_digest TEXT, error TEXT);
            CREATE TABLE IF NOT EXISTS plugin_versions (
                plugin_id TEXT NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
                version TEXT NOT NULL, digest TEXT NOT NULL, origin TEXT NOT NULL,
                manifest TEXT NOT NULL, bundle TEXT,
                PRIMARY KEY(plugin_id,digest), UNIQUE(plugin_id,version));
        """)
        self._db.commit()

    def close(self) -> None:
        with self._lock:
            self._db.close()

    def record(
        self,
        manifest: PluginManifest,
        digest: str,
        origin: str,
        *,
        bundle: str | None = None,
        update: bool = False,
    ) -> None:
        if not re.fullmatch(r"[a-f0-9]{64}", digest) or len(origin) > 1024:
            raise PluginError("plugin_identity_invalid")
        with self._lock, self._db:
            existing = self._db.execute(
                "SELECT * FROM plugins WHERE id=?", (manifest.id,)
            ).fetchone()
            same = self._db.execute(
                "SELECT digest FROM plugin_versions WHERE plugin_id=? AND version=?",
                (manifest.id, manifest.version),
            ).fetchone()
            if same:
                if same["digest"] != digest:
                    raise PluginError("plugin_version_conflict")
                return
            if existing and not update:
                raise PluginError("plugin_id_conflict")
            if not existing:
                self._db.execute(
                    "INSERT INTO plugins(id,state,selected_digest) VALUES(?,?,?)",
                    (manifest.id, PluginState.DISABLED, digest),
                )
            self._db.execute(
                "INSERT INTO plugin_versions VALUES(?,?,?,?,?,?)",
                (manifest.id, manifest.version, digest, origin, manifest.model_dump_json(), bundle),
            )

    def list(self) -> list[dict]:
        with self._lock:
            rows = self._db.execute("""SELECT p.*,v.version,v.origin,v.manifest,v.bundle
                FROM plugins p JOIN plugin_versions v
                ON v.plugin_id=p.id AND v.digest=p.selected_digest ORDER BY p.id""").fetchall()
            return [{**dict(row), "manifest": json.loads(row["manifest"])} for row in rows]

    def get(self, id_: str) -> dict:
        for row in self.list():
            if row["id"] == id_:
                return row
        raise PluginError("plugin_not_found")

    def versions(self, id_: str) -> list[dict]:
        with self._lock:
            rows = self._db.execute(
                "SELECT * FROM plugin_versions WHERE plugin_id=? ORDER BY rowid DESC", (id_,)
            ).fetchall()
            return [{**dict(row), "manifest": json.loads(row["manifest"])} for row in rows]

    def version(self, id_: str, digest: str) -> dict:
        for row in self.versions(id_):
            if row["digest"] == digest:
                return row
        raise PluginError("plugin_version_not_found")

    def change_version(
        self, id_: str, digest: str, *, reviewed_digest: str, reviewed_capabilities: set[str]
    ) -> dict:
        with self._lock, self._db:
            current = self.get(id_)
            target = self.version(id_, digest)
            if current["state"] == PluginState.ACTIVE or current["active_digest"] is not None:
                raise PluginError("plugin_disable_first")
            if reviewed_digest != digest or reviewed_capabilities != set(
                target["manifest"]["capabilities"]
            ):
                raise PluginError("plugin_review_required")
            if current["selected_digest"] == digest:
                return current
            # Um único UPDATE/commit publica a revisão inteira; grants não atravessam update.
            self._db.execute(
                """UPDATE plugins SET previous_digest=selected_digest,
                selected_digest=?,active_digest=NULL,state=?,error=NULL WHERE id=?""",
                (digest, PluginState.DISABLED, id_),
            )
        return self.get(id_)

    def rollback(self, id_: str, *, reviewed_digest: str, reviewed_capabilities: set[str]) -> dict:
        previous = self.get(id_)["previous_digest"]
        if not previous:
            raise PluginError("plugin_rollback_unavailable")
        return self.change_version(
            id_,
            previous,
            reviewed_digest=reviewed_digest,
            reviewed_capabilities=reviewed_capabilities,
        )

    def remove(self, id_: str) -> list[dict]:
        with self._lock, self._db:
            row = self.get(id_)
            if row["state"] == PluginState.ACTIVE or row["active_digest"] is not None:
                raise PluginError("plugin_disable_first")
            versions = self.versions(id_)
            self._db.execute("DELETE FROM plugins WHERE id=?", (id_,))
            return versions

    def transition(self, id_: str, state: PluginState, *, error: str | None = None) -> None:
        if error and not re.fullmatch(r"[a-z0-9_]{1,80}", error):
            raise PluginError("plugin_error_code_invalid")
        with self._lock, self._db:
            if not self._db.execute("SELECT 1 FROM plugins WHERE id=?", (id_,)).fetchone():
                raise PluginError("plugin_not_found")
            self._db.execute(
                "UPDATE plugins SET state=?,error=?,active_digest=CASE WHEN ?='active' "
                "THEN selected_digest ELSE NULL END WHERE id=?",
                (state.value, error, state.value, id_),
            )
