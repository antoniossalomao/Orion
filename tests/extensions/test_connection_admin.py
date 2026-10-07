import json
import sys
from pathlib import Path

from fastapi.testclient import TestClient

from orion.app import create_app
from orion.config import Settings

TOKEN = "fixture-mcp-admin-token"
AUTH = {"Authorization": f"Bearer {TOKEN}"}


def test_discovery_no_write_persistence_disable_and_reconfigure(tmp_path):
    settings = Settings(data_dir=tmp_path, admin_token=TOKEN, jobs_enabled=False, _env_file=None)
    config = {
        "id": "fixture",
        "transport": "stdio",
        "command": sys.executable,
        "args": [str(Path("tests/extensions/mcp_server.py").resolve())],
        "enabled": True,
        "trusted": True,
        "classifications": {"estado_fixture": "read", "alterar_fixture": "destructive"},
    }
    with TestClient(create_app(settings), base_url="http://127.0.0.1") as client:
        assert client.post("/mcp/connections", json=config, headers=AUTH).status_code == 200
        result = client.post("/mcp/connections/fixture/test", headers=AUTH).json()
        assert result["state"] == "connected" and result["tools"]
        m = client.app.state.orion.extensions
        entry = next(e for e in m.catalog.entries.values() if e.remote_name == "estado_fixture")
        captured = m.catalog.registry.get(entry.name)
        value = json.loads(client.portal.call(captured.run_async, {}))
        assert value["structured"]["changes"] == 0
        assert (
            client.post("/mcp/connections/fixture/disable", headers=AUTH).json()["state"]
            == "disabled"
        )
        assert not m.catalog.entries
        assert (
            client.post("/mcp/connections/fixture/test", headers=AUTH).json()["state"] == "disabled"
        )
        assert client.put("/mcp/connections/fixture", json=config, headers=AUTH).status_code == 200
        assert (
            client.post("/mcp/connections/fixture/test", headers=AUTH).json()["state"]
            == "connected"
        )
        assert entry.name not in m.catalog.entries
    with TestClient(create_app(settings), base_url="http://127.0.0.1") as client:
        rows = client.get("/mcp/connections", headers=AUTH).json()
        assert rows[0]["state"] == "disabled"
        assert "command" not in rows[0] and "args" not in rows[0]
        assert client.delete("/mcp/connections/fixture", headers=AUTH).json() == {"ok": True}
    with TestClient(create_app(settings), base_url="http://127.0.0.1") as client:
        assert not client.get("/mcp/connections", headers=AUTH).json()
