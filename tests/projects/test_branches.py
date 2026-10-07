from fastapi.testclient import TestClient

from orion.app import create_app
from orion.config import Settings
from orion.policy import Context, ToolCall
from tests.projects.test_projects import AUTH, TOKEN


def test_edited_request_versions_artifact_provenance_no_approval_replay(tmp_path):
    settings = Settings(data_dir=tmp_path, admin_token=TOKEN, jobs_enabled=False, _env_file=None)
    with TestClient(create_app(settings), base_url="http://127.0.0.1") as c:
        s = c.app.state.orion
        original = s.memory.new_session("web", "Original")
        s.memory.add_message(original.id, "user", "Antes")
        s.memory.add_message(original.id, "assistant", "Contexto anterior")
        message = s.memory.add_message(original.id, "user", "Pedido original")
        tool = s.memory.add_message(original.id, "tool", "resultado de ação")
        reply = s.memory.add_message(original.id, "assistant", "Resultado original")
        fact = s.memory.add_fact("Canário de aprovação", "ensaio")
        decision = s.policy.evaluate(
            ToolCall("esquecer_fato", {"id": fact.id}), Context(original.id)
        )
        s.policy.approvals.decide(decision.approval_id, True, channel="web", actor="admin")
        result = c.post(
            "/artifacts",
            headers=AUTH,
            json={
                "title": "Original",
                "session_id": original.id,
                "message_id": reply.id,
                "content": "Resultado original",
            },
        ).json()
        body = {"session_id": original.id, "text": "Pedido revisado"}
        assert c.post(f"/branches/messages/{message.id}", json=body).status_code == 401
        target = c.post(f"/branches/messages/{message.id}", json=body, headers=AUTH).json()
        assert target["requires_send"]
        new = target["session_id"]
        history = s.memory.history(new)
        assert [m.text for m in history] == ["Antes", "Contexto anterior"]
        assert tool.text not in str(history)
        assert s.memory.history(original.id)[2].text == "Pedido original"
        assert not s.policy.approvals.pending(new, include_approved=True)
        assert s.memory.facts()[0].text == "Canário de aprovação"
        paths = c.get(f"/branches/{new}", headers=AUTH).json()
        assert [p["artifacts"] for p in paths] == [1, 0]
        assert paths[1]["edited_text"] == "Pedido revisado"
        artifact = c.get(f"/artifacts/{result['id']}", headers=AUTH).json()
        assert artifact["session_id"] == original.id
        assert (
            c.get(f"/artifacts/{result['id']}/versions", headers=AUTH).json()[0]["message_id"]
            == reply.id
        )
        s.memory.add_message(new, "user", "Pedido revisado")
        assert c.get(f"/branches/{new}", headers=AUTH).json()[1]["edited_text"] is None
        assert c.post(f"/branches/messages/{reply.id}", json=body, headers=AUTH).status_code == 404
    with TestClient(create_app(settings), base_url="http://127.0.0.1") as c:
        assert len(c.get(f"/branches/{new}", headers=AUTH).json()) == 2
