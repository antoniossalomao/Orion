import asyncio
import os
import sys
from pathlib import Path

import pytest

from orion.extensions.catalog import Catalog
from orion.extensions.host import MCPError, MCPHost, StdioConfig
from orion.policy import Context, PathGuard, PolicyEngine, Risk, Status, ToolCall
from orion.tools.registry import ToolRegistry


async def test_timeout_cancel_disconnect_reconnect_and_no_replay():
    cfg = StdioConfig(
        id="ensaio",
        command=sys.executable,
        args=[str(Path(__file__).with_name("mcp_server.py"))],
        enabled=True,
        trusted=True,
        classifications={"lento": Risk.EXEC},
    )
    host = MCPHost([cfg])
    await host.start()
    connection = host.connections["ensaio"]
    policy = PolicyEngine(path_guard=PathGuard(protected_roots=(), safe_roots=(), system_roots=()))
    catalog = Catalog(host, ToolRegistry(), policy)
    try:
        await catalog.refresh()
        old_name = next(iter(catalog.entries))
        approval = policy.evaluate(ToolCall(old_name, {"seconds": 5}), Context("s"))
        policy.approvals.decide(approval.approval_id, True, channel="web", actor="teste")
        pid = (await connection.call("eco", {"texto": "pid"})).structured_content["pid"]
        connection.timeout = 0.1
        with pytest.raises(MCPError) as failure:
            await connection.call("lento", {"seconds": 5})
        assert failure.value.code == "call_timeout" and failure.value.possibly_active
        assert connection.last_call["possibly_active"]
        connection.timeout = 5
        assert (await connection.call("chamadas_lentas", {})).structured_content["calls"] == 1
        task = asyncio.create_task(connection.call("lento", {"seconds": 5}))
        await asyncio.sleep(0.05)
        task.cancel()
        with pytest.raises(asyncio.CancelledError):
            await task
        assert connection.last_call["state"] == "cancelled"
        assert (await connection.call("chamadas_lentas", {})).structured_content["calls"] == 2
        await catalog.reconnect("ensaio")
        assert old_name not in catalog.entries
        assert policy.approvals.get(approval.approval_id).status is Status.DENIED
        assert (await connection.call("chamadas_lentas", {})).structured_content["calls"] == 0
        if os.name == "posix":
            with pytest.raises(ProcessLookupError):
                os.kill(pid, 0)
        with pytest.raises(MCPError) as failure:
            await connection.call("queda", {})
        assert failure.value.possibly_active
        await connection.close()
        assert connection.client is None
        await catalog.refresh()
        assert not catalog.entries
    finally:
        await host.close()


async def test_shutdown_cancels_call_and_closes_process():
    host = MCPHost(
        [
            StdioConfig(
                id="close",
                command=sys.executable,
                args=[str(Path(__file__).with_name("mcp_server.py"))],
                enabled=True,
                trusted=True,
            )
        ]
    )
    await host.start()
    connection = host.connections["close"]
    pid = (await connection.call("eco", {"texto": "pid"})).structured_content["pid"]
    call = asyncio.create_task(connection.call("lento", {"seconds": 10}))
    await asyncio.sleep(0.05)
    await host.close()
    assert call.cancelled() and connection.client is None
    if os.name == "posix":
        with pytest.raises(ProcessLookupError):
            os.kill(pid, 0)
