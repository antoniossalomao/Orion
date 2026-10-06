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
    agente = SimpleNamespace(gateway=gw, tools=SimpleNamespace(names=lambda: ["x", "y", "z"]))
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
