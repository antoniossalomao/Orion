import json

from fastapi.testclient import TestClient

from orion.app import create_app
from orion.config import Settings

TOKEN = "fixture-plugin-admin-token"
AUTH = {"Authorization": f"Bearer {TOKEN}"}


def test_plugin_admin_install_activate_revoke_update_and_restart(bundle, tmp_path):
    settings = Settings(
        data_dir=tmp_path / "data", admin_token=TOKEN, jobs_enabled=False, _env_file=None
    )
    app = create_app(settings)
    with TestClient(app, base_url="http://127.0.0.1") as client:
        for route in ["/plugins", "/mcp/connections"]:
            assert client.get(route).status_code == 401
            assert client.get(route, headers={"Authorization": "Bearer wrong"}).status_code == 401
        assert client.post("/plugins/install", json={"folder": str(bundle)}).status_code == 401
        row = client.post("/plugins/install", json={"folder": str(bundle)}, headers=AUTH).json()
        assert row["state"] == "disabled"
        assert "bundle" not in row and str(tmp_path) not in json.dumps(row)
        digest = row["selected_digest"]
        assert not list((tmp_path / "data").rglob("*.ran"))
        review = {"digest": digest, "capabilities": []}
        assert (
            client.post(
                "/plugins/pesquisa/activate",
                json={"digest": digest, "capabilities": ["scripts"]},
                headers=AUTH,
            ).status_code
            == 422
        )
        assert (
            client.post("/plugins/pesquisa/activate", json=review, headers=AUTH).json()["state"]
            == "active"
        )
        skills = client.get("/skills", headers=AUTH).json()
        assert skills[0]["id"] == "pesquisa:revisar" and skills[0]["enabled"]
        runtime = app.state.orion.skills
        selection = runtime.select("revisar", ["pesquisa:revisar"])
        assert selection.allowed_tools == frozenset()
        assert selection.authorized()
        manifest = json.loads((bundle / "manifest.json").read_text())
        manifest["version"] = "2.0.0"
        (bundle / "manifest.json").write_text(json.dumps(manifest))
        client.post("/plugins/install", json={"folder": str(bundle), "update": True}, headers=AUTH)
        versions = client.get("/plugins/pesquisa/versions", headers=AUTH).json()
        newer = versions[0]["digest"]
        result = client.post(
            "/plugins/pesquisa/version", json={"digest": newer, "capabilities": []}, headers=AUTH
        )
        assert result.status_code == 200 and result.json()["selected_digest"] == newer
        assert not selection.authorized()
        assert result.json()["previous_digest"] == digest
        assert (
            client.post("/plugins/pesquisa/version", json=review, headers=AUTH).status_code == 200
        )
    with TestClient(create_app(settings), base_url="http://127.0.0.1") as client:
        assert client.get("/plugins", headers=AUTH).json()[0]["state"] == "disabled"
        assert client.delete("/plugins/pesquisa", headers=AUTH).json() == {"ok": True}
        assert not client.get("/plugins", headers=AUTH).json()


def test_import_limits_and_connection_requires_explicit_trust(tmp_path):
    app = create_app(
        Settings(data_dir=tmp_path, admin_token=TOKEN, jobs_enabled=False, _env_file=None)
    )
    with TestClient(app, base_url="http://127.0.0.1") as client:
        assert client.post("/plugins/import", content=b"bad", headers=AUTH).status_code == 415
        assert (
            client.post(
                "/plugins/import",
                content=b"bad",
                headers={**AUTH, "Content-Type": "application/zip"},
            ).status_code
            == 422
        )
        response = client.post(
            "/mcp/connections",
            json={"id": "local", "transport": "stdio", "command": "/tmp/fake", "enabled": True},
            headers=AUTH,
        )
        assert (
            response.status_code == 422 and response.json()["detail"] == "connection_config_invalid"
        )
        assert client.get("/capabilities").json()["features"]["plugins"]


def test_packaged_mcp_requires_local_review_and_discovery_never_writes(bundle, tmp_path):
    from pathlib import Path

    manifest = json.loads((bundle / "manifest.json").read_text())
    manifest["mcp"] = [
        {
            "id": "fixture",
            "transport": "stdio",
            "entrypoint": "server.py",
            "tools": {"estado_fixture": "read", "alterar_fixture": "destructive"},
        }
    ]
    manifest["capabilities"] = ["mcp:fixture:estado_fixture", "mcp:fixture:alterar_fixture"]
    (bundle / "manifest.json").write_text(json.dumps(manifest))
    (bundle / "server.py").write_bytes(Path("tests/extensions/mcp_server.py").read_bytes())
    app = create_app(
        Settings(data_dir=tmp_path / "data", admin_token=TOKEN, jobs_enabled=False, _env_file=None)
    )
    with TestClient(app, base_url="http://127.0.0.1") as client:
        row = client.post("/plugins/install", json={"folder": str(bundle)}, headers=AUTH).json()
        review = {"digest": row["selected_digest"], "capabilities": manifest["capabilities"]}
        assert (
            client.post("/plugins/pesquisa/activate", json=review, headers=AUTH).json()["state"]
            == "waiting_connection"
        )
        review.update(
            trusted_local=True,
            classifications={
                "fixture": {"estado_fixture": "read", "alterar_fixture": "destructive"}
            },
        )
        result = client.post("/plugins/pesquisa/activate", json=review, headers=AUTH)
        assert result.status_code == 200 and result.json()["state"] == "active"
        m = app.state.orion.extensions
        entries = m.catalog.entries
        read = next(entry for entry in entries.values() if entry.remote_name == "estado_fixture")
        result = client.portal.call(m.catalog.registry.get(read.name).run_async, {})
        assert json.loads(result)["structured"]["changes"] == 0
        cid = read.connection
        diagnostics = client.get("/plugins/pesquisa/diagnostics", headers=AUTH).text
        assert str(tmp_path) not in diagnostics and TOKEN not in diagnostics
        assert (
            client.post("/plugins/pesquisa/deactivate", headers=AUTH).json()["state"] == "disabled"
        )
        assert cid not in m.host.connections and not m.catalog.entries
