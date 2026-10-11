"""E3.5: editar o pedido cria uma nova versão e refaz o turno; o resto fica guardado, fora do contexto."""

import pytest
from fastapi.testclient import TestClient

from orion.memory import MemoryStore
from tests.fakes import FakeGateway, fala
from tests.test_app_sessions import AUTH, app_at


def _store(tmp_path):
    return MemoryStore(tmp_path / "m.db")


def test_supersede_esconde_o_pedido_e_o_que_veio_depois(tmp_path):
    m = _store(tmp_path)
    s = m.new_session("web").id
    m.add_message(s, "user", "primeiro")
    m.add_message(s, "assistant", "resposta 1")
    p2 = m.add_message(s, "user", "segundo")
    m.add_message(s, "assistant", "resposta 2")
    raiz = m.supersede_from(s, p2.id)
    assert raiz == p2.id
    nova = m.add_message(s, "user", "segundo, melhor", version_of=raiz)
    assert [x.text for x in m.history(s)] == ["primeiro", "resposta 1", "segundo, melhor"]
    assert [x.text for x in m.context_history(s)] == ["primeiro", "resposta 1", "segundo, melhor"]
    msgs, total, _ = m.history_page(s)
    assert total == 3 and [x.text for x in msgs][-1] == "segundo, melhor"
    assert [v.text for v in m.message_versions(s, raiz)] == ["segundo", "segundo, melhor"]
    # editar a versão nova continua apontando para a primeira
    assert m.supersede_from(s, nova.id) == raiz


def test_supersede_recusa_o_que_nao_e_pedido_editavel(tmp_path):
    m = _store(tmp_path)
    s = m.new_session("web").id
    outra = m.new_session("web").id
    resp = m.add_message(s, "assistant", "r")
    ext = m.add_message(s, "user", "[CONTEXTO EXTERNO SELECIONADO: x]")
    ped = m.add_message(s, "user", "pedido")
    m.add_message(outra, "user", "de outra")
    for mid in (resp.id, ext.id, 99999):
        with pytest.raises(KeyError):
            m.supersede_from(s, mid)
    with pytest.raises(KeyError):
        m.supersede_from(s, m.history(outra)[0].id)
    m.supersede_from(s, ped.id)
    with pytest.raises(KeyError):  # já substituído
        m.supersede_from(s, ped.id)
    m.archive_session(s)
    with pytest.raises(ValueError):
        m.supersede_from(s, ped.id)


def test_marcador_de_contexto_externo_vai_junto_com_o_pedido(tmp_path):
    m = _store(tmp_path)
    s = m.new_session("web").id
    m.add_message(s, "user", "antes")
    m.add_message(s, "user", "[CONTEXTO EXTERNO SELECIONADO: referência]\n{}")
    ped = m.add_message(s, "user", "pedido com contexto")
    m.supersede_from(s, ped.id)
    assert [x.text for x in m.history(s)] == ["antes"]


def test_rota_editar_refaz_o_turno_e_o_historico_mostra_as_versoes(tmp_path):
    gw = FakeGateway(fala("Resposta velha."), fala("Resposta nova."))
    with TestClient(app_at(tmp_path, gateway=gw), base_url="http://127.0.0.1") as c:
        memory = c.app.state.orion.memory
        sid = memory.active_session("web").id
        c.post("/chat", headers=AUTH, json={"texto": "pergunta torta"})
        pedido = next(x for x in memory.history(sid) if x.role == "user")
        r = c.post(f"/historico/{pedido.id}/editar", headers=AUTH, json={"texto": "pergunta certa"})
        assert r.status_code == 200 and "Resposta nova." in r.text
        h = c.get("/historico", headers=AUTH, params={"sessao": sid}).json()
        assert [(m["role"], m["content"]) for m in h["mensagens"]] == [
            ("user", "pergunta certa"),
            ("assistant", "Resposta nova."),
        ]
        versoes = h["mensagens"][0]["versoes"]
        assert [v["texto"] for v in versoes] == ["pergunta torta", "pergunta certa"]
        assert [v["atual"] for v in versoes] == [False, True]
        # o modelo não viu o pedido velho nem a resposta velha no segundo turno
        assert all("pergunta torta" not in str(x) for x in gw.chamadas[1])
        assert "Resposta velha." not in str(gw.chamadas[1])
        # nada foi apagado do banco
        assert memory.query("SELECT count(*) FROM messages WHERE superseded=1")[0][0] == 2


def test_rota_editar_valida_e_bloqueia(tmp_path):
    gw = FakeGateway(fala("ok"), fala("ok2"))
    with TestClient(app_at(tmp_path, gateway=gw), base_url="http://127.0.0.1") as c:
        state = c.app.state.orion
        sid = state.memory.active_session("web").id
        c.post("/chat", headers=AUTH, json={"texto": "oi"})
        msgs = state.memory.history(sid)
        resposta = next(m for m in msgs if m.role == "assistant")
        pedido = next(m for m in msgs if m.role == "user")
        corpo = {"texto": "novo"}
        assert c.post(f"/historico/{pedido.id}/editar", json=corpo).status_code == 401
        # só pedido do Antônio, existente, desta conversa
        assert (
            c.post(f"/historico/{resposta.id}/editar", headers=AUTH, json=corpo).status_code == 404
        )
        assert c.post("/historico/99999/editar", headers=AUTH, json=corpo).status_code == 404
        assert (
            c.post(f"/historico/{pedido.id}/editar", headers=AUTH, json={"texto": ""}).status_code
            == 422
        )
        # aprovação pendente bloqueia
        state.policy.approvals.request(sid, "esquecer_fato", {"id": 1}, "teste")
        assert c.post(f"/historico/{pedido.id}/editar", headers=AUTH, json=corpo).status_code == 409


def test_banco_v17_sobe_para_v18_sem_perder_mensagens(tmp_path):
    import sqlite3

    from orion.memory import schema

    caminho = tmp_path / "v17.db"
    c = sqlite3.connect(caminho)
    antigas = [getattr(schema, f"DDL_V{n}") for n in range(1, 18)]
    c.executescript("".join(antigas))
    c.execute("INSERT INTO meta VALUES ('schema_version', '17')")
    c.execute(
        "INSERT INTO sessions(id, channel, created_at, last_active_at) VALUES ('s1','web',1,1)"
    )
    c.execute(
        "INSERT INTO messages(session_id, role, text, created_at) VALUES ('s1','user','oi',1.0)"
    )
    c.commit()
    c.close()
    store = MemoryStore(caminho)
    try:
        assert store.query("SELECT value FROM meta WHERE key='schema_version'")[0][0] == "18"
        [m] = store.history("s1")
        assert m.text == "oi" and m.version_of is None
        assert store.query("SELECT superseded FROM messages")[0][0] == 0
    finally:
        store.close()
