"""Projetos persistentes; arquivar nunca remove conversas ou fontes."""

from __future__ import annotations

import uuid
from pathlib import Path

from .memory import MemoryStore
from .memory.store import Session


class ProjectError(ValueError):
    pass


class Projects:
    def __init__(self, memory: MemoryStore):
        self.memory = memory

    def get(self, id_: str) -> dict:
        rows = self.memory.query("SELECT * FROM projects WHERE id=?", (id_,))
        if not rows:
            raise ProjectError("project_not_found")
        return {
            **dict(rows[0]),
            "archived": bool(rows[0]["archived"]),
            "share_personal": bool(rows[0]["share_personal"]),
        }

    def list(self, *, archived: bool = False) -> list[dict]:
        return [
            self.get(row["id"])
            for row in self.memory.query(
                "SELECT id FROM projects WHERE archived=? ORDER BY updated_at DESC,id",
                (int(archived),),
            )
        ]

    @staticmethod
    def validate_root(root: str | None) -> str | None:
        if root is None:
            return None
        path = Path(root)
        if not path.is_absolute() or not path.is_dir():
            raise ProjectError("project_root_invalid")
        return str(path.resolve())

    def create(
        self,
        name: str,
        instructions: str = "",
        *,
        share_personal: bool = False,
        root: str | None = None,
    ) -> dict:
        id_, now = uuid.uuid4().hex, self.memory.clock()
        with self.memory.transaction() as db:
            db.execute(
                "INSERT INTO projects(id,name,instructions,share_personal,archived,created_at,"
                "updated_at,root) VALUES(?,?,?,?,?,?,?,?)",
                (
                    id_,
                    name.strip(),
                    instructions,
                    int(share_personal),
                    0,
                    now,
                    now,
                    self.validate_root(root),
                ),
            )
        return self.get(id_)

    def update(
        self,
        id_: str,
        *,
        name: str | None = None,
        instructions: str | None = None,
        share_personal: bool | None = None,
        archived: bool | None = None,
        root: str | None = None,
        root_set: bool = False,
    ) -> dict:
        with self.memory.transaction() as db:
            old = self.get(id_)
            db.execute(
                "UPDATE projects SET name=?,instructions=?,share_personal=?,archived=?,"
                "updated_at=?,root=? WHERE id=?",
                (
                    name.strip() if name is not None else old["name"],
                    instructions if instructions is not None else old["instructions"],
                    int(share_personal)
                    if share_personal is not None
                    else int(old["share_personal"]),
                    int(archived) if archived is not None else int(old["archived"]),
                    self.memory.clock(),
                    self.validate_root(root) if root_set else old["root"],
                    id_,
                ),
            )
        return self.get(id_)

    def associate(self, channel: str, session_id: str, project_id: str | None) -> Session:
        with self.memory.transaction() as db:
            session = self.memory.get_session(session_id)
            if session is None or session.channel != channel:
                raise ProjectError("session_not_found")
            if project_id is not None and self.get(project_id)["archived"]:
                raise ProjectError("project_archived")
            db.execute("UPDATE sessions SET project_id=? WHERE id=?", (project_id, session_id))
        result = self.memory.get_session(session_id)
        assert result is not None
        return result

    def activate(self, channel: str, project_id: str | None) -> Session:
        with self.memory.transaction():
            if project_id is not None and self.get(project_id)["archived"]:
                raise ProjectError("project_archived")
            candidates = self.memory.query(
                """SELECT s.id FROM sessions s WHERE s.channel=?
                AND s.project_id IS ? AND s.archived=0
                AND NOT EXISTS(SELECT 1 FROM imported WHERE kind='sessao' AND ref=s.id)
                ORDER BY s.last_active_at DESC,s.created_at DESC,s.id DESC LIMIT 1""",
                (channel, project_id),
            )
            if candidates:
                return self.memory.activate_session(channel, candidates[0]["id"])
            return self.memory.new_session(channel, project_id=project_id)
