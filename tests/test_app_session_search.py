import pytest
from fastapi.testclient import TestClient

from tests.test_app_sessions import AUTH, app_at


def test_busca_corpo_titulo_acentos_paginas_e_canais(tmp_path):
    with TestClient(app_at(tmp_path), base_url="http://127.0.0.1") as c:
        m = c.app.state.orion.memory
        for i in range(7):
            sid = m.new_session("web", f"Conversa {i}").id
            m.add_message(sid, "user", f"Uma mensagem com ornitorrinco {i}")
        estrangeira = m.new_session("telegram", "ornitorrinco").id
        m.add_message(estrangeira, "user", "Segredo de outro canal ornitorrinco")
        ids = []
        params = {"texto": "ornitorrinco", "limite": 3}
        while True:
            r = c.get("/sessoes/busca", headers=AUTH, params=params)
            assert r.status_code == 200
            d = r.json()
            assert d["total"] == 7
            assert all("ornitorrinco" in x["trecho"] for x in d["sessoes"])
            ids.extend(x["sessao_id"] for x in d["sessoes"])
            if not d["mais"]:
                break
            params["offset"] = d["proximo_offset"]
        assert len(set(ids)) == 7 and estrangeira not in ids
        sid = m.new_session("web", "Cérebro e memória").id
        d = c.get("/sessoes/busca", headers=AUTH, params={"texto": "cerebro"}).json()
        assert d["sessoes"][0]["sessao_id"] == sid
        c.patch(f"/sessoes/{sid}", headers=AUTH, json={"titulo": "Novo título"})
        assert (
            c.get("/sessoes/busca", headers=AUTH, params={"texto": "cerebro"}).json()["total"] == 0
        )
        assert (
            c.get("/sessoes/busca", headers=AUTH, params={"texto": "inexistente"}).json()["total"]
            == 0
        )
        assert (
            c.get(
                "/sessoes/busca", headers=AUTH, params={"texto": "'; DROP TABLE sessions;--"}
            ).status_code
            == 200
        )
    with TestClient(app_at(tmp_path), base_url="http://127.0.0.1") as c:
        assert (
            c.get("/sessoes/busca", headers=AUTH, params={"texto": "ornitorrinco"}).json()["total"]
            == 7
        )
        assert (
            c.get("/sessoes/busca", headers=AUTH, params={"texto": "titulo"}).json()["total"] == 1
        )


@pytest.mark.parametrize("token,status", [("", 503), ("sessions-test-token-16chars", 401)])
def test_busca_exige_admin(tmp_path, token, status):
    with TestClient(app_at(tmp_path, token=token), base_url="http://127.0.0.1") as c:
        assert c.get("/sessoes/busca", params={"texto": "termo"}).status_code == status
        assert not c.app.state.orion.memory.list_sessions()
