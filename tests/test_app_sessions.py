"""Seleção persistente da conversa ativa por canal (o resto das rotas: test_app_sessoes.py)."""

from fastapi.testclient import TestClient

from orion.app import create_app
from orion.config import Settings
from tests.fakes import FakeGateway, fala

TOKEN = "sessions-test-token-16chars"
AUTH = {"Authorization": f"Bearer {TOKEN}"}


def app_at(tmp_path, *, gateway=None):
    return create_app(
        Settings(data_dir=tmp_path, admin_token=TOKEN, jobs_enabled=False, _env_file=None),
        gateway_factory=lambda _: gateway,
    )


def test_conversa_ativa_sobrevive_ao_reinicio_e_chat_continua_nela(tmp_path):
    gw = FakeGateway(fala("Resposta A"), fala("Resposta B"), fala("Retomei A"))
    with TestClient(app_at(tmp_path, gateway=gw), base_url="http://127.0.0.1") as c:
        a = c.app.state.orion.memory.active_session("web").id
        assert c.post("/chat", headers=AUTH, json={"texto": "Pergunta A"}).status_code == 200
        b = c.post("/sessoes", headers=AUTH).json()["sessao_id"]
        assert a != b
        c.post("/chat", headers=AUTH, json={"texto": "Pergunta B"})
        r = c.post("/sessoes/ativar", headers=AUTH, json={"sessao_id": a})
        assert r.status_code == 200 and r.json()["sessao_id"] == a
        memory = c.app.state.orion.memory
        # resposta atrasada em B não devolve a seleção para B
        memory.add_message(b, "assistant", "atrasada")
        assert c.get("/sessoes", headers=AUTH).json()["ativa"] == a
    with TestClient(app_at(tmp_path, gateway=gw), base_url="http://127.0.0.1") as c:
        assert c.get("/sessoes", headers=AUTH).json()["ativa"] == a
        c.post("/chat", headers=AUTH, json={"texto": "Continue A"})
        assert c.app.state.orion.memory.history(a)[-1].text == "Retomei A"


def test_apagar_a_ativa_nao_reabre_outra_por_conta_propria(tmp_path):
    with TestClient(app_at(tmp_path), base_url="http://127.0.0.1") as c:
        memory = c.app.state.orion.memory
        antiga = memory.new_session("web")
        atual = memory.new_session("web")
        assert c.delete(f"/sessoes/{atual.id}", headers=AUTH).status_code == 200
        assert memory.selected_session("web") is None
        assert memory.active_session("web").id not in {antiga.id, atual.id}
