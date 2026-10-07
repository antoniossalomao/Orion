from types import SimpleNamespace

import pytest

from orion.calendar import Calendar
from orion.calendar_events import Events
from orion.extensions.host import MCPError
from orion.extensions.profiles import profile
from orion.file_plans import FilePlans, Plan
from orion.memory import MemoryStore
from orion.policy import PathGuard, PolicyEngine
from orion.tools.registry import ToolRegistry


def setup(tmp_path):
    folder = tmp_path / "work"
    folder.mkdir()
    (folder / "a.md").write_text("Fonte original")
    (folder / "b.txt").write_text("Outro original")
    memory = MemoryStore(tmp_path / "m.db")
    session = memory.new_session("web")
    policy = PolicyEngine(path_guard=PathGuard(safe_roots=(folder,)))
    registry = ToolRegistry()
    plans = FilePlans(Events(Calendar(memory, SimpleNamespace(), registry, policy)))
    return folder, memory, session, policy, plans


def approve(plans, policy, row):
    approval = plans.review(row["id"], None, row["digest"])["approval_id"]
    policy.approvals.decide(approval, True, channel="web", actor="fixture")
    return approval


def test_review_reject_then_exact_copies_once_preserves_sources(tmp_path):
    folder, memory, session, policy, plans = setup(tmp_path)
    original = {p.name: p.read_bytes() for p in folder.iterdir()}
    row = plans.create(Plan(root=str(folder), session_id=session.id), None)
    assert {p.name: p.read_bytes() for p in folder.iterdir()} == original
    decision = plans.review(row["id"], None, row["digest"])
    policy.approvals.decide(decision["approval_id"], False, channel="web", actor="fixture")
    with pytest.raises(MCPError, match="approval_unavailable"):
        plans.apply(row["id"], None, decision["approval_id"])
    assert not (folder / "Organizados").exists()
    approval = approve(plans, policy, row)
    assert plans.apply(row["id"], None, approval)["copies"] == 2
    for name, raw in original.items():
        assert (folder / name).read_bytes() == raw
        assert (folder / "Organizados/Textos" / name).read_bytes() == raw
    with pytest.raises(MCPError, match="already_submitted"):
        plans.apply(row["id"], None, approval)
    assert plans.get(row["id"], None)["completed"] == 2
    assert plans.registry.get("aplicar_organizacao") is None
    assert profile("orion-arquivos").manifest.version == "1.0.0"
    memory.close()


def test_snapshot_change_before_effects_scope_links_and_guard(tmp_path):
    folder, memory, session, policy, plans = setup(tmp_path)
    (folder / "linked.md").symlink_to(folder / "a.md")
    (folder / ".env.local.md").write_text("secret canary")
    (folder / "hard.md").hardlink_to(folder / "b.txt")
    row = plans.create(Plan(root=str(folder), session_id=session.id), None)
    assert [r["source"] for r in row["copies"]] == ["a.md"]
    approval = approve(plans, policy, row)
    (folder / "a.md").write_text("Changed after review")
    with pytest.raises(MCPError, match="snapshot_changed"):
        plans.apply(row["id"], None, approval)
    assert not (folder / "Organizados").exists()
    with pytest.raises(MCPError, match="root_not_authorized"):
        plans.create(Plan(root=str(tmp_path), session_id=session.id), None)
    with pytest.raises(MCPError, match="not_found"):
        plans.get(row["id"], "a" * 32)
    memory.close()


def test_partial_failure_never_replays_or_overwrites(tmp_path, monkeypatch):
    folder, memory, session, policy, plans = setup(tmp_path)
    row = plans.create(Plan(root=str(folder), session_id=session.id), None)
    approval = approve(plans, policy, row)
    import os

    real_open = os.open

    def fail_second(path, flags, *args):
        if flags & os.O_EXCL and str(path).endswith("b.txt"):
            raise OSError("fixture disk full")
        return real_open(path, flags, *args)

    monkeypatch.setattr(os, "open", fail_second)
    with pytest.raises(MCPError, match="partial_failure"):
        plans.apply(row["id"], None, approval)
    state = plans.get(row["id"], None)
    assert state["status"] == "partial" and state["completed"] == 1
    assert (folder / "Organizados/Textos/a.md").read_bytes() == (folder / "a.md").read_bytes()
    with pytest.raises(MCPError, match="already_submitted"):
        plans.apply(row["id"], None, approval)
    memory.close()
