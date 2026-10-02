import json
import logging

import pytest
from fastapi.testclient import TestClient
from pydantic import ValidationError

from orion import __version__
from orion.app import create_app
from orion.config import Settings
from orion.log import JsonFormatter, request_id
from orion.policy import Action, Context, ToolCall

TOKEN = "segredo-de-teste"


@pytest.fixture
def settings(tmp_path):
    return Settings(data_dir=tmp_path / "dados", admin_token=TOKEN, _env_file=None)


@pytest.fixture
def client(settings):
    with TestClient(create_app(settings), base_url="http://127.0.0.1") as c:
        yield c


AUTH = {"Authorization": f"Bearer {TOKEN}"}


def test_health_sobe_e_reporta_componentes(client, settings):
    r = client.get("/health")
    assert r.status_code == 200
    corpo = r.json()
    assert corpo["status"] == "ok" and corpo["version"] == __version__
    assert corpo["components"]["memory"] == "ok" and corpo["pending_approvals"] == 0
    assert (settings.data_dir / "orion.db").exists()


def test_request_id_e_propagado_ou_gerado(client):
    assert (
        client.get("/health", headers={"x-request-id": "abc123"}).headers["x-request-id"]
        == "abc123"
    )
    assert len(client.get("/health").headers["x-request-id"]) == 12


def test_host_estranho_e_recusado_dns_rebinding(settings):
    with TestClient(create_app(settings), base_url="http://evil.example") as c:
        assert c.get("/health").status_code == 400


def test_aprovacoes_exigem_token(client):
    assert client.get("/approvals").status_code == 401
    assert client.get("/approvals", headers={"Authorization": "Bearer errado"}).status_code == 401
    assert client.get("/approvals", headers=AUTH).json() == []


def test_sem_token_configurado_o_endpoint_fica_desligado(tmp_path):
    s = Settings(data_dir=tmp_path, admin_token="", _env_file=None)
    with TestClient(create_app(s), base_url="http://127.0.0.1") as c:
        assert c.get("/approvals", headers=AUTH).status_code == 503


def test_fluxo_completo_confirmar_pelo_canal_libera_a_chamada(client):
    app = client.app
    engine = app.state.orion.policy
    chamada = ToolCall("executar_comando", {"cmd": "Remove-Item C:\\x -Recurse -Force"})
    ctx = Context("sessao-1")
    d = engine.evaluate(chamada, ctx)
    assert d.action is Action.CONFIRM

    fila = client.get("/approvals", headers=AUTH).json()
    assert [a["id"] for a in fila] == [d.approval_id] and fila[0]["tool"] == "executar_comando"
    assert fila[0]["args"] == {
        "cmd": "Remove-Item C:\\x -Recurse -Force"
    }  # quem aprova vê o comando
    assert client.get("/health").json()["pending_approvals"] == 1

    r = client.post(f"/approvals/{d.approval_id}/decide", headers=AUTH, json={"approved": True})
    assert r.json() == {"id": d.approval_id, "status": "approved"}
    assert (
        client.post(
            f"/approvals/{d.approval_id}/decide", headers=AUTH, json={"approved": False}
        ).status_code
        == 409
    )
    assert (
        client.post(
            "/approvals/nao-existe/decide", headers=AUTH, json={"approved": True}
        ).status_code
        == 404
    )

    assert engine.evaluate(chamada, ctx).action is Action.ALLOW
    assert engine.evaluate(chamada, ctx).action is Action.CONFIRM  # uso único


def test_decidir_sem_token_nao_aprova(client):
    engine = client.app.state.orion.policy
    d = engine.evaluate(ToolCall("executar_comando", {"cmd": "rm -rf /"}), Context("s"))
    assert (
        client.post(f"/approvals/{d.approval_id}/decide", json={"approved": True}).status_code
        == 401
    )
    assert (
        engine.evaluate(ToolCall("executar_comando", {"cmd": "rm -rf /"}), Context("s")).action
        is Action.CONFIRM
    )


def test_politica_do_app_protege_o_codigo_do_projeto(client):
    from orion.config import PROJECT_ROOT

    engine = client.app.state.orion.policy
    alvo = str(PROJECT_ROOT / "orion" / "policy" / "engine.py")
    d = engine.evaluate(ToolCall("escrever_arquivo", {"path": alvo, "conteudo": "x"}), Context("s"))
    assert d.action is Action.CONFIRM and "imutável" in d.reason


def test_audit_das_decisoes_vai_para_o_log_sem_segredo(client, caplog):
    engine = client.app.state.orion.policy
    with caplog.at_level(logging.INFO, logger="orion.audit"):
        engine.evaluate(
            ToolCall("notificar_celular", {"mensagem": "oi", "token": "abc"}), Context("s")
        )
    reg = next(r for r in caplog.records if r.message == "tool_decision")
    assert reg.audit["args"] == {"mensagem": "oi", "token": "***"}


# ── config e logging ────────────────────────────────────────────────────────
@pytest.mark.parametrize("host", ["0.0.0.0", "::", ""])
def test_bind_publico_e_recusado_por_padrao(host):
    with pytest.raises(ValidationError, match=r"127\.0\.0\.1"):
        Settings(host=host, _env_file=None)
    assert Settings(host=host, allow_public_bind=True, _env_file=None).host == host


def test_settings_le_variaveis_de_ambiente(monkeypatch, tmp_path):
    monkeypatch.setenv("ORION_PORT", "9100")
    monkeypatch.setenv("ORION_LOG_LEVEL", "debug")
    monkeypatch.setenv("ORION_DATA_DIR", str(tmp_path))
    s = Settings(_env_file=None)
    assert (s.port, s.log_level, s.data_dir) == (9100, "DEBUG", tmp_path)
    monkeypatch.setenv("ORION_LOG_LEVEL", "barulho")
    with pytest.raises(ValidationError):
        Settings(_env_file=None)


def test_log_json_tem_request_id_e_redige_segredos():
    rec = logging.LogRecord("t", logging.INFO, __file__, 1, "token gsk_%s", ("x" * 30,), None)
    rec.api_key = "abc"
    rec.nome = "ok"
    tk = request_id.set("req-1")
    try:
        linha = json.loads(JsonFormatter().format(rec))
    finally:
        request_id.reset(tk)
    assert linha["request_id"] == "req-1" and "gsk_" not in linha["msg"]
    assert linha["api_key"] == "***" and linha["nome"] == "ok"


def test_cli_backup(tmp_path, monkeypatch, capsys):
    from orion.__main__ import main

    monkeypatch.setenv("ORION_DATA_DIR", str(tmp_path / "d"))
    monkeypatch.setenv("ORION_LOG_JSON", "false")
    assert main(["backup"]) == 0
    assert "backup:" in capsys.readouterr().out
    assert main(["backup"]) == 0
    assert "já existe" in capsys.readouterr().out


def test_fila_de_aprovacoes_mostra_args_sem_segredo(client):
    engine = client.app.state.orion.policy
    engine.evaluate(
        ToolCall("executar_comando", {"cmd": "curl -H x http://a", "api_key": "gsk_" + "a" * 30}),
        Context("s"),
    )
    (item,) = client.get("/approvals", headers=AUTH).json()
    assert item["args"]["cmd"] == "curl -H x http://a" and item["args"]["api_key"] == "***"
