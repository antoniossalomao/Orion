"""Sessões reais: autenticação, isolamento, seleção estável e reinício com SQLite."""

import pytest
from fastapi.testclient import TestClient

from orion.app import create_app
from orion.config import Settings
from tests.fakes import FakeGateway, fala

TOKEN = "sessions-test-token-16chars"
AUTH = {"Authorization": f"Bearer {TOKEN}"}


def app_at(tmp_path, *, token=TOKEN, gateway=None):
    return create_app(
        Settings(data_dir=tmp_path, admin_token=token, jobs_enabled=False, _env_file=None),
        gateway_factory=lambda _: gateway,
    )


def test_duas_sessoes_troca_chat_e_reinicio(tmp_path):
    gw = FakeGateway(fala("Resposta A"), fala("Resposta B"), fala("Retomei A"))
    with TestClient(app_at(tmp_path, gateway=gw), base_url="http://127.0.0.1") as c:
        assert c.get("/sessoes", headers=AUTH).json() == {
            "sessoes": [],
            "total": 0,
            "ativa": None,
        }
        a = c.post("/sessoes", headers=AUTH, json={"titulo": "Primeira"}).json()
        assert a["ok"] and a["canal"] == "web" and a["mensagens"] == []
        assert c.post("/chat", headers=AUTH, json={"texto": "Pergunta A"}).status_code == 200
        b = c.post("/sessoes", headers=AUTH).json()
        assert a["sessao_id"] != b["sessao_id"]
        c.post("/chat", headers=AUTH, json={"texto": "Pergunta B"})
        assert c.get("/sessoes", headers=AUTH).json()["ativa"] == b["sessao_id"]
        r = c.post("/sessoes/ativar", headers=AUTH, json={"sessao_id": a["sessao_id"]})
        assert r.status_code == 200
        assert [m["content"] for m in r.json()["mensagens"]] == ["Pergunta A", "Resposta A"]
        assert r.json()["mensagens"][1]["provenance"] is not None
        c.post("/chat", headers=AUTH, json={"texto": "Continue A"})
        assert [m.text for m in c.app.state.orion.memory.history(b["sessao_id"])] == [
            "Pergunta B",
            "Resposta B",
        ]
        assert gw.chamadas[-1][-1]["content"] == "Continue A"
        assert any(m.get("content") == "Pergunta A" for m in gw.chamadas[-1])
        assert not any(m.get("content") == "Pergunta B" for m in gw.chamadas[-1])
    with TestClient(app_at(tmp_path), base_url="http://127.0.0.1") as c:
        data = c.get("/sessoes", headers=AUTH).json()
        assert data["ativa"] == a["sessao_id"] and data["total"] == 2
        assert sum(s["ativa"] for s in data["sessoes"]) == 1
        assert c.app.state.orion.memory.history(a["sessao_id"])[-1].text == "Retomei A"
        assert c.get("/capabilities").json()["features"]["sessions"] is True
        assert c.get("/capabilities").json()["features"]["history"] is False


@pytest.mark.parametrize("canal", ["telegram", "voz"])
def test_sessoes_de_outro_canal_nao_sao_listadas_nem_ativadas(tmp_path, canal):
    with TestClient(app_at(tmp_path), base_url="http://127.0.0.1") as c:
        web = c.post("/sessoes", headers=AUTH).json()["sessao_id"]
        outra = c.post("/sessoes", headers=AUTH, json={"canal": canal}).json()["sessao_id"]
        data = c.get("/sessoes", headers=AUTH).json()
        assert [s["sessao_id"] for s in data["sessoes"]] == [web]
        estrangeira = c.post("/sessoes/ativar", headers=AUTH, json={"sessao_id": outra})
        inexistente = c.post("/sessoes/ativar", headers=AUTH, json={"sessao_id": "0" * 32})
        assert estrangeira.status_code == inexistente.status_code == 404
        assert estrangeira.json() == inexistente.json()
        assert c.get("/sessoes", headers=AUTH).json()["ativa"] == web
        assert c.get("/sessoes", headers=AUTH, params={"canal": canal}).json()["ativa"] == outra
        assert (
            c.post(
                "/sessoes/ativar", headers=AUTH, json={"sessao_id": web, "canal": canal}
            ).status_code
            == 404
        )


@pytest.mark.parametrize("token,esperado", [("", 503), (TOKEN, 401)])
def test_todas_as_operacoes_exigem_admin(tmp_path, token, esperado):
    with TestClient(app_at(tmp_path, token=token), base_url="http://127.0.0.1") as c:
        for method, path, body in [
            ("GET", "/sessoes", None),
            ("POST", "/sessoes", None),
            ("POST", "/sessoes/ativar", {"sessao_id": "a" * 32}),
        ]:
            assert c.request(method, path, json=body).status_code == esperado
            assert (
                c.request(
                    method, path, json=body, headers={"Authorization": "Bearer errado"}
                ).status_code
                == esperado
            )
        assert c.app.state.orion.memory.list_sessions() == []


def test_legadas_e_arquivadas_sao_somente_leitura(tmp_path):
    with TestClient(app_at(tmp_path), base_url="http://127.0.0.1") as c:
        memory = c.app.state.orion.memory
        ativa = memory.new_session("web")
        legado, _ = memory.import_session("evento-antigo", "web", "Importada", 1)
        memory.import_message(legado, "msg-antiga", "user", "Texto antigo", 2)
        arquivada = memory.new_session("web")
        memory.archive_session(arquivada.id)
        memory.activate_session("web", ativa.id)
        data = c.get("/sessoes", headers=AUTH).json()
        assert {s["sessao_id"] for s in data["sessoes"] if s["somente_leitura"]} == {
            legado,
            arquivada.id,
        }
        for sid in (legado, arquivada.id):
            assert (
                c.post("/sessoes/ativar", headers=AUTH, json={"sessao_id": sid}).status_code == 409
            )
        assert memory.selected_session("web").id == ativa.id
        assert memory.history(legado)[0].text == "Texto antigo"


@pytest.mark.parametrize("canal", ["../web", "WEB", "", "x" * 33])
def test_canal_invalido_recusado_em_todas_as_operacoes(tmp_path, canal):
    with TestClient(app_at(tmp_path), base_url="http://127.0.0.1") as c:
        assert c.get("/sessoes", headers=AUTH, params={"canal": canal}).status_code == 422
        assert c.post("/sessoes", headers=AUTH, json={"canal": canal}).status_code == 422
        assert (
            c.post(
                "/sessoes/ativar", headers=AUTH, json={"sessao_id": "a" * 32, "canal": canal}
            ).status_code
            == 422
        )


def test_limites_e_snapshot_de_ativacao(tmp_path):
    with TestClient(app_at(tmp_path), base_url="http://127.0.0.1") as c:
        assert c.post("/sessoes", headers=AUTH, json={"titulo": "x" * 121}).status_code == 422
        a = c.post("/sessoes", headers=AUTH, json={"titulo": "  Teste  "}).json()
        assert a["titulo"] == "Teste"
        memory = c.app.state.orion.memory
        for i in range(60):
            memory.add_message(a["sessao_id"], "user", str(i))
        for limite in (0, 101):
            assert c.get("/sessoes", headers=AUTH, params={"limite": limite}).status_code == 422
        response = c.post(
            "/sessoes/ativar", headers=AUTH, json={"sessao_id": a["sessao_id"]}
        ).json()
        assert len(response["mensagens"]) == 50
        assert response["mensagens"][0]["content"] == "10"
        assert response["mensagens"][-1]["content"] == "59"
