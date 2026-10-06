import json
import subprocess
import sys

import pytest

from orion.tools.system_tools import git_argv, system_tools


class Runner:
    def __init__(self, saida=b"", erro=b"", codigo=0, levanta=None):
        self.chamadas: list[tuple[list[str], dict]] = []
        self.saida, self.erro, self.codigo, self.levanta = saida, erro, codigo, levanta

    def __call__(self, argv, **kw):
        self.chamadas.append((argv, kw))
        if self.levanta:
            raise self.levanta
        return subprocess.CompletedProcess(argv, self.codigo, self.saida, self.erro)


def montar(
    platform,
    runner=None,
    launcher=None,
    disponivel=("pwsh", "xclip", "wl-copy", "wl-paste", "notify-send", "xdg-open"),
):
    runner, launcher = runner or Runner(), launcher or Runner()
    ts = system_tools(
        platform=platform, runner=runner, launcher=launcher,
        which=lambda n: f"/usr/bin/{n}" if n in disponivel else None,
    )  # fmt: skip
    return {t.name: t for t in ts}, runner, launcher


def roda(tool, **args):
    return json.loads(tool.run(args))


# ── saúde ─────────────────────────────────────────────────────────────────────
def test_saude_devolve_numeros_reais_da_maquina():
    ts, *_ = montar(sys.platform)
    r = roda(ts["checar_saude_sistema"])
    assert r["ok"] and 0 <= r["ram_percentual"] <= 100 and r["ram_total_gb"] > 0
    assert r["disco_livre_gb"] >= 0 and r["cpu_nucleos"] >= 1


# ── git ───────────────────────────────────────────────────────────────────────
def test_git_nao_deixa_o_repositorio_executar_nada():
    argv = git_argv("/repo", "status", 10)
    assert argv[:4] == ["git", "--no-pager", "--no-optional-locks", "-c"]
    assert "core.fsmonitor=false" in argv and "diff.external=" in argv and "core.pager=cat" in argv
    assert argv[-3:] == ["status", "--short", "--branch"]
    assert "--no-ext-diff" in git_argv("/repo", "diff", 1) and "--no-textconv" in git_argv(
        "/repo", "diff", 1
    )
    assert "-5" in git_argv("/repo", "log", 5) and git_argv("/repo", "branch", 1)[-2:] == [
        "branch",
        "--show-current",
    ]


def test_git_valida_comando_pasta_e_limita_o_limite(tmp_path):
    ts, runner, _ = montar(sys.platform)
    assert (
        "inválido" in roda(ts["consultar_git"], repo_path=str(tmp_path), comando="commit")["erro"]
    )
    assert "não é uma pasta" in roda(ts["consultar_git"], repo_path=str(tmp_path / "x"))["erro"]
    assert runner.chamadas == []
    roda(ts["consultar_git"], repo_path=str(tmp_path), comando="log", limite=10**9)
    assert "-100" in runner.chamadas[0][0]  # teto de 100
    env = runner.chamadas[0][1]["env"]
    assert env["GIT_TERMINAL_PROMPT"] == "0" and not any(k.startswith("ORION_") for k in env)


@pytest.mark.skipif(not __import__("shutil").which("git"), reason="git não instalado")
def test_git_de_verdade_status_e_log(tmp_path):
    def g(*a):
        subprocess.run(["git", "-C", str(tmp_path), *a], check=True, capture_output=True)

    g("init", "-q")
    g("config", "user.email", "t@t.t")
    g("config", "user.name", "T")
    (tmp_path / "a.txt").write_text("1")
    g("add", ".")
    g("commit", "-qm", "primeiro")
    ts = {t.name: t for t in system_tools()}
    assert "primeiro" in roda(ts["consultar_git"], repo_path=str(tmp_path), comando="log")["stdout"]
    (tmp_path / "b.txt").write_text("2")
    assert "b.txt" in roda(ts["consultar_git"], repo_path=str(tmp_path))["stdout"]


def test_git_nao_herda_variaveis_git_do_ambiente(tmp_path, monkeypatch):
    monkeypatch.setenv("GIT_DIR", "/outro/lugar")
    monkeypatch.setenv("GIT_CONFIG_COUNT", "3")
    monkeypatch.setenv("GIT_EXTERNAL_DIFF", "/bin/evil")
    ts, runner, _ = montar(sys.platform)
    roda(ts["consultar_git"], repo_path=str(tmp_path))
    env = runner.chamadas[0][1]["env"]
    assert sorted(k for k in env if k.startswith("GIT_")) == [
        "GIT_CONFIG_NOSYSTEM",
        "GIT_TERMINAL_PROMPT",
    ]


def test_git_com_fsmonitor_malicioso_no_config_nao_executa(tmp_path):
    if not __import__("shutil").which("git") or sys.platform == "win32":
        pytest.skip("precisa de git e sh")
    marca = tmp_path / "executou"
    subprocess.run(["git", "-C", str(tmp_path), "init", "-q"], check=True, capture_output=True)
    (tmp_path / ".git" / "config").write_text(
        f"[core]\n\tfsmonitor = touch {marca}\n", encoding="utf-8"
    )
    ts = {t.name: t for t in system_tools()}
    roda(ts["consultar_git"], repo_path=str(tmp_path), comando="status")
    assert not marca.exists()


# ── abrir app ─────────────────────────────────────────────────────────────────
def test_abrir_app_windows_passa_o_nome_por_variavel_e_nao_pelo_comando():
    ts, _, launcher = montar("win32")
    roda(ts["abrir_app"], nome="calc'; Remove-Item -Recurse C:\\ #")
    argv, kw = launcher.chamadas[0]
    assert argv[-1] == "Start-Process -FilePath $env:APP_ALVO"
    assert kw["env"]["APP_ALVO"] == "calc'; Remove-Item -Recurse C:\\ #"
    assert not any(k.startswith("ORION_") for k in kw["env"])


def test_abrir_app_mac_e_linux(tmp_path):
    arq = tmp_path / "doc.pdf"
    arq.write_text("x")
    ts, _, launcher = montar("darwin")
    roda(ts["abrir_app"], nome="Safari")
    roda(ts["abrir_app"], nome=str(arq))
    assert launcher.chamadas[0][0] == ["open", "-a", "Safari"]
    assert launcher.chamadas[1][0] == ["open", str(arq.resolve())]
    ts, _, launcher = montar("linux", disponivel=("firefox", "xdg-open"))
    roda(ts["abrir_app"], nome="firefox")
    roda(ts["abrir_app"], nome=str(arq))
    assert launcher.chamadas[0][0] == ["/usr/bin/firefox"]
    assert launcher.chamadas[1][0] == ["/usr/bin/xdg-open", str(arq.resolve())]
    assert "não encontrei" in roda(ts["abrir_app"], nome="programa-que-nao-existe")["erro"]
    assert "inválido" in roda(ts["abrir_app"], nome="  ")["erro"]


# ── notificação ───────────────────────────────────────────────────────────────
def test_notificacao_por_sistema_sem_texto_na_linha_de_comando():
    perigoso = 'x"; Start-Process calc; "'
    ts, _, launcher = montar("win32")
    assert roda(ts["notificar_usuario"], titulo=perigoso, mensagem="m")["ok"]
    argv, kw = launcher.chamadas[0]
    assert perigoso not in " ".join(argv) and kw["env"]["NOTIF_TITULO"] == perigoso

    ts, runner, _ = montar("darwin")
    roda(ts["notificar_usuario"], titulo=perigoso, mensagem='a "b" \\ c')
    argv = runner.chamadas[0][0]
    assert argv[0] == "osascript" and argv[-2:] == [
        perigoso,
        'a "b" \\ c',
    ]  # argumentos, nunca script
    assert all(perigoso not in a for a in argv[:-2])

    ts, runner, _ = montar("linux")
    roda(ts["notificar_usuario"], titulo="t", mensagem="m", urgencia="alta")
    assert runner.chamadas[0][0] == ["notify-send", "-u", "critical", "--", "t", "m"]
    ts, runner, _ = montar("linux", disponivel=())
    assert "notify-send" in roda(ts["notificar_usuario"], titulo="t", mensagem="m")["erro"]


# ── área de transferência ─────────────────────────────────────────────────────
def test_clipboard_linux_mac_e_windows():
    ts, runner, _ = montar("linux")
    runner.saida = "texto copiado ção".encode()
    r = roda(ts["ler_clipboard"])
    assert r["texto"] == "texto copiado ção" and runner.chamadas[0][0][:2] == [
        "wl-paste",
        "--no-newline",
    ]
    assert roda(ts["escrever_clipboard"], texto="abc")["ok"]
    argv, kw = runner.chamadas[1]
    assert argv == ["wl-copy"] and kw["input"] == b"abc"

    ts, runner, _ = montar("win32")
    roda(ts["escrever_clipboard"], texto='"; calc; "')
    argv, kw = runner.chamadas[0]
    assert (
        argv[-1] == "Set-Clipboard -Value $env:CLIP_TEXTO"
        and kw["env"]["CLIP_TEXTO"] == '"; calc; "'
    )

    ts, runner, _ = montar("darwin")
    roda(ts["ler_clipboard"])
    assert runner.chamadas[0][0] == ["pbpaste"]


def test_clipboard_sem_comando_e_texto_grande():
    ts, *_ = montar("linux", disponivel=())
    assert "nenhum comando" in roda(ts["ler_clipboard"])["erro"]
    assert "nenhum comando" in roda(ts["escrever_clipboard"], texto="x")["erro"]
    ts, runner, _ = montar("linux")
    assert "passa de" in roda(ts["escrever_clipboard"], texto="x" * 30000)["erro"]
    assert runner.chamadas == []
    runner.saida = b"y" * 25000
    r = roda(ts["ler_clipboard"])
    assert len(r["texto"]) == 20000 and r["truncado"] is True
