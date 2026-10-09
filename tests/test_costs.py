"""Custo zero e cota gratuita (regra 46), telemetria por provedor e "modelos fora do ar"."""

import json
from datetime import datetime
from types import SimpleNamespace

import httpx
import pytest
from fastapi.testclient import TestClient

from orion import saidas
from orion.app import create_app
from orion.config import Settings
from orion.costs import Cota, Custos, cotas_de, host_gratuito
from orion.doctor import checar
from orion.gateway import ChatGateway, Endpoint
from orion.jobs import JobRunner
from orion.memory import MemoryStore
from orion.memory.ops import Operations
from tests.test_gateway import resposta, sse, texto

TOKEN = "token-de-teste-com-16+"
AUTH = {"Authorization": f"Bearer {TOKEN}"}
AGORA = datetime(2026, 10, 9, 15).timestamp()


@pytest.fixture
def mundo(tmp_path):
    t = {"agora": AGORA}
    store = MemoryStore(tmp_path / "c.db", clock=lambda: t["agora"])
    ops = Operations(store)
    custos = Custos(
        store,
        {"gateway": Cota("gateway:", "Gateway", 10), "brave": Cota("brave", "Brave", 4, "mes")},
        ops=ops,
        clock=lambda: t["agora"],
    )
    yield SimpleNamespace(t=t, store=store, ops=ops, custos=custos)
    store.close()


def chamadas(store, provider, n, ts=None, ok=True):
    for _ in range(n):
        store.add_external_call(provider=provider, kind="chat", ok=ok, latency_ms=100, ts=ts)


def test_uso_conta_hoje_e_o_mes_e_soma_as_camadas_do_gateway(mundo):
    chamadas(mundo.store, "gateway:padrão", 3)
    chamadas(mundo.store, "gateway:rapido", 2)
    chamadas(mundo.store, "ollama", 5)  # local não gasta cota
    chamadas(mundo.store, "gateway:padrão", 4, ts=AGORA - 86400)  # ontem
    chamadas(mundo.store, "brave", 2, ts=datetime(2026, 10, 1, 9).timestamp())  # começo do mês
    chamadas(mundo.store, "brave", 1, ts=datetime(2026, 9, 30, 9).timestamp())  # mês passado
    assert mundo.custos.uso_hoje("gateway") == 5
    assert mundo.custos.uso("brave") == 2


def test_job_opcional_para_com_90_porcento_e_avisa_uma_vez_por_dia(mundo):
    chamadas(mundo.store, "gateway:padrão", 8)
    assert mundo.custos.pode_usar("gateway", opcional=True)  # 80%
    chamadas(mundo.store, "gateway:padrão", 1)
    assert not mundo.custos.pode_usar("gateway", opcional=True, quem="o ciclo de sono")
    assert not mundo.custos.pode_usar("gateway", opcional=True, quem="o ciclo de sono")
    avisos = mundo.ops.pending_notifications()
    assert len(avisos) == 1 and avisos[0]["kind"] == "cota"
    assert (
        "9/10 hoje" in avisos[0]["text"]
        and "o ciclo de sono ficou para depois" in avisos[0]["text"]
    )
    assert "O chat continua" in avisos[0]["text"]
    assert mundo.custos.pode_usar("gateway", opcional=False)  # o chat nunca é bloqueado
    mundo.t["agora"] += 86400  # dia novo: cota zerada
    assert mundo.custos.pode_usar("gateway", opcional=True)


def test_resumo_mostra_estado_de_cada_cota(mundo):
    chamadas(mundo.store, "gateway:padrão", 12)
    r = {c["provedor"]: c for c in mundo.custos.resumo()}
    assert r["gateway"] == {
        "provedor": "gateway",
        "nome": "Gateway",
        "usado": 12,
        "limite": 10,
        "periodo": "dia",
        "pct": 120,
        "estado": "estourada",
    }
    assert r["brave"]["estado"] == "ok" and r["brave"]["periodo"] == "mes"


def test_limite_zero_e_sem_limite(tmp_path):
    s = Settings(data_dir=tmp_path, quota_gemini_dia=0, _env_file=None)
    store = MemoryStore(tmp_path / "x.db")
    try:
        c = Custos(store, cotas_de(s))
        chamadas(store, "gemini", 50)
        assert c.pode_usar("gemini", opcional=True)
        assert {x["provedor"]: x["estado"] for x in c.resumo()}["gemini"] == "sem_limite"
    finally:
        store.close()


async def test_jobs_opcionais_sao_adiados_pela_cota(mundo):
    chamadas(mundo.store, "gateway:padrão", 10)
    consolidador = SimpleNamespace(rodou=0)

    async def run():
        consolidador.rodou += 1

    consolidador.run = run
    jobs = JobRunner(
        mundo.store,
        mundo.ops,
        consolidator=consolidador,
        custos=mundo.custos,
        clock=lambda: mundo.t["agora"],
    )
    rel = await jobs.tick()
    assert consolidador.rodou == 0 and rel.adiados == ["a consolidação da memória"]
    assert [n["kind"] for n in mundo.ops.pending_notifications()] == ["cota"]


def test_lista_de_gratuitos_e_doctor(tmp_path):
    assert host_gratuito("http://127.0.0.1:20128/v1")
    assert host_gratuito("https://api.groq.com/openai/v1")
    assert not host_gratuito("https://api.openai.com/v1")
    s = Settings(
        data_dir=tmp_path / "d",
        gateway_url="https://api.openai.com/v1",
        gateway_model="gpt",
        _env_file=None,
    )
    itens = {c.nome: c for c in checar(s)}
    item = itens["custo (gateway)"]
    assert item.nivel == "aviso" and "ORION_ALLOW_PAID" in item.detalhe and "§17" in item.detalhe
    s2 = s.model_copy(update={"allow_paid": True})
    assert {c.nome: c for c in checar(s2)}["custo (gateway)"].nivel == "ok"
    local = Settings(
        data_dir=tmp_path / "d", gateway_url="http://127.0.0.1:20128/v1", _env_file=None
    )
    assert "custo (gateway)" not in {c.nome for c in checar(local)}


# ── E1.4: modelos fora do ar ──────────────────────────────────────────────────
def test_aviso_de_gateway_fora_do_ar_e_de_volta(mundo):
    jobs = JobRunner(mundo.store, mundo.ops, gateway_down_min=10, clock=lambda: mundo.t["agora"])
    chamadas(mundo.store, "gateway:padrão", 1, ts=AGORA - 3600)  # funcionou há 1 h
    for minutos in (12, 8, 3):  # 3 falhas seguidas, a primeira há 12 min
        chamadas(mundo.store, "gateway:padrão", 1, ts=AGORA - minutos * 60, ok=False)
    assert jobs._saude_gateway() == "fora"
    (aviso,) = mundo.ops.pending_notifications()
    assert aviso["kind"] == "gateway" and "fora do ar desde" in aviso["text"]
    assert not aviso["urgent"]  # o não perturbe segura este aviso
    assert jobs._saude_gateway() is None  # um aviso só
    mundo.t["agora"] += 60
    chamadas(mundo.store, "gateway:padrão", 1)  # voltou
    assert jobs._saude_gateway() == "voltou"
    assert "de volta" in mundo.ops.pending_notifications()[-1]["text"]


def test_falhas_recentes_demais_ou_poucas_nao_avisam(mundo):
    jobs = JobRunner(mundo.store, mundo.ops, gateway_down_min=10, clock=lambda: mundo.t["agora"])
    for minutos in (5, 3, 1):  # 3 falhas, mas a primeira há só 5 min
        chamadas(mundo.store, "gateway:padrão", 1, ts=AGORA - minutos * 60, ok=False)
    assert jobs._saude_gateway() is None
    chamadas(mundo.store, "ollama", 1)  # o modelo local respondeu: o gateway continua fora
    mundo.t["agora"] += 15 * 60
    assert jobs._saude_gateway() == "fora"


# ── E1.3: provedores no painel (a prova da etapa) ─────────────────────────────
def test_prova_dez_chamadas_com_duas_falhas_aparecem_em_provedores(tmp_path):
    n = {"i": 0}

    def handler(req):
        n["i"] += 1
        if n["i"] in (3, 7):
            return httpx.Response(502, text="fora")
        return resposta(sse(texto("ok", "stop"), "[DONE]"))

    gw = ChatGateway(
        [
            Endpoint("gateway", "http://gw.test/v1", "modelo-livre", "k"),
            Endpoint("reserva", "http://gw2.test/v1", "modelo-livre", "k"),
        ],
        client=httpx.AsyncClient(transport=httpx.MockTransport(handler)),
    )
    settings = Settings(data_dir=tmp_path / "d", admin_token=TOKEN, _env_file=None)
    app = create_app(settings, gateway_factory=lambda _: gw)
    with TestClient(app, base_url="http://127.0.0.1") as c:
        for i in range(8):  # 8 turnos; em 2 o primeiro endpoint falhou e o reserva respondeu
            r = c.post("/chat", json={"texto": f"oi {i}"}, headers=AUTH)
            assert r.status_code == 200
        p = c.get("/painel", headers=AUTH).json()
    (linha,) = [x for x in p["provedores"] if x["provider"] == "gateway:padrão"]
    assert (linha["chamadas"], linha["falhas"], linha["modelo"]) == (10, 2, "modelo-livre")
    assert linha["p95_ms"] >= linha["p50_ms"] >= 0
    assert {c["provedor"] for c in p["cota"]} == {"gateway", "groq", "gemini", "brave"}
    assert "oi 3" not in json.dumps(p)
    assert saidas._destino is None  # o app desliga o registro ao parar
