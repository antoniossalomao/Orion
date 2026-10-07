from fastapi.testclient import TestClient

from orion.app import create_app
from orion.config import Settings
from orion.memory.scope import data_scope
from orion.projects import Projects
from tests.projects.test_projects import AUTH, TOKEN


def test_facts_search_edit_forget_approval_scope_revocation_and_restart(tmp_path):
    settings = Settings(data_dir=tmp_path, admin_token=TOKEN, jobs_enabled=False, _env_file=None)
    with TestClient(create_app(settings), base_url="http://127.0.0.1") as client:
        state = client.app.state.orion
        a, b = [Projects(state.memory).create(n)["id"] for n in ("A", "B")]
        with data_scope(a, include_personal=False):
            fact = state.memory.add_fact("AMBER original", "nota do projeto")
        sa = state.memory.new_session("web", project_id=a)
        sb = state.memory.new_session("web", project_id=b)
        query = {"project_id": a}
        assert client.get("/facts").status_code == 401
        assert client.get("/facts", headers=AUTH).json() == []
        assert (
            client.get("/facts", params={**query, "query": "amber"}, headers=AUTH).json()[0][
                "source"
            ]
            == "nota do projeto"
        )
        assert (
            client.get(f"/facts/{fact.id}", params={"project_id": b}, headers=AUTH).status_code
            == 404
        )
        body = {"action": "edit", "text": "AMBER revisado", "session_id": sa.id}
        assert (
            client.post(
                f"/facts/{fact.id}/review",
                params=query,
                json={**body, "session_id": sb.id},
                headers=AUTH,
            ).status_code
            == 409
        )
        pending = client.post(
            f"/facts/{fact.id}/review", params=query, json=body, headers=AUTH
        ).json()
        aid = pending["approval_id"]
        old_forget = client.post(
            f"/facts/{fact.id}/review",
            params=query,
            json={"action": "forget", "session_id": sa.id},
            headers=AUTH,
        ).json()["approval_id"]
        client.post(f"/approvals/{old_forget}/decide", json={"approved": True}, headers=AUTH)
        assert (
            client.get(f"/facts/{fact.id}", params=query, headers=AUTH).json()["text"]
            == "AMBER original"
        )
        assert (
            client.post(
                f"/facts/{fact.id}/resume", params=query, json={"approval_id": aid}, headers=AUTH
            ).status_code
            == 409
        )
        client.post(f"/approvals/{aid}/decide", json={"approved": True}, headers=AUTH)
        # Solicitar outra revisão não consome a autorização já concedida.
        duplicate = client.post(
            f"/facts/{fact.id}/review", params=query, json=body, headers=AUTH
        ).json()
        assert state.policy.approvals.get(aid).status == "approved"
        result = client.post(
            f"/facts/{fact.id}/resume", params=query, json={"approval_id": aid}, headers=AUTH
        )
        assert result.status_code == 200 and result.json()["fact"]["source"] == fact.source
        assert state.policy.approvals.get(old_forget).status == "denied"
        assert (
            client.post(
                f"/facts/{fact.id}/resume",
                params=query,
                json={"approval_id": old_forget},
                headers=AUTH,
            ).status_code
            == 409
        )
        assert (
            client.post(
                f"/facts/{fact.id}/resume", params=query, json={"approval_id": aid}, headers=AUTH
            ).status_code
            == 409
        )
        client.post(
            f"/approvals/{duplicate['approval_id']}/decide", json={"approved": False}, headers=AUTH
        )
        pending = client.post(
            f"/facts/{fact.id}/review",
            params=query,
            json={"action": "forget", "session_id": sa.id},
            headers=AUTH,
        ).json()
        client.post(
            f"/approvals/{pending['approval_id']}/decide", json={"approved": True}, headers=AUTH
        )
        assert (
            client.patch(
                f"/projects/{a}", json={"instructions": "revisão de permissões"}, headers=AUTH
            ).status_code
            == 200
        )
        assert (
            client.post(
                f"/facts/{fact.id}/resume",
                params=query,
                json={"approval_id": pending["approval_id"]},
                headers=AUTH,
            ).status_code
            == 409
        )
        pending = client.post(
            f"/facts/{fact.id}/review",
            params=query,
            json={"action": "forget", "session_id": sa.id},
            headers=AUTH,
        ).json()
        client.post(
            f"/approvals/{pending['approval_id']}/decide", json={"approved": True}, headers=AUTH
        )
        assert client.post(
            f"/facts/{fact.id}/resume",
            params=query,
            json={"approval_id": pending["approval_id"]},
            headers=AUTH,
        ).json()["forgotten"]
        with data_scope(a, include_personal=False):
            assert not state.memory.search("AMBER", kinds=["fact"])
    with TestClient(create_app(settings), base_url="http://127.0.0.1") as client:
        assert client.get("/facts", params=query, headers=AUTH).json() == []
