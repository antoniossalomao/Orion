"""E4.1 e E4.5 pela API: mover fato de projeto, validade, substituição e os avisos do sono."""

import asyncio

from fastapi.testclient import TestClient

from orion.app import create_app
from orion.config import Settings
from orion.memory.ops import Operations
from orion.memory.scope import data_scope
from orion.memory.sleep import SleepCycle
from orion.projects import Projects
from tests.projects.test_projects import AUTH, TOKEN


def cliente(tmp_path):
    s = Settings(data_dir=tmp_path, admin_token=TOKEN, jobs_enabled=False, _env_file=None)
    return TestClient(create_app(s), base_url="http://127.0.0.1")


def test_mover_fato_de_projeto_isola_e_respeita_o_escopo(tmp_path):
    with cliente(tmp_path) as c:
        m = c.app.state.orion.memory
        a, b = [Projects(m).create(n)["id"] for n in ("A", "B")]
        fato = m.add_fact("Prefiro café sem açúcar", "m")
        r = c.post(f"/facts/{fato.id}/move", json={"project_id": a}, headers=AUTH)
        assert r.status_code == 200 and r.json()["project_id"] == a
        assert c.get("/facts", headers=AUTH).json() == []  # saiu do pessoal
        assert [
            f["id"] for f in c.get("/facts", params={"project_id": a}, headers=AUTH).json()
        ] == [fato.id]
        assert c.get("/facts", params={"project_id": b}, headers=AUTH).json() == []
        # só se move a partir do escopo em que o fato está
        assert (
            c.post(f"/facts/{fato.id}/move", json={"project_id": b}, headers=AUTH).status_code
            == 404
        )
        r = c.post(
            f"/facts/{fato.id}/move",
            params={"project_id": a},
            json={"project_id": None},
            headers=AUTH,
        )
        assert r.json()["project_id"] is None
        assert (
            c.post(
                f"/facts/{fato.id}/move", json={"project_id": "f" * 32}, headers=AUTH
            ).status_code
            == 409
        )  # projeto que não existe
        assert c.post(f"/facts/{fato.id}/move", json={}, headers=AUTH).status_code == 422
        assert c.post(f"/facts/{fato.id}/move", json={"project_id": a}).status_code == 401


def test_apagar_o_repetido_e_um_clique_do_antonio(tmp_path):
    with cliente(tmp_path) as c:
        m = c.app.state.orion.memory
        a = m.add_fact("Antônio mora em Marília", "x")
        assert c.delete(f"/facts/{a.id}", headers=AUTH).json() == {"ok": True}
        assert m.get_fact(a.id) is None
        assert c.delete(f"/facts/{a.id}", headers=AUTH).status_code == 404


def test_validade_e_substituicao_pela_api(tmp_path):
    with cliente(tmp_path) as c:
        m = c.app.state.orion.memory
        velho, novo = m.add_fact("O plano é gratuito", "m"), m.add_fact("O plano é pago", "m")
        r = c.post(f"/facts/{velho.id}/validity", json={"valid_until": 1.0}, headers=AUTH).json()
        assert r["valid_until"] == 1.0 and r["old"] is True
        r = c.post(f"/facts/{velho.id}/validity", json={"valid_until": None}, headers=AUTH).json()
        assert r["valid_until"] is None and r["old"] is False
        r = c.post(f"/facts/{velho.id}/supersede", json={"by": novo.id}, headers=AUTH).json()
        assert r["superseded_by"] == novo.id and r["old"] is True
        assert (
            c.post(f"/facts/{velho.id}/supersede", json={"by": velho.id}, headers=AUTH).status_code
            == 422
        )
        assert (
            c.post(f"/facts/{velho.id}/supersede", json={"by": 999}, headers=AUTH).status_code
            == 404
        )
        listados = {f["id"]: f for f in c.get("/facts", headers=AUTH).json()}
        assert listados[velho.id]["old"] is True and listados[novo.id]["old"] is False


def test_sono_avisa_contradicao_e_o_antonio_decide_pelos_botoes(tmp_path):
    with cliente(tmp_path) as c:
        s = c.app.state.orion
        m = s.memory
        velho = m.add_fact("Antônio mora em Marília desde 2020", "m")
        novo = m.add_fact("Antônio mora em Bauru desde 2026", "m")
        ciclo = SleepCycle(m, Operations(m), None, at="03:00")
        r = asyncio.run(ciclo.run())
        assert r.conflitos == 1
        avisos = [n for n in s.ops.pending_notifications() if n["kind"] == "sono"]
        assert [n["ref"] for n in avisos] == [f"sono:conflito:{velho.id}:{novo.id}"]
        pend = c.get("/facts/conflicts", headers=AUTH).json()
        assert [(p["older"]["id"], p["newer"]["id"]) for p in pend] == [(velho.id, novo.id)]
        corpo = {"older_id": velho.id, "newer_id": novo.id, "decision": "novo"}
        assert c.post("/facts/conflicts/resolve", json=corpo, headers=AUTH).json() == {"ok": True}
        assert m.get_fact(velho.id).superseded_by == novo.id
        assert c.get("/facts/conflicts", headers=AUTH).json() == []
        assert c.post("/facts/conflicts/resolve", json=corpo, headers=AUTH).status_code == 404
        m.counter_set("sono:ultimo", 0)
        assert asyncio.run(ciclo.run()).conflitos == 0  # o mesmo par nunca é avisado de novo


def test_conflitos_de_um_projeto_nao_aparecem_no_pessoal(tmp_path):
    with cliente(tmp_path) as c:
        m = c.app.state.orion.memory
        a = Projects(m).create("A")["id"]
        with data_scope(a, include_personal=False):
            x = m.add_fact("O prazo do projeto é março de 2026", "m")
            y = m.add_fact("O prazo do projeto é junho de 2026", "m")
        m.record_conflict(x.id, y.id)
        assert c.get("/facts/conflicts", headers=AUTH).json() == []
        achados = c.get("/facts/conflicts", params={"project_id": a}, headers=AUTH).json()
        assert len(achados) == 1


def test_resumo_da_conversa_pela_api(tmp_path):
    with cliente(tmp_path) as c:
        m = c.app.state.orion.memory
        sessao = m.active_session("web")
        url = f"/sessoes/{sessao.id}/resumo"
        assert c.get(url, headers=AUTH).json() == {"resumo": None}
        m.set_summary(sessao.id, 7, "- decidiram usar SQLite", "local:qwen3.5:4b")
        r = c.get(url, headers=AUTH).json()["resumo"]
        assert r["texto"] == "- decidiram usar SQLite" and r["ate_mensagem"] == 7 and r["gerado"]
        assert c.delete(url, headers=AUTH).json() == {"ok": True}
        assert c.get(url, headers=AUTH).json() == {"resumo": None}
        assert c.get("/sessoes/" + "a" * 32 + "/resumo", headers=AUTH).status_code == 404
        assert c.get(url).status_code == 401
