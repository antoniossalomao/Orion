import subprocess
from types import SimpleNamespace

import pytest

from orion.development import Development
from orion.extensions.host import MCPError
from orion.memory import MemoryStore
from orion.memory.scope import data_scope
from orion.policy import Action, Context, PathGuard, PolicyEngine, ToolCall
from orion.projects import Projects
from orion.tools.registry import ToolRegistry


def test_git_read_preserves_index_refs_and_rejects_secret_diff(tmp_path):
    root = tmp_path / "repo"
    root.mkdir()

    def git(*args):
        return subprocess.run(["git", *args], cwd=root, check=True, capture_output=True)

    git("init")
    git("config", "user.name", "Fixture")
    git("config", "user.email", "fixture@example.invalid")
    (root / "app.py").write_text("print('before')\n")
    git("add", ".")
    git("commit", "-m", "fixture")
    (root / "app.py").write_text("print('after')\n")
    memory = MemoryStore(tmp_path / "m.db")
    policy = PolicyEngine(path_guard=PathGuard(safe_roots=(root,)))
    dev = Development(memory, ToolRegistry(), policy)
    snapshot = {p: p.read_bytes() for p in (root / ".git").rglob("*") if p.is_file()}
    assert "app.py" in dev.read(str(root), "status")["output"]
    assert "after" in dev.read(str(root), "diff")["output"]
    assert "fixture" in dev.read(str(root), "log")["output"]
    assert {p: p.read_bytes() for p in snapshot} == snapshot
    assert not (root / ".git/index.lock").exists()
    (root / ".env").write_text("PRIVATE_CANARY=not-a-key")
    git("add", ".env")
    with pytest.raises(MCPError, match="secret_or_link"):
        dev.read(str(root), "diff")
    a = Projects(memory).create("A", root=str(root))["id"]
    b = Projects(memory).create("B", root=str(tmp_path))["id"]
    with data_scope(a):
        assert dev.read(str(root), "status")["ok"]
    with data_scope(b), pytest.raises(MCPError, match="out_of_project"):
        dev.read(str(root), "log")
    memory.close()


def test_development_profile_review_does_not_invoke_cli(tmp_path):
    from fastapi.testclient import TestClient

    from orion.app import create_app
    from orion.config import Settings
    from tests.fakes import FakeGateway, chama, fala, pede
    from tests.projects.test_projects import AUTH, TOKEN

    root = tmp_path / "repo"
    root.mkdir()
    subprocess.run(["git", "init", str(root)], check=True, capture_output=True)
    (root / "app.py").write_text("print('fixture')")
    gateway = FakeGateway(
        pede(chama("consultar_git", root=str(root), operation="status")),
        fala("app.py ainda não rastreado; nenhum arquivo alterado."),
    )
    app = create_app(
        Settings(data_dir=tmp_path / "data", admin_token=TOKEN, jobs_enabled=False, _env_file=None),
        gateway_factory=lambda _: gateway,
    )
    with TestClient(app, base_url="http://127.0.0.1") as c:
        s = app.state.orion
        project = Projects(s.memory).create("Repo", root=str(root))["id"]
        s.memory.new_session("web", project_id=project)
        package = c.post("/plugins/builtin/orion-desenvolvimento", headers=AUTH).json()
        result = c.post(
            "/plugins/orion-desenvolvimento/activate",
            headers=AUTH,
            json={
                "digest": package["selected_digest"],
                "capabilities": package["capabilities"],
                "scope": "project:" + project,
            },
        )
        assert result.json()["state"] == "active"
        response = c.post(
            "/chat",
            headers=AUTH,
            json={"texto": "Revise", "skills": ["orion-desenvolvimento:revisar-alteracao"]},
        )
        assert response.status_code == 200 and "app.py" in str(gateway.chamadas)
        assert not s.memory.query("SELECT key FROM meta WHERE key LIKE 'counter:delegar:%'")
        assert (root / "app.py").read_text() == "print('fixture')"


def test_git_failure_and_cli_policy_remains_explicit(tmp_path, monkeypatch):
    memory = MemoryStore(tmp_path / "m.db")
    policy = PolicyEngine(path_guard=PathGuard(safe_roots=(tmp_path,)))
    dev = Development(memory, ToolRegistry(), policy)
    with pytest.raises(MCPError, match="command_failed"):
        dev.command(tmp_path, ["log"])

    class Hanging:
        returncode = None

        def poll(self):
            return self.returncode

        def kill(self):
            self.returncode = -9

        def wait(self):
            return self.returncode

    process = Hanging()
    monkeypatch.setattr(subprocess, "Popen", lambda *a, **kw: process)
    with pytest.raises(MCPError, match="timeout"):
        dev.command(tmp_path, ["log"], timeout=0)
    assert process.returncode == -9
    from orion.tools.builtin import default_registry

    registry = default_registry(memory, delegator=SimpleNamespace())
    assert registry.get("delegar")
    call = ToolCall("delegar", {"tarefa": "review", "pasta": str(tmp_path)})
    assert policy.evaluate(call, Context("fixture")).action == Action.CONFIRM
    memory.close()
