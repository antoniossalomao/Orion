"""Resultados versionados em SQLite: IDs opacos, escopo e conteúdo sem caminhos arbitrários."""

from __future__ import annotations

import base64
import hashlib
import io
import json
import uuid
from typing import Literal
from urllib.parse import quote

from fastapi import APIRouter, Depends, HTTPException, Request, Response
from PIL import Image, UnidentifiedImageError
from pydantic import BaseModel, ConfigDict, Field, ValidationError, field_validator

from .memory import MemoryStore

MAX_CONTENT = 4 * 1024 * 1024
MAX_REQUEST = 6 * 1024 * 1024
MAX_TOTAL = 256 * 1024 * 1024
KINDS = {
    "text": ("text/plain; charset=utf-8", ".txt"),
    "markdown": ("text/markdown; charset=utf-8", ".md"),
    "code": ("text/plain; charset=utf-8", ".txt"),
    "image": ("image/png", ".png"),
}


class ArtifactError(ValueError):
    pass


class Payload(BaseModel):
    model_config = ConfigDict(extra="forbid")
    title: str = Field(min_length=1, max_length=160)
    kind: Literal["text", "markdown", "code", "image"] = "markdown"
    content: str = Field(min_length=1, max_length=MAX_REQUEST)
    session_id: str = Field(pattern=r"^[a-f0-9]{32}$")
    message_id: int | None = Field(default=None, ge=1)
    language: str = Field(default="", max_length=48, pattern=r"^[a-zA-Z0-9_+.-]*$")
    expected_version: int | None = Field(default=None, ge=1)

    @field_validator("title")
    @classmethod
    def safe_title(cls, value: str) -> str:
        if not value.strip() or any(ord(c) < 32 or c in "/\\" for c in value):
            raise ValueError("artifact_title_invalid")
        return value.strip()


def encode(payload: Payload) -> bytes:
    if payload.kind != "image":
        data = payload.content.encode()
    else:
        try:
            raw = base64.b64decode(payload.content, validate=True)
            if len(raw) > MAX_CONTENT:
                raise ArtifactError("artifact_size_limit")
            with Image.open(io.BytesIO(raw)) as image:
                if (
                    image.format not in {"PNG", "JPEG", "WEBP"}
                    or image.width > 8192
                    or image.height > 8192
                    or image.width * image.height > 16_000_000
                ):
                    raise ArtifactError("artifact_image_invalid")
                image.load()
                target = io.BytesIO()
                image.convert("RGBA").save(target, format="PNG")
                data = target.getvalue()
        except (ValueError, OSError, UnidentifiedImageError, Image.DecompressionBombError) as error:
            raise ArtifactError("artifact_image_invalid") from error
    if not data or len(data) > MAX_CONTENT:
        raise ArtifactError("artifact_size_limit")
    return data


class Artifacts:
    def __init__(self, memory: MemoryStore):
        self.memory = memory

    def get(self, id_: str, project_id: str | None = None) -> dict:
        rows = self.memory.query(
            "SELECT * FROM artifacts WHERE id=? AND project_id IS ?", (id_, project_id)
        )
        if not rows:
            raise ArtifactError("artifact_not_found")
        return dict(rows[0])

    def list(self, project_id: str | None = None, query: str = "") -> list[dict]:
        return [
            dict(row)
            for row in self.memory.query(
                "SELECT a.*, (SELECT max(version) FROM artifact_versions WHERE artifact_id=a.id) "
                "AS version FROM artifacts a WHERE project_id IS ? "
                "AND instr(lower(title),lower(?))>0 "
                "ORDER BY updated_at DESC,id LIMIT 200",
                (project_id, query),
            )
        ]

    def save(
        self, payload: Payload, *, id_: str | None = None, project_id: str | None = None
    ) -> dict:
        data = encode(payload)
        session = self.memory.get_session(payload.session_id)
        if session is None or session.project_id != project_id:
            raise ArtifactError("artifact_session_out_of_scope")
        if session.read_only or session.archived:
            raise ArtifactError("artifact_session_read_only")
        if (
            project_id
            and self.memory.query("SELECT archived FROM projects WHERE id=?", (project_id,))[0][0]
        ):
            raise ArtifactError("project_archived")
        provenance = None
        if payload.message_id:
            rows = self.memory.query(
                "SELECT provenance FROM messages WHERE id=? AND session_id=?",
                (payload.message_id, session.id),
            )
            if not rows:
                raise ArtifactError("artifact_message_out_of_scope")
            provenance = rows[0][0]
        now, artifact_id = self.memory.clock(), id_ or uuid.uuid4().hex
        with self.memory.transaction() as db:
            if id_:
                old = self.get(id_, project_id)
                if old["session_id"] != session.id or old["kind"] != payload.kind:
                    raise ArtifactError("artifact_version_out_of_scope")
                previous = db.execute(
                    "SELECT max(version) FROM artifact_versions WHERE artifact_id=?", (id_,)
                ).fetchone()[0]
                if payload.expected_version != previous:
                    raise ArtifactError("artifact_version_conflict")
                version = previous + 1
                if version > 50:
                    raise ArtifactError("artifact_version_limit")
                db.execute(
                    "UPDATE artifacts SET title=?,updated_at=? WHERE id=?",
                    (payload.title, now, id_),
                )
            else:
                if db.execute("SELECT count(*) FROM artifacts").fetchone()[0] >= 1000:
                    raise ArtifactError("artifact_count_limit")
                version = 1
                db.execute(
                    "INSERT INTO artifacts VALUES(?,?,?,?,?,?,?,?)",
                    (
                        artifact_id,
                        project_id,
                        session.id,
                        payload.title,
                        payload.kind,
                        payload.language,
                        now,
                        now,
                    ),
                )
            total = db.execute(
                "SELECT coalesce(sum(length(content)),0) FROM artifact_versions"
            ).fetchone()[0]
            if total + len(data) > MAX_TOTAL:
                raise ArtifactError("artifact_storage_limit")
            digest = hashlib.sha256(data).hexdigest()
            db.execute(
                "INSERT INTO artifact_versions VALUES(?,?,?,?,?,?,?)",
                (artifact_id, version, data, digest, payload.message_id, provenance, now),
            )
        return {**self.get(artifact_id, project_id), "version": version, "digest": digest}

    def versions(self, id_: str, project_id: str | None) -> list[dict]:
        self.get(id_, project_id)
        return [
            {
                **dict(row),
                "provenance": json.loads(row["provenance"]) if row["provenance"] else None,
            }
            for row in self.memory.query(
                "SELECT version,digest,message_id,provenance,created_at,length(content) AS bytes "
                "FROM artifact_versions WHERE artifact_id=? ORDER BY version DESC",
                (id_,),
            )
        ]

    def read(
        self, id_: str, project_id: str | None, version: int | None = None
    ) -> tuple[dict, dict]:
        row = self.get(id_, project_id)
        versions = self.memory.query(
            "SELECT * FROM artifact_versions WHERE artifact_id=? "
            "AND (? IS NULL OR version=?) ORDER BY version DESC LIMIT 1",
            (id_, version, version),
        )
        if not versions:
            raise ArtifactError("artifact_version_not_found")
        item = dict(versions[0])
        if hashlib.sha256(item["content"]).hexdigest() != item["digest"]:
            raise ArtifactError("artifact_integrity_failed")
        return row, item


async def body(request: Request) -> Payload:
    raw = bytearray()
    async for chunk in request.stream():
        raw.extend(chunk)
        if len(raw) > MAX_REQUEST:
            raise HTTPException(413, "artifact_size_limit")
    try:
        return Payload.model_validate_json(bytes(raw))
    except ValidationError:
        raise HTTPException(422, "artifact_request_invalid") from None


def router(require_admin) -> APIRouter:
    api = APIRouter(prefix="/artifacts", dependencies=[Depends(require_admin)])

    def store(request: Request) -> Artifacts:
        return Artifacts(request.app.state.orion.memory)

    @api.get("")
    def list_results(request: Request, project_id: str | None = None, query: str = ""):
        return store(request).list(project_id, query[:200])

    @api.post("")
    async def create(request: Request, project_id: str | None = None):
        return store(request).save(await body(request), project_id=project_id)

    @api.post("/{id_}/versions")
    async def append(id_: str, request: Request, project_id: str | None = None):
        return store(request).save(await body(request), id_=id_, project_id=project_id)

    @api.get("/{id_}/versions")
    def versions(id_: str, request: Request, project_id: str | None = None):
        return store(request).versions(id_, project_id)

    @api.get("/{id_}")
    def detail(
        id_: str, request: Request, project_id: str | None = None, version: int | None = None
    ):
        row, item = store(request).read(id_, project_id, version)
        return {
            **row,
            "version": item["version"],
            "digest": item["digest"],
            "message_id": item["message_id"],
            "provenance": json.loads(item["provenance"]) if item["provenance"] else None,
            "content": None if row["kind"] == "image" else item["content"].decode(),
        }

    @api.get("/{id_}/download")
    def download(
        id_: str, request: Request, project_id: str | None = None, version: int | None = None
    ):
        row, item = store(request).read(id_, project_id, version)
        mime, extension = KINDS[row["kind"]]
        return Response(
            item["content"],
            media_type=mime,
            headers={
                "Content-Disposition": "attachment; filename*=UTF-8''"
                + quote(row["title"] + extension),
                "X-Content-Type-Options": "nosniff",
                "Cache-Control": "no-store",
                "Content-Security-Policy": "default-src 'none'; sandbox",
            },
        )

    return api
