"""As ferramentas novas da fase 4 sob a política: classe de risco, caminhos e conteúdo externo."""

import json

import pytest

from orion.memory import MemoryStore
from orion.memory.ops import Operations
from orion.policy import Action, Context, PathGuard, PolicyEngine, Risk, ToolCall
from orion.tools.ops_tools import ops_tools


@pytest.fixture
def politica(tmp_path):
    seguro = tmp_path / "Documents"
    seguro.mkdir()
    projeto = tmp_path / "projeto"
    projeto.mkdir()
    return (
        PolicyEngine(path_guard=PathGuard(protected_roots=(projeto,), safe_roots=(seguro,))),
        seguro,
        projeto,
    )


def decide(p, nome, args, ctx=None):
    return p.evaluate(ToolCall(nome, args), ctx or Context("s"))


def test_abrir_app_virou_execucao_e_sempre_confirma(politica):
    p, *_ = politica
    d = decide(p, "abrir_app", {"nome": "calc"})
    assert d.risk is Risk.EXEC and d.action is Action.CONFIRM


def test_escrever_arquivo_so_roda_direto_nas_pastas_seguras(politica):
    p, seguro, projeto = politica
    assert (
        decide(p, "escrever_arquivo", {"path": str(seguro / "nota.txt"), "conteudo": "x"}).action
        is Action.ALLOW
    )
    assert (
        decide(p, "escrever_arquivo", {"path": str(projeto / "orion.py"), "conteudo": "x"}).action
        is Action.CONFIRM
    )
    assert (
        decide(
            p, "gerar_documento", {"tipo": "txt", "conteudo": "x", "path": str(projeto / "a.txt")}
        ).action
        is Action.CONFIRM
    )
    assert (
        decide(
            p, "gerar_documento", {"tipo": "txt", "conteudo": "x", "path": str(seguro / "a.txt")}
        ).action
        is Action.ALLOW
    )


def test_organizar_pasta_pessoal_ou_codigo_do_orion_confirma(politica):
    p, seguro, projeto = politica
    assert decide(p, "organizar_pasta", {"path": str(projeto)}).action is Action.CONFIRM
    assert decide(p, "organizar_pasta", {"path": str(seguro / "bagunca")}).action is Action.ALLOW


def test_ler_documento_e_consultar_git_de_segredo_pedem_confirmacao(politica):
    p, seguro, _ = politica
    assert decide(p, "ler_documento", {"path": str(seguro / "a.pdf")}).action is Action.ALLOW
    assert decide(p, "ler_documento", {"path": "/home/x/.ssh/id_rsa.pdf"}).action is Action.CONFIRM
    assert decide(p, "consultar_git", {"repo_path": str(seguro)}).action is Action.ALLOW
    assert (
        decide(p, "consultar_git", {"repo_path": "/home/x/.aws/projeto"}).action is Action.CONFIRM
    )


def test_processo_em_segundo_plano_segue_a_regra_do_shell(politica):
    p, *_ = politica
    assert (
        decide(p, "iniciar_processo_bg", {"nome": "n", "comando": "Get-ChildItem"}).action
        is Action.ALLOW
    )
    assert (
        decide(p, "iniciar_processo_bg", {"nome": "n", "comando": "Remove-Item -Recurse x"}).action
        is Action.CONFIRM
    )
    assert (
        decide(p, "iniciar_processo_bg", {"nome": "n", "comando": "echo $(rm -rf /)"}).action
        is Action.CONFIRM
    )


def test_depois_de_ler_web_ou_documento_escrita_e_execucao_confirmam(politica):
    p, seguro, _ = politica
    ctx = Context("s")
    chamada = ToolCall("buscar_url", {"url": "https://exemplo.com"})
    assert p.evaluate(chamada, ctx).action is Action.ALLOW
    p.note_result(chamada, ctx)
    assert ctx.tainted
    assert (
        decide(p, "escrever_arquivo", {"path": str(seguro / "n.txt"), "conteudo": "x"}, ctx).action
        is Action.CONFIRM
    )
    assert decide(p, "escrever_clipboard", {"texto": "x"}, ctx).action is Action.CONFIRM
    assert (
        decide(p, "notificar_usuario", {"titulo": "t", "mensagem": "m"}, ctx).action
        is Action.CONFIRM
    )
    ctx2 = Context("s2")
    p.note_result(ToolCall("ler_documento", {"path": "a.pdf"}), ctx2)
    assert ctx2.tainted  # documento é conteúdo externo
    ctx3 = Context("s3")
    p.note_result(ToolCall("ler_clipboard", {}), ctx3)
    assert ctx3.tainted
    ctx4 = Context("s4")
    p.note_result(ToolCall("pesquisar_com_ia", {"query": "x"}), ctx4)
    assert ctx4.tainted


# ── ferramentas de operação novas ─────────────────────────────────────────────
@pytest.fixture
def ops(tmp_path):
    s = MemoryStore(tmp_path / "m.db")
    yield Operations(s)
    s.close()


def test_consultar_audit_log_filtra_por_ferramenta_e_so_bloqueados(ops):
    ops.audit_add(
        {
            "session_id": "s",
            "tool": "abrir_app",
            "action": "confirm",
            "risk": "exec",
            "reason": "execução",
            "args": {"nome": "calc"},
        }
    )
    ops.audit_add(
        {
            "session_id": "s",
            "tool": "executar_comando",
            "action": "deny",
            "risk": "exec",
            "reason": "limite",
        }
    )
    ops.audit_add({"session_id": "s", "tool": "ler_arquivo", "action": "allow", "risk": "read"})
    t = {x.name: x for x in ops_tools(ops)}["consultar_audit_log"]
    todos = json.loads(t.run({}))
    assert todos["total"] == 3 and todos["decisoes"][0]["ferramenta"] == "ler_arquivo"
    so_negados = json.loads(t.run({"apenas_bloqueados": True}))
    assert [d["ferramenta"] for d in so_negados["decisoes"]] == ["executar_comando"]
    um = json.loads(t.run({"tool_filtro": "abrir_app"}))["decisoes"][0]
    assert (
        um["decisao"] == "confirm" and um["args"] == {"nome": "calc"} and um["contaminada"] is False
    )


def test_notificar_celular_poe_o_aviso_na_fila_do_telegram(ops):
    t = {x.name: x for x in ops_tools(ops)}["notificar_celular"]
    r = json.loads(t.run({"mensagem": "Backup terminou", "titulo": "Orion", "urgente": True}))
    assert r["ok"]
    (n,) = ops.pending_notifications()
    assert n["text"] == "❗ Orion: Backup terminou" and n["kind"] == "agente"


def test_app_registra_as_ferramentas_so_com_os_opt_ins(tmp_path):
    from fastapi.testclient import TestClient

    from orion.app import create_app
    from orion.config import Settings
    from tests.fakes import FakeGateway, fala

    def nomes(**kw):
        s = Settings(
            data_dir=tmp_path / str(len(kw)),
            jobs_enabled=False,
            mcp_enabled=False,
            _env_file=None,
            **kw,
        )
        app = create_app(s, gateway_factory=lambda _: FakeGateway(fala("oi")))
        with TestClient(app, base_url="http://127.0.0.1") as c:
            return set(c.app.state.orion.agent.tools.names()), c.app.state.orion.policy.tools

    base, _ = nomes()
    assert "escrever_arquivo" not in base and "buscar_url" not in base
    assert {"consultar_audit_log", "notificar_celular", "buscar_memoria"} <= base
    desktop, politica = nomes(desktop_tools=True)
    assert {"escrever_arquivo", "iniciar_processo_bg", "ler_documento", "consultar_git"} <= desktop
    assert "buscar_url" not in desktop
    web, _ = nomes(web_tools=True)
    assert {
        "buscar_url",
        "consultar_clima",
        "pesquisar_com_ia",
    } <= web and "escrever_arquivo" not in web
    # toda ferramenta registrada tem classe na política (sem classe, a política nega)
    assert (desktop | web) <= set(politica)
