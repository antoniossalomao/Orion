"""C05/C06: arquivar conversa (sem apagar mensagens) e buscar por título e conteúdo."""

import pytest
from fastapi.testclient import TestClient

from orion.app import create_app
from orion.config import Settings
from orion.memory.store import MemoryStore

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


def _conversa(m, canal, texto, titulo=None):
    s = m.new_session(canal, titulo)
    m.add_message(s.id, "user", texto)
    return s


def test_arquivar_tira_da_barra_mantem_mensagens_e_desfaz(c):
    m = mem(c)
    s = _conversa(m, "web", "receita de bolo")
    r = c.patch(f"/sessoes/{s.id}", json={"arquivada": True}, headers=AUTH)
    assert r.status_code == 200 and r.json()["sessao"]["arquivada"] is True
    ids = [i["sessao_id"] for i in c.get("/sessoes", headers=AUTH).json()["sessoes"]]
    assert s.id not in ids
    arq = c.get("/sessoes?arquivadas=true", headers=AUTH).json()["sessoes"]
    assert [i["sessao_id"] for i in arq] == [s.id]
    assert [x.text for x in m.history(s.id)] == ["receita de bolo"]
    c.patch(f"/sessoes/{s.id}", json={"arquivada": False}, headers=AUTH)
    ids = [i["sessao_id"] for i in c.get("/sessoes", headers=AUTH).json()["sessoes"]]
    assert s.id in ids


def test_arquivar_a_ativa_nao_a_mantem_ativa_e_ativar_desarquiva(c):
    m = mem(c)
    s = _conversa(m, "web", "oi")
    assert m.selected_session("web").id == s.id
    c.patch(f"/sessoes/{s.id}", json={"arquivada": True}, headers=AUTH)
    sel = m.selected_session("web")
    assert sel is None or sel.id != s.id
    r = c.post("/sessoes/ativar", json={"sessao_id": s.id}, headers=AUTH)
    assert r.status_code == 200 and not m.get_session(s.id).shelved


def test_arquivar_desfixa_e_importada_nao_arquiva(c):
    m = mem(c)
    s = _conversa(m, "web", "oi")
    c.patch(f"/sessoes/{s.id}", json={"favorita": True}, headers=AUTH)
    c.patch(f"/sessoes/{s.id}", json={"arquivada": True}, headers=AUTH)
    assert not m.get_session(s.id).pinned
    imp_id, _ = m.import_session("ext-1", "web", "velha", 1.0)
    r = c.patch(f"/sessoes/{imp_id}", json={"arquivada": True}, headers=AUTH)
    assert r.status_code == 409


def test_patch_vazio_continua_422(c):
    s = _conversa(mem(c), "web", "oi")
    assert c.patch(f"/sessoes/{s.id}", json={}, headers=AUTH).status_code == 422


def test_busca_acha_termo_so_no_corpo_e_devolve_trecho(c):
    m = mem(c)
    a = _conversa(m, "web", "como configurar o tailscale no notebook")
    _conversa(m, "web", "receita de bolo")
    r = c.get("/sessoes/busca?q=tailscale", headers=AUTH).json()
    assert [x["sessao_id"] for x in r["resultados"]] == [a.id]
    assert "[tailscale]" in r["resultados"][0]["trecho"].lower()


def test_busca_por_titulo_zero_resultados_e_arquivadas(c):
    m = mem(c)
    s = _conversa(m, "web", "oi", titulo="Plano da venda do PC")
    assert [
        x["sessao_id"] for x in c.get("/sessoes/busca?q=venda", headers=AUTH).json()["resultados"]
    ] == [s.id]
    assert c.get("/sessoes/busca?q=inexistente", headers=AUTH).json()["total"] == 0
    c.patch(f"/sessoes/{s.id}", json={"arquivada": True}, headers=AUTH)
    assert c.get("/sessoes/busca?q=venda", headers=AUTH).json()["total"] == 1


def test_busca_isola_canal_e_ignora_apagadas_e_exige_login(c):
    m = mem(c)
    _conversa(m, "telegram", "segredo do telegram")
    d = _conversa(m, "web", "assunto apagado")
    c.delete(f"/sessoes/{d.id}", headers=AUTH)
    assert c.get("/sessoes/busca?q=telegram", headers=AUTH).json()["total"] == 0
    assert c.get("/sessoes/busca?q=apagado", headers=AUTH).json()["total"] == 0
    assert c.get("/sessoes/busca?q=x").status_code == 401


def test_busca_com_aspas_e_curingas_nao_quebra(c):
    _conversa(mem(c), "web", "100% certo")
    for q in ['"', "a OR", "%", "_", "*"]:
        assert c.get("/sessoes/busca", params={"q": q}, headers=AUTH).status_code == 200


def test_migracao_v5_para_v6_preserva_conversas(tmp_path):
    db = tmp_path / "m.db"
    m = MemoryStore(db)
    s = _conversa(m, "web", "antes")
    m._conn.executescript(
        "DROP INDEX idx_sessions_project; ALTER TABLE sessions DROP COLUMN project_id;"
        "ALTER TABLE documents DROP COLUMN project_id;"
        "DROP TABLE screen_log_fts; DROP TABLE screen_log;"
        "DROP TABLE projects; ALTER TABLE sessions DROP COLUMN shelved;"
        "UPDATE meta SET value='5' WHERE key='schema_version';"
    )
    m._conn.close()
    m2 = MemoryStore(db)
    assert m2.get_session(s.id).shelved is False
    assert m2.shelve_session(s.id, True)
