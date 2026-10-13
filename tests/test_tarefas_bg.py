"""E9.2 (regra 59): tarefa em segundo plano só lê, tem teto e deixa propostas para você decidir."""

import asyncio

import pytest
from fastapi.testclient import TestClient

from orion.agent import AgentEvent
from orion.app import create_app
from orion.config import Settings
from orion.memory import MemoryStore
from orion.memory.ops import Operations
from orion.tarefas_bg import TarefaBgError, TarefasBg, separar_propostas
from tests.fakes import FakeGateway, chama, fala, pede

TOKEN = "token-de-teste-com-16+"
AUTH = {"Authorization": f"Bearer {TOKEN}"}


@pytest.fixture
def mem(tmp_path):
    m = MemoryStore(tmp_path / "bg.db")
    yield m
    m.close()


def tarefas(mem, eventos, **kw):
    chamadas = []

    async def run_turn(canal, texto):
        chamadas.append((canal, texto))
        for ev in eventos:
            if callable(ev):
                await ev()
            else:
                yield ev
        yield AgentEvent("done", {})

    return TarefasBg(memory=mem, ops=Operations(mem), run_turn=run_turn, **kw), chamadas


def texto(t):
    return AgentEvent("text", {"text": t})


def test_propostas_so_vem_da_secao_e_em_linhas_de_lista():
    corpo, itens = separar_propostas(
        "Achei 3 fontes.\n\nPROPOSTAS:\n- Salvar o resumo no vault\nlinha solta\n- Criar lembrete\n-\n"
    )
    assert corpo == "Achei 3 fontes."
    assert [i["texto"] for i in itens] == ["Salvar o resumo no vault", "Criar lembrete"]
    assert {i["estado"] for i in itens} == {"pendente"}
    assert separar_propostas("sem seção") == ("sem seção", [])
    assert len(separar_propostas("PROPOSTAS:\n" + "- x\n" * 30)[1]) == 10


async def test_tarefa_roda_em_conversa_propria_e_guarda_relatorio_e_propostas(mem):
    t, chamadas = tarefas(mem, [texto("Relatório pronto.\nPROPOSTAS:\n- Salvar no vault\n")])
    r = t.criar("pesquisar preços de SSD")
    assert r["status"] == "rodando" and r["budget"] == {"max_passos": 10, "max_min": 10}
    await t.aguardar(r["id"])
    feito = t.obter(r["id"])
    assert feito["status"] == "concluida" and feito["report"] == "Relatório pronto."
    assert feito["proposals"] == [{"texto": "Salvar no vault", "estado": "pendente"}]
    assert chamadas[0][0] == "tarefa-bg" and "pesquisar preços de SSD" in chamadas[0][1]
    assert "NÃO escreva" in chamadas[0][1]
    assert mem.list_sessions("tarefa-bg")[0].title.startswith("Tarefa: pesquisar")
    avisos = Operations(mem).pending_notifications(todos=True)
    assert avisos and "concluida" in avisos[0]["text"]


async def test_teto_de_passos_e_de_tempo(mem):
    ferramenta = AgentEvent("tool", {"name": "buscar_url", "decision": "allow"})
    t, _ = tarefas(mem, [ferramenta] * 5 + [texto("nunca chega")])
    r = t.criar("x", max_passos=3)
    await t.aguardar(r["id"])
    assert t.obter(r["id"])["status"] == "estourou_passos"

    async def travar():
        await asyncio.sleep(30)

    t2, _ = tarefas(mem, [travar])
    with mem.transaction() as c:
        c.execute(
            "INSERT INTO background_tasks(objective, budget, status, created_at) VALUES ('y','{}','rodando',0)"
        )
    tid = mem.query("SELECT max(id) FROM background_tasks")[0][0]
    await t2._executar(tid, "y", 10, 0.001)  # 60 ms de limite
    assert t2.obter(tid)["status"] == "estourou_tempo"


async def test_ferramenta_negada_nao_conta_passo_e_erro_marca_falha(mem):
    negada = AgentEvent("tool", {"name": "apagar", "decision": "deny"})
    t, _ = tarefas(mem, [negada] * 5 + [texto("ok")])
    r = t.criar("x", max_passos=1)
    await t.aguardar(r["id"])
    assert t.obter(r["id"])["status"] == "concluida"
    t2, _ = tarefas(mem, [AgentEvent("error", {"message": "sem modelo"})])
    r2 = t2.criar("y")
    await t2.aguardar(r2["id"])
    assert t2.obter(r2["id"])["status"] == "falhou" and "sem modelo" in t2.obter(r2["id"])["report"]


async def test_recusas_panico_cota_pedido_ruim_e_uma_por_vez(mem):
    class Modos:
        def panico(self):
            return True

    class Custos:
        def __init__(self, ok):
            self.ok = ok

        def pode_usar(self, *a, **k):
            return self.ok

    async def travar():
        await asyncio.sleep(30)

    with pytest.raises(TarefaBgError) as e:
        tarefas(mem, [], modos=Modos())[0].criar("x")
    assert e.value.status == 409
    with pytest.raises(TarefaBgError) as e:
        tarefas(mem, [], custos=Custos(False))[0].criar("x")
    assert e.value.status == 429
    t, _ = tarefas(mem, [travar])
    for ruim in ("", "  ", "x" * 501):
        with pytest.raises(TarefaBgError):
            t.criar(ruim)
    with pytest.raises(TarefaBgError):
        t.criar("ok", max_passos=0)
    with pytest.raises(TarefaBgError):
        t.criar("ok", max_min=31)
    r = t.criar("lenta")
    with pytest.raises(TarefaBgError, match="já há uma"):
        t.criar("outra")
    assert t.cancelar(r["id"])["id"] == r["id"]
    await t.aguardar(r["id"])
    assert t.obter(r["id"])["status"] == "cancelada"
    with pytest.raises(TarefaBgError, match="não está rodando"):
        t.cancelar(r["id"])
    with pytest.raises(TarefaBgError) as e:
        t.obter(999)
    assert e.value.status == 404


async def test_decidir_proposta_so_devolve_texto_e_nao_decide_duas_vezes(mem):
    t, _ = tarefas(mem, [texto("R\nPROPOSTAS:\n- A\n- B\n")])
    r = t.criar("x")
    await t.aguardar(r["id"])
    assert t.decidir(r["id"], 0, "aceitar") == {"ok": True, "texto": "A"}
    assert t.decidir(r["id"], 1, "descartar") == {"ok": True, "texto": None}
    estados = [p["estado"] for p in t.obter(r["id"])["proposals"]]
    assert estados == ["aceita", "descartada"]
    with pytest.raises(TarefaBgError, match="já foi decidida"):
        t.decidir(r["id"], 0, "aceitar")
    with pytest.raises(TarefaBgError) as e:
        t.decidir(r["id"], 5, "aceitar")
    assert e.value.status == 404
    with pytest.raises(TarefaBgError):
        t.decidir(r["id"], 0, "executar")


def test_pelo_app_a_tarefa_nao_escreve_nada_mesmo_que_o_modelo_tente(tmp_path):
    gw = FakeGateway(
        pede(chama("salvar_memoria", texto="segredo=123", fonte="bg")),
        fala("Não consegui salvar.\nPROPOSTAS:\n- Salvar o fato X na memória\n"),
    )
    settings = Settings(
        data_dir=tmp_path / "d", admin_token=TOKEN, jobs_enabled=False, _env_file=None
    )
    with TestClient(
        create_app(settings, gateway_factory=lambda _: gw), base_url="http://127.0.0.1"
    ) as c:
        assert c.post("/tarefas-bg", json={"objetivo": "x"}).status_code == 401
        assert (
            c.post("/tarefas-bg", json={"objetivo": "x", "extra": 1}, headers=AUTH).status_code
            == 422
        )
        r = c.post("/tarefas-bg", json={"objetivo": "organizar minhas notas"}, headers=AUTH)
        assert r.status_code == 202
        tid = r.json()["id"]
        estado = c.app.state.orion
        for _ in range(100):
            t = c.get(f"/tarefas-bg/{tid}", headers=AUTH).json()
            if t["status"] != "rodando":
                break
            import time

            time.sleep(0.05)
        assert t["status"] == "concluida" and t["proposals"][0]["texto"].startswith("Salvar o fato")
        assert estado.memory.facts() == []  # a escrita foi negada: ninguém aprovou
        lista = c.get("/tarefas-bg", headers=AUTH).json()
        assert lista["total"] == 1
        d = c.post(f"/tarefas-bg/{tid}/propostas/0", json={"acao": "aceitar"}, headers=AUTH).json()
        assert d["texto"].startswith("Salvar o fato")
        assert (
            c.post(
                f"/tarefas-bg/{tid}/propostas/0", json={"acao": "aceitar"}, headers=AUTH
            ).status_code
            == 409
        )
        assert c.get("/tarefas-bg/999", headers=AUTH).status_code == 404


def test_sem_gateway_a_rota_explica(tmp_path):
    settings = Settings(
        data_dir=tmp_path / "d", admin_token=TOKEN, jobs_enabled=False, _env_file=None
    )
    with TestClient(
        create_app(settings, gateway_factory=lambda _: None), base_url="http://127.0.0.1"
    ) as c:
        assert c.post("/tarefas-bg", json={"objetivo": "x"}, headers=AUTH).status_code == 503
