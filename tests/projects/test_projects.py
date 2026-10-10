import sqlite3

import pytest
from fastapi.testclient import TestClient

from orion.app import create_app
from orion.config import Settings
from orion.memory import MemoryStore
from orion.memory.schema import DDL_V1, DDL_V2, DDL_V3, DDL_V4, DDL_V5
from orion.policy import Context, ToolCall
from orion.projects import Projects

TOKEN = "fixture-project-admin-token"
AUTH = {"Authorization": f"Bearer {TOKEN}"}


def test_additive_v5_migration_association_archive_and_restart(tmp_path):
    path = tmp_path / "old.db"
    with sqlite3.connect(path) as db:
        db.executescript(DDL_V1 + DDL_V2 + DDL_V3 + DDL_V4 + DDL_V5)
        db.execute("INSERT INTO meta VALUES('schema_version','5')")
        db.execute(
            "INSERT INTO sessions(id,channel,title,created_at,last_active_at) VALUES(?,?,?,?,?)",
            ("a" * 32, "web", "Histórico antigo", 1, 2),
        )
        db.execute(
            "INSERT INTO messages(session_id,role,text,created_at) VALUES(?,?,?,?)",
            ("a" * 32, "user", "canário antigo", 3),
        )
    memory = MemoryStore(path)
    try:
        old = memory.get_session("a" * 32)
        assert old.project_id is None
        projects = Projects(memory)
        project = projects.create("Pesquisa", "Cite fontes verificáveis")
        linked = projects.associate("web", old.id, project["id"])
        assert linked.id == old.id and linked.project_id == project["id"]
        projects.update(project["id"], name="Pesquisa revisada", archived=True)
        assert memory.history(old.id)[0].text == "canário antigo"
        with pytest.raises(ValueError, match="project_unavailable"):
            memory.new_session("web", project_id=project["id"])
    finally:
        memory.close()
    memory = MemoryStore(path)
    try:
        assert Projects(memory).list(archived=True)[0]["name"] == "Pesquisa revisada"
        assert memory.get_session(old.id).project_id == project["id"]
        assert memory.history(old.id)[0].text == "canário antigo"
    finally:
        memory.close()


def test_project_admin_crud_activation_and_pending_approval_guard(tmp_path):
    app = create_app(
        Settings(data_dir=tmp_path, admin_token=TOKEN, jobs_enabled=False, _env_file=None)
    )
    with TestClient(app, base_url="http://127.0.0.1") as client:
        assert client.get("/projects").status_code == 401
        assert client.post("/projects", json={"name": "A"}).status_code == 401
        assert client.post("/projects", json={"name": "  "}, headers=AUTH).status_code == 422
        project = client.post(
            "/projects", json={"name": "A", "instructions": "instrução A"}, headers=AUTH
        ).json()
        assert project["share_personal"] is False
        active = client.post(
            "/projects/activate", json={"project_id": project["id"]}, headers=AUTH
        ).json()
        assert active["project_id"] == project["id"]
        session_id = active["session_id"]
        assert (
            client.get("/sessoes", headers=AUTH).json()["sessoes"][0]["project_id"] == project["id"]
        )
        app.state.orion.memory.add_message(session_id, "user", "histórico")
        decision = app.state.orion.policy.evaluate(
            ToolCall("esquecer_fato", {"id": 1}), Context(session_id)
        )
        assert client.post("/projects/activate", json={}, headers=AUTH).status_code == 409
        assert (
            client.patch(
                f"/projects/{project['id']}", json={"archived": True}, headers=AUTH
            ).status_code
            == 409
        )
        app.state.orion.policy.approvals.decide(
            decision.approval_id, False, channel="web", actor="fixture"
        )
        assert (
            client.patch(
                f"/projects/{project['id']}", json={"archived": True}, headers=AUTH
            ).status_code
            == 200
        )
        detail = client.get(f"/projects/{project['id']}", headers=AUTH).json()
        assert detail["sessions"][0]["id"] == session_id
        assert app.state.orion.memory.history(session_id)[0].text == "histórico"
        personal = client.post("/projects/activate", json={}, headers=AUTH).json()
        assert personal["project_id"] is None and personal["session_id"] != session_id
