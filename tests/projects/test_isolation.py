from contextlib import closing
from pathlib import Path

import pytest

from orion.agent import Agent
from orion.extensions.context import ContextReader, ContextRequest
from orion.extensions.host import Connection, MCPError, MCPHost, StdioConfig
from orion.extensions.skill_runtime import SkillRuntime, SkillSource
from orion.extensions.skills import SkillError
from orion.memory import MemoryStore
from orion.memory.scope import data_scope
from orion.policy import Action, Context, PathGuard, PolicyEngine, Risk, ToolCall, ToolSpec
from orion.projects import Projects
from orion.tools import ToolRegistry, memory_tools
from tests.fakes import FakeGateway, chama, fala, pede


class Embedder:
    dim = 2

    def embed(self, texts):
        return [[1, 0] for _ in texts]


def test_lexical_and_vector_scope_share_dedupe_prune_and_restart(tmp_path):
    path = tmp_path / "scopes.db"
    with closing(MemoryStore(path, embedder=Embedder())) as memory:
        a, b = [Projects(memory).create(name)["id"] for name in ("A", "B")]
        personal = memory.add_fact("Canário pessoal", "personal")
        for project, marker in ((a, "AMBER"), (b, "BLUE")):
            with data_scope(project, include_personal=False):
                memory.add_fact(f"Canário {marker}", marker)
                memory.index_document("same.md", marker, f"Canário {marker}")
                session = memory.new_session("web", project_id=project)
                memory.add_message(session.id, "user", f"Canário {marker}")
                duplicate = memory.add_fact("Canário pessoal", marker)
                assert duplicate.id != personal.id
        for project, marker, forbidden in ((a, "AMBER", "BLUE"), (b, "BLUE", "AMBER")):
            with data_scope(project, include_personal=False):
                for query in ("Canário", "semantic-nonmatching-query"):
                    hits = memory.search(query, k=40)
                    assert hits and any(marker in h.text for h in hits)
                    assert not any(forbidden in h.text or h.source == "personal" for h in hits)
                assert memory.forget_fact(personal.id) is False
                with pytest.raises(KeyError):
                    memory.update_fact(personal.id, "não permitido")
            with data_scope(project, include_personal=True):
                assert any(f.id == personal.id for f in memory.facts())
                assert not any(forbidden in h.text for h in memory.search("Canário", k=40))
        vault = tmp_path / "vault"
        vault.mkdir()
        memory.index_vault(vault)
        with data_scope(a, include_personal=False):
            assert any(h.kind == "chunk" for h in memory.search("AMBER"))
    with closing(MemoryStore(path)) as memory:
        with data_scope(b, include_personal=False):
            assert all("AMBER" not in h.text for h in memory.search("Canário", k=40))
        assert [f.id for f in memory.facts()] == [personal.id]


async def test_agent_actual_session_scopes_native_tools_and_approval_revision(tmp_path):
    with closing(MemoryStore(tmp_path / "agent.db")) as memory:
        projects = Projects(memory)
        a, b = [projects.create(name)["id"] for name in ("A", "B")]
        policy = PolicyEngine(path_guard=PathGuard())
        gateway = FakeGateway(
            pede(chama("salvar_memoria", texto="AMBER dado", fonte="chat")),
            fala("salvo"),
            fala("vazio"),
            pede(chama("esquecer_fato", id=1)),
            fala("revisar"),
        )
        agent = Agent(
            gateway=gateway, tools=ToolRegistry(memory_tools(memory)), policy=policy, memory=memory
        )
        sa = projects.activate("web", a)
        await _collect(agent.run("web", "AMBER"))
        with data_scope(a, include_personal=False):
            assert memory.facts()[0].text == "AMBER dado"
        projects.activate("web", b)
        await _collect(agent.run("web", "AMBER"))
        assert not any("AMBER dado" in m.get("content", "") for m in gateway.chamadas[2])
        projects.activate("web", a)
        events = await _collect(agent.run("web", "esquecer"))
        approval = next(e.data["id"] for e in events if e.kind == "approval")
        policy.approvals.decide(approval, True, channel="web", actor="fixture")
        projects.update(a, instructions="revisão nova")
        result = await _collect(agent.resume("web", approval))
        assert result[0].kind == "error"
        with data_scope(a, include_personal=False):
            assert memory.facts()
        result = await _collect(agent.run("web", "x", expected_session="0" * 32))
        assert result[0].data["message"] == "session_scope_changed"
        assert memory.get_session(sa.id).project_id == a


async def _collect(generator):
    return [event async for event in generator]


def test_policy_hard_scope_root_and_cross_project_approval(tmp_path):
    root = tmp_path / "A"
    root.mkdir()
    outside = tmp_path / "B"
    outside.mkdir()
    (root / "escape").symlink_to(outside, target_is_directory=True)
    policy = PolicyEngine(path_guard=PathGuard())
    a = Context("web:test", project_id="a" * 32, root=str(root), project_revision=1)
    b = Context("web:test", project_id="b" * 32, root=str(outside), project_revision=1)
    for name, args in (
        ("ler_arquivo", {"path": str(outside / "secret")}),
        ("ler_arquivo", {"path": str(root / "escape" / "secret")}),
        ("executar_comando", {"cmd": "cat /etc/passwd"}),
        ("listar_numeros", {}),
    ):
        assert policy.evaluate(ToolCall(name, args), a).action is Action.DENY
    call = ToolCall("esquecer_fato", {"id": 1})
    decision = policy.evaluate(call, a)
    policy.approvals.decide(decision.approval_id, True, channel="web", actor="fixture")
    assert policy.evaluate(call, b).action is Action.CONFIRM
    assert policy.evaluate(call, a).action is Action.ALLOW
    policy.tools["external"] = ToolSpec("external", Risk.READ, scope="project:" + a.project_id)
    assert policy.evaluate(ToolCall("external"), b).action is Action.DENY
    assert policy.evaluate(ToolCall("external"), a).action is Action.ALLOW


async def test_skill_and_mcp_context_reject_other_scope_before_rpc(tmp_path):
    folder = tmp_path / "skills" / "retomar"
    folder.mkdir(parents=True)
    (folder / "SKILL.md").write_text(
        "---\nname: retomar\ndescription: retomar contexto pesquisado\n---\nAMBER", encoding="utf-8"
    )
    scope = "project:" + "a" * 32
    runtime = SkillRuntime(
        [SkillSource(root=folder.parent, namespace="a", enabled=True, scope=scope)]
    )
    try:
        assert runtime.select("retomar contexto", context="personal").skills == ()
        with pytest.raises(SkillError, match="skill_out_of_scope"):
            runtime.select("", ["a:retomar"], context="project:" + "b" * 32)
        assert runtime.select("", ["a:retomar"], context=scope).skills
    finally:
        await runtime.close()
    host = MCPHost([])
    host.connections["a"] = Connection(
        StdioConfig(
            id="a",
            command=str(Path("/usr/bin/python3")),
            resources=["fixture://private"],
            scope=scope,
        )
    )
    reader = ContextReader(host)
    with pytest.raises(MCPError, match="context_out_of_scope"):
        await reader.selected(
            [ContextRequest(connection="a", kind="resource", key="fixture://private")]
        )
    assert host.connections["a"].state == "disabled"
