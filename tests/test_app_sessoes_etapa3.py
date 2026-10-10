"""E3: conversas arquivadas, filtro por projeto e apagar conversa arquivada."""

from fastapi.testclient import TestClient

from tests.test_app_sessions import AUTH, app_at


def _cliente(tmp_path):
    return TestClient(app_at(tmp_path), base_url="http://127.0.0.1")


def test_lista_so_arquivadas_e_filtra_por_projeto(tmp_path):
    with _cliente(tmp_path) as c:
        m = c.app.state.orion.memory
        proj = c.post("/projects", headers=AUTH, json={"name": "Estágio"}).json()["id"]
        a = m.new_session("web", "Ativa").id
        b = m.new_session("web", "Arquivada").id
        d = m.new_session("web", "Do projeto", project_id=proj).id
        c.patch(f"/sessoes/{b}", headers=AUTH, json={"arquivada": True})

        def ids(**params):
            return {
                s["sessao_id"]
                for s in c.get("/sessoes", headers=AUTH, params=params).json()["sessoes"]
            }

        assert ids() == {a, b, d}
        assert ids(arquivadas="true") == {b}
        assert ids(projeto=proj) == {d}
        assert ids(projeto="nenhum") == {a, b}
        assert ids(projeto="nenhum", arquivadas="true") == {b}


def test_busca_so_entre_arquivadas(tmp_path):
    with _cliente(tmp_path) as c:
        m = c.app.state.orion.memory
        a = m.new_session("web", "Orçamento ativo").id
        b = m.new_session("web", "Orçamento velho").id
        c.patch(f"/sessoes/{b}", headers=AUTH, json={"arquivada": True})
        todos = c.get("/sessoes/busca", headers=AUTH, params={"texto": "orçamento"}).json()
        assert {s["sessao_id"] for s in todos["sessoes"]} == {a, b}
        so_arq = c.get(
            "/sessoes/busca", headers=AUTH, params={"texto": "orçamento", "arquivadas": "true"}
        ).json()
        assert [s["sessao_id"] for s in so_arq["sessoes"]] == [b] and so_arq["total"] == 1


def test_apagar_exige_arquivada_e_leva_as_mensagens(tmp_path):
    with _cliente(tmp_path) as c:
        m = c.app.state.orion.memory
        sid = m.new_session("web", "Descartável").id
        m.add_message(sid, "user", "texto único zebra")
        assert c.delete(f"/sessoes/{sid}", headers=AUTH).status_code == 409  # não arquivada
        c.patch(f"/sessoes/{sid}", headers=AUTH, json={"arquivada": True})
        assert c.delete(f"/sessoes/{sid}", headers=AUTH).status_code == 200
        assert m.get_session(sid) is None and m.history(sid) == []
        assert c.get("/sessoes/busca", headers=AUTH, params={"texto": "zebra"}).json()["total"] == 0
        assert c.delete(f"/sessoes/{sid}", headers=AUTH).status_code == 404


def test_apagar_recusa_conversa_com_ramo_ou_importada(tmp_path):
    with _cliente(tmp_path) as c:
        state = c.app.state.orion
        m = state.memory
        sid = m.new_session("web", "Com artefato").id
        with m._tx() as con:  # artefato apontando para a conversa
            con.execute(
                "INSERT INTO artifacts(id, session_id, title, kind, created_at, updated_at)"
                " VALUES ('a1', ?, 't', 'code', 1, 1)",
                (sid,),
            )
        c.patch(f"/sessoes/{sid}", headers=AUTH, json={"arquivada": True})
        r = c.delete(f"/sessoes/{sid}", headers=AUTH)
        assert r.status_code == 409 and "artefatos" in r.json()["detail"]
        assert m.get_session(sid) is not None
        antiga, _ = m.import_session("antiga", "web", "Antiga", 1)
        assert c.delete(f"/sessoes/{antiga}", headers=AUTH).status_code == 409


def test_apagar_recusa_com_aprovacao_pendente_e_outro_canal(tmp_path):
    with _cliente(tmp_path) as c:
        state = c.app.state.orion
        sid = state.memory.new_session("web").id
        c.patch(f"/sessoes/{sid}", headers=AUTH, json={"arquivada": True})
        state.policy.approvals.request(sid, "esquecer_fato", {"id": 1}, "teste")
        assert c.delete(f"/sessoes/{sid}", headers=AUTH).status_code == 409
        outra = state.memory.new_session("telegram").id
        assert c.delete(f"/sessoes/{outra}", headers=AUTH).status_code == 404
