import json
import socket
import threading
import time

import httpx2
import pytest
import uvicorn
from mcp import Client
from mcp.client.streamable_http import streamable_http_client

from orion.app import create_app
from orion.artifacts import Artifacts, Payload
from orion.config import Settings
from orion.export_credentials import Grant
from orion.memory.scope import data_scope
from orion.projects import Projects
from tests.projects.test_projects import TOKEN


@pytest.fixture
def export_backend(tmp_path):
    app = create_app(
        Settings(data_dir=tmp_path, admin_token=TOKEN, jobs_enabled=False, _env_file=None),
        gateway_factory=lambda _: None,
    )
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    url = f"http://127.0.0.1:{port}"
    server = uvicorn.Server(uvicorn.Config(app, host="127.0.0.1", port=port, log_level="error"))
    thread = threading.Thread(target=server.run, daemon=True)
    thread.start()
    for _ in range(100):
        if server.started:
            break
        time.sleep(0.05)
    assert server.started
    try:
        yield url, app.state.orion
    finally:
        server.should_exit = True
        thread.join(timeout=10)
        assert not thread.is_alive()


@pytest.mark.parametrize("mode,version", [("auto", "2026-07-28"), ("legacy", "2025-11-25")])
async def test_official_sdk_reads_scope_sources_results_and_denies_other_powers(
    export_backend, mode, version
):
    url, s = export_backend
    a, b = Projects(s.memory).create("A")["id"], Projects(s.memory).create("B")["id"]
    with data_scope(a, include_personal=False):
        s.memory.add_fact("AMBER assunto", "nota-a.md")
        s.memory.index_document("nota-a.md", "Nota A", "AMBER fonte indexada")
    with data_scope(b, include_personal=False):
        s.memory.add_fact("BLUE_PRIVATE", "nota-b.md")
    s.memory.add_fact("PERSONAL_PRIVATE", "personal.md")
    session = s.memory.new_session("web", project_id=a)
    artifact = Artifacts(s.memory).save(
        Payload(title="Resultado A", content="AMBER resultado", session_id=session.id), project_id=a
    )
    other = s.memory.new_session("web", project_id=b)
    foreign = Artifacts(s.memory).save(
        Payload(title="Resultado B", content="BLUE_PRIVATE", session_id=other.id), project_id=b
    )
    grant = s.export_credentials.issue(
        Grant(name="SDK A", project_id=a, permissions=["search", "facts", "sources", "artifacts"])
    )
    async with httpx2.AsyncClient(
        headers={"Authorization": "Bearer " + grant["token"]}, trust_env=False
    ) as http:
        async with Client(
            streamable_http_client(url + "/mcp-export/rpc", http_client=http),
            mode=mode,
            cache=None,
            read_timeout_seconds=5,
        ) as client:
            assert client.session.protocol_version == version
            assert {t.name for t in (await client.list_tools()).tools} == {
                "search_context",
                "list_facts",
                "read_source",
                "list_artifacts",
                "read_artifact",
            }
            result = await client.call_tool("search_context", {"query": "AMBER"})
            assert not result.is_error and "AMBER" in str(result)
            assert "BLUE_PRIVATE" not in str(result) and "PERSONAL_PRIVATE" not in str(result)
            resources = await client.list_resources()
            assert len(resources.resources) == 3
            result = await client.read_resource("orion://sources")
            doc_id = json.loads(result.contents[0].text)["items"][0]["id"]
            result = await client.call_tool("read_source", {"id": doc_id})
            assert "AMBER fonte indexada" in str(result)
            result = await client.call_tool("read_artifact", {"id": artifact["id"]})
            assert "AMBER resultado" in str(result)
            assert (await client.call_tool("read_artifact", {"id": foreign["id"]})).is_error
            assert (await client.call_tool("execute", {"command": "anything"})).is_error
            assert (await client.call_tool("read_source", {"path": "/etc/passwd"})).is_error
    async with httpx2.AsyncClient(trust_env=False) as http:
        assert (await http.get(url + "/mcp")).status_code == 404
        assert (await http.post(url + "/mcp-export/rpc", json={})).status_code == 401


async def test_permission_filters_discovery_and_direct_calls(export_backend):
    url, s = export_backend
    grant = s.export_credentials.issue(Grant(name="Facts only", permissions=["facts"]))
    async with httpx2.AsyncClient(
        headers={"Authorization": "Bearer " + grant["token"]}, trust_env=False
    ) as http:
        async with Client(
            streamable_http_client(url + "/mcp-export/rpc", http_client=http),
            cache=None,
            read_timeout_seconds=5,
        ) as client:
            assert [t.name for t in (await client.list_tools()).tools] == ["list_facts"]
            assert [str(r.uri) for r in (await client.list_resources()).resources] == [
                "orion://facts"
            ]
            assert (await client.call_tool("search_context", {"query": "private"})).is_error
