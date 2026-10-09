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

TOKEN = "segredo-de-teste-16chars"


@pytest.fixture
def settings(tmp_path):
    return Settings(data_dir=tmp_path / "dados", admin_token=TOKEN, _env_file=None)


@pytest.fixture
def client(settings):
    with TestClient(create_app(settings), base_url="http://127.0.0.1") as c:
        yield c


AUTH = {"Authorization": f"Bearer {TOKEN}"}


def test_health_sobe_e_reporta_componentes(client, settings):
    r = client.get("/health", headers=AUTH)
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
    assert client.get("/health", headers=AUTH).json()["pending_approvals"] == 1

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


def test_token_de_admin_fraco_e_recusado():
    with pytest.raises(ValidationError, match="16"):
        Settings(admin_token="curto", _env_file=None)


# ── avisos, jobs, embeddings e acesso de fora ───────────────────────────────
def test_avisos_exigem_token_listam_e_confirmam_uma_vez(client):
    ops = client.app.state.orion.ops
    assert client.get("/notifications").status_code == 401
    assert client.post("/notifications/1/ack").status_code == 401
    a = ops.notify("lembrete", "Pagar boleto", ref="reminder:1")
    assert client.get("/health", headers=AUTH).json()["pending_notifications"] == 1
    lista = client.get("/notifications", headers=AUTH).json()
    assert [(x["id"], x["text"]) for x in lista] == [(a, "Pagar boleto")]
    assert client.post(f"/notifications/{a}/ack", headers=AUTH).json() == {"ok": True}
    assert client.post(f"/notifications/{a}/ack", headers=AUTH).status_code == 404
    assert client.get("/notifications", headers=AUTH).json() == []
    assert client.get("/health", headers=AUTH).json()["pending_notifications"] == 0


def test_jobs_sobem_com_o_app_e_um_lembrete_vencido_vira_aviso(client):
    estado = client.app.state.orion
    assert (
        estado.jobs is not None
        and client.get("/health", headers=AUTH).json()["components"]["jobs"] is True
    )
    estado.ops.add_reminder("Pagar boleto", "2020-01-01T09:00:00")
    rel = client.portal.call(estado.jobs.tick)
    assert rel.lembretes == 1 and rel.erros == []
    assert [x["kind"] for x in client.get("/notifications", headers=AUTH).json()] == ["lembrete"]


def test_jobs_desligados_por_configuracao(tmp_path):
    s = Settings(data_dir=tmp_path, admin_token=TOKEN, jobs_enabled=False, _env_file=None)
    with TestClient(create_app(s), base_url="http://127.0.0.1") as c:
        assert c.app.state.orion.jobs is None
        assert c.get("/health", headers=AUTH).json()["components"]["jobs"] is False


def test_host_de_fora_so_sobe_com_login_senha_ou_token(tmp_path):
    tailnet = ["127.0.0.1", "localhost", "orion.tail1234.ts.net"]
    assert Settings(_env_file=None).hosts_de_fora == []
    assert Settings(allowed_hosts=["*"], _env_file=None).hosts_de_fora == ["*"]

    def sobe(nome, **kw):
        s = Settings(
            data_dir=tmp_path / nome,
            allowed_hosts=tailnet,
            jobs_enabled=False,
            _env_file=None,
            **kw,
        )
        return TestClient(create_app(s), base_url="http://orion.tail1234.ts.net")

    with pytest.raises(RuntimeError, match="exige login"):  # nem senha nem token: recusa subir
        with sobe("a"):
            pass
    with sobe("b", admin_token=TOKEN) as c:  # token de máquina basta
        assert c.get("/health").status_code == 200
    from orion.auth import AuthService

    (tmp_path / "c").mkdir()
    AuthService(tmp_path / "c" / "auth.db").set_password("uma-senha-bem-longa-123")  # ou a senha
    with sobe("c") as c:
        assert c.get("/auth/status").json()["configured"] is True


def test_embeddings_so_com_chave_e_o_modelo_vem_da_configuracao(tmp_path, monkeypatch):
    from orion.app import embedder_from_settings

    monkeypatch.setattr("orion.app.get_secret", lambda nome: None)
    assert embedder_from_settings(Settings(data_dir=tmp_path, _env_file=None)) is None
    s = Settings(data_dir=tmp_path, embed_api_key="k" * 20, embed_dim=256, _env_file=None)
    e = embedder_from_settings(s)
    assert e is not None and e.dim == 256
    monkeypatch.setattr("orion.app.get_secret", lambda nome: "do-cofre-" + "x" * 12)
    assert embedder_from_settings(Settings(data_dir=tmp_path, _env_file=None)) is not None


def test_trocar_a_dimensao_do_embedding_sobe_sem_vetores_em_vez_de_nao_subir(tmp_path):
    from orion.app import memory_from_settings
    from orion.memory import MemoryStore
    from tests.memory.conftest import FakeEmbedder

    s = Settings(data_dir=tmp_path / "d", embed_api_key="k" * 20, embed_dim=64, _env_file=None)
    s.data_dir.mkdir()
    antigo = MemoryStore(s.db_path, embedder=FakeEmbedder())  # índice de 128 dimensões
    antigo.close()
    novo = memory_from_settings(s)  # configuração pede 64: não derruba a subida
    try:
        assert novo.vectors_available is False and novo.ping()
    finally:
        novo.close()


def test_pasta_vazia_na_configuracao_significa_nao_definida_e_nao_a_pasta_atual(
    monkeypatch, tmp_path
):
    monkeypatch.setenv("ORION_VAULT_DIR", "")
    monkeypatch.setenv("ORION_BACKUP_DIR", "  ")
    s = Settings(data_dir=tmp_path, _env_file=None)
    assert s.vault_dir is None and s.backup_dir is None
    assert s.effective_backup_dir == tmp_path / "backups"
    monkeypatch.setenv("ORION_BACKUP_DIR", str(tmp_path / "nuvem"))
    assert Settings(data_dir=tmp_path, _env_file=None).effective_backup_dir == tmp_path / "nuvem"


def test_fila_de_aprovacoes_diz_quando_o_argumento_foi_cortado(client):
    engine = client.app.state.orion.policy
    engine.evaluate(ToolCall("executar_comando", {"cmd": "rm -rf ./build"}), Context("s1"))
    engine.evaluate(
        ToolCall("executar_comando", {"cmd": "echo " + "x" * 2500 + "; rm -rf ~"}), Context("s2")
    )
    fila = {a["session_id"]: a for a in client.get("/approvals", headers=AUTH).json()}
    assert fila["s1"]["args_truncated"] is False and fila["s2"]["args_truncated"] is True


def test_decisoes_da_politica_vao_para_a_tabela_audit_e_falha_de_gravacao_nega(client):
    estado = client.app.state.orion
    ctx = Context("sessao-audit")
    estado.policy.evaluate(ToolCall("executar_comando", {"cmd": "Remove-Item x -Recurse"}), ctx)
    estado.policy.evaluate(ToolCall("buscar_memoria", {"consulta": "oi"}), ctx)
    registro = estado.ops.audit_recent(ferramenta="executar_comando")[0]
    assert registro["action"] == "confirm" and registro["risk"] == "exec"
    assert registro["session_id"] == "sessao-audit"

    def quebrado(_):
        raise OSError("disco cheio")

    estado.ops.audit_add = quebrado  # type: ignore[method-assign]
    leitura = estado.policy.evaluate(ToolCall("buscar_memoria", {"consulta": "oi"}), ctx)
    escrita = estado.policy.evaluate(ToolCall("salvar_memoria", {"texto": "x"}), ctx)
    assert leitura.action is Action.ALLOW  # leitura segue (regra 8)
    assert escrita.action is Action.DENY and "audit" in escrita.reason  # o resto é fail-closed


def test_atividade_junta_avisos_aprovacoes_e_erros_de_jobs(tmp_path):
    from fastapi.testclient import TestClient

    from orion.app import create_app
    from orion.config import Settings

    settings = Settings(
        data_dir=tmp_path / "d", admin_token="token-de-teste-com-16+", _env_file=None
    )
    auth = {"Authorization": "Bearer token-de-teste-com-16+"}
    with TestClient(
        create_app(settings, gateway_factory=lambda _: None), base_url="http://127.0.0.1"
    ) as c:
        assert c.get("/atividade").status_code == 401
        ops = c.app.state.orion.ops
        a = ops.notify("briefing", "☀️ Bom dia")
        ops.notify("sono", "🌙 2 pares de fatos quase iguais")
        ops.ack_notification(a)
        d = c.get("/atividade", headers=auth).json()
        assert [n["tipo"] for n in d["avisos"]] == ["sono", "briefing"]  # o mais novo primeiro
        assert [n["entregue"] for n in d["avisos"]] == [False, True]
        assert d["nao_lidos"] == 1 and d["aprovacoes"] == 0 and d["erros_dos_jobs"] == []
        assert c.get("/atividade?limite=1", headers=auth).json()["avisos"][0]["tipo"] == "sono"
