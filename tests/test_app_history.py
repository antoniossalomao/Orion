"""Histórico real, paginação estável e limpeza sem destruir o registro permanente."""

import pytest
from fastapi.testclient import TestClient

from tests.fakes import FakeGateway, fala
from tests.test_app_sessions import AUTH, app_at


@pytest.mark.parametrize(
    "method,path", [("GET", "/historico"), ("DELETE", "/historico"), ("GET", "/exportar")]
)
@pytest.mark.parametrize("token,status", [("", 503), ("sessions-test-token-16chars", 401)])
def test_protegido_sem_criar_sessao(tmp_path, method, path, token, status):
    with TestClient(app_at(tmp_path, token=token), base_url="http://127.0.0.1") as c:
        assert c.request(method, path).status_code == status
        assert not c.app.state.orion.memory.list_sessions()


def test_paginas_exportacao_e_limpeza_persistente(tmp_path):
    with TestClient(app_at(tmp_path), base_url="http://127.0.0.1") as c:
        assert c.get("/historico", headers=AUTH).json()["mensagens"] == []
        assert c.get("/exportar", headers=AUTH).json()["total_msgs"] == 0
        assert c.delete("/historico", headers=AUTH).status_code == 200
        m = c.app.state.orion.memory
        assert not m.list_sessions()
        a = m.new_session("web")
        b = m.new_session("web")
        m.add_message(b.id, "user", "outra conversa")
        for i in range(205):
            m.add_message(
                a.id,
                "assistant" if i % 2 else "user",
                f"mensagem {i:03}",
                provenance={"memoria": [{"fonte": "documento.md"}]} if i % 2 else None,
            )
        params = {"sessao": a.id, "limite": 37}
        paginas = []
        while True:
            r = c.get("/historico", headers=AUTH, params=params)
            assert r.status_code == 200
            d = r.json()
            assert d["total"] == 205
            paginas.insert(0, d["mensagens"])
            if not d["mais"]:
                break
            params["antes"] = d["proximo_antes"]
        msgs = [msg for page in paginas for msg in page]
        assert [x["content"] for x in msgs] == [f"mensagem {i:03}" for i in range(205)]
        assert len({x["id"] for x in msgs}) == 205
        assert msgs[1]["provenance"]["memoria"][0]["fonte"] == "documento.md"
        assert msgs[0]["timestamp"].endswith("+00:00")
        export = c.get("/exportar", headers=AUTH, params={"sessao": a.id}).json()
        assert export["total_msgs"] == 205
        assert export["markdown"].index("mensagem 000") < export["markdown"].index("mensagem 204")
        assert "documento.md" in export["markdown"] and "outra conversa" not in export["markdown"]
        assert c.delete("/historico", headers=AUTH, params={"sessao": a.id}).status_code == 200
        assert c.get("/historico", headers=AUTH, params={"sessao": a.id}).json()["total"] == 0
        assert len(m.history(a.id, limit=500)) == 205
        assert m.history(b.id)[0].text == "outra conversa"
        assert c.get("/exportar", headers=AUTH, params={"sessao": a.id}).json()["total_msgs"] == 0
        assert (
            c.get("/exportar", headers=AUTH, params={"sessao": a.id, "completo": True}).json()[
                "total_msgs"
            ]
            == 205
        )
        m.activate_session(a.id)
    with TestClient(app_at(tmp_path), base_url="http://127.0.0.1") as c:
        assert c.get("/historico", headers=AUTH).json()["total"] == 0
        assert (
            c.post("/sessoes/ativar", headers=AUTH, json={"sessao_id": a.id}).json()["mensagens"]
            == []
        )
        assert c.get("/historico", headers=AUTH, params={"completo": True}).json()["total"] == 205


def test_importadas_e_isolamento(tmp_path):
    with TestClient(app_at(tmp_path), base_url="http://127.0.0.1") as c:
        m = c.app.state.orion.memory
        sid, _ = m.import_session("antiga", "web", "Importada", 1)
        m.import_message(sid, "mensagem", "user", "antiga sem metadados", 2)
        estrangeira = m.new_session("telegram").id
        for path in ("/historico", "/exportar"):
            r = c.get(path, headers=AUTH, params={"sessao": sid})
            assert r.status_code == 200
            assert c.get(path, headers=AUTH, params={"sessao": estrangeira}).status_code == 404
        hist = c.get("/historico", headers=AUTH, params={"sessao": sid}).json()
        assert hist["somente_leitura"] and hist["mensagens"][0]["provenance"] is None
        assert hist["mensagens"][0]["timestamp"] == "1970-01-01T00:00:02+00:00"
        assert c.delete("/historico", headers=AUTH, params={"sessao": sid}).status_code == 409
        assert (
            c.delete("/historico", headers=AUTH, params={"sessao": estrangeira}).status_code == 404
        )


def test_proximo_turno_sem_contexto_antigo(tmp_path):
    gw = FakeGateway(fala("primeira"), fala("segunda"))
    with TestClient(app_at(tmp_path, gateway=gw), base_url="http://127.0.0.1") as c:
        c.post("/chat", headers=AUTH, json={"texto": "texto anterior"})
        assert c.delete("/historico", headers=AUTH).status_code == 200
        c.post("/chat", headers=AUTH, json={"texto": "novo contexto"})
        assert not any(x.get("content") == "texto anterior" for x in gw.chamadas[-1])
        assert c.get("/historico", headers=AUTH).json()["total"] == 2
        assert c.get("/historico", headers=AUTH, params={"completo": True}).json()["total"] == 4


@pytest.mark.parametrize("gateway", [False, True])
def test_limpeza_com_aprovacao_pendente_recusada_pela_api(tmp_path, gateway):
    with TestClient(
        app_at(tmp_path, gateway=FakeGateway(fala("ok")) if gateway else None),
        base_url="http://127.0.0.1",
    ) as c:
        state = c.app.state.orion
        sid = state.memory.new_session("web").id
        state.memory.add_message(sid, "user", "pedido ainda aguardando")
        a = state.policy.approvals.request(sid, "esquecer_fato", {"id": 1}, "teste")
        assert c.delete("/historico", headers=AUTH).status_code == 409
        state.policy.approvals.decide(a.id, True, channel="web", actor="teste")
        assert c.delete("/historico", headers=AUTH).status_code == 409
        assert len(state.memory.context_history(sid)) == 1
