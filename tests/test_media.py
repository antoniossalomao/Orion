"""controlar_midia e controlar_janela: o argv por sistema, o que nunca vai na linha de comando e
a política (janela confirma sempre, e o título é conteúdo externo)."""

import json
import subprocess

import pytest

from orion.policy import Action, Context, PathGuard, PolicyEngine, Risk, ToolCall
from orion.tools.media import ACOES_JANELA, ACOES_MIDIA, argv_midia, media_tools

WMCTRL_L = (
    "0x04600003  0 host Terminal — bash\n"
    "0x04800005  0 host Firefox — Cartão de crédito\n"
    "0x04a00007  0 host Firefox — Notas\n"
    "linha-estranha\n"
)


class Runner:
    def __init__(self, saida=b"", erro=b"", codigo=0, levanta=None, por_comando=None):
        self.chamadas: list[tuple[list[str], dict]] = []
        self.saida, self.erro, self.codigo, self.levanta = saida, erro, codigo, levanta
        self.por_comando = por_comando or {}

    def __call__(self, argv, **kw):
        self.chamadas.append((argv, kw))
        if self.levanta:
            raise self.levanta
        saida = self.por_comando.get(argv[1] if len(argv) > 1 else "", self.saida)
        return subprocess.CompletedProcess(argv, self.codigo, saida, self.erro)


def montar(platform, runner=None, disponivel=("wmctrl", "playerctl", "wpctl", "pwsh")):
    runner = runner or Runner()
    ts = media_tools(
        platform=platform,
        runner=runner,
        which=lambda n: f"/usr/bin/{n}" if n in disponivel else None,
    )
    return {t.name: t for t in ts}, runner


def roda(tool, **args):
    return json.loads(tool.run(args))


# ── mídia ─────────────────────────────────────────────────────────────────────
def test_midia_valida_a_acao_antes_de_qualquer_comando():
    ts, runner = montar("linux")
    assert "inválida" in roda(ts["controlar_midia"], acao="formatar_disco")["erro"]
    assert runner.chamadas == []


@pytest.mark.parametrize("acao", ACOES_MIDIA)
def test_toda_acao_de_midia_tem_comando_em_cada_sistema(acao):
    todos = lambda n: f"/usr/bin/{n}"  # noqa: E731
    for plat in ("win32", "darwin", "linux"):
        assert argv_midia(acao, plat, todos) is not None, (plat, acao)


def test_midia_linux_usa_playerctl_e_wpctl_com_alternativas():
    ts, runner = montar("linux")
    assert roda(ts["controlar_midia"], acao="tocar_pausar")["ok"]
    assert runner.chamadas[0][0] == ["playerctl", "play-pause"]
    roda(ts["controlar_midia"], acao="volume_mais")
    assert runner.chamadas[1][0] == ["wpctl", "set-volume", "@DEFAULT_AUDIO_SINK@", "5%+"]
    tudo = lambda n: n if n in ("pactl",) else None  # noqa: E731
    assert argv_midia("mudo", "linux", tudo)[0] == [
        "pactl", "set-sink-mute", "@DEFAULT_SINK@", "toggle",
    ]  # fmt: skip
    assert argv_midia("volume_menos", "linux", lambda n: n if n == "amixer" else None)[0] == [
        "amixer", "-q", "sset", "Master", "5%-",
    ]  # fmt: skip
    assert argv_midia("proxima", "linux", lambda n: None) is None


def test_midia_sem_comando_ou_com_falha_diz_o_motivo():
    ts, _ = montar("linux", disponivel=())
    assert "playerctl" in roda(ts["controlar_midia"], acao="proxima")["erro"]
    ts, _ = montar("linux", Runner(codigo=1, erro=b"Nenhum player"))
    r = roda(ts["controlar_midia"], acao="proxima")
    assert r["ok"] is False and "Nenhum player" in r["erro"]
    ts, _ = montar("linux", Runner(levanta=subprocess.TimeoutExpired("playerctl", 1)))
    assert "TimeoutExpired" in roda(ts["controlar_midia"], acao="proxima")["erro"]


def test_midia_windows_manda_o_codigo_da_tecla_por_variavel_e_repete_o_volume():
    argv, env = argv_midia("tocar_pausar", "win32", lambda n: n if n == "pwsh" else None)
    assert argv[0] == "pwsh" and "keybd_event" in argv[-1]
    assert env["MIDIA_VK"] == "179" and env["MIDIA_VEZES"] == "1"
    _, env = argv_midia("volume_mais", "win32", lambda n: n if n == "pwsh" else None)
    assert env["MIDIA_VK"] == "175" and env["MIDIA_VEZES"] == "5"
    assert not any(k.startswith("ORION_") for k in env)


def test_midia_mac_usa_osascript_com_script_fixo():
    argv, _ = argv_midia("proxima", "darwin", lambda n: n)
    assert argv[:2] == ["osascript", "-e"] and "next track" in argv[2]
    assert "Spotify" in argv[2] and "Music" in argv[2]
    argv, _ = argv_midia("mudo", "darwin", lambda n: n)
    assert "output muted" in argv[2]


# ── janelas ───────────────────────────────────────────────────────────────────
def ts_linux(codigo=0):
    return montar("linux", Runner(saida=WMCTRL_L.encode(), codigo=codigo))


def test_janela_valida_acao_titulo_e_tamanho_antes_de_agir():
    ts, runner = ts_linux()
    assert "inválida" in roda(ts["controlar_janela"], acao="explodir")["erro"]
    assert "título" in roda(ts["controlar_janela"], acao="fechar")["erro"]
    assert "inválido" in roda(ts["controlar_janela"], acao="fechar", titulo="x" * 201)["erro"]
    assert "inválido" in roda(ts["controlar_janela"], acao="fechar", titulo="a\x00b")["erro"]
    assert runner.chamadas == []


def test_janela_linux_lista_sem_a_linha_estranha():
    ts, runner = ts_linux()
    r = roda(ts["controlar_janela"], acao="listar")
    assert r["janelas"] == ["Terminal — bash", "Firefox — Cartão de crédito", "Firefox — Notas"]
    assert runner.chamadas[0][0] == ["wmctrl", "-l"]


def test_janela_linux_age_pelo_id_e_nunca_poe_o_titulo_no_comando():
    ts, runner = ts_linux()
    r = roda(ts["controlar_janela"], acao="focar", titulo="terminal")
    assert r["ok"] and r["janela"] == "Terminal — bash"
    assert runner.chamadas[-1][0] == ["wmctrl", "-i", "-a", "0x04600003"]
    roda(ts["controlar_janela"], acao="fechar", titulo="Terminal")
    assert runner.chamadas[-1][0] == ["wmctrl", "-i", "-c", "0x04600003"]
    roda(ts["controlar_janela"], acao="maximizar", titulo="Terminal")
    assert runner.chamadas[-1][0][:5] == ["wmctrl", "-i", "-r", "0x04600003", "-b"]
    roda(ts["controlar_janela"], acao="minimizar", titulo="Terminal")
    assert runner.chamadas[-1][0][-1] == "add,hidden"
    for argv, _ in runner.chamadas:
        assert "-- rm" not in " ".join(argv)


def test_janela_ambigua_ou_inexistente_nao_age():
    ts, runner = ts_linux()
    r = roda(ts["controlar_janela"], acao="fechar", titulo="firefox")
    assert r["ok"] is False and "mais de uma" in r["erro"] and len(r["janelas"]) == 2
    assert "não encontrada" in roda(ts["controlar_janela"], acao="fechar", titulo="zzz")["erro"]
    assert all(c[0][:2] == ["wmctrl", "-l"] for c in runner.chamadas)  # só listou, não agiu


def test_janela_linux_sem_wmctrl_ou_sem_x11_diz_o_motivo():
    ts, _ = montar("linux", disponivel=())
    assert "wmctrl" in roda(ts["controlar_janela"], acao="listar")["erro"]
    ts, _ = ts_linux(codigo=1)
    assert "X11" in roda(ts["controlar_janela"], acao="listar")["erro"]


def test_janela_windows_manda_acao_e_titulo_por_variavel_de_ambiente():
    saida = "firefox|Notas — Firefox\n".encode()
    ts, runner = montar("win32", Runner(saida=saida))
    segredo = "'; Remove-Item -Recurse C:\\ ; '"
    r = roda(ts["controlar_janela"], acao="fechar", titulo=segredo)
    argv, kw = runner.chamadas[0]
    assert r["ok"] and r["janela"] == "firefox|Notas — Firefox"
    assert kw["env"]["JANELA_ACAO"] == "fechar" and kw["env"]["JANELA_TITULO"] == segredo
    assert segredo not in " ".join(argv)  # o texto do modelo não entra na linha de comando
    assert "CloseMainWindow" in argv[-1] and "Count -gt 1" in argv[-1]
    assert not any(k.startswith("ORION_") for k in kw["env"])


def test_janela_windows_ambigua_devolve_a_lista_sem_agir():
    ts, _ = montar("win32", Runner(saida=b"a|Um\nb|Dois\n", erro=b"mais de uma janela", codigo=3))
    r = roda(ts["controlar_janela"], acao="fechar", titulo="u")
    assert r["ok"] is False and "mais de uma" in r["erro"] and r["janelas"] == ["a|Um", "b|Dois"]


def test_janela_no_mac_nao_finge_que_funciona():
    ts, runner = montar("darwin")
    assert "macOS" in roda(ts["controlar_janela"], acao="listar")["erro"]
    assert runner.chamadas == []


def test_todas_as_acoes_de_janela_existem_no_esquema():
    ts, _ = montar("linux")
    enum = ts["controlar_janela"].parameters["properties"]["acao"]["enum"]
    assert tuple(enum) == ACOES_JANELA


# ── política ──────────────────────────────────────────────────────────────────
def test_janela_confirma_sempre_e_o_titulo_e_conteudo_externo(tmp_path):
    p = PolicyEngine(path_guard=PathGuard(protected_roots=(tmp_path,), safe_roots=()))
    d = p.evaluate(ToolCall("controlar_janela", {"acao": "listar"}), Context("s"))
    assert d.risk is Risk.EXEC and d.action is Action.CONFIRM
    ctx = Context("s")
    p.note_result(ToolCall("controlar_janela", {"acao": "listar"}), ctx)
    assert ctx.tainted
    m = p.evaluate(ToolCall("controlar_midia", {"acao": "mudo"}), Context("s"))
    assert m.risk is Risk.WRITE and m.action is Action.ALLOW
