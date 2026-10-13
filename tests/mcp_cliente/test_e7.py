"""E7: protocolo incompatível, cancelamento, diagnóstico, índice, recursos e prompts de MCP,
sempre com um servidor MCP de verdade (JSON-RPC por stdio) em `servidor_cru.py`."""

import asyncio
import json
import sys
import time
from pathlib import Path

import pytest
from fastapi.testclient import TestClient

from orion.agent import Agent
from orion.app import create_app
from orion.config import Settings
from orion.mcp_client import (
    SPEC_RECURSO,
    McpConfig,
    McpManager,
    ServerConfig,
    describe,
)
from orion.mcp_indice import SPEC as SPEC_INDICE
from orion.mcp_indice import Indice
from orion.memory import MemoryStore
from orion.policy import ApprovalStore, PathGuard, PolicyEngine
from orion.policy.classes import Risk, ToolSpec
from orion.tools import ToolRegistry
from tests.fakes import FakeGateway, chama, fala, pede

CRU = str(Path(__file__).with_name("servidor_cru.py"))
TOKEN = "token-de-teste-com-16+"
AUTH = {"Authorization": f"Bearer {TOKEN}"}


def cru(env=None, **kw):
    return ServerConfig(command=sys.executable, args=[CRU], env=env or {}, timeout_s=20, **kw)


def gerente(servers, **kw):
    g = McpManager(McpConfig(servers=servers), connect_timeout_s=30, **kw)
    g.start()
    return g


# ── E7.1: protocolo ───────────────────────────────────────────────────────────
@pytest.mark.parametrize("versao", ["2024-11-05", "1900-01-01"])
def test_servidor_com_protocolo_nao_comprovado_e_recusado_com_mensagem_clara(versao):
    g = gerente({"velho": cru({"PROTOCOLO": versao})})
    try:
        assert g.tools == [] and g.status["velho"].startswith("incompatível: ")
        assert f"o servidor velho fala o protocolo {versao}, o Orion fala" in g.status["velho"]
        assert "2025-11-25" in g.status["velho"] and "2026-07-28" in g.status["velho"]
        d = g.diagnostico()[0]
        assert d["estado"] == "incompatível" and d["ultima_falha"]["mensagem"] in g.status["velho"]
        assert any("incompatível" in linha for linha in describe(g))
    finally:
        g.stop()


def test_protocolo_comprovado_passa_e_aparece_no_diagnostico():
    g = gerente({"novo": cru({"PROTOCOLO": "2025-11-25"})})
    try:
        assert g.status["novo"].startswith("ok") and g.diagnostico()[0]["protocolo"] == "2025-11-25"
    finally:
        g.stop()


def test_mcp_check_sai_com_erro_e_explica_o_protocolo(tmp_path, capsys):
    from orion.__main__ import _mcp_check

    conf = tmp_path / "mcp.json"
    conf.write_text(
        json.dumps(
            {
                "servers": {
                    "velho": {
                        "command": sys.executable,
                        "args": [CRU],
                        "env": {"PROTOCOLO": "2024-11-05"},
                        "timeout_s": 20,
                    }
                }
            }
        ),
        encoding="utf-8",
    )
    assert _mcp_check(conf) == 1
    assert "o servidor velho fala o protocolo 2024-11-05" in capsys.readouterr().out


# ── E7.2: cancelamento ────────────────────────────────────────────────────────
def _esperar_linhas(arquivo, n=1, ate=8.0):
    fim = time.monotonic() + ate
    while time.monotonic() < fim:
        if arquivo.exists() and len(arquivo.read_text(encoding="utf-8").splitlines()) >= n:
            break
        time.sleep(0.1)
    return arquivo.read_text(encoding="utf-8").splitlines() if arquivo.exists() else []


async def test_cancelar_a_ferramenta_avisa_o_servidor(tmp_path):
    log = tmp_path / "cancelado.log"
    g = await asyncio.to_thread(
        gerente, {"cru": cru({"LOG_CANCELADO": str(log)}, default_risk=Risk.EXEC)}
    )
    try:
        dormir = next(t for t in g.tools if t.name == "cru__dormir")
        tarefa = asyncio.create_task(dormir.run_async({}))
        await asyncio.sleep(1.5)
        tarefa.cancel()
        with pytest.raises(asyncio.CancelledError):
            await tarefa
        linhas = await asyncio.to_thread(_esperar_linhas, log)
        assert linhas and "caller cancelled" in linhas[0]
        assert g.cancelamentos == 1
    finally:
        await asyncio.to_thread(g.stop)


async def test_cancelar_o_turno_do_agente_chega_ao_servidor(tmp_path):
    log = tmp_path / "cancelado.log"
    g = await asyncio.to_thread(gerente, {"cru": cru({"LOG_CANCELADO": str(log)})})
    store = MemoryStore(tmp_path / "a.db")
    try:
        policy = PolicyEngine(
            path_guard=PathGuard(protected_roots=(), safe_roots=(tmp_path,), system_roots=()),
            approvals=ApprovalStore(),
        )
        for spec in g.specs.values():
            policy.register_tool(ToolSpec(spec.name, Risk.READ))
        agent = Agent(
            gateway=FakeGateway(pede(chama("cru__dormir")), fala("fim")),
            tools=ToolRegistry(g.tools),
            policy=policy,
            memory=store,
        )

        async def turno():
            return [e async for e in agent.run("web", "durma")]

        t = asyncio.create_task(turno())
        await asyncio.sleep(2)
        t.cancel()
        with pytest.raises(asyncio.CancelledError):
            await t
        assert await asyncio.to_thread(_esperar_linhas, log)
    finally:
        store.close()
        await asyncio.to_thread(g.stop)


# ── E7.3: diagnóstico e teste ─────────────────────────────────────────────────
def test_diagnostico_lista_estado_falha_e_risco_sem_segredo():
    g = gerente(
        {
            "bom": cru({"SEGREDO": "valor-secreto"}, default_risk=Risk.WRITE),
            "ruim": ServerConfig(command="/nao/existe", timeout_s=5),
            "off": ServerConfig(command="x", enabled=False),
        }
    )
    try:
        por_nome = {d["nome"]: d for d in g.diagnostico()}
        assert por_nome["bom"]["estado"] == "ok"
        assert {f["risco"] for f in por_nome["bom"]["ferramentas"]} == {"write"}
        assert por_nome["ruim"]["estado"] == "falhou" and por_nome["ruim"]["ultima_falha"]
        assert por_nome["off"]["estado"] == "desligado"
        assert "valor-secreto" not in json.dumps(g.diagnostico())
    finally:
        g.stop()


def test_testar_servidor_de_pe_e_desconhecido_e_desligado():
    g = gerente({"bom": cru(), "off": ServerConfig(command="x", enabled=False)})
    try:
        r = g.testar("bom")
        assert r["ok"] and r["ferramentas"] == 2 and r["ms"] >= 0
        assert g.testar("nada") == {"ok": False, "erro": "servidor desconhecido"}
        assert not g.testar("off")["ok"]
    finally:
        g.stop()


def test_rotas_de_diagnostico_e_teste(tmp_path):
    g = McpManager(McpConfig(servers={"bom": cru()}), connect_timeout_s=30)
    app = create_app(
        Settings(data_dir=tmp_path / "d", admin_token=TOKEN, jobs_enabled=False, _env_file=None),
        gateway_factory=lambda _: FakeGateway(fala("oi")),
        mcp_factory=lambda _s: g,
    )
    with TestClient(app, base_url="http://127.0.0.1") as c:
        assert c.get("/mcp/servidores").status_code == 401
        r = c.get("/mcp/servidores", headers=AUTH).json()
        assert r["ativo"] and r["servidores"][0]["nome"] == "bom"
        assert c.post("/mcp/servidores/bom/testar", headers=AUTH).json()["ok"]
        assert c.post("/mcp/servidores/zzz/testar", headers=AUTH).status_code == 404
        assert c.post("/mcp/servidores/bom/testar").status_code == 401


def test_sem_mcp_a_rota_diz_que_nao_ha_servidor(tmp_path):
    app = create_app(
        Settings(data_dir=tmp_path / "d", admin_token=TOKEN, jobs_enabled=False, _env_file=None),
        gateway_factory=lambda _: FakeGateway(fala("oi")),
    )
    with TestClient(app, base_url="http://127.0.0.1") as c:
        assert c.get("/mcp/servidores", headers=AUTH).json() == {"ativo": False, "servidores": []}
        assert c.post("/mcp/servidores/x/testar", headers=AUTH).status_code == 409


# ── E7.4: descoberta sob demanda ──────────────────────────────────────────────
def test_indice_so_liga_acima_do_limite_e_acha_por_assunto():
    g = gerente({"cru": cru({"N_FERRAMENTAS": "40"})})
    try:
        assert len(g.tools) == 42
        assert not Indice(g.tools, 42).ativo and Indice(g.tools, 30).ativo
        idx = Indice(g.tools, 30)
        achadas = [t.name for t in idx.procurar("agenda")]
        assert achadas and all(int(n.split("_")[-1]) % 4 == 1 for n in achadas)
        assert "cru__extra_001: Ferramenta extra" in idx.bloco()
        assert idx.procurar("zzzzqqqq") == []
    finally:
        g.stop()


async def test_agente_so_mostra_o_esquema_depois_de_procurar(tmp_path):
    g = await asyncio.to_thread(gerente, {"cru": cru({"N_FERRAMENTAS": "40"})})
    store = MemoryStore(tmp_path / "a.db")
    try:
        policy = PolicyEngine(
            path_guard=PathGuard(protected_roots=(), safe_roots=(tmp_path,), system_roots=()),
            approvals=ApprovalStore(),
        )
        for spec in g.specs.values():
            policy.register_tool(ToolSpec(spec.name, Risk.READ))
        policy.register_tool(SPEC_INDICE)
        idx = Indice(g.tools, 30)
        gw = FakeGateway(
            pede(chama("procurar_ferramenta", consulta="agenda")),
            pede(chama("cru__extra_001")),
            fala("pronto"),
        )
        agent = Agent(
            gateway=gw,
            tools=ToolRegistry([*g.tools, idx.tool()]),
            policy=policy,
            memory=store,
            mcp_indice=idx,
        )
        eventos = [e async for e in agent.run("web", "veja minha agenda")]
        nomes = [{t["function"]["name"] for t in chamada} for chamada in gw.ferramentas]
        assert "procurar_ferramenta" in nomes[0]
        assert not any(n.startswith("cru__") for n in nomes[0])  # nenhum esquema MCP
        assert "cru__extra_001" in nomes[1] and "cru__extra_002" not in nomes[1]
        assert len(nomes[0]) < 15  # só as nativas + a de busca
        assert "[FERRAMENTAS DE SERVIDORES MCP: 42" in gw.chamadas[0][0]["content"]
        decisoes = [e.data["decision"] for e in eventos if e.kind == "tool"]
        assert decisoes == ["allow", "allow"]  # a política decidiu como sempre
    finally:
        store.close()
        await asyncio.to_thread(g.stop)


# ── E7.5: recursos e prompts ──────────────────────────────────────────────────
@pytest.fixture
def com_recursos():
    g = gerente({"cru": cru({"COM_RECURSOS": "1"}, resources=["file:///docs/*"], external=True)})
    yield g
    g.stop()


def test_recurso_so_com_uri_listada_e_na_lista_do_mcp_json(com_recursos):
    g = com_recursos
    ok = g.ler_recurso("cru", "file:///docs/guia.md")
    assert ok["texto"].startswith("# Guia")
    assert (
        "resources" in g.ler_recurso("cru", "file:///segredo/chave.txt")["erro"]
    )  # listada, mas fora
    assert "lista" in g.ler_recurso("cru", "file:///docs/inventado.md")["erro"]  # nem listada
    assert "não está conectado" in g.ler_recurso("outro", "file:///docs/guia.md")["erro"]


def test_recurso_grande_e_cortado_em_200_kb(com_recursos):
    r = com_recursos.ler_recurso("cru", "file:///docs/grande.txt")
    assert r["truncado"] and len(r["texto"].encode()) == 200 * 1024


def test_ferramenta_de_recurso_e_externa_e_so_existe_com_lista():
    assert SPEC_RECURSO.external and SPEC_RECURSO.risk is Risk.READ
    g = gerente({"cru": cru({"COM_RECURSOS": "1"})})
    try:
        assert not g.recursos_ativos()
    finally:
        g.stop()


async def test_ferramenta_ler_recurso_pelo_registro(com_recursos):
    tool = com_recursos.tool_recurso()
    r = json.loads(await tool.run_async({"servidor": "cru", "uri": "file:///docs/guia.md"}))
    assert "conteudo do guia" in r["texto"]
    ruim = json.loads(await tool.run_async({"servidor": "cru"}))
    assert ruim["codigo"] == "arguments_invalid"


def test_prompts_so_para_voce_e_com_argumentos(com_recursos):
    lista = com_recursos.listar_prompts()
    assert lista == [
        {
            "servidor": "cru",
            "nome": "revisar",
            "descricao": "Revisa um texto.",
            "argumentos": [{"nome": "texto", "obrigatorio": True}],
        }
    ]
    assert com_recursos.obter_prompt("cru", "revisar", {"texto": "olá"})["texto"] == "Revise: olá"
    assert "erro" in com_recursos.obter_prompt("zzz", "revisar", {})
    # o modelo não vê prompt: nenhuma ferramenta de prompt existe
    assert not [t for t in com_recursos.tools if "prompt" in t.name]


def test_rotas_de_prompt(tmp_path):
    g = McpManager(McpConfig(servers={"cru": cru({"COM_RECURSOS": "1"})}), connect_timeout_s=30)
    app = create_app(
        Settings(data_dir=tmp_path / "d", admin_token=TOKEN, jobs_enabled=False, _env_file=None),
        gateway_factory=lambda _: FakeGateway(fala("oi")),
        mcp_factory=lambda _s: g,
    )
    with TestClient(app, base_url="http://127.0.0.1") as c:
        assert c.get("/mcp/prompts").status_code == 401
        assert c.get("/mcp/prompts", headers=AUTH).json()["total"] == 1
        r = c.post("/mcp/prompts/cru/revisar", json={"argumentos": {"texto": "x"}}, headers=AUTH)
        assert r.json() == {"texto": "Revise: x", "truncado": False, "origem": "mcp:cru"}
        assert c.post("/mcp/prompts/zzz/revisar", json={}, headers=AUTH).status_code == 502
        assert c.post("/mcp/prompts/cru/revisar", json={"x": 1}, headers=AUTH).status_code == 422
