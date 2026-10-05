import json
import sys
from pathlib import Path

from orion.agent import Agent
from orion.extensions.catalog import Catalog
from orion.extensions.host import MCPHost, StdioConfig
from orion.memory import MemoryStore
from orion.policy import Action, Context, PathGuard, PolicyEngine, Risk, Status, ToolCall
from orion.tools.registry import ToolRegistry
from tests.fakes import FakeGateway, chama, fala, pede


async def test_classification_approval_taint_revision_and_collision(tmp_path):
    configurations = [
        StdioConfig(
            id=name,
            command=sys.executable,
            args=[str(Path(__file__).with_name("mcp_server.py"))],
            enabled=True,
            trusted=True,
            classifications={"eco": Risk.READ, "alterar_fixture": Risk.DESTRUCTIVE},
        )
        for name in ("um", "dois")
    ]
    host = MCPHost(configurations)
    registry = ToolRegistry()
    records = []
    policy = PolicyEngine(
        path_guard=PathGuard(protected_roots=(), safe_roots=(), system_roots=()),
        audit=records.append,
    )
    store = MemoryStore(tmp_path / "memory.db")
    await host.start()
    try:
        catalog = Catalog(host, registry, policy)
        await catalog.refresh()
        assert len(catalog.entries) == 4 and len(set(registry.names())) == 4
        assert all(e.remote_name != "ambiente" for e in catalog.entries.values())
        assert policy.evaluate(ToolCall("ambiente"), Context("x")).action is Action.DENY
        read = next(
            e for e in catalog.entries.values() if e.connection == "um" and e.remote_name == "eco"
        )
        write = next(
            e
            for e in catalog.entries.values()
            if e.connection == "um" and e.remote_name == "alterar_fixture"
        )
        gw = FakeGateway(
            pede(chama(read.name, texto="ensaio")),
            fala("lido"),
            pede(chama(write.name)),
            fala("aguardando"),
            fala("feito"),
        )
        agent = Agent(gateway=gw, tools=registry, policy=policy, memory=store)
        events = [e async for e in agent.run("web", "leitura")]
        assert any(e.kind == "tool" and e.data["decision"] == "allow" for e in events)
        assert agent._context(store.active_session("web").id).tainted
        events = [e async for e in agent.run("web", "alterar")]
        approval = next(e for e in events if e.kind == "approval")
        connection = host.connections["um"]
        assert (await connection.call("estado_fixture", {})).structured_content["changes"] == 0
        policy.approvals.decide(approval.data["id"], True, channel="web", actor="teste")
        _ = [e async for e in agent.resume("web", approval.data["id"])]
        assert (await connection.call("estado_fixture", {})).structured_content["changes"] == 1
        assert records[-1]["origin"] == write.canonical_id
        assert records[-1]["revision"] == write.revision
        # A revisão do schema/origem revoga aprovações e referências capturadas.
        old_tool = registry.get(write.name)
        a = policy.evaluate(ToolCall(write.name), Context("z"))
        policy.approvals.decide(a.approval_id, True, channel="web", actor="teste")
        connection.config = connection.config.model_copy(
            update={"classifications": {"eco": Risk.READ}}
        )
        await catalog.refresh()
        assert policy.approvals.get(a.approval_id).status is Status.DENIED
        assert policy.evaluate(ToolCall(write.name), Context("z")).action is Action.DENY
        result = json.loads(await old_tool.run_async({}))
        assert result["codigo"] == "execution_error" and "origin_revoked" in result["erro"]
        assert (await connection.call("estado_fixture", {})).structured_content["changes"] == 1
        # Limite por identidade, sem afetar ferramentas nativas.
        entry = next(e for e in catalog.entries.values() if e.connection == "um")
        policy.rate.set_limit(entry.name, (1, 60))
        assert (
            policy.evaluate(ToolCall(entry.name, {"texto": "x"}), Context("t")).action
            is Action.ALLOW
        )
        assert (
            policy.evaluate(ToolCall(entry.name, {"texto": "x"}), Context("t")).action
            is Action.DENY
        )
    finally:
        await host.close()
        store.close()
