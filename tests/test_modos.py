"""Modo pânico e não perturbe (regra 48): corte que não volta sozinho e fila que só espera."""

import json
from datetime import datetime
from types import SimpleNamespace

import pytest
from fastapi.testclient import TestClient

from orion.__main__ import main
from orion.agent import Agent
from orion.app import create_app
from orion.channels.telegram import TelegramChannel
from orion.config import Settings
from orion.jobs import JobRunner
from orion.memory import MemoryStore
from orion.memory.ops import Operations
from orion.modos import ModoError, Modos, na_janela, proxima
from orion.policy import ApprovalStore, PathGuard, PolicyEngine
from orion.policy.classes import Risk, ToolSpec
from orion.tools import Tool, ToolRegistry
from tests.fakes import FakeGateway, chama, fala, pede

TOKEN = "token-de-teste-com-16+"
AUTH = {"Authorization": f"Bearer {TOKEN}"}
AGORA = datetime(2026, 10, 9, 23, 0).timestamp()


@pytest.fixture
def mundo(tmp_path):
    t = {"agora": AGORA}
    store = MemoryStore(tmp_path / "m.db", clock=lambda: t["agora"])
    ops = Operations(store)
    modos = Modos(store, audit=ops.audit_add, clock=lambda: t["agora"])
    yield SimpleNamespace(t=t, store=store, ops=ops, modos=modos)
    store.close()


# ── pânico ────────────────────────────────────────────────────────────────────
def test_panico_liga_nao_volta_sozinho_e_o_audit_registra(mundo):
    assert not mundo.modos.panico()
    assert mundo.modos.entrar_panico("cli") and not mundo.modos.entrar_panico("cli")
    mundo.t["agora"] += 30 * 86400  # um mês depois: continua ligado
    assert mundo.modos.panico() and mundo.modos.estado()["panico_origem"] == "cli"
    assert mundo.modos.sair_panico("telegram") and not mundo.modos.panico()
    acoes = [(r["tool"], r["action"]) for r in reversed(mundo.ops.audit_recent(10))]
    assert acoes == [("modo_panico", "entrar"), ("modo_panico", "sair")]


def test_sem_audit_nao_sai_do_panico_mas_entra_sempre(mundo):
    def quebra(_ev):
        raise OSError("disco cheio")

    modos = Modos(mundo.store, audit=quebra)
    assert modos.entrar_panico("web")  # cortar nunca é barrado
    with pytest.raises(ModoError):
        modos.sair_panico("web")
    assert modos.panico()


def test_estado_corrompido_continua_em_panico(mundo):
    mundo.store.meta_set("modo:panico", "{quebrado")
    assert mundo.modos.panico()


def _politica(tmp_path):
    guard = PathGuard(protected_roots=(tmp_path / "Orion",), safe_roots=(), system_roots=())
    p = PolicyEngine(path_guard=guard, approvals=ApprovalStore())
    p.register_tool(ToolSpec("ler_coisa", Risk.READ))
    p.register_tool(ToolSpec("salvar_coisa", Risk.WRITE))
    p.register_tool(ToolSpec("rodar_coisa", Risk.EXEC))
    p.register_tool(ToolSpec("buscar_na_web", Risk.READ, external=True))
    p.register_tool(ToolSpec("abrir_url", Risk.READ, egress=True))
    p.register_tool(ToolSpec("apagar_coisa", Risk.DESTRUCTIVE))
    return p


def _ferramentas():
    nomes = [
        "ler_coisa", "salvar_coisa", "rodar_coisa", "buscar_na_web", "abrir_url",
        "apagar_coisa", "sem_classe",
    ]  # fmt: skip
    vazio = {"type": "object", "properties": {}}
    return ToolRegistry([Tool(n, n, vazio, lambda: {"ok": True}) for n in nomes])


async def test_panico_tira_do_modelo_rede_externo_execucao_e_destrutiva(mundo, tmp_path):
    gw = FakeGateway(fala("oi"), pede(chama("rodar_coisa")), fala("não deu"))
    ag = Agent(
        gateway=gw,
        tools=_ferramentas(),
        policy=_politica(tmp_path),
        memory=mundo.store,
        modos=mundo.modos,
    )
    mundo.modos.entrar_panico("web")
    [e async for e in ag.run("web", "oi")]
    nomes = sorted(t["function"]["name"] for t in gw.ferramentas[0])
    assert nomes == ["ler_coisa", "salvar_coisa"]
    # se o modelo pedir assim mesmo, é negada (e nada roda)
    eventos = [e async for e in ag.run("web", "roda")]
    negada = next(e.data for e in eventos if e.kind == "tool")
    assert negada["decision"] == "deny" and "pânico" in negada["reason"]
    mundo.modos.sair_panico("web")
    gw.roteiros.append(fala("de volta"))
    [e async for e in ag.run("web", "e agora?")]
    assert len(gw.ferramentas[-1]) == 7


async def test_panico_para_tela_embeddings_e_jobs_de_rede(mundo):
    tela = SimpleNamespace(rodou=0, devida=lambda: True)
    tela.run = lambda: tela.__setattr__("rodou", tela.rodou + 1) or "gravada"
    sono = SimpleNamespace(devida=lambda: True, rodou=0)

    async def sono_run():
        sono.rodou += 1

    sono.run = sono_run
    jobs = JobRunner(mundo.store, mundo.ops, screen=tela, sleep=sono, modos=mundo.modos)
    mundo.store.embed_pending = lambda: pytest.fail("embeddings vão ao Gemini: não em pânico")
    mundo.modos.entrar_panico("cli")
    rel = await jobs.tick()
    assert tela.rodou == 0 and sono.rodou == 0 and rel.tela is None
    mundo.modos.sair_panico("cli")
    del mundo.store.embed_pending
    await jobs.tick()
    assert tela.rodou == 1


# ── não perturbe ──────────────────────────────────────────────────────────────
def test_janela_que_atravessa_a_meia_noite():
    def em(h, m):
        return datetime(2026, 10, 9, h, m).timestamp()

    assert na_janela("22:30-07:00", em(23, 0)) and na_janela("22:30-07:00", em(6, 59))
    assert not na_janela("22:30-07:00", em(7, 0)) and not na_janela("22:30-07:00", em(12, 0))
    assert na_janela("13:00-14:00", em(13, 30)) and not na_janela("", em(13, 30))
    assert proxima("07:00", em(23, 0)) == datetime(2026, 10, 10, 7, 0).timestamp()


def test_nao_perturbe_segura_os_nao_urgentes_e_entrega_depois(mundo):
    mundo.ops.segurar = mundo.modos.nao_perturbe
    mundo.ops.notify("gateway", "modelos fora do ar")
    mundo.ops.notify("lembrete", "remédio", urgente=True)
    fim = mundo.modos.ligar_nao_perturbe("07:00")
    assert fim == datetime(2026, 10, 10, 7, 0).timestamp()
    assert [n["kind"] for n in mundo.ops.pending_notifications()] == ["lembrete"]
    assert len(mundo.ops.pending_notifications(todos=True)) == 2  # nada descartado
    mundo.t["agora"] = fim + 1  # acabou: a fila sai
    assert [n["kind"] for n in mundo.ops.pending_notifications()] == ["gateway", "lembrete"]


async def test_nao_perturbe_por_horario_pausa_a_memoria_da_tela(mundo):
    modos = Modos(mundo.store, dnd_at="22:30-07:00", clock=lambda: mundo.t["agora"])
    tela = SimpleNamespace(rodou=0, devida=lambda: True)
    tela.run = lambda: tela.__setattr__("rodou", tela.rodou + 1) or "gravada"
    jobs = JobRunner(mundo.store, mundo.ops, screen=tela, modos=modos)
    await jobs.tick()  # 23h: dentro da janela
    assert tela.rodou == 0 and modos.estado()["nao_perturbe_horario"] == "22:30-07:00"
    mundo.t["agora"] = datetime(2026, 10, 10, 9, 0).timestamp()
    await jobs.tick()
    assert tela.rodou == 1


def test_lembrete_e_agendamento_sao_urgentes(mundo):
    mundo.ops.add_reminder("remédio", AGORA - 60)
    jobs = JobRunner(mundo.store, mundo.ops, clock=lambda: mundo.t["agora"])
    jobs._lembretes()
    assert mundo.ops.pending_notifications(todos=True)[0]["urgent"] == 1


# ── API, Telegram e CLI ───────────────────────────────────────────────────────
def _cliente(tmp_path):
    settings = Settings(data_dir=tmp_path / "d", admin_token=TOKEN, _env_file=None)
    return TestClient(
        create_app(settings, gateway_factory=lambda _: None), base_url="http://127.0.0.1"
    )


def test_api_entra_com_um_clique_e_sai_so_com_a_senha(tmp_path):
    with _cliente(tmp_path) as c:
        assert c.post("/modo/panico", json={"ativo": True}).status_code == 401
        r = c.post("/modo/panico", json={"ativo": True}, headers=AUTH)
        assert r.status_code == 200 and r.json()["panico"] is True
        assert c.get("/painel", headers=AUTH).json()["modos"]["panico"] is True
        r = c.post("/modo/panico", json={"ativo": False, "senha": "errada"}, headers=AUTH)
        assert r.status_code == 403 and c.get("/modo", headers=AUTH).json()["panico"] is True
        r = c.post("/modo/panico", json={"ativo": False, "senha": TOKEN}, headers=AUTH)
        assert r.status_code == 200 and r.json()["panico"] is False


def test_api_sai_do_panico_com_a_senha_do_login(tmp_path):
    settings = Settings(data_dir=tmp_path / "d", _env_file=None)
    app = create_app(settings, gateway_factory=lambda _: None)
    with TestClient(app, base_url="http://127.0.0.1") as c:
        c.app.state.orion.auth.set_password("uma senha longa e boa 123")
        c.post("/auth/login", json={"usuario": "admin", "senha": "uma senha longa e boa 123"})
        assert c.post("/modo/panico", json={"ativo": True}).json()["panico"] is True
        r = c.post("/modo/panico", json={"ativo": False, "senha": "outra senha qualquer"})
        assert r.status_code == 403
        r = c.post("/modo/panico", json={"ativo": False, "senha": "uma senha longa e boa 123"})
        assert r.status_code == 200 and r.json()["panico"] is False


def test_api_nao_perturbe(tmp_path):
    with _cliente(tmp_path) as c:
        r = c.post("/modo/nao-perturbe", json={"ate": "23:59"}, headers=AUTH)
        assert r.status_code == 200 and r.json()["nao_perturbe"] is True
        assert c.post("/modo/nao-perturbe", json={"ate": "25:00"}, headers=AUTH).status_code == 422
        r = c.post("/modo/nao-perturbe", json={"ate": None}, headers=AUTH)
        assert r.json()["nao_perturbe_ate"] is None


def test_telegram_panico_e_sair(mundo):
    canal = TelegramChannel.__new__(TelegramChannel)
    canal.modos = mundo.modos
    assert "LIGADO" in canal._panico("") and mundo.modos.panico()
    assert canal._panico("estado") == "Modo pânico LIGADO."
    assert "desligado" in canal._panico("sair") and not mundo.modos.panico()
    assert canal._panico("talvez").startswith("Use /panico")


def test_cli_liga_e_o_servidor_obedece(tmp_path, monkeypatch, capsys):
    monkeypatch.setenv("ORION_DATA_DIR", str(tmp_path / "d"))
    monkeypatch.chdir(tmp_path)
    with _cliente(tmp_path) as c:
        assert main(["panico"]) == 0 and "PÂNICO LIGADO" in capsys.readouterr().out
        assert c.get("/modo", headers=AUTH).json()["panico"] is True  # mesmo banco
        assert main(["panico", "--estado"]) == 0 and "LIGADO" in capsys.readouterr().out
        assert main(["panico", "--sair"]) == 0
        assert c.get("/modo", headers=AUTH).json()["panico"] is False
        assert (
            main(["nao-perturbe", "07:00"]) == 0 and "não perturbe até" in capsys.readouterr().out
        )
        assert c.get("/modo", headers=AUTH).json()["nao_perturbe"] is True
        assert main(["nao-perturbe", "--sair"]) == 0
        acoes = json.dumps([r["action"] for r in c.app.state.orion.ops.audit_recent(20)])
    assert "entrar" in acoes and "sair" in acoes


def test_doctor_avisa_panico_ligado(tmp_path):
    from orion.doctor import checar

    s = Settings(data_dir=tmp_path / "d", _env_file=None)
    store = MemoryStore(s.db_path)
    Modos(store).entrar_panico("cli")
    store.close()
    assert {c.nome: c.nivel for c in checar(s)}["modo pânico"] == "aviso"


def test_texto_do_painel_no_telegram_mostra_panico_nao_perturbe_e_cota(mundo):
    from orion.painel import texto_do_painel

    p = {
        "uptime_s": 60,
        "modelos": {"configurado": False, "endpoints": []},
        "clis": [],
        "aprovacoes": {"pendentes": 0, "itens": []},
        "decisoes": {"janela_h": 24, "total": 0, "por_acao": {"allow": 0, "confirm": 0, "deny": 0},
                     "mais_usadas": []},
        "jobs": {"ativo": False},
        "avisos": {"pendentes": 0},
        "ferramentas": 0,
        "memoria": {"ok": True},
        "mcp": {},
        "modos": {"panico": True, "nao_perturbe": True},
        "cota": [{"nome": "Gateway", "usado": 950, "limite": 1000, "periodo": "dia",
                  "estado": "alta"}],
    }  # fmt: skip
    t = texto_do_painel(p)
    assert "MODO PÂNICO" in t and "Não perturbe" in t and "Gateway: 950/1000 hoje ⚠️" in t
