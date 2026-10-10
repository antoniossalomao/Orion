import asyncio
import json
import os

import pytest

from orion.agent import Agent
from orion.extensions.skill_runtime import SkillRuntime, SkillSource
from orion.extensions.skills import SkillError, metadata
from orion.memory import MemoryStore
from orion.policy import PathGuard, PolicyEngine
from orion.tools.registry import ToolRegistry
from tests.fakes import FakeGateway, chama, fala, pede


@pytest.fixture
def source(tmp_path):
    root = tmp_path / "skills"
    skill = root / "ensaio"
    (skill / "scripts").mkdir(parents=True)
    (skill / "SKILL.md").write_text("""---
name: ensaio
description: Scripts controlados de ensaio
allowed-tools: executar_skill_script
---
Script [eco](scripts/eco.py), [lento](scripts/lento.py) e [grande](scripts/grande.py).
""")
    (skill / "scripts/eco.py").write_text("""import json,os,sys
print(json.dumps({"pid":os.getpid(),"secret":os.getenv("ORION_SCRIPT_SECRET"),"args":sys.argv[1:]}))
""")
    (skill / "scripts/lento.py").write_text("import time; time.sleep(10)")
    (skill / "scripts/grande.py").write_text("print('x' * 64000)")
    return root


def policy(records=None):
    return PolicyEngine(
        path_guard=PathGuard(protected_roots=(), safe_roots=(), system_roots=()),
        audit=records.append if records is not None else None,
    )


def test_disabled_untrusted_and_discovery_never_runs(source):
    for enabled in (False, True):
        rt = SkillRuntime([SkillSource(root=source, namespace="fixture", enabled=enabled)])
        registry = ToolRegistry()
        rt.attach_tools(registry, policy())
        assert not rt.script_reviews and not registry.names()


@pytest.mark.skipif(os.name != "posix", reason="Windows aguarda validação real do runner")
async def test_trusted_script_requires_approval_minimal_env_and_revision(
    source, tmp_path, monkeypatch
):
    monkeypatch.setenv("ORION_SCRIPT_SECRET", "segredo-apenas-de-ensaio")
    rt = SkillRuntime(
        [SkillSource(root=source, namespace="fixture", enabled=True, trusted_scripts=True)]
    )
    records = []
    engine = policy(records)
    registry = ToolRegistry()
    rt.attach_tools(registry, engine)
    store = MemoryStore(tmp_path / "memory.db")
    try:
        args = {
            "skill": "fixture:ensaio",
            "script": "scripts/eco.py",
            "argv": ["literal; echo não-shell"],
        }
        gw = FakeGateway(
            pede(chama("executar_skill_script", **args)), fala("aguardando"), fala("feito")
        )
        agent = Agent(gateway=gw, tools=registry, policy=engine, memory=store)
        selection = rt.select("oi", ["fixture:ensaio"])
        events = [e async for e in agent.run("web", "execute", selection=selection)]
        approval = next(e for e in events if e.kind == "approval")
        assert not rt.runner.running and len(gw.chamadas) == 2
        engine.approvals.decide(approval.data["id"], True, channel="web", actor="teste")
        _ = [e async for e in agent.resume("web", approval.data["id"])]
        note = store.history(store.active_session("web").id)[-2].text
        result = json.loads(note.split("Resultado: ", 1)[1].split("\n", 2)[1])
        output = json.loads(result["stdout"])
        assert output["secret"] is None and output["args"] == ["literal; echo não-shell"]
        with pytest.raises(ProcessLookupError):
            os.kill(output["pid"], 0)
        assert all(record["args"].get("argv", "***") == "***" for record in records)
        script = source / "ensaio/scripts/eco.py"
        script.write_text("raise RuntimeError('nova revisão')")
        result = json.loads(await registry.get("executar_skill_script").run_async(args))
        assert result["codigo"] == "script_revision_changed"
        rt.enabled.clear()
        result = json.loads(await registry.get("executar_skill_script").run_async(args))
        assert result["codigo"] == "scripts_untrusted_or_disabled"
    finally:
        await rt.close()
        store.close()


@pytest.mark.skipif(os.name != "posix", reason="Windows aguarda validação real do runner")
async def test_timeout_output_cancel_and_scope(source):
    rt = SkillRuntime(
        [
            SkillSource(
                root=source,
                namespace="fixture",
                enabled=True,
                trusted_scripts=True,
                script_timeout_s=0.1,
            )
        ]
    )
    registry = ToolRegistry()
    rt.attach_tools(registry, policy())
    tool = registry.get("executar_skill_script")
    try:
        assert (
            json.loads(
                await tool.run_async({"skill": "fixture:ensaio", "script": "scripts/lento.py"})
            )["codigo"]
            == "script_timeout"
        )
        result = json.loads(
            await tool.run_async({"skill": "fixture:ensaio", "script": "scripts/grande.py"})
        )
        assert result["codigo"] == "script_output_limit"
        result = json.loads(
            await tool.run_async({"skill": "fixture:ensaio", "script": "../fuera.py"})
        )
        assert result["codigo"] == "script_arguments_invalid"
        rt.script_reviews["fixture:ensaio"] = (rt.script_reviews["fixture:ensaio"][0], 10)
        task = asyncio.create_task(
            tool.run_async({"skill": "fixture:ensaio", "script": "scripts/lento.py"})
        )
        await asyncio.sleep(0.05)
        task.cancel()
        with pytest.raises(asyncio.CancelledError):
            await task
        assert not rt.runner.running
    finally:
        await rt.close()


def test_secret_reference_blocked_before_loading_body(source):
    skill = source / "ensaio"
    (skill / ".env").write_text("segredo-de-fixture")
    (skill / "SKILL.md").write_text("---\nname: ensaio\ndescription: ensaio\n---\n[segredo](.env)")
    with pytest.raises(SkillError):
        metadata(skill, namespace="fixture").load()
