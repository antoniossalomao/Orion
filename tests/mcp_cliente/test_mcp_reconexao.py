"""C12: servidor MCP que cai volta na próxima chamada, sem repetir a que o encontrou fora do ar."""

import sys
import time
from pathlib import Path

import pytest

from orion.mcp_client import McpConfig, McpManager, ServerConfig, ToolRule
from orion.policy import Risk

SERVIDOR = str(Path(__file__).with_name("fake_server.py"))


@pytest.fixture
def gerente():
    g = McpManager(
        McpConfig(
            servers={
                "fake": ServerConfig(
                    command=sys.executable,
                    args=[SERVIDOR],
                    timeout_s=20,
                    tools={
                        "somar": ToolRule(risk=Risk.READ),
                        "sair": ToolRule(risk=Risk.READ),
                    },
                )
            }
        ),
        connect_timeout_s=40,
    )
    g.start()
    yield g
    g.stop()


def test_servidor_que_morre_durante_a_chamada_reconecta_na_proxima(gerente):
    assert "5" in str(gerente.call("fake__somar", {"a": 2, "b": 3}))
    morto = gerente.call("fake__sair", {})
    assert "caiu durante a chamada" in morto["erro"] and "caiu" in gerente.status["fake"]
    # a chamada seguinte reconecta, mas NÃO é repetida
    gerente.RECONECTAR_A_CADA_S = 0
    volta = gerente.call("fake__somar", {"a": 2, "b": 3})
    assert "reconectado agora" in volta["erro"] and "NÃO foi repetida" in volta["erro"]
    assert gerente.status["fake"] == "ok (reconectado)"
    # e a seguinte funciona normalmente
    assert "5" in str(gerente.call("fake__somar", {"a": 2, "b": 3}))


def test_tentativas_de_reconexao_tem_intervalo(gerente):
    gerente.call("fake__sair", {})
    t0 = time.monotonic()
    gerente._tentou["fake"] = t0  # acabou de tentar
    r = gerente.call("fake__somar", {"a": 1, "b": 1})
    assert "nova tentativa de conexão em breve" in r["erro"]
    assert "fake" in gerente._caiu  # continua marcado como caído


def test_parar_o_gerente_depois_de_reconectar_nao_deixa_tarefa_pendurada(gerente):
    gerente.call("fake__sair", {})
    gerente.RECONECTAR_A_CADA_S = 0
    gerente.call("fake__somar", {"a": 1, "b": 1})
    gerente.stop()
    assert gerente._loop is None
