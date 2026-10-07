"""O /mcp do legado não aceita página de navegador de outro site (nem a de origem `null`)."""

import pytest
from fastapi import FastAPI
from fastapi.testclient import TestClient
from origem import (
    RecusaOrigemEstranhaNoMcp,
    origem_permitida,
    origem_permitida_no_mcp,
)
from starlette.middleware.trustedhost import TrustedHostMiddleware


@pytest.fixture
def cliente():
    app = FastAPI()
    app.add_middleware(TrustedHostMiddleware, allowed_hosts=["127.0.0.1", "localhost"])
    app.add_middleware(RecusaOrigemEstranhaNoMcp)

    @app.post("/mcp")
    def mcp():
        return {"ok": True}

    @app.get("/mcp/sub")
    def sub():
        return {"ok": True}

    @app.get("/historico")
    def historico():
        return {"ok": True}

    return TestClient(app, base_url="http://127.0.0.1")


def test_cliente_de_linha_de_comando_sem_origin_continua_funcionando(cliente):
    assert cliente.post("/mcp").status_code == 200
    assert cliente.get("/mcp/sub").status_code == 200


@pytest.mark.parametrize(
    "origem", ["https://site-malicioso.example", "null", "file://", "http://127.0.0.1:9999"]
)
def test_pagina_de_outro_site_ou_origem_null_e_recusada_no_mcp(cliente, origem):
    r = cliente.post("/mcp", headers={"Origin": origem})
    assert r.status_code == 403 and r.json() == {"detail": "origem nao permitida"}
    assert cliente.get("/mcp/sub", headers={"Origin": origem}).status_code == 403


def test_a_propria_pagina_do_cerebro_continua_podendo(cliente):
    for origem in ("http://127.0.0.1:8000", "http://localhost:8000"):
        assert cliente.post("/mcp", headers={"Origin": origem}).status_code == 200


def test_o_resto_da_api_nao_muda(cliente):
    # o pywebview manda `Origin: null` para a API normal: a regra do /mcp não vale fora dele
    assert cliente.get("/historico", headers={"Origin": "null"}).status_code == 200
    assert origem_permitida("null") is True
    assert origem_permitida_no_mcp("null") is False


def test_host_estranho_dns_rebinding_e_recusado(cliente):
    r = cliente.get("/historico", headers={"Host": "evil.example"})
    assert r.status_code == 400
    assert cliente.get("/historico", headers={"Host": "localhost:8000"}).status_code == 200
