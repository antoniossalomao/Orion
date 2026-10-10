import os
import sys
from pathlib import Path

import pytest
from pydantic import ValidationError

from orion.extensions.host import MCPHost, StdioConfig

SERVER = str(Path(__file__).with_name("mcp_server.py"))


def config(**kwargs):
    return StdioConfig(id="ensaio", command=sys.executable, args=[SERVER], **kwargs)


async def test_config_disabled_never_spawns_and_requires_trust():
    host = MCPHost([config()])
    await host.start()
    assert host.statuses()[0]["state"] == "disabled"
    assert host.connections["ensaio"]._task is None
    await host.close()
    with pytest.raises(ValidationError):
        config(enabled=True)
    with pytest.raises(ValidationError):
        StdioConfig(id="bad", command="python")
    with pytest.raises(ValueError):
        MCPHost([config(), config()])


async def test_connection_lifecycle_owned_by_worker_and_minimal_environment(monkeypatch):
    monkeypatch.setenv("ORION_TEST_SECRET", "segredo-controlado-nao-herdar")
    host = MCPHost([config(enabled=True, trusted=True)])
    await host.start()
    connection = host.connections["ensaio"]
    assert connection.status()["state"] == "connected"
    assert (await connection.list_tools())[0].name == "eco"
    result = await connection.call("eco", {"texto": "lifecycle"})
    pid = result.structured_content["pid"]
    result = await connection.call("ambiente", {})
    assert result.structured_content["secret"] is None
    # shutdown from a different task still closes AnyIO scopes in the owner.
    await host.close()
    if os.name == "posix":
        with pytest.raises(ProcessLookupError):
            os.kill(pid, 0)


async def test_failure_has_stable_sanitized_state(tmp_path):
    command = str(tmp_path / "segredo-nao-expor")
    host = MCPHost([StdioConfig(id="bad", command=command, enabled=True, trusted=True)])
    await host.start()
    status = host.statuses()[0]
    assert status["state"] == "failed" and status["error"] == "start_failed"
    assert "segredo" not in str(status)
    await host.close()


def test_app_lifespan_closes_server(tmp_path):
    from fastapi.testclient import TestClient

    from orion.app import create_app
    from orion.config import Settings

    settings = Settings(
        data_dir=tmp_path,
        jobs_enabled=False,
        _env_file=None,
        mcp_connections=[config(enabled=True, trusted=True)],
    )
    with TestClient(create_app(settings), base_url="http://127.0.0.1") as app:
        assert app.get("/health").status_code == 200
        host = app.app.state.orion.mcp_host
        result = app.portal.call(host.connections["ensaio"].call, "eco", {"texto": "app"})
        pid = result.structured_content["pid"]
    if os.name == "posix":
        with pytest.raises(ProcessLookupError):
            os.kill(pid, 0)
