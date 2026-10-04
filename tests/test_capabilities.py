"""Descoberta de recursos é pública; dados da instalação exigem autenticação."""

import json

import pytest
from fastapi.testclient import TestClient

from orion.app import create_app
from orion.capabilities import PENDING_FEATURES
from orion.config import Settings
from tests.fakes import FakeGateway, fala

TOKEN = "capabilities-test-token-16chars"
AUTH = {"Authorization": f"Bearer {TOKEN}"}


@pytest.mark.parametrize("gateway", [False, True])
@pytest.mark.parametrize("admin", [False, True])
def test_capacidades_refletem_agente_e_autenticacao(tmp_path, gateway, admin):
    settings = Settings(
        data_dir=tmp_path,
        admin_token=TOKEN if admin else "",
        jobs_enabled=False,
        _env_file=None,
    )
    gw = FakeGateway(fala("Olá"))
    app = create_app(settings, gateway_factory=lambda _: gw if gateway else None)
    with TestClient(app, base_url="http://127.0.0.1") as c:
        r = c.get("/capabilities")
        assert r.status_code == 200
        data = r.json()
        assert data["contract_version"] == 1 and data["backend"] == "orion"
        assert data["api"] == "online"
        assert data["model"] == ("ready" if gateway else "unavailable")
        assert data["features"]["chat"] is (gateway and admin)
        assert data["features"]["approvals"] is admin
        assert data["features"]["notifications"] is admin
        for name in PENDING_FEATURES:
            assert data["features"][name] is False
            assert data["unavailable"][name] == "not_implemented"
        if gateway and admin:
            assert "chat" not in data["unavailable"]
        else:
            assert data["unavailable"]["chat"] == (
                "gateway_not_configured" if admin else "auth_not_configured"
            )
        # Descoberta não testa o modelo nem produz mensagem.
        assert gw.chamadas == []


def test_descoberta_nao_expoe_dados_da_instalacao(tmp_path):
    settings = Settings(
        data_dir=tmp_path / "dados-privados",
        admin_token=TOKEN,
        gateway_url="https://gateway.example/v1",
        gateway_model="modelo-privado",
        gateway_api_key="chave-privada",
        jobs_enabled=False,
        _env_file=None,
    )
    app = create_app(settings, gateway_factory=lambda _: FakeGateway())
    with TestClient(app, base_url="http://127.0.0.1") as c:
        public = c.get("/capabilities").text
        for secret in (
            TOKEN,
            str(settings.data_dir),
            "gateway.example",
            "modelo-privado",
            "chave-privada",
        ):
            assert secret not in public
        assert c.get("/capabilities/details").status_code == 401
        assert (
            c.get("/capabilities/details", headers={"Authorization": "Bearer errado"}).status_code
            == 401
        )
        details = c.get("/capabilities/details", headers=AUTH)
        assert details.status_code == 200
        assert details.json()["gateway_model"] == "modelo-privado"
        assert details.json()["components"]["jobs"] is False
        serialized = json.dumps(details.json())
        assert TOKEN not in serialized and "chave-privada" not in serialized


def test_detalhes_desligados_sem_admin_e_ui_desligada_nao_impede_descoberta(tmp_path):
    settings = Settings(data_dir=tmp_path, serve_ui=False, jobs_enabled=False, _env_file=None)
    with TestClient(create_app(settings), base_url="http://127.0.0.1") as c:
        assert c.get("/capabilities").json()["features"]["chat"] is False
        assert c.get("/capabilities/details", headers=AUTH).status_code == 503
        assert c.get("/ui/").status_code == 404
