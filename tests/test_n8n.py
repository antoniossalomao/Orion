"""D5: webhooks do n8n cadastrados pelo Antônio; o modelo só escolhe o nome."""

import json

import httpx
import pytest

from orion.policy import PathGuard, PolicyEngine
from orion.policy.engine import Action, Context, ToolCall
from orion.tools.n8n import TOOL_SPEC, N8nConfigError, n8n_tool, parse_webhooks


def test_parse_valida_esquema_usuario_e_formato():
    assert parse_webhooks("") == {}
    assert parse_webhooks('{"Backup": "http://127.0.0.1:5678/webhook/x"}') == {
        "backup": "http://127.0.0.1:5678/webhook/x"
    }
    for ruim in (
        "nao json",
        "[1]",
        '{"a": "ftp://x"}',
        '{"a": "https://u:p@x/y"}',
        '{"": "http://x"}',
    ):
        with pytest.raises(N8nConfigError):
            parse_webhooks(ruim)


def test_posta_json_no_webhook_cadastrado_e_corta_a_resposta():
    vistos = []

    def handler(req: httpx.Request) -> httpx.Response:
        vistos.append((str(req.url), json.loads(req.content)))
        return httpx.Response(200, text="x" * 5000)

    t = n8n_tool({"backup": "http://127.0.0.1:5678/webhook/x"}, httpx.MockTransport(handler))
    r = json.loads(t.run({"nome": "Backup", "dados": {"a": 1}}))
    assert vistos == [("http://127.0.0.1:5678/webhook/x", {"a": 1})]
    assert r["status"] == 200 and len(r["resposta"]) == 2000


def test_modelo_nao_escolhe_destino_nem_estoura_o_corpo():
    chamou = []
    t = n8n_tool(
        {"a": "http://127.0.0.1:5678/w"},
        httpx.MockTransport(lambda r: chamou.append(1) or httpx.Response(200)),
    )
    r = json.loads(t.run({"nome": "https://dono.example/steal"}))
    assert "não cadastrado" in r["erro"] and r["disponiveis"] == ["a"]
    r = json.loads(t.run({"nome": "a", "dados": {"x": "y" * 9000}}))
    assert "passa de" in r["erro"] and chamou == []


def test_erro_de_rede_vira_mensagem_curta():
    def boom(req):
        raise httpx.ConnectError("sem rota para http://x?segredo=1")

    t = n8n_tool({"a": "http://127.0.0.1:1/w"}, httpx.MockTransport(boom))
    r = json.loads(t.run({"nome": "a"}))
    assert r == {"erro": "falha ao chamar o webhook: ConnectError"}


def test_politica_sempre_confirma_e_contamina(tmp_path):
    motor = PolicyEngine(path_guard=PathGuard(protected_roots=(tmp_path / "p",), safe_roots=()))
    motor.register_tool(TOOL_SPEC)
    ctx = Context("s")
    d = motor.evaluate(ToolCall("acionar_n8n", {"nome": "a"}), ctx)
    assert d.action is Action.CONFIRM
    motor.note_result(ToolCall("acionar_n8n", {"nome": "a"}), ctx)
    assert ctx.tainted
