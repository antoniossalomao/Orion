import asyncio

from fastapi.testclient import TestClient

from orion.app import create_app
from orion.config import Settings
from orion.jobs import JobRunner
from orion.memory.scope import data_scope
from orion.projects import Projects
from tests.projects.test_projects import AUTH, TOKEN


def test_activity_scope_once_read_preferences_restart(tmp_path):
    settings = Settings(data_dir=tmp_path, admin_token=TOKEN, jobs_enabled=False, _env_file=None)
    for restart in (False, True):
        with TestClient(create_app(settings), base_url="http://127.0.0.1") as c:
            s = c.app.state.orion
            if not restart:
                a, b = [Projects(s.memory).create(n)["id"] for n in ("A", "B")]
                with data_scope(a):
                    s.ops.add_reminder("AMBER", 1)
                    s.ops.add_schedule("aviso sem executar", when=1, tool="escrever_arquivo")
                with data_scope(b):
                    s.ops.add_reminder("BLUE", 1)
                runner = JobRunner(s.memory, s.ops)
                assert asyncio.run(runner.tick()).lembretes == 2
                assert asyncio.run(runner.tick()).lembretes == 0
                assert c.get("/activity").status_code == 401
                assert not c.get("/activity", headers=AUTH).json()["notifications"]
                rows = c.get("/notifications", params={"project_id": a}, headers=AUTH).json()
                assert len(rows) == 2 and "BLUE" not in str(rows)
                nid = rows[0]["id"]
                assert (
                    c.post(
                        f"/notifications/{nid}/ack", params={"project_id": b}, headers=AUTH
                    ).status_code
                    == 404
                )
                assert (
                    c.post(
                        f"/notifications/{nid}/ack", params={"project_id": a}, headers=AUTH
                    ).status_code
                    == 200
                )
                assert (
                    len(
                        c.get(
                            "/activity", params={"project_id": a, "unread": True}, headers=AUTH
                        ).json()["notifications"]
                    )
                    == 1
                )
                prefs = {"completion": False, "question": True, "approval": False}
                assert (
                    c.put(
                        "/activity/preferences", params={"project_id": a}, headers=AUTH, json=prefs
                    ).json()
                    == prefs
                )
                with data_scope(a):
                    assert s.ops.list_reminders()[0]["title"] == "AMBER"
                    other = s.memory.query("SELECT id FROM reminders WHERE project_id=?", (b,))[0][
                        0
                    ]
                    assert not s.ops.remove_reminder(other)
            else:
                data = c.get("/activity", params={"project_id": a}, headers=AUTH).json()
                assert sum(bool(n["delivered_at"]) for n in data["notifications"]) == 1
                assert data["preferences"] == prefs
                assert c.get("/activity", params={"project_id": b}, headers=AUTH).json()[
                    "preferences"
                ]["completion"]
