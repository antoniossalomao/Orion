"""Rotas de conversa do orion.app (a barra lateral do front): listar, abrir, renomear, fixar, apagar."""

import pytest
from fastapi.testclient import TestClient

from orion.app import create_app
from orion.config import Settings

TOKEN = "token-de-teste-com-16+"
AUTH = {"Authorization": f"Bearer {TOKEN}"}


@pytest.fixture
def c(tmp_path):
    settings = Settings(data_dir=tmp_path / "d", admin_token=TOKEN, _env_file=None)
    with TestClient(
        create_app(settings, gateway_factory=lambda _: None), base_url="http://127.0.0.1"
    ) as cli:
        yield cli


def mem(c):
    return c.app.state.orion.memory


def test_todas_as_rotas_exigem_autenticacao(c):
    assert c.get("/sessoes").status_code == 401
    assert c.post("/sessoes").status_code == 401
    assert c.post("/sessoes/ativar", json={"sessao_id": "x"}).status_code == 401
    assert c.get("/historico?sessao=x").status_code == 401
    assert c.patch("/sessoes/x", json={"titulo": "a"}).status_code == 401
    assert c.delete("/sessoes/x").status_code == 401


def test_listar_usa_a_primeira_fala_como_titulo_e_marca_a_ativa(c):
    s = mem(c).active_session("web")
    mem(c).add_message(s.id, "user", "me ajuda   com o backup do vault")
    d = c.get("/sessoes", headers=AUTH).json()
    assert d["ativa"] == s.id and d["total"] == 1
    item = d["sessoes"][0]
    assert item["titulo"] == "me ajuda com o backup do vault"
    assert item["ativa"] and not item["favorita"] and not item["somente_leitura"]
    assert item["criada"].endswith("+00:00") and item["ultima_atividade"]


def test_conversa_vazia_ganha_titulo_padrao(c):
    assert c.get("/sessoes", headers=AUTH).json()["sessoes"][0]["titulo"] == "Nova conversa"


def test_nova_conversa_reaproveita_a_vazia_e_cria_outra_depois_de_usada(c):
    a = c.post("/sessoes", headers=AUTH).json()["sessao_id"]
    assert c.post("/sessoes", headers=AUTH).json()["sessao_id"] == a  # ainda vazia
    mem(c).add_message(a, "user", "oi")
    b = c.post("/sessoes", headers=AUTH).json()["sessao_id"]
    assert b != a and c.get("/sessoes", headers=AUTH).json()["ativa"] == b


def test_ativar_devolve_so_user_e_assistant_e_troca_a_ativa(c):
    m = mem(c)
    a = m.new_session("web")
    m.add_message(a.id, "user", "pergunta")
    m.add_message(a.id, "tool", "ruído interno")
    m.add_message(a.id, "assistant", "resposta")
    b = m.new_session("web")
    m.add_message(b.id, "user", "outra")
    r = c.post("/sessoes/ativar", json={"sessao_id": a.id}, headers=AUTH).json()
    assert [(x["role"], x["content"]) for x in r["mensagens"]] == [
        ("user", "pergunta"),
        ("assistant", "resposta"),
    ]
    assert c.get("/sessoes", headers=AUTH).json()["ativa"] == a.id
    assert (
        c.post("/sessoes/ativar", json={"sessao_id": "nao-existe"}, headers=AUTH).status_code == 404
    )


def test_importada_abre_so_pelo_historico(c):
    sid, _ = mem(c).import_session("ext-1", "legado", "conversa antiga", 1.0)
    mem(c).import_message(sid, "m1", "user", "oi de antes", 1.0)
    item = next(
        s for s in c.get("/sessoes", headers=AUTH).json()["sessoes"] if s["sessao_id"] == sid
    )
    assert item["somente_leitura"] and item["titulo"] == "conversa antiga"
    assert c.post("/sessoes/ativar", json={"sessao_id": sid}, headers=AUTH).status_code == 409
    h = c.get(f"/historico?sessao={sid}", headers=AUTH).json()
    assert h["total"] == 1 and h["mensagens"][0]["content"] == "oi de antes"


def test_conversa_de_outro_canal_nao_e_alcancavel(c):
    tg = mem(c).new_session("telegram")
    assert c.get(f"/historico?sessao={tg.id}", headers=AUTH).status_code == 404
    assert c.patch(f"/sessoes/{tg.id}", json={"titulo": "x"}, headers=AUTH).status_code == 404
    assert c.delete(f"/sessoes/{tg.id}", headers=AUTH).status_code == 404
    assert mem(c).get_session(tg.id).title is None  # nada mudou
    assert tg.id not in [s["sessao_id"] for s in c.get("/sessoes", headers=AUTH).json()["sessoes"]]


def test_renomear_e_fixar(c):
    s = mem(c).new_session("web")
    mem(c).add_message(s.id, "user", "texto original")
    r = c.patch(f"/sessoes/{s.id}", json={"titulo": "  Plano   novo "}, headers=AUTH)
    assert r.status_code == 200 and r.json()["sessao"]["titulo"] == "Plano novo"
    r = c.patch(f"/sessoes/{s.id}", json={"favorita": True}, headers=AUTH)
    assert r.json()["sessao"]["favorita"] is True
    assert c.get("/sessoes", headers=AUTH).json()["sessoes"][0]["sessao_id"] == s.id  # fixada sobe
    assert c.patch(f"/sessoes/{s.id}", json={}, headers=AUTH).status_code == 422
    assert c.patch(f"/sessoes/{s.id}", json={"titulo": ""}, headers=AUTH).status_code == 422
    assert c.patch(f"/sessoes/{s.id}", json={"titulo": "   "}, headers=AUTH).status_code == 422
    assert c.patch("/sessoes/nao-existe", json={"titulo": "a"}, headers=AUTH).status_code == 404


def test_apagar_esconde_e_deixa_as_mensagens(c):
    s = mem(c).new_session("web")
    mem(c).add_message(s.id, "user", "fica no banco")
    assert c.delete(f"/sessoes/{s.id}", headers=AUTH).json() == {"ok": True}
    assert s.id not in [x["sessao_id"] for x in c.get("/sessoes", headers=AUTH).json()["sessoes"]]
    assert c.delete(f"/sessoes/{s.id}", headers=AUTH).status_code == 404
    assert [m.text for m in mem(c).history(s.id)] == ["fica no banco"]


def test_canal_invalido_e_recusado(c):
    assert c.get("/sessoes?canal=../x", headers=AUTH).status_code == 422
    assert c.delete("/sessoes/x?canal=A%20B", headers=AUTH).status_code == 422


def test_cookie_de_login_nao_apaga_a_partir_de_outra_origem(tmp_path):
    settings = Settings(data_dir=tmp_path / "d", admin_token=TOKEN, _env_file=None)
    with TestClient(
        create_app(settings, gateway_factory=lambda _: None), base_url="http://127.0.0.1"
    ) as cli:
        s = cli.app.state.orion.memory.new_session("web")
        cli.app.state.orion.auth.set_password("uma-senha-bem-longa-123")
        token = cli.app.state.orion.auth.login("uma-senha-bem-longa-123", "teste")
        cli.cookies.set("orion_session", token)
        r = cli.delete(f"/sessoes/{s.id}", headers={"Origin": "http://evil.example"})
        assert r.status_code == 403
        assert cli.app.state.orion.memory.session_visible(s.id, "web") is not None  # nada mudou
        ok = cli.delete(f"/sessoes/{s.id}", headers={"Origin": "http://127.0.0.1"})
        assert ok.status_code == 200
        assert cli.app.state.orion.memory.session_visible(s.id, "web") is None
