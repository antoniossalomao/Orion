"""C31: projetos com instruções próprias, ligados a conversas."""

import pytest
from fastapi.testclient import TestClient

from orion.agent import Agent
from orion.app import create_app
from orion.config import Settings
from orion.memory import MemoryStore
from orion.policy import ApprovalStore, PathGuard, PolicyEngine
from orion.tools import ToolRegistry, memory_tools
from tests.fakes import FakeGateway, fala

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


def test_exige_login(c):
    assert c.get("/projetos").status_code == 401
    assert c.post("/projetos", json={"nome": "x"}).status_code == 401


def test_crud_do_projeto_e_nome_unico_sem_caixa(c):
    r = c.post("/projetos", json={"nome": "TCC", "instrucoes": "Responda formal"}, headers=AUTH)
    assert r.status_code == 201
    pid = r.json()["projeto"]["id"]
    assert c.post("/projetos", json={"nome": "tcc"}, headers=AUTH).status_code == 409
    r = c.patch(f"/projetos/{pid}", json={"instrucoes": "Responda curto"}, headers=AUTH)
    assert r.json()["projeto"]["instrucoes"] == "Responda curto"
    assert c.patch("/projetos/999", json={"nome": "x"}, headers=AUTH).status_code == 404
    c.patch(f"/projetos/{pid}", json={"arquivado": True}, headers=AUTH)
    assert c.get("/projetos", headers=AUTH).json()["total"] == 0
    assert c.get("/projetos?arquivados=true", headers=AUTH).json()["total"] == 1
    assert c.delete(f"/projetos/{pid}", headers=AUTH).json() == {"ok": True}
    assert c.delete(f"/projetos/{pid}", headers=AUTH).status_code == 404


def test_associar_conversa_filtrar_e_desassociar(c):
    m = mem(c)
    pid = c.post("/projetos", json={"nome": "Orion"}, headers=AUTH).json()["projeto"]["id"]
    a, b = m.new_session("web"), m.new_session("web")
    m.add_message(a.id, "user", "a")
    m.add_message(b.id, "user", "b")
    r = c.patch(f"/sessoes/{a.id}", json={"projeto_id": pid}, headers=AUTH)
    assert r.json()["sessao"]["projeto_id"] == pid
    ids = [i["sessao_id"] for i in c.get(f"/sessoes?projeto={pid}", headers=AUTH).json()["sessoes"]]
    assert ids == [a.id]
    c.patch(f"/sessoes/{a.id}", json={"projeto_id": None}, headers=AUTH)  # tira do projeto
    assert c.get(f"/sessoes?projeto={pid}", headers=AUTH).json()["total"] == 0
    assert c.patch(f"/sessoes/{a.id}", json={"projeto_id": 999}, headers=AUTH).status_code == 404


def test_apagar_projeto_mantem_as_conversas(c):
    m = mem(c)
    pid = c.post("/projetos", json={"nome": "Velho"}, headers=AUTH).json()["projeto"]["id"]
    s = m.new_session("web")
    m.add_message(s.id, "user", "oi")
    c.patch(f"/sessoes/{s.id}", json={"projeto_id": pid}, headers=AUTH)
    c.delete(f"/projetos/{pid}", headers=AUTH)
    assert m.get_session(s.id).project_id is None and [x.text for x in m.history(s.id)] == ["oi"]


def test_migracao_v6_para_v7(tmp_path):
    db = tmp_path / "m.db"
    m = MemoryStore(db)
    s = m.new_session("web")
    m._conn.executescript(
        "DROP INDEX idx_sessions_project; ALTER TABLE sessions DROP COLUMN project_id;"
        "DROP TABLE screen_log_fts; DROP TABLE screen_log;"
        "DROP TABLE projects; UPDATE meta SET value='6' WHERE key='schema_version';"
    )
    m._conn.close()
    m2 = MemoryStore(db)
    p = m2.create_project("Novo")
    assert m2.assign_session(s.id, p.id) and m2.get_session(s.id).project_id == p.id


async def test_instrucoes_do_projeto_entram_no_prompt_da_conversa_certa(tmp_path):
    store = MemoryStore(tmp_path / "a.db")
    p = store.create_project("TCC", "Cite sempre a norma ABNT")
    com, sem = store.new_session("web"), store.new_session("telegram")
    store.assign_session(com.id, p.id)
    policy = PolicyEngine(
        path_guard=PathGuard(protected_roots=(tmp_path / "x",), safe_roots=()),
        approvals=ApprovalStore(),
    )
    gw = FakeGateway(fala("ok"), fala("ok"))
    agent = Agent(gateway=gw, tools=ToolRegistry(memory_tools(store)), policy=policy, memory=store)
    store.activate_session(com.id)
    [e async for e in agent.run("web", "oi")]
    store.activate_session(sem.id)
    [e async for e in agent.run("telegram", "oi")]
    assert "Cite sempre a norma ABNT" in gw.chamadas[0][0]["content"]
    assert "[PROJETO: TCC" in gw.chamadas[0][0]["content"]
    assert "Cite sempre" not in gw.chamadas[1][0]["content"]
    store.close()
