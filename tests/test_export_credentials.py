import pytest
from fastapi.testclient import TestClient

from orion.app import create_app
from orion.config import Settings
from orion.export_credentials import ExportCredentials, Grant
from orion.memory import MemoryStore
from orion.projects import Projects
from tests.projects.test_projects import AUTH, TOKEN


def test_external_identity_scope_expiry_revoke_persist_hash_only(tmp_path):
    memory = MemoryStore(tmp_path / "m.db", clock=lambda: 100)
    project = Projects(memory).create("A")["id"]
    credentials = ExportCredentials(memory)
    issued = credentials.issue(Grant(name="Client A", project_id=project, permissions=["facts"]))
    assert credentials.verify(issued["token"], "facts")["project_id"] == project
    assert issued["token"] not in str(
        [dict(r) for r in memory.query("SELECT * FROM export_clients")]
    )
    assert "token_hash" not in str(credentials.listing())
    with pytest.raises(PermissionError, match="permission_denied"):
        credentials.verify(issued["token"], "artifacts")
    for invalid in ["", "orion_client_invalid", TOKEN]:
        with pytest.raises(PermissionError):
            credentials.verify(invalid)
    memory.close()
    memory = MemoryStore(tmp_path / "m.db", clock=lambda: 101)
    credentials = ExportCredentials(memory)
    assert credentials.verify(issued["token"], "facts")["project_id"] == project
    credentials.revoke(issued["id"])
    with pytest.raises(PermissionError):
        credentials.verify(issued["token"])
    expired = credentials.issue(Grant(name="Expires", permissions=["search"], lifetime_hours=1))
    memory.close()
    memory = MemoryStore(tmp_path / "m.db", clock=lambda: 4000)
    with pytest.raises(PermissionError):
        ExportCredentials(memory).verify(expired["token"])
    memory.close()


def test_external_token_cannot_administer_rest_or_issue_clients(tmp_path):
    app = create_app(
        Settings(data_dir=tmp_path, admin_token=TOKEN, jobs_enabled=False, _env_file=None),
        gateway_factory=lambda _: None,
    )
    with TestClient(app, base_url="http://127.0.0.1") as c:
        issued = c.post(
            "/mcp-export/clients",
            headers=AUTH,
            json={"name": "Read-only", "permissions": ["facts"]},
        )
        assert issued.status_code == 200
        auth = {"Authorization": "Bearer " + issued.json()["token"]}
        for endpoint in [
            "/mcp-export/clients",
            "/plugins",
            "/mcp/connections",
            "/sessoes",
            "/facts",
        ]:
            assert c.get(endpoint, headers=auth).status_code in {401, 403}
        assert c.post("/mcp-export/clients", headers=auth, json={}).status_code in {401, 403}
        assert c.get("/mcp-export/clients").status_code in {401, 403}
        assert (
            c.post(
                "/mcp-export/clients",
                headers=AUTH,
                json={"name": "Write", "permissions": ["execute"]},
            ).status_code
            == 422
        )
