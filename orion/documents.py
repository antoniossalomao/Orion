"""Ingestão limitada e recuperável: originais no banco, texto isolado por projeto."""

from __future__ import annotations

import asyncio
import json
import os
import subprocess
import sys
import tempfile
import threading
import uuid
from pathlib import Path
from urllib.parse import quote

from fastapi import APIRouter, Depends, HTTPException, Request, Response

from .memory.scope import data_scope

MAX_UPLOAD = 6 * 1024 * 1024
MAX_TOTAL = 256 * 1024 * 1024
_WORKERS = threading.BoundedSemaphore(2)


def process(memory, id_: str, project_id: str | None):
    with _WORKERS:
        rows = memory.query(
            "SELECT * FROM uploads WHERE id=? AND project_id IS ?", (id_, project_id)
        )
        if not rows:
            raise HTTPException(404, "document_not_found")
        row = dict(rows[0])
        with memory.transaction() as c:
            c.execute("UPDATE uploads SET status='processing',error=NULL WHERE id=?", (id_,))
        result = {"error": "document_extraction_failed"}
        with tempfile.TemporaryDirectory(prefix="orion-document-") as temp:
            input_path = Path(temp) / "input"
            input_path.write_bytes(row["raw"])
            env = {
                k: os.environ[k] for k in ("SYSTEMROOT", "WINDIR", "TEMP", "TMP") if k in os.environ
            }
            try:
                completed = subprocess.run(  # noqa: S603 — argv fixo, sem shell
                    [
                        sys.executable,
                        "-I",
                        "-m",
                        "orion.document_extract",
                        str(input_path),
                        row["kind"],
                    ],
                    env=env,
                    capture_output=True,
                    timeout=12,
                    check=False,
                )
                if completed.returncode == 0 and len(completed.stdout) <= 14 * 1024 * 1024:
                    result = json.loads(completed.stdout)
            except (subprocess.TimeoutExpired, ValueError):
                result = {"error": "document_extraction_timeout"}
        text = result.get("text")
        if isinstance(text, str):
            with data_scope(project_id, include_personal=False):
                memory.index_document(f"upload:{id_}/{row['name']}", row["name"], text)
        with memory.transaction() as c:
            c.execute(
                "UPDATE uploads SET status=?,error=? WHERE id=?",
                ("ready" if isinstance(text, str) else "error", result.get("error"), id_),
            )


def router(require_admin):
    api = APIRouter(prefix="/documents", dependencies=[Depends(require_admin)])

    def validate(s, project_id):
        if project_id:
            rows = s.memory.query("SELECT archived FROM projects WHERE id=?", (project_id,))
            if not rows or rows[0][0]:
                raise HTTPException(409, "project_missing_or_archived")

    def rows(s, project_id):
        return [
            dict(r)
            for r in s.memory.query(
                "SELECT id,name,kind,status,error,created_at,length(raw) AS size FROM "
                "uploads WHERE project_id IS ? ORDER BY created_at DESC LIMIT 200",
                (project_id,),
            )
        ]

    @api.get("")
    def listing(request: Request, project_id: str | None = None):
        return rows(request.app.state.orion, project_id)

    @api.post("")
    async def upload(request: Request, name: str, project_id: str | None = None):
        s = request.app.state.orion
        validate(s, project_id)
        kind = Path(name).suffix.lower()
        if (
            not name.strip()
            or len(name) > 160
            or any(ord(ch) < 32 or ch in "/\\" for ch in name)
            or kind not in {".pdf", ".txt", ".md"}
        ):
            raise HTTPException(422, "document_type_or_name_invalid")
        raw = bytearray()
        async for part in request.stream():
            raw.extend(part)
            if len(raw) > MAX_UPLOAD:
                raise HTTPException(413, "document_size_limit")
        if not raw:
            raise HTTPException(422, "document_empty")
        id_ = uuid.uuid4().hex
        with s.memory.transaction() as c:
            total = c.execute("SELECT COALESCE(sum(length(raw)),0) FROM uploads").fetchone()[0]
            if total + len(raw) > MAX_TOTAL:
                raise HTTPException(413, "document_storage_limit")
            c.execute(
                "INSERT INTO uploads(id,project_id,name,kind,raw,status,created_at) "
                "VALUES (?,?,?,?,?,?,?)",
                (id_, project_id, name, kind, bytes(raw), "received", s.memory.clock()),
            )
        await asyncio.to_thread(process, s.memory, id_, project_id)
        return next(r for r in rows(s, project_id) if r["id"] == id_)

    @api.post("/{id_}/retry")
    async def retry(id_: str, request: Request, project_id: str | None = None):
        s = request.app.state.orion
        validate(s, project_id)
        await asyncio.to_thread(process, s.memory, id_, project_id)
        return next(r for r in rows(s, project_id) if r["id"] == id_)

    @api.get("/{id_}/download")
    def download(id_: str, request: Request, project_id: str | None = None):
        records = request.app.state.orion.memory.query(
            "SELECT raw,name FROM uploads WHERE id=? AND project_id IS ?", (id_, project_id)
        )
        if not records:
            raise HTTPException(404, "document_not_found")
        raw, name = records[0]
        return Response(
            raw,
            media_type="application/octet-stream",
            headers={
                "Content-Disposition": f"attachment; filename*=UTF-8''{quote(name)}",
                "X-Content-Type-Options": "nosniff",
                "Cache-Control": "no-store",
                "Content-Security-Policy": "default-src 'none'; sandbox",
            },
        )

    return api
