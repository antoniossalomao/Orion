import sys
from pathlib import Path

from orion.agent import Agent
from orion.extensions.catalog import Catalog
from orion.extensions.host import MCPHost, StdioConfig
from orion.memory import MemoryStore
from orion.policy import Context, PathGuard, PolicyEngine, Risk, Status, ToolCall
from orion.tools.registry import Tool, ToolRegistry
from tests.fakes import FakeGateway, chama, fala, pede


async def test_large_catalog_only_selected_schemas_and_removal(tmp_path):
    reviewed = dict.fromkeys([*(f"dado_{i:03}" for i in range(120)), "planetas"], Risk.READ)
    host = MCPHost(
        [
            StdioConfig(
                id="grande",
                command=sys.executable,
                args=[str(Path(__file__).with_name("mcp_large_server.py"))],
                enabled=True,
                trusted=True,
                classifications=reviewed,
            )
        ]
    )
    registry = ToolRegistry([Tool("nativa", "sempre disponível", {"type": "object"}, lambda: {})])
    policy = PolicyEngine(path_guard=PathGuard(protected_roots=(), safe_roots=(), system_roots=()))
    store = MemoryStore(tmp_path / "memory.db")
    await host.start()
    try:
        catalog = Catalog(host, registry, policy)
        await catalog.refresh()
        assert len(catalog.entries) == 121
        target = next(e for e in catalog.entries.values() if e.remote_name == "planetas")
        other = next(e for e in catalog.entries.values() if e.remote_name == "dado_000")
        summaries = registry.catalog("astronomia")
        assert len(summaries) == 1 and "parameters" not in summaries[0]
        schemas = registry.schemas(query="astronomia planetas")
        assert len(schemas) == 2 and schemas[0]["function"]["name"] == "nativa"
        assert registry.context_usage(schemas)["bytes"] < 2000
        assert len(registry.schemas(query="inventário peças")) == 9
        assert len(registry.schemas(selected=[target.name], budget=1)) == 1
        gw = FakeGateway(pede(chama(other.name)), fala("bloqueado"))
        agent = Agent(gateway=gw, tools=registry, policy=policy, memory=store)
        agent.refresh_tools = catalog.refresh
        events = [e async for e in agent.run("web", "astronomia")]
        assert any(e.kind == "tool" and e.data.get("reason") == "not_selected" for e in events)
        assert len(gw.ferramentas[0]) == 2
        pending = policy.approvals.request("s", target.name, {}, "ensaio")
        await host.connections["grande"].call("retirar", {})
        await catalog.refresh()
        assert target.name not in registry.names()
        assert policy.approvals.get(pending.id).status is Status.DENIED
        assert policy.evaluate(ToolCall(target.name), Context("s")).action.value == "deny"
        # Falha de descoberta também revoga, em vez de manter catálogo obsoleto.
        connection = host.connections["grande"]

        async def fail():
            raise RuntimeError("falha controlada")

        connection.list_tools = fail
        await catalog.refresh()
        assert not catalog.entries and registry.names() == ["nativa"]
    finally:
        await host.close()
        store.close()
