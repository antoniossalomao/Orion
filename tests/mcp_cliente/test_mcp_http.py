"""MCP por Streamable HTTP (C10): servidor de verdade do SDK numa porta local, com e sem cabeçalho."""

import socket
import subprocess
import sys
import time
from pathlib import Path

import httpx
import pytest

from orion.mcp_client import McpConfig, McpManager, ServerConfig, ToolRule
from orion.policy import Risk

SERVIDOR = str(Path(__file__).with_name("fake_server.py"))


def _porta() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


@pytest.fixture(scope="module")
def url():
    porta = _porta()
    proc = subprocess.Popen(
        [sys.executable, SERVIDOR, "http", str(porta)],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    alvo = f"http://127.0.0.1:{porta}/mcp"
    for _ in range(100):
        try:
            httpx.get(alvo, timeout=0.5)  # qualquer resposta (até 4xx) = o servidor subiu
            break
        except httpx.HTTPError:
            time.sleep(0.2)
    else:
        proc.kill()
        pytest.skip("o servidor HTTP de teste não subiu")
    yield alvo
    proc.terminate()
    proc.wait(timeout=10)


def test_config_exige_comando_ou_url_e_valida_o_endereco():
    ServerConfig(command="x")
    ServerConfig(url="https://mcp.exemplo.com/mcp", headers={"Authorization": "Bearer ${TOKEN}"})
    for ruim in (
        {},
        {"command": "x", "url": "http://a/mcp"},
        {"url": "ftp://a/mcp"},
        {"url": "http://user:senha@a/mcp"},
        {"url": "http://a/mcp", "args": ["x"]},
        {"url": "http://a/mcp", "env": {"A": "b"}},
        {"command": "x", "headers": {"A": "b"}},
    ):
        with pytest.raises(ValueError):
            ServerConfig(**ruim)


def test_conecta_por_http_lista_e_chama(url):
    g = McpManager(
        McpConfig(
            servers={
                "remoto": ServerConfig(
                    url=url, timeout_s=20, tools={"somar": ToolRule(risk=Risk.READ)}
                )
            }
        ),
        connect_timeout_s=30,
    )
    try:
        tools = g.start()
        nomes = {t.name for t in tools}
        assert "remoto__somar" in nomes
        r = g.call("remoto__somar", {"a": 2, "b": 3})
        assert "5" in str(r) and "erro" not in r
        assert g.specs["remoto__somar"].risk is Risk.READ
        assert g.specs["remoto__apagar"].risk is Risk.EXEC  # o que não foi classificado confirma
    finally:
        g.stop()


def test_segredo_do_cabecalho_vem_do_cofre_e_falta_vira_erro_claro(url):
    g = McpManager(
        McpConfig(
            servers={
                "remoto": ServerConfig(url=url, headers={"Authorization": "Bearer ${TOKEN_MCP}"})
            }
        ),
        connect_timeout_s=10,
        secret_lookup=lambda nome: None,
    )
    g.start()
    assert "TOKEN_MCP" in g.status["remoto"] and "remoto__somar" not in g.specs
    g.stop()


def test_servidor_remoto_fora_do_ar_falha_sem_vazar_o_cabecalho():
    g = McpManager(
        McpConfig(
            servers={
                "morto": ServerConfig(
                    url=f"http://127.0.0.1:{_porta()}/mcp",
                    headers={"Authorization": "Bearer SEGREDO-XYZ"},
                )
            }
        ),
        connect_timeout_s=5,
    )
    try:
        g.start()
    except Exception as e:
        assert "SEGREDO-XYZ" not in str(e)
    assert g.status.get("morto", "").startswith(("falhou", "erro")) or "morto" not in g.specs
    g.stop()
