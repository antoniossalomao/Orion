import pytest
from fastapi.testclient import TestClient

from tests.test_app_sessions import AUTH, app_at


def test_metadados_persistem_arquivar_preserva_mensagens(tmp_path):
    with TestClient(app_at(tmp_path), base_url="http://127.0.0.1") as c:
        sid = c.post("/sessoes", headers=AUTH).json()["sessao_id"]
        state = c.app.state.orion
        state.memory.add_message(sid, "user", "Mensagem preservada")
        r = c.patch(f"/sessoes/{sid}", headers=AUTH, json={"titulo": "Nome novo", "favorita": True})
        assert r.status_code == 200
        assert r.json()["titulo"] == "Nome novo" and r.json()["favorita"]
    with TestClient(app_at(tmp_path), base_url="http://127.0.0.1") as c:
        state = c.app.state.orion
        session = state.memory.get_session(sid)
        assert session.title == "Nome novo" and session.favorite
        c.post("/sessoes/ativar", headers=AUTH, json={"sessao_id": sid})
        assert state.memory.selected_session("web").favorite
        assert c.patch(f"/sessoes/{sid}", headers=AUTH, json={"arquivada": True}).json()[
            "arquivada"
        ]
        assert state.memory.selected_session("web") is None
        assert state.memory.history(sid)[0].text == "Mensagem preservada"
        assert c.get("/historico", headers=AUTH, params={"sessao": sid}).json()["somente_leitura"]
        assert not c.patch(f"/sessoes/{sid}", headers=AUTH, json={"arquivada": False}).json()[
            "arquivada"
        ]
        assert state.memory.selected_session("web") is None
        assert c.post("/sessoes/ativar", headers=AUTH, json={"sessao_id": sid}).status_code == 200


@pytest.mark.parametrize("token,status", [("", 503), ("sessions-test-token-16chars", 401)])
def test_gestao_exige_admin(tmp_path, token, status):
    with TestClient(app_at(tmp_path, token=token), base_url="http://127.0.0.1") as c:
        sid = c.app.state.orion.memory.new_session("web").id
        assert c.patch(f"/sessoes/{sid}", json={"titulo": "Indevido"}).status_code == status
        assert c.app.state.orion.memory.get_session(sid).title is None


def test_isolamento_importadas_e_aprovacoes(tmp_path):
    with TestClient(app_at(tmp_path), base_url="http://127.0.0.1") as c:
        state = c.app.state.orion
        sid = state.memory.new_session("telegram").id
        assert c.patch(f"/sessoes/{sid}", headers=AUTH, json={"favorita": True}).status_code == 404
        antiga, _ = state.memory.import_session("antiga", "web", "Antiga", 1)
        assert (
            c.patch(f"/sessoes/{antiga}", headers=AUTH, json={"titulo": "Outro"}).status_code == 409
        )
        sid = state.memory.new_session("web").id
        state.policy.approvals.request(sid, "esquecer_fato", {"id": 1}, "teste")
        assert c.patch(f"/sessoes/{sid}", headers=AUTH, json={"arquivada": True}).status_code == 409
        assert not state.memory.get_session(sid).archived
        assert c.patch(f"/sessoes/{sid}", headers=AUTH, json={"titulo": " "}).status_code == 409
