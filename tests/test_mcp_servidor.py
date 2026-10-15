"""E7.7 (REGRAS.md: o Orion como servidor MCP só lê): o Orion como servidor MCP só de leitura, com o SDK oficial por stdio."""

import json
import os
import subprocess
import sys

import pytest
from mcp import Client, StdioServerParameters

from orion.mcp_servidor import (
    AcessoNegado,
    Consultas,
    conferir,
    gerar_token,
    revogar,
)
from orion.memory import MemoryStore
from orion.projects import Projects


@pytest.fixture
def banco(tmp_path):
    caminho = tmp_path / "dados" / "orion.db"
    caminho.parent.mkdir()
    m = MemoryStore(caminho)
    s = m.new_session("web", "Plano da semana")
    m.add_message(s.id, "user", "preciso estudar redes neurais amanhã")
    m.add_fact("Antônio estuda Sistemas de Informação na Unimar", "manual")
    projeto = Projects(m).create("Alfa")
    sp = m.new_session("web", "Conversa do Alfa", project_id=projeto["id"])
    m.add_message(sp.id, "user", "o cliente do alfa quer relatório mensal")
    from orion.memory.scope import data_scope

    with data_scope(projeto["id"], include_personal=False):
        m.add_fact("O Alfa usa PostgreSQL", "manual")
    yield m, caminho, projeto
    m.close()


def test_token_aparece_uma_vez_so_o_hash_fica_e_revogar_vale_na_hora(banco):
    m, _, _ = banco
    with pytest.raises(AcessoNegado, match="revogado ou nunca gerado"):
        conferir(m, "qualquer")
    t1 = gerar_token(m)
    assert t1.startswith("omcp_") and t1 not in (m.meta_get("mcp_servidor:token_sha256") or "")
    conferir(m, t1)
    t2 = gerar_token(m)  # gerar de novo invalida o anterior
    with pytest.raises(AcessoNegado, match="inválido"):
        conferir(m, t1)
    conferir(m, t2)
    assert revogar(m) is True and revogar(m) is False
    with pytest.raises(AcessoNegado):
        conferir(m, t2)
    with pytest.raises(AcessoNegado):
        conferir(m, None)


def test_leituras_respeitam_o_projeto_e_vao_ao_audit_com_o_cliente(banco):
    m, _, projeto = banco
    token = gerar_token(m)
    c = Consultas(m, lambda: token)
    pessoal = c.buscar_memoria("Unimar", cliente="claude-code")
    assert pessoal and "Unimar" in pessoal[0]["texto"]
    assert c.buscar_memoria("PostgreSQL") == []  # a pessoal não enxerga o projeto
    assert c.buscar_memoria("PostgreSQL", "Alfa")[0]["tipo"] == "fact"
    assert c.buscar_memoria("Unimar", projeto["id"]) == []  # nem o projeto, a pessoal
    assert [f["texto"] for f in c.listar_fatos()] == [
        "Antônio estuda Sistemas de Informação na Unimar"
    ]
    assert [f["texto"] for f in c.listar_fatos("alfa")] == ["O Alfa usa PostgreSQL"]
    conv = c.buscar_conversas("redes neurais")
    assert [x["titulo"] for x in conv] == ["Plano da semana"]
    assert [x["titulo"] for x in c.buscar_conversas("relatório mensal", "Alfa")] == [
        "Conversa do Alfa"
    ]
    assert c.buscar_conversas("relatório mensal") == []
    with pytest.raises(ValueError, match="não existe"):
        c.listar_fatos("Fantasma")
    audit = m.query("SELECT tool, action, reason, args FROM audit ORDER BY id")
    assert {r["tool"] for r in audit} >= {
        "mcp_servidor_buscar_memoria",
        "mcp_servidor_listar_fatos",
    }
    assert any("cliente=claude-code" in r["reason"] for r in audit)
    assert all(r["action"] == "allow" for r in audit)
    assert "Unimar" not in "".join(r["args"] or "" for r in audit)  # a consulta não vai ao audit


def test_revogar_derruba_a_proxima_chamada_sem_reiniciar(banco):
    m, _, _ = banco
    token = gerar_token(m)
    c = Consultas(m, lambda: token)
    assert c.listar_fatos()
    revogar(m)
    with pytest.raises(AcessoNegado):
        c.listar_fatos()


def _params(caminho, token):
    env = {k: v for k, v in os.environ.items() if not k.startswith("ORION_")}
    env["ORION_DATA_DIR"] = str(caminho.parent)
    if token is not None:
        env["ORION_MCP_SERVER_TOKEN"] = token
    return StdioServerParameters(
        command=sys.executable, args=["-m", "orion", "mcp-servidor"], env=env
    )


async def test_pelo_protocolo_lista_exatamente_tres_ferramentas_de_leitura(banco):
    m, caminho, _ = banco
    token = gerar_token(m)
    async with Client(_params(caminho, token), read_timeout_seconds=30) as cli:
        nomes = sorted(t.name for t in (await cli.list_tools()).tools)
        assert nomes == ["buscar_conversas", "buscar_memoria", "listar_fatos"]
        r = await cli.call_tool("buscar_memoria", {"consulta": "Unimar"})
        assert not r.is_error and "Unimar" in json.dumps(r.structured_content or r.content[0].text)
        r = await cli.call_tool("listar_fatos", {"projeto": "Fantasma"})
        assert r.is_error
    registro = m.query("SELECT reason FROM audit WHERE tool='mcp_servidor_buscar_memoria'")
    assert registro and registro[0]["reason"].startswith("cliente=")
    assert registro[0]["reason"] != "cliente=?"  # o nome veio do initialize


async def test_revogado_no_meio_a_chamada_seguinte_falha(banco):
    m, caminho, _ = banco
    token = gerar_token(m)
    async with Client(_params(caminho, token), read_timeout_seconds=30) as cli:
        assert not (await cli.call_tool("listar_fatos", {})).is_error
        revogar(m)
        r = await cli.call_tool("listar_fatos", {})
        assert r.is_error and "revogado" in json.dumps([c.text for c in r.content])


@pytest.mark.parametrize("token", [None, "omcp_errado"])
def test_sem_token_valido_o_processo_nem_sobe(banco, token):
    m, caminho, _ = banco
    gerar_token(m)
    env = _params(caminho, token).env
    r = subprocess.run(
        [sys.executable, "-m", "orion", "mcp-servidor"],
        env=env, capture_output=True, text=True, timeout=60, stdin=subprocess.DEVNULL,
    )  # fmt: skip
    assert r.returncode == 2 and r.stdout == ""  # stdout é do protocolo: nada vaza nele
    assert "orion mcp-servidor:" in r.stderr


def test_cli_token_e_revogar(banco):
    m, caminho, _ = banco
    env = _params(caminho, None).env
    r = subprocess.run(
        [sys.executable, "-m", "orion", "mcp-servidor", "--token"],
        env=env, capture_output=True, text=True, timeout=60,
    )  # fmt: skip
    assert r.returncode == 0 and "claude mcp add orion -e ORION_MCP_SERVER_TOKEN=omcp_" in r.stdout
    token = r.stdout.split("ORION_MCP_SERVER_TOKEN=")[1].split()[0]
    conferir(m, token)
    r = subprocess.run(
        [sys.executable, "-m", "orion", "mcp-servidor", "--revogar"],
        env=env, capture_output=True, text=True, timeout=60,
    )  # fmt: skip
    assert "revogado" in r.stdout
    with pytest.raises(AcessoNegado):
        conferir(m, token)
