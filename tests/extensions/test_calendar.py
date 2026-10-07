import json
from pathlib import Path
from types import SimpleNamespace

from mcp.types import Tool as RemoteTool

from orion.calendar import Binding, Calendar
from orion.extensions.host import MCPError, StdioConfig
from orion.memory import MemoryStore
from orion.memory.scope import data_scope
from orion.policy import Context, PathGuard, PolicyEngine, Risk, ToolCall
from orion.projects import Projects
from orion.tools.registry import ToolRegistry


class Connector:
    def __init__(self, scope):
        self.config = StdioConfig(
            id="agenda",
            command="/fixture/node",
            scope=scope,
            classifications={"list-events": Risk.READ, "get-freebusy": Risk.READ},
        )
        self.state = "connected"
        self.generation = "fixture-generation"
        self.authorized = None
        self.calls = []
        self.mode = "ok"

    async def list_tools(self):
        return [
            RemoteTool.model_validate(t)
            for t in json.loads(
                (Path(__file__).parent / "fixtures/calendar_read_tools.json").read_text()
            )
        ]

    async def call(self, name, args):
        self.calls.append((name, args))
        if self.mode == "quota":
            return SimpleNamespace(is_error=True, structured_content={"error": "quota"}, content=[])
        if self.mode == "timeout":
            raise MCPError("call_timeout")
        return SimpleNamespace(
            is_error=False,
            structured_content={
                "events": [{"summary": "AMBER", "start": {"dateTime": "2026-10-07T09:00:00-03:00"}}]
            },
            content=[],
        )


async def test_calendar_explicit_account_timezone_scope_read_only_quota_revoke(tmp_path):
    memory = MemoryStore(tmp_path / "memory.db")
    a, b = [Projects(memory).create(n)["id"] for n in ("A", "B")]
    scope = f"project:{a}"
    conn = Connector(scope)
    host = SimpleNamespace(connections={"agenda": conn})
    registry = ToolRegistry()
    policy = PolicyEngine(path_guard=PathGuard())
    calendar = Calendar(memory, host, registry, policy)
    body = Binding(scope=scope, connection_id="agenda", account="work", reviewed_read_only=True)
    await calendar.bind(body)
    period = {"start": "2026-10-07T00:00:00-03:00", "end": "2026-10-08T00:00:00-03:00"}
    assert (
        policy.evaluate(
            ToolCall("consultar_agenda", period), Context("sid", project_id=a)
        ).action.value
        == "allow"
    )
    with data_scope(a):
        result = await calendar.read("list-events", **period)
        assert result["ok"] and result["source"]["timezone"] == "America/Sao_Paulo"
        assert result["data"]["structured"]["events"][0]["start"]["dateTime"].endswith("-03:00")
        assert conn.calls[-1][1]["account"] == "work"
        assert conn.calls[-1][1]["calendarId"] == "primary"
        assert (await calendar.read("get-freebusy", **period))["ok"]
        assert conn.calls[-1][1]["calendars"] == [{"id": "primary"}]
        conn.mode = "quota"
        assert (await calendar.read("list-events", **period))["code"] == "calendar_quota"
        conn.mode = "timeout"
        assert (await calendar.read("list-events", **period))["code"] == "call_timeout"
    count = len(conn.calls)
    with data_scope(b):
        assert (await calendar.read("list-events", **period))["code"] == "calendar_not_configured"
    assert len(conn.calls) == count
    with data_scope(a):
        conn.state = "disabled"
        assert (await calendar.read("list-events", **period))[
            "code"
        ] == "calendar_connection_unavailable"
    assert {name for name, args in conn.calls} <= {"list-events", "get-freebusy"}
    memory.close()
