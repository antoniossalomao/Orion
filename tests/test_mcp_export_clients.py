import json
import os
import shutil
import subprocess
from pathlib import Path

import httpx2
import pytest
from mcp import Client
from mcp.client.streamable_http import streamable_http_client

from orion.export_credentials import Grant
from orion.memory.scope import data_scope
from orion.projects import Projects
from tests.test_mcp_export import running_backend

pytest_plugins = ("tests.test_mcp_export",)


async def test_two_live_clients_revocation_isolation_and_scope_cannot_be_overridden(export_backend):
    url, s = export_backend
    a, b = Projects(s.memory).create("A")["id"], Projects(s.memory).create("B")["id"]
    with data_scope(a):
        s.memory.add_fact("AMBER_A", "a.md")
    with data_scope(b):
        s.memory.add_fact("BLUE_B", "b.md")
    ga = s.export_credentials.issue(Grant(name="A", project_id=a, permissions=["facts"]))
    gb = s.export_credentials.issue(Grant(name="B", project_id=b, permissions=["facts"]))
    async with (
        httpx2.AsyncClient(
            headers={"Authorization": "Bearer " + ga["token"]}, trust_env=False
        ) as ha,
        httpx2.AsyncClient(
            headers={"Authorization": "Bearer " + gb["token"]}, trust_env=False
        ) as hb,
        Client(
            streamable_http_client(url + "/mcp-export/rpc", http_client=ha),
            cache=None,
            read_timeout_seconds=5,
        ) as ca,
        Client(
            streamable_http_client(url + "/mcp-export/rpc", http_client=hb),
            cache=None,
            read_timeout_seconds=5,
        ) as cb,
    ):
        first = await ca.call_tool("list_facts", {})
        second = await cb.call_tool("list_facts", {})
        assert "AMBER_A" in str(first) and "BLUE_B" not in str(first)
        assert "BLUE_B" in str(second) and "AMBER_A" not in str(second)
        assert (await ca.call_tool("list_facts", {"project_id": b})).is_error
        s.export_credentials.revoke(ga["id"])
        try:
            revoked = await ca.call_tool("list_facts", {})
        except Exception:
            revoked = None
        assert revoked is None or revoked.is_error
        assert not (await cb.call_tool("list_facts", {})).is_error


def test_actual_node_sdk_scopes_and_revocation(export_backend):
    sdk = os.environ.get("ORION_TEST_NODE_SDK", "")
    node = shutil.which("node")
    if not sdk or not node:
        pytest.skip("Defina ORION_TEST_NODE_SDK para comprovar o SDK Node instalado.")
    assert Path(sdk, "package.json").is_file()
    url, s = export_backend
    a, b = Projects(s.memory).create("A")["id"], Projects(s.memory).create("B")["id"]
    for id_, text in [(a, "AMBER_A"), (b, "BLUE_B")]:
        with data_scope(id_):
            s.memory.add_fact(text, "fixture.md")
    ga = s.export_credentials.issue(Grant(name="Node A", project_id=a, permissions=["facts"]))
    gb = s.export_credentials.issue(Grant(name="Node B", project_id=b, permissions=["facts"]))

    def run():
        result = subprocess.run(
            [node, str(Path(__file__).parent / "extensions/fixtures/export_client.mjs")],
            env={
                "PATH": str(Path(node).parent),
                "ORION_TEST_NODE_SDK": sdk,
                "ORION_TEST_RPC": url + "/mcp-export/rpc",
                "ORION_TEST_CLIENT_A": ga["token"],
                "ORION_TEST_CLIENT_B": gb["token"],
            },
            capture_output=True,
            text=True,
            timeout=15,
            check=True,
        )
        return json.loads(result.stdout)

    first, second = run()
    assert first["protocol"] == second["protocol"] == "2025-11-25"
    assert first["tools"] == second["tools"] == ["list_facts"]
    assert "AMBER_A" in first["text"] and "BLUE_B" not in first["text"]
    assert "BLUE_B" in second["text"] and "AMBER_A" not in second["text"]
    s.export_credentials.revoke(ga["id"])
    revoked, still_active = run()
    assert revoked["denied"] and not still_active["denied"]


async def test_server_restart_keeps_grants_and_revocation(tmp_path):
    with running_backend(tmp_path) as (_, s):
        s.memory.add_fact("PERSISTED_FACT", "restart.md")
        active = s.export_credentials.issue(Grant(name="Active", permissions=["facts"]))
        revoked = s.export_credentials.issue(Grant(name="Revoked", permissions=["facts"]))
        s.export_credentials.revoke(revoked["id"])
    with running_backend(tmp_path) as (url, _):
        async with httpx2.AsyncClient(
            headers={"Authorization": "Bearer " + active["token"]}, trust_env=False
        ) as http:
            async with Client(
                streamable_http_client(url + "/mcp-export/rpc", http_client=http),
                cache=None,
                read_timeout_seconds=5,
            ) as client:
                assert "PERSISTED_FACT" in str(await client.call_tool("list_facts", {}))
        async with httpx2.AsyncClient(trust_env=False) as http:
            result = await http.post(
                url + "/mcp-export/rpc",
                headers={"Authorization": "Bearer " + revoked["token"]},
                json={},
            )
            assert result.status_code == 401
