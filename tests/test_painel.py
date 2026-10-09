"""Painel único: contadores do gateway, uso das CLIs, o JSON do /painel e o texto do Telegram."""

import json
import subprocess
from datetime import datetime
from types import SimpleNamespace

import httpx
import pytest
from fastapi.testclient import TestClient

from orion.app import create_app
from orion.config import Settings
from orion.delegate import CliAgent, Delegator
from orion.gateway import ChatGateway, Endpoint
from orion.jobs import JobRunner
from orion.memory import MemoryStore
from orion.memory.ops import Operations
from orion.painel import Painel, texto_do_painel
from orion.policy import ApprovalStore, PathGuard, PolicyEngine
from tests.test_gateway import coletar, gateway, resposta, sse, texto

TOKEN = "token-de-teste-com-16+"
AUTH = {"Authorization": f"Bearer {TOKEN}"}
SEGREDO = "sk-SEGREDO-que-nao-pode-aparecer-0123456789"


# ── contadores do gateway ─────────────────────────────────────────────────────
async def test_gateway_conta_ok_falha_cota_e_quarentena_sem_vazar_o_corpo():
    t = {"agora": 0.0}

    def handler(req):
        if req.url.host == "a.test":
            return httpx.Response(
                429, text=f"cota estourada {SEGREDO}", headers={"retry-after": "30"}
            )
        return resposta(sse(texto("ok", "stop"), "[DONE]"))

    gw = gateway(handler, "a", "b", relogio=lambda: t["agora"], wall=lambda: 1_700_000_000.0)
    await coletar(gw)
    a, b = gw.stats()
    assert (a["nome"], a["modelo"]) == ("a", "modelo-a")
    assert (a["chamadas"], a["ok"], a["falhas"], a["limitada"]) == (1, 0, 1, 1)
    assert a["quarentena_s"] == 30 and a["ultimo_erro"] == "HTTP 429" and a["ultimo_ok"] is None
    assert (b["chamadas"], b["ok"], b["falhas"], b["ultimo_ok"]) == (1, 1, 0, 1_700_000_000.0)
    assert SEGREDO not in json.dumps(gw.stats())  # só o tipo do erro, nunca o corpo

    t["agora"] = 10  # ainda em quarentena: pulado
    await coletar(gw)
    a, b = gw.stats()
    assert a["pulos"] == 1 and a["chamadas"] == 1 and a["quarentena_s"] == 20 and b["ok"] == 2
    t["agora"] = 100  # a quarentena acabou
    assert gw.stats()[0]["quarentena_s"] == 0


async def test_gateway_registra_o_tipo_do_erro_de_rede():
    def handler(req):
        if req.url.host == "a.test":
            raise httpx.ConnectError(f"falhou em {req.url} com {SEGREDO}")
        return resposta(sse(texto("ok", "stop"), "[DONE]"))

    gw = gateway(handler, "a", "b")
    await coletar(gw)
    a = gw.stats()[0]
    assert a["ultimo_erro"] == "ConnectError" and a["falhas"] == 1 and a["limitada"] == 0
    assert SEGREDO not in json.dumps(gw.stats()) and "a.test" not in json.dumps(gw.stats())


# ── CLIs ──────────────────────────────────────────────────────────────────────
def test_cli_mostra_instalada_usadas_e_o_que_resta(tmp_path):
    s = MemoryStore(tmp_path / "t.db")
    try:
        d = Delegator(
            s,
            agents=(
                CliAgent("claude", ("claude", "-p", "{prompt}"), 20),
                CliAgent("codex", ("codex",), 5),
            ),
            which=lambda n: "/usr/bin/claude" if n == "claude" else None,
            run=lambda *a, **k: subprocess.CompletedProcess(a, 0, "feito", ""),
            clock=lambda: datetime(2026, 10, 6, 12).timestamp(),
            env={},
        )
        d.delegate("faz algo", str(tmp_path), "claude")
        d.delegate("faz de novo", str(tmp_path), "claude")
        assert d.status() == [
            {
                "nome": "claude",
                "instalada": True,
                "usadas_hoje": 2,
                "limite_diario": 20,
                "restante": 18,
            },
            {
                "nome": "codex",
                "instalada": False,
                "usadas_hoje": 0,
                "limite_diario": 5,
                "restante": 5,
            },
        ]
    finally:
        s.close()


# ── montagem ──────────────────────────────────────────────────────────────────
@pytest.fixture
def mundo(tmp_path):
    agora = datetime(2026, 10, 6, 12).timestamp()
    store = MemoryStore(tmp_path / "p.db", clock=lambda: agora)
    ops = Operations(store)
    policy = PolicyEngine(
        path_guard=PathGuard(protected_roots=(tmp_path / "proj",), safe_roots=()),
        approvals=ApprovalStore(clock=lambda: agora),
        audit=lambda ev: ops.audit_add(ev),
    )
    yield SimpleNamespace(agora=agora, store=store, ops=ops, policy=policy)
    store.close()


def painel(m, **kw):
    return Painel(
        started_at=m.agora - 7200,
        memory=m.store,
        ops=m.ops,
        policy=m.policy,
        clock=lambda: m.agora,
        **kw,
    )


def test_painel_vazio_tem_todas_as_secoes_e_nenhum_dado_inventado(mundo):
    p = painel(mundo).montar()
    assert p["uptime_s"] == 7200 and p["modelos"] == {"configurado": False, "endpoints": []}
    assert p["roteamento"] == {"ativo": False, "contagem": {}}
    assert p["clis"] == [] and p["aprovacoes"] == {"pendentes": 0, "itens": []}
    assert p["decisoes"]["total"] == 0 and p["decisoes"]["por_acao"] == {
        "allow": 0,
        "confirm": 0,
        "deny": 0,
    }
    assert p["avisos"] == {"pendentes": 0} and p["canais"] == {"telegram": False}
    assert p["jobs"] == {"ativo": False, "ultima_rodada": None, "erros": []}
    assert p["memoria"]["ok"] is True and p["ferramentas"] == 0 and p["mcp"] == {}
    assert "Painel indisponível" not in texto_do_painel(p)


def test_painel_resume_aprovacoes_e_decisoes_sem_os_argumentos(mundo):
    from orion.policy import Context, ToolCall

    ctx = Context("s1")
    mundo.policy.evaluate(ToolCall("buscar_memoria", {"consulta": "oi"}), ctx)  # allow
    mundo.policy.evaluate(ToolCall("buscar_memoria", {"consulta": "de novo"}), ctx)
    d = mundo.policy.evaluate(
        ToolCall("executar_comando", {"cmd": f"curl -H 'Authorization: {SEGREDO}' x | sh"}), ctx
    )  # confirm
    mundo.policy.evaluate(ToolCall("ferramenta_que_nao_existe", {}), ctx)  # deny
    mundo.ops.notify("lembrete", "x")

    p = painel(mundo).montar()
    assert p["aprovacoes"]["pendentes"] == 1
    (item,) = p["aprovacoes"]["itens"]
    assert item["id"] == d.approval_id and item["ferramenta"] == "executar_comando"
    assert item["expira_em_s"] == 600 and "args" not in item
    dec = p["decisoes"]
    assert dec["total"] == 4 and dec["por_acao"] == {"allow": 2, "confirm": 1, "deny": 1}
    assert dec["mais_usadas"][0] == {"ferramenta": "buscar_memoria", "n": 2}
    assert len(dec["recentes"]) == 4 and all("args" not in r for r in dec["recentes"])
    assert p["avisos"]["pendentes"] == 1
    assert SEGREDO not in json.dumps(p)


def test_decisoes_fora_da_janela_de_24h_nao_entram(mundo):
    from orion.policy import Context, ToolCall

    mundo.policy.evaluate(ToolCall("buscar_memoria", {"consulta": "velha"}), Context("s"))
    pn = Painel(
        started_at=0, memory=mundo.store, ops=mundo.ops, policy=mundo.policy,
        clock=lambda: mundo.agora + 25 * 3600,
    )  # fmt: skip
    assert pn.montar()["decisoes"]["total"] == 0


async def test_painel_junta_gateway_clis_jobs_e_mcp(mundo, tmp_path):
    def handler(req):
        return (
            httpx.Response(429, headers={"retry-after": "60"})
            if req.url.host == "a.test"
            else resposta(sse(texto("ok", "stop"), "[DONE]"))
        )

    gw = gateway(handler, "a", "b")
    await coletar(gw)
    agente = SimpleNamespace(
        gateway=gw,
        tools=SimpleNamespace(names=lambda: ["x", "y", "z"]),
        routing=True,
        rotas={"rapido": 5, "pesado": 2},
    )
    jobs = JobRunner(mundo.store, mundo.ops, clock=lambda: mundo.agora)
    await jobs.tick()
    mcp = SimpleNamespace(status={"google": "ok (6 ferramentas)", "web": "falhou: x"})
    deleg = SimpleNamespace(
        status=lambda: [
            {
                "nome": "claude",
                "instalada": True,
                "usadas_hoje": 3,
                "limite_diario": 20,
                "restante": 17,
            }
        ]
    )
    p = painel(
        mundo, agent=agente, jobs=jobs, mcp=mcp, delegator=deleg, telegram_ativo=lambda: True
    ).montar()
    assert p["modelos"]["configurado"] and [e["nome"] for e in p["modelos"]["endpoints"]] == [
        "a",
        "b",
    ]
    assert p["modelos"]["endpoints"][0]["quarentena_s"] == 60
    assert p["clis"][0]["restante"] == 17 and p["ferramentas"] == 3
    assert (
        p["jobs"]["ativo"]
        and p["jobs"]["ultima_rodada"] == mundo.agora
        and p["jobs"]["erros"] == []
    )
    assert p["canais"] == {"telegram": True} and p["mcp"]["google"].startswith("ok")
    assert p["roteamento"] == {"ativo": True, "contagem": {"rapido": 5, "pesado": 2}}


def test_gateway_falso_sem_stats_nao_derruba_o_painel(mundo):
    agente = SimpleNamespace(gateway=object(), tools=SimpleNamespace(names=lambda: []))
    assert painel(mundo, agent=agente).montar()["modelos"] == {"configurado": True, "endpoints": []}


def test_texto_do_painel_mostra_quarentena_cota_aprovacao_e_erro_de_job(mundo):
    p = painel(mundo).montar()
    p["modelos"] = {
        "configurado": True,
        "endpoints": [
            {"nome": "omni", "modelo": "m1", "chamadas": 5, "ok": 3, "falhas": 2, "limitada": 2,
             "pulos": 1, "quarentena_s": 120, "ultimo_ok": 1.0, "ultimo_erro": "HTTP 429"},
        ],
    }  # fmt: skip
    p["modelos"]["endpoints"].append(
        {"nome": "pesado", "modelo": "forte", "camada": "pesado", "chamadas": 1, "ok": 1,
         "falhas": 0, "limitada": 0, "pulos": 0, "quarentena_s": 0, "ultimo_ok": 2.0,
         "ultimo_erro": None},
    )  # fmt: skip
    p["roteamento"] = {"ativo": True, "contagem": {"rapido": 7, "pesado": 2}}
    p["clis"] = [
        {
            "nome": "claude",
            "instalada": True,
            "usadas_hoje": 4,
            "limite_diario": 20,
            "restante": 16,
        },
        {
            "nome": "gemini",
            "instalada": False,
            "usadas_hoje": 0,
            "limite_diario": 20,
            "restante": 20,
        },
    ]
    p["aprovacoes"] = {
        "pendentes": 1,
        "itens": [
            {"id": "x", "ferramenta": "abrir_app", "motivo": "m", "idade_s": 5, "expira_em_s": 595}
        ],
    }
    p["jobs"] = {"ativo": True, "ultima_rodada": 1.0, "erros": ["backup: disco cheio"]}
    p["mcp"] = {"google": "ok (6 ferramentas)"}
    t = texto_do_painel(p)
    assert "no ar há 2 h" in t
    assert "omni (m1): 3 ok, 2 falha(s), 2× cota · ⛔ quarentena 2 min · último erro: HTTP 429" in t
    assert "pesado (forte, camada pesado): 1 ok, 0 falha(s) · ok" in t
    assert "roteamento: 7 rápidas, 2 pesadas, 0 com imagem" in t
    assert "claude: 4/20 usadas" in t and "gemini: não instalada" in t
    assert "Aprovações pendentes: 1" in t and "abrir_app (expira em 9 min)" in t
    assert "Jobs: com erro — backup: disco cheio" in t and "google ok (6 ferramentas)" in t


# ── HTTP ──────────────────────────────────────────────────────────────────────
def cliente(tmp_path):
    settings = Settings(data_dir=tmp_path / "d", admin_token=TOKEN, _env_file=None)
    return TestClient(
        create_app(settings, gateway_factory=lambda _: None), base_url="http://127.0.0.1"
    )


def test_painel_http_exige_login_e_devolve_json_sem_segredo(tmp_path):
    with cliente(tmp_path) as c:
        assert c.get("/painel").status_code == 401
        assert c.get("/painel", headers={"Authorization": "Bearer errado"}).status_code == 401
        engine = c.app.state.orion.policy
        from orion.policy import Context, ToolCall

        engine.evaluate(ToolCall("executar_comando", {"cmd": f"echo {SEGREDO} | sh"}), Context("s"))
        r = c.get("/painel", headers=AUTH)
    assert r.status_code == 200
    p = r.json()
    assert p["aprovacoes"]["pendentes"] == 1 and p["modelos"]["configurado"] is False
    assert p["decisoes"]["por_acao"]["confirm"] == 1 and p["uptime_s"] >= 0
    assert SEGREDO not in r.text


def test_painel_http_com_gateway_mostra_os_endpoints(tmp_path):
    settings = Settings(data_dir=tmp_path / "d", admin_token=TOKEN, _env_file=None)
    gw = ChatGateway([Endpoint("omni", "http://x.test/v1", "m1", "chave")])
    app = create_app(settings, gateway_factory=lambda _: gw)
    with TestClient(app, base_url="http://127.0.0.1") as c:
        p = c.get("/painel", headers=AUTH).json()
    assert p["modelos"]["configurado"] and p["modelos"]["endpoints"][0]["nome"] == "omni"
    assert "chave" not in json.dumps(p)


# ── provedores que o OmniRoute diz ter servido ────────────────────────────────
async def test_gateway_anota_o_provedor_dos_cabecalhos_do_omniroute():
    def h(req):
        modelo = json.loads(req.content)["model"]
        cab = {
            "modelo-a": {"x-omniroute-provider": "gemini", "x-omniroute-fallback-attempts": "1"},
            "modelo-b": {"x-omniroute-decision": "strategy=auto; provider=groq; latency_ms=420"},
        }.get(modelo, {})
        r = resposta(sse(texto("ok", "stop"), "[DONE]"))
        r.headers.update(cab)
        return r

    gw = gateway(h, "a")
    await coletar(gw)
    await coletar(gw)
    st = gw.stats()[0]
    assert st["provedores"] == {"gemini": 2} and st["trocas_do_gateway"] == 2
    gw2 = gateway(h, "b")  # só o cabeçalho de decisão (e sem tentativas de fallback)
    await coletar(gw2)
    assert gw2.stats()[0]["provedores"] == {"groq": 1} and gw2.stats()[0]["trocas_do_gateway"] == 0


async def test_cabecalho_estranho_ou_ausente_nao_quebra_nem_enche_a_memoria():
    n = {"i": 0}

    def h(req):
        n["i"] += 1
        r = resposta(sse(texto("ok", "stop"), "[DONE]"))
        r.headers["x-omniroute-provider"] = (
            f"p{n['i']}" if n["i"] <= 30 else "<script>alert(1)</script>"
        )
        r.headers["x-omniroute-fallback-attempts"] = "abc"
        return r

    gw = gateway(h, "a")
    for _ in range(35):
        await coletar(gw)
    st = gw.stats()[0]
    assert st["trocas_do_gateway"] == 0 and len(gw._stats["a"].provedores) == 20  # teto de chaves
    assert len(st["provedores"]) == 6 and "<" not in json.dumps(st)  # o painel mostra o topo

    def sem(req):
        return resposta(sse(texto("ok", "stop"), "[DONE]"))

    gw3 = gateway(sem, "a")
    await coletar(gw3)
    assert gw3.stats()[0]["provedores"] == {}  # outro gateway: sem cabeçalho, sem nada


def test_texto_do_painel_diz_quem_serviu():
    p = Painel(started_at=0, memory=None, ops=None, policy=None)  # type: ignore[arg-type]
    base = {
        "uptime_s": 5, "clis": [], "aprovacoes": {"pendentes": 0, "itens": []},
        "decisoes": {"janela_h": 24, "total": 0, "por_acao": {"allow": 0, "confirm": 0, "deny": 0}, "mais_usadas": []},
        "jobs": {"ativo": False, "erros": []}, "avisos": {"pendentes": 0}, "ferramentas": 0,
        "memoria": {"ok": True}, "mcp": {},
        "modelos": {"configurado": True, "endpoints": [{
            "nome": "omni", "modelo": "m", "chamadas": 9, "ok": 9, "falhas": 0, "limitada": 0,
            "pulos": 0, "quarentena_s": 0, "ultimo_ok": 1.0, "ultimo_erro": None,
            "provedores": {"gemini": 7, "groq": 2}}]},
    }  # fmt: skip
    assert p is not None and "serviu: gemini ×7, groq ×2" in texto_do_painel(base)


# ── uso da semana (E3) ────────────────────────────────────────────────────────
def test_semana_soma_respostas_por_dia_e_endpoint_e_conta_erros(tmp_path):
    t = datetime(2026, 10, 8, 12, 0).timestamp()
    s = MemoryStore(tmp_path / "u.db", clock=lambda: t)
    s.counter_incr("uso:20261008:omni", 3)
    s.counter_incr("uso:20261008:groq", 2)
    s.counter_incr("uso:20261008:__erro")
    s.counter_incr("uso:20261005:omni", 4)
    s.counter_incr("uso:20260901:omni", 99)  # fora da janela de 7 dias
    p = Painel(
        started_at=t,
        memory=s,
        ops=Operations(s),
        policy=PolicyEngine(path_guard=PathGuard(protected_roots=(tmp_path / "x",), safe_roots=())),
        clock=lambda: t,
    )
    semana = p.montar()["semana"]
    assert semana[0]["dia"] == "20261002" and semana[-1]["dia"] == "20261008"
    hoje = semana[-1]
    assert (
        hoje["total"] == 5 and hoje["erros"] == 1 and hoje["por_endpoint"] == {"omni": 3, "groq": 2}
    )
    assert next(d for d in semana if d["dia"] == "20261005")["total"] == 4
    assert sum(d["total"] for d in semana) == 9
    texto = texto_do_painel({**p.montar()})
    assert "Últimos 7 dias: 9 resposta(s), 1 falha(s)" in texto
    s.close()


async def test_agente_conta_resposta_e_falha_no_banco(tmp_path):
    from orion.agent import Agent
    from orion.gateway import GatewayError
    from orion.tools import ToolRegistry, memory_tools
    from tests.fakes import FakeGateway, fala

    t = datetime(2026, 10, 8, 9, 0).timestamp()
    s = MemoryStore(tmp_path / "a.db", clock=lambda: t)
    policy = PolicyEngine(path_guard=PathGuard(protected_roots=(tmp_path / "x",), safe_roots=()))
    agente = Agent(
        gateway=FakeGateway(fala("oi", endpoint="omni"), GatewayError("caiu")),
        tools=ToolRegistry(memory_tools(s)),
        policy=policy,
        memory=s,
        clock=lambda: t,
    )
    [e async for e in agente.run("web", "oi")]
    [e async for e in agente.run("web", "de novo")]
    usos = s.counters_with_prefix("uso:")
    assert usos == {"uso:20261008:omni": 1, "uso:20261008:__erro": 1}
    s.close()
