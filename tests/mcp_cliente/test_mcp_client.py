"""Cliente MCP com servidor de verdade (SDK oficial, via stdin/stdout) e as regras de risco."""

import json
import sys
from pathlib import Path

import pytest
from fastapi.testclient import TestClient

from orion.app import create_app
from orion.config import Settings
from orion.mcp_client import (
    McpConfig,
    McpConfigError,
    McpManager,
    ServerConfig,
    ToolRule,
    describe,
    load_config,
    manager_from_file,
    resolve_env,
    spec_for,
    tool_name,
)
from orion.policy import Action, Context, PathGuard, PolicyEngine, Risk, ToolCall

SERVIDOR = str(Path(__file__).with_name("fake_server.py"))


def cfg(**kw):
    base = {"command": sys.executable, "args": [SERVIDOR], "timeout_s": 20}
    return ServerConfig(**{**base, **kw})


@pytest.fixture
def gerente():
    g = McpManager(
        McpConfig(
            servers={
                "fake": cfg(
                    tools={
                        "somar": ToolRule(risk=Risk.READ),
                        "ler_texto": ToolRule(risk=Risk.READ, read_path_arg="path"),
                        "falha": ToolRule(risk=Risk.READ),
                        "dormir": ToolRule(risk=Risk.READ),
                    }
                )
            }
        ),
        connect_timeout_s=40,
    )
    g.start()
    yield g
    g.stop()


# ── nomes, esquema do arquivo e ambiente ───────────────────────────────────────
def test_nome_da_ferramenta_e_valido_e_cabe_no_limite():
    assert tool_name("fs", "read_file") == "fs__read_file"
    assert tool_name("fs", "ler arquivo ção!") == "fs__ler_arquivo___o_"
    longo = tool_name("fs", "x" * 200)
    assert len(longo) == 64 and longo.startswith("fs__xxx")
    assert tool_name("fs", "x" * 200) == longo  # estável
    assert tool_name("fs", "x" * 199 + "y") != longo  # o hash separa nomes que o corte juntaria


def test_arquivo_ausente_e_nenhum_servidor_e_invalido_e_erro(tmp_path):
    assert load_config(tmp_path / "nao-existe.json").servers == {}
    assert manager_from_file(tmp_path / "nao-existe.json") is None
    ruim = tmp_path / "mcp.json"
    for conteudo in ("{quebrado", '{"servers": {"Maiúscula": {"command": "x"}}}',
                     '{"servers": {"a__b": {"command": "x"}}}',
                     '{"servers": {"a": {"command": "x", "extra": 1}}}',
                     '{"servers": {"a": {"command": "x", "default_risk": "read"}}}',
                     '{"servers": {"a": {"command": ""}}}'):  # fmt: skip
        ruim.write_text(conteudo, encoding="utf-8")
        with pytest.raises(McpConfigError):
            load_config(ruim)
    ruim.write_text('{"servers": {"a": {"command": "x", "enabled": false}}}', encoding="utf-8")
    assert manager_from_file(ruim) is None  # nada habilitado


def test_o_exemplo_do_repositorio_e_valido_e_comentarios_com_underline_sao_ignorados():
    exemplo = Path(__file__).resolve().parents[2] / "mcp.example.json"
    c = load_config(exemplo)
    assert set(c.servers) == {"arquivos", "web", "git", "navegador", "google"}
    assert (
        c.servers["web"].external and c.servers["arquivos"].tools["write_file"].path_arg == "path"
    )
    # o que está desligado no exemplo não sobe sozinho
    assert not c.servers["navegador"].enabled and not c.servers["google"].enabled
    assert all(s.default_risk is Risk.EXEC for s in c.servers.values())


def test_env_com_segredo_vem_do_cofre_e_falta_vira_erro_claro():
    cofre = {"CHAVE": "valor-secreto"}.get
    assert resolve_env({"A": "literal", "B": "${CHAVE}"}, cofre) == {
        "A": "literal",
        "B": "valor-secreto",
    }
    with pytest.raises(McpConfigError, match="FALTA"):
        resolve_env({"X": "${FALTA}"}, cofre)


def test_classe_vem_da_configuracao_e_o_padrao_e_exec():
    c = cfg(default_risk=Risk.WRITE, external=True, tools={"ler": ToolRule(risk=Risk.READ)})
    assert spec_for("s", c, "ler").risk is Risk.READ and spec_for("s", c, "ler").external
    assert spec_for("s", c, "qualquer").risk is Risk.WRITE
    assert spec_for("s", cfg(), "qualquer").risk is Risk.EXEC
    c2 = cfg(tools={"x": ToolRule(external=False)}, external=True)
    assert spec_for("s", c2, "x").external is False  # a regra da ferramenta vence a do servidor


# ── servidor de verdade ────────────────────────────────────────────────────────
def test_lista_as_ferramentas_com_descricao_marcada_e_status(gerente):
    nomes = {t.name for t in gerente.tools}
    assert {"fake__somar", "fake__ler_texto", "fake__apagar", "fake__falha"} <= nomes
    assert any(n.startswith("fake__nome_estranho_com_espacos") for n in nomes)
    assert all(re_ok(n) for n in nomes)
    somar = next(t for t in gerente.tools if t.name == "fake__somar")
    assert somar.description.startswith("[MCP fake] Soma")
    assert somar.parameters["required"] == ["a", "b"]
    assert gerente.status["fake"].startswith("ok (")
    linhas = "\n".join(describe(gerente))
    assert "fake__somar  [read]" in linhas and "fake__apagar  [exec]" in linhas
    assert "lê=path" in linhas


def re_ok(nome: str) -> bool:
    import re

    return bool(re.fullmatch(r"[A-Za-z0-9_-]{1,64}", nome))


def test_chama_a_ferramenta_e_devolve_o_texto(gerente):
    somar = next(t for t in gerente.tools if t.name == "fake__somar")
    assert json.loads(somar.run({"a": 2, "b": 3})) == {"texto": "5"}
    ler = next(t for t in gerente.tools if t.name == "fake__ler_texto")
    assert json.loads(ler.run({"path": "/x"})) == {"texto": "conteudo de /x"}


def test_erro_do_servidor_vira_resultado_nao_excecao(gerente):
    falha = next(t for t in gerente.tools if t.name == "fake__falha")
    assert "erro" in json.loads(falha.run({}))
    somar = next(t for t in gerente.tools if t.name == "fake__somar")
    assert "erro" in json.loads(somar.run({"a": "nao-e-numero", "b": 1}))  # validação do servidor
    assert "obrigatórios" in json.loads(somar.run({"a": 1}))["erro"]  # validação local


def test_tempo_esgotado_devolve_erro_e_o_servidor_segue_de_pe():
    g = McpManager(
        McpConfig(
            servers={"fake": cfg(timeout_s=1, tools={"dormir": ToolRule(risk=Risk.READ)}, allow=["dormir", "somar"])}
        ),
        connect_timeout_s=40,
    )  # fmt: skip
    g.start()
    try:
        dormir = next(t for t in g.tools if t.name == "fake__dormir")
        assert "esgotado" in json.loads(dormir.run({"segundos": 4}))["erro"]
        somar = next(t for t in g.tools if t.name == "fake__somar")
        assert json.loads(somar.run({"a": 1, "b": 1})) == {"texto": "2"}
    finally:
        g.stop()


def test_allow_esconde_o_que_nao_foi_liberado():
    g = McpManager(McpConfig(servers={"fake": cfg(allow=["somar"])}), connect_timeout_s=40)
    g.start()
    try:
        assert [t.name for t in g.tools] == ["fake__somar"]
        assert list(g.specs) == ["fake__somar"]
    finally:
        g.stop()


def test_servidor_que_nao_sobe_e_pulado_e_registrado_sem_derrubar_os_outros():
    g = McpManager(
        McpConfig(
            servers={
                "quebrado": ServerConfig(command="/nao/existe/binario", timeout_s=5),
                "bom": cfg(allow=["somar"]),
            }
        ),
        connect_timeout_s=40,
    )
    g.start()
    try:
        assert g.status["quebrado"].startswith("falhou")
        assert g.status["bom"].startswith("ok")
        assert [t.name for t in g.tools] == ["bom__somar"]
    finally:
        g.stop()


def test_servidor_nao_herda_orion_nem_segredos_do_ambiente(monkeypatch, tmp_path):
    monkeypatch.setenv("ORION_ADMIN_TOKEN", "segredo-do-orion-123456")
    monkeypatch.setenv("OPENAI_API_KEY", "sk-segredo")
    from mcp.client.stdio import get_default_environment

    ambiente = get_default_environment()
    assert "ORION_ADMIN_TOKEN" not in ambiente and "OPENAI_API_KEY" not in ambiente


# ── política ──────────────────────────────────────────────────────────────────
def test_politica_usa_a_classe_do_arquivo_e_confirma_o_resto(gerente, tmp_path):
    politica = PolicyEngine(
        path_guard=PathGuard(protected_roots=(tmp_path / "proj",), safe_roots=(tmp_path,))
    )
    for spec in gerente.specs.values():
        politica.register_tool(spec)
    ctx = Context("s1")
    assert politica.evaluate(ToolCall("fake__somar", {"a": 1, "b": 2}), ctx).action is Action.ALLOW
    d = politica.evaluate(ToolCall("fake__apagar", {"path": str(tmp_path / "x")}), ctx)
    assert d.action is Action.CONFIRM and d.risk is Risk.EXEC  # sem classe no arquivo: confirma
    # leitura com read_path_arg: segredo pede confirmação mesmo sendo "read"
    segredo = politica.evaluate(ToolCall("fake__ler_texto", {"path": "/home/x/.ssh/id_rsa"}), ctx)
    assert segredo.action is Action.CONFIRM


def test_servidor_nao_troca_a_classe_de_uma_ferramenta_ja_registrada(gerente, tmp_path):
    politica = PolicyEngine(path_guard=PathGuard(protected_roots=(), safe_roots=(tmp_path,)))
    spec = next(iter(gerente.specs.values()))
    politica.register_tool(spec)
    with pytest.raises(ValueError, match="já registrada"):
        politica.register_tool(spec)
    from orion.policy import ToolSpec

    with pytest.raises(ValueError):
        politica.register_tool(ToolSpec("ler_arquivo", Risk.READ))  # nome nativo


def test_servidor_externo_marca_a_sessao_como_contaminada(tmp_path):
    g = McpManager(McpConfig(servers={"web": cfg(external=True, tools={"somar": ToolRule(risk=Risk.READ)}, allow=["somar", "apagar"])}), connect_timeout_s=40)  # fmt: skip
    g.start()
    try:
        politica = PolicyEngine(path_guard=PathGuard(protected_roots=(), safe_roots=(tmp_path,)))
        for spec in g.specs.values():
            politica.register_tool(spec)
        ctx = Context("s1")
        chamada = ToolCall("web__somar", {"a": 1, "b": 1})
        assert politica.evaluate(chamada, ctx).action is Action.ALLOW
        politica.note_result(chamada, ctx)
        assert ctx.tainted  # depois de ler conteúdo externo, o resto pede confirmação (regra 4)
    finally:
        g.stop()


# ── integração com o app ───────────────────────────────────────────────────────
def test_app_sobe_mcp_registra_na_politica_e_derruba_no_fim(tmp_path):
    from tests.fakes import FakeGateway, fala

    settings = Settings(
        data_dir=tmp_path / "d", admin_token="token-de-teste-com-16+", jobs_enabled=False,
        _env_file=None,
    )  # fmt: skip
    gerente = McpManager(McpConfig(servers={"fake": cfg(allow=["somar"], tools={"somar": ToolRule(risk=Risk.READ)})}), connect_timeout_s=40)  # fmt: skip
    app = create_app(
        settings,
        gateway_factory=lambda _: FakeGateway(fala("oi")),
        mcp_factory=lambda _s: gerente,
    )
    with TestClient(app, base_url="http://127.0.0.1") as c:
        estado = c.app.state.orion
        assert "fake__somar" in estado.agent.tools.names()
        assert estado.policy.tools["fake__somar"].risk is Risk.READ
        saude = c.get("/health", headers={"Authorization": "Bearer token-de-teste-com-16+"}).json()
        assert saude["components"]["mcp"]["fake"].startswith("ok")
    assert gerente._loop is None  # parou no desligamento


def test_app_com_mcp_json_invalido_sobe_sem_mcp(tmp_path, caplog):
    from tests.fakes import FakeGateway, fala

    (tmp_path / "d").mkdir()
    (tmp_path / "d" / "mcp.json").write_text("{quebrado", encoding="utf-8")
    settings = Settings(data_dir=tmp_path / "d", jobs_enabled=False, _env_file=None)
    with TestClient(
        create_app(settings, gateway_factory=lambda _: FakeGateway(fala("oi"))),
        base_url="http://127.0.0.1",
    ) as c:
        assert c.app.state.orion.mcp is None
    assert "MCP desligado" in caplog.text


def test_egress_vem_do_mcp_json_e_o_exemplo_marca_o_fetch():
    exemplo = Path(__file__).resolve().parents[2] / "mcp.example.json"
    c = load_config(exemplo)
    assert c.servers["web"].tools["fetch"].egress is True
    assert spec_for("web", c.servers["web"], "fetch").egress is True
    assert spec_for("web", c.servers["web"], "outra").egress is False
    assert cfg(tools={"x": ToolRule(egress=True)}).tools["x"].egress is True


# ── validação pelo esquema completo (C08) ──────────────────────────────────────
def test_argumento_de_tipo_errado_e_recusado_antes_de_chamar_o_servidor(gerente):
    somar = next(t for t in gerente.tools if t.name == "fake__somar")
    ok = json.loads(somar.run({"a": 2, "b": 3}))
    assert "erro" not in ok
    errado = json.loads(somar.run({"a": "dois", "b": 3}))
    assert errado["erro"].startswith("argumento inválido: a:") and "integer" in errado["erro"]
    ausente = json.loads(somar.run({"a": 1}))
    assert "obrigatórios ausentes" in ausente["erro"]


def test_ferramentas_nativas_continuam_aceitando_numero_como_texto():
    from orion.tools import Tool

    t = Tool("x", "x", {"type": "object", "properties": {"n": {"type": "integer"}}}, lambda n: n)
    assert json.loads(t.run({"n": "5"})) == "5"  # nativas não validam tipo (comportamento antigo)
    assert json.loads(Tool("x", "x", t.parameters, lambda n: n, validar=True).run({"n": "5"}))[
        "erro"
    ]


def test_esquema_que_o_validador_nao_entende_nao_derruba_a_ferramenta():
    from orion.tools import Tool

    t = Tool(
        "x",
        "x",
        {"type": "object", "properties": {"n": {"type": "inexistente"}}},
        lambda n: n,
        validar=True,
    )
    assert json.loads(t.run({"n": 1})) == 1


def test_resolve_env_troca_segredo_dentro_do_valor():
    assert resolve_env({"Authorization": "Bearer ${T}"}, lambda n: "abc") == {
        "Authorization": "Bearer abc"
    }
    with pytest.raises(McpConfigError, match="T"):
        resolve_env({"a": "x ${T} y"}, lambda n: None)
