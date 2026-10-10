import json
import sys
from pathlib import Path

from orion.agent import Agent
from orion.app import sse
from orion.extensions.catalog import Catalog
from orion.extensions.host import MCPHost, StdioConfig
from orion.memory import MemoryStore
from orion.policy import PathGuard, PolicyEngine, Risk
from orion.tools.registry import ToolRegistry
from tests.fakes import FakeGateway, chama, fala, pede


async def test_real_mcp_activity_stream_and_persisted_provenance(tmp_path):
    host = MCPHost(
        [
            StdioConfig(
                id="fixture",
                command=sys.executable,
                args=[str(Path(__file__).with_name("mcp_server.py"))],
                trusted=True,
                enabled=True,
                classifications={"eco": Risk.READ},
            )
        ]
    )
    registry = ToolRegistry()
    policy = PolicyEngine(path_guard=PathGuard())
    database = tmp_path / "memory.db"
    memory = MemoryStore(database)
    await host.start()
    try:
        catalog = Catalog(host, registry, policy)
        await catalog.refresh()
        entry = next(iter(catalog.entries.values()))
        gateway = FakeGateway(
            pede(chama(entry.name, texto="dado externo")), fala("Resposta com fonte.")
        )
        agent = Agent(gateway=gateway, tools=registry, policy=policy, memory=memory)
        events = [ev async for ev in agent.run("web", "eco")]
        start = next(ev for ev in events if ev.kind == "activity")
        complete = next(ev for ev in events if ev.kind == "tool")
        assert start.data["state"] == "processing" and complete.data["state"] == "completed"
        assert start.data["call_id"] == complete.data["call_id"]
        assert complete.data["label"] == "eco" and complete.data["revision"] == entry.revision
        done = events[-1]
        wire = sse(done)
        assert '"provenance"' in wire and wire.endswith("data: [DONE]\n\n")
        assert done.data["provenance"]["atividades"][0]["origin"] == "fixture"
        assert "dado externo" not in json.dumps(done.data)
        session_id = memory.active_session("web").id
    finally:
        await host.close()
        memory.close()
    restored = MemoryStore(database)
    try:
        assert (
            restored.history(session_id)[-1].provenance["atividades"][0]["revision"]
            == entry.revision
        )
    finally:
        restored.close()
