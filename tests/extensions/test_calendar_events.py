from pathlib import Path
from types import SimpleNamespace

import pytest
from mcp.types import Tool as RemoteTool

from orion.calendar import Binding, Calendar
from orion.calendar_events import Events, Proposal
from orion.extensions.host import MCPError
from orion.memory import MemoryStore
from orion.policy import PathGuard, PolicyEngine
from orion.tools.registry import ToolRegistry
from tests.extensions.test_calendar import Connector


class EventConnector(Connector):
    async def list_tools(self):
        tools = [
            *await super().list_tools(),
            RemoteTool.model_validate_json(
                (Path(__file__).parent / "fixtures/calendar_create_tool.json").read_text()
            ),
        ]
        if getattr(self, "schema_changed", False):
            tools[-1] = tools[-1].model_copy(update={"description": "new revision"})
        return tools

    async def call(self, name, args):
        if name == "create-event":
            self.calls.append((name, args))
            assert "attendees" not in args and args["sendUpdates"] == "none"
            if self.mode == "timeout":
                raise MCPError("call_timeout", possibly_active=True)
            return SimpleNamespace(is_error=False)
        return await super().call(name, args)


async def test_propose_reject_approve_use_once_revoke_unknown_no_replay(tmp_path):
    memory = MemoryStore(tmp_path / "m.db")
    session = memory.new_session("web")
    conn = EventConnector("personal")
    registry = ToolRegistry()
    policy = PolicyEngine(path_guard=PathGuard())
    calendar = Calendar(memory, SimpleNamespace(connections={"agenda": conn}), registry, policy)
    await calendar.bind(Binding(connection_id="agenda", account="work", reviewed_read_only=True))
    events = Events(calendar)
    assert registry.get("criar_evento_agenda") is None
    proposal = events.create(
        Proposal(
            session_id=session.id,
            title="Evento revisado",
            start="2026-10-07T09:00:00-03:00",
            end="2026-10-07T10:00:00-03:00",
        ),
        None,
    )
    assert conn.calls == []
    review = await events.review(proposal["id"], None, proposal["digest"])
    policy.approvals.decide(review["approval_id"], False, channel="web", actor="fixture")
    with pytest.raises(MCPError, match="event_approval_unavailable"):
        await events.resume(proposal["id"], None, review["approval_id"])
    assert conn.calls == []
    review = await events.review(proposal["id"], None, proposal["digest"])
    policy.approvals.decide(review["approval_id"], True, channel="web", actor="fixture")
    assert (await events.resume(proposal["id"], None, review["approval_id"]))["status"] == "created"
    assert len(conn.calls) == 1 and conn.calls[0][1]["summary"] == "Evento revisado"
    with pytest.raises(MCPError, match="event_already_submitted"):
        await events.resume(proposal["id"], None, review["approval_id"])
    second = events.create(
        Proposal(
            session_id=session.id,
            title="Revogar",
            start="2026-10-07T11:00:00-03:00",
            end="2026-10-07T12:00:00-03:00",
        ),
        None,
    )
    review = await events.review(second["id"], None, second["digest"])
    policy.approvals.decide(review["approval_id"], True, channel="web", actor="fixture")
    conn.generation = "revoked-generation"
    with pytest.raises(MCPError, match="event_origin_review_changed"):
        await events.resume(second["id"], None, review["approval_id"])
    assert len(conn.calls) == 1
    third = events.create(
        Proposal(
            session_id=session.id,
            title="Timeout",
            start="2026-10-07T13:00:00-03:00",
            end="2026-10-07T14:00:00-03:00",
        ),
        None,
    )
    review = await events.review(third["id"], None, third["digest"])
    policy.approvals.decide(review["approval_id"], True, channel="web", actor="fixture")
    conn.mode = "timeout"
    with pytest.raises(MCPError, match="call_timeout"):
        await events.resume(third["id"], None, review["approval_id"])
    assert events.get(third["id"], None)["status"] == "unknown"
    with pytest.raises(MCPError, match="event_already_submitted"):
        await events.resume(third["id"], None, review["approval_id"])
    assert len(conn.calls) == 2
    fourth = events.create(
        Proposal(
            session_id=session.id,
            title="Schema change",
            start="2026-10-07T15:00:00-03:00",
            end="2026-10-07T16:00:00-03:00",
        ),
        None,
    )
    review = await events.review(fourth["id"], None, fourth["digest"])
    policy.approvals.decide(review["approval_id"], True, channel="web", actor="fixture")
    conn.schema_changed = True
    with pytest.raises(MCPError, match="event_creation_schema_changed"):
        await events.resume(fourth["id"], None, review["approval_id"])
    assert len(conn.calls) == 2
    from orion.extensions.profiles import profile

    assert profile("orion-agenda").manifest.skills == [
        "skills/planejar-dia",
        "skills/preparar-reuniao",
    ]
    memory.close()
