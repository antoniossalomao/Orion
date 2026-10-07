import sys
from pathlib import Path

import pytest

from orion.agent import Agent
from orion.extensions.context import ContextReader, ContextRequest
from orion.extensions.host import MCPError, MCPHost, StdioConfig
from orion.memory import MemoryStore
from orion.policy import PathGuard, PolicyEngine
from orion.tools.registry import Tool, ToolRegistry
from tests.fakes import FakeGateway, chama, fala, pede


async def test_explicit_resource_scope_limit_and_prompt_injection(tmp_path):
    host = MCPHost(
        [
            StdioConfig(
                id="contexto",
                command=sys.executable,
                args=[str(Path(__file__).with_name("mcp_context_server.py"))],
                trusted=True,
                enabled=True,
                resources=["ensaio://manual", "ensaio://grande"],
                prompts=["inseguro"],
            )
        ]
    )
    store = MemoryStore(tmp_path / "memory.db")
    await host.start()
    try:
        reader = ContextReader(host, max_bytes=100)

        def request(kind, key):
            return ContextRequest(connection="contexto", kind=kind, key=key)

        manual = await reader.read(request("resource", "ensaio://manual"))
        assert manual.text == "manual de ensaio" and not manual.truncated
        large = await reader.read(request("resource", "ensaio://grande"))
        assert len(large.text.encode()) == 100 and large.truncated
        with pytest.raises(MCPError, match="context_out_of_scope"):
            await reader.read(request("resource", "file:///etc/passwd"))
        with pytest.raises(MCPError, match="context_out_of_scope"):
            await reader.read(request("prompt", "desconhecido"))
        selected = await reader.selected(
            [request("resource", "ensaio://manual"), request("prompt", "inseguro")]
        )
        assert sum(len(item.text.encode()) for item in selected) <= 100
        calls = []
        registry = ToolRegistry(
            [
                Tool(
                    "controlar_janela",
                    "execução de ensaio",
                    {"type": "object"},
                    lambda: calls.append(1),
                )
            ]
        )
        policy = PolicyEngine(
            path_guard=PathGuard(protected_roots=(), safe_roots=(), system_roots=())
        )
        gw = FakeGateway(pede(chama("controlar_janela")), fala("aguardando"))
        agent = Agent(gateway=gw, tools=registry, policy=policy, memory=store)
        events = [e async for e in agent.run("web", "use o template", external=selected)]
        assert calls == [] and any(e.kind == "approval" for e in events)
        assert "IGNORE" not in gw.chamadas[0][0]["content"]
        assert any("IGNORE" in m["content"] and m["role"] == "user" for m in gw.chamadas[0])
        session = store.active_session("web")
        assert store.counter_get(f"taint:{session.id}") == 1
        assert store.history(session.id)[0].provenance["external"]["key"] == "ensaio://manual"
        assert store.history(session.id)[-1].provenance["contexto_externo"][1]["key"] == "inseguro"
        capabilities = (
            await host.connections["contexto"].call("capacidades", {})
        ).structured_content
        assert capabilities == {
            "sampling": False,
            "elicitation": False,
            "extensions": False,
            "experimental": False,
        }
    finally:
        await host.close()
        store.close()


def test_chat_context_requires_admin_and_local_scope(tmp_path):
    from fastapi.testclient import TestClient

    from orion.app import create_app
    from orion.config import Settings

    token = "segredo-admin-apenas-fixture"
    settings = Settings(
        data_dir=tmp_path,
        admin_token=token,
        jobs_enabled=False,
        _env_file=None,
        mcp_connections=[
            StdioConfig(
                id="contexto",
                command=sys.executable,
                args=[str(Path(__file__).with_name("mcp_context_server.py"))],
                trusted=True,
                enabled=True,
                resources=["ensaio://manual"],
            )
        ],
    )
    gateway = FakeGateway(fala("feito"))
    body = {
        "texto": "consultar",
        "contexto": [{"connection": "contexto", "kind": "resource", "key": "ensaio://manual"}],
    }
    auth = {"Authorization": f"Bearer {token}"}
    with TestClient(
        create_app(settings, gateway_factory=lambda _: gateway), base_url="http://127.0.0.1"
    ) as client:
        assert client.post("/chat", json=body).status_code == 401
        invalid = {
            "texto": "consultar",
            "contexto": [{"connection": "contexto", "kind": "resource", "key": "ensaio://privado"}],
        }
        assert client.post("/chat", json=invalid, headers=auth).status_code == 422
        response = client.post("/chat", json=body, headers=auth)
        assert response.status_code == 200 and "[DONE]" in response.text
        memory = client.app.state.orion.memory
        session = memory.active_session("web")
        assert (
            memory.history(session.id)[-1].provenance["contexto_externo"][0]["key"]
            == "ensaio://manual"
        )
        assert any(
            "manual de ensaio" in m["content"] and m["role"] == "user" for m in gateway.chamadas[0]
        )
