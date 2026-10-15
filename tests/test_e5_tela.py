"""E5.1 a E5.3 (estende REGRAS.md: memória da tela: só texto, local e com opt-in): busca pela interface, exclusão por programa e tela bloqueada."""

import pytest
from fastapi.testclient import TestClient

from orion.app import create_app
from orion.config import Settings
from orion.memory import MemoryStore
from orion.screen_memory import ScreenMemory, app_em_foco, tela_bloqueada
from tests.test_screen_memory import AUTH, TEXTO, TOKEN, Relogio


@pytest.fixture
def relogio():
    return Relogio()


@pytest.fixture
def store(tmp_path, relogio):
    s = MemoryStore(tmp_path / "t5.db", clock=relogio)
    yield s
    s.close()


def montar(store, relogio, *, locked=None, app=None, apps=(), permitir=False, titulo="Janela"):
    return ScreenMemory(
        store,
        capture=lambda d: d.write_bytes(b"\xff\xd8") or True,
        ocr=lambda i: TEXTO,
        title=lambda: titulo,
        allow_unknown_title=permitir,
        clock=relogio,
        app=app,
        exclude_apps=list(apps),
        locked=locked,
    )


# ── E5.3 ────────────────────────────────────────────────────────────────────
def test_tela_bloqueada_nao_gera_linha_no_screen_log(store, relogio):
    estado = {"v": True}
    t = montar(store, relogio, locked=lambda: estado["v"])
    assert t.run() == "bloqueada" and store.screen_count() == 0
    assert t.stats["bloqueada"] == 1
    estado["v"] = False
    relogio.t += 400
    assert t.run() == "gravada" and store.screen_count() == 1
    estado["v"] = True
    relogio.t += 400
    store.clear_screen()
    assert t.run() == "bloqueada" and store.screen_count() == 0


def test_sem_como_saber_se_esta_bloqueada_nao_captura_a_menos_que_libere(store, relogio):
    assert montar(store, relogio, locked=lambda: None).run() == "bloqueada"
    assert store.screen_count() == 0
    assert montar(store, relogio, locked=lambda: None, permitir=True).run() == "gravada"


def test_a_checagem_de_bloqueio_vem_antes_de_capturar_ou_ler_o_titulo(store, relogio):
    chamadas = []
    t = ScreenMemory(
        store,
        capture=lambda d: chamadas.append("captura") or True,
        ocr=lambda i: TEXTO,
        title=lambda: chamadas.append("titulo") or "x",
        locked=lambda: True,
        clock=relogio,
    )
    assert t.run() == "bloqueada" and chamadas == []


class R:
    def __init__(self, out="", rc=0):
        self.stdout, self.returncode = out.encode(), rc


def runner_de(saida, rc=0):
    chamados = []

    def run(argv, **kw):
        chamados.append(argv)
        return R(saida, rc)

    run.chamados = chamados  # type: ignore[attr-defined]
    return run


def test_tela_bloqueada_por_sistema():
    achar = {"loginctl": "/usr/bin/loginctl", "pwsh": "/usr/bin/pwsh"}.get
    r = runner_de("yes\n")
    assert (
        tela_bloqueada(platform="linux", runner=r, which=achar, environ={"XDG_SESSION_ID": "7"})
        is True
    )
    assert r.chamados[0][:3] == ["loginctl", "show-session", "7"]
    assert (
        tela_bloqueada(platform="linux", runner=runner_de("no"), which=achar, environ={}) is False
    )
    assert (
        tela_bloqueada(platform="linux", runner=runner_de("lixo"), which=achar, environ={}) is None
    )
    assert tela_bloqueada(platform="linux", runner=runner_de("yes"), which=lambda _: None) is None
    assert tela_bloqueada(platform="win32", runner=runner_de("locked"), which=achar) is True
    assert tela_bloqueada(platform="win32", runner=runner_de("unlocked"), which=achar) is False
    assert tela_bloqueada(platform="win32", runner=runner_de("", 1), which=achar) is None
    assert tela_bloqueada(platform="darwin", runner=runner_de("locked"), which=achar) is True
    assert tela_bloqueada(platform="darwin", runner=runner_de("unlocked"), which=achar) is False


# ── E5.2 ────────────────────────────────────────────────────────────────────
def test_programa_excluido_nem_e_capturado(store, relogio):
    capturou = []
    t = ScreenMemory(
        store,
        capture=lambda d: capturou.append(1) or d.write_bytes(b"\xff") or True,
        ocr=lambda i: TEXTO,
        title=lambda: "Cofre de senhas",
        app=lambda: "KeePassXC.exe",
        exclude_apps=["keepassxc"],
        clock=relogio,
    )
    assert t.run() == "excluida" and capturou == [] and store.screen_count() == 0
    assert t.stats["excluidas"] == 1


def test_programa_normal_e_gravado_com_o_nome_do_programa(store, relogio):
    t = montar(store, relogio, app=lambda: "Code", apps=["KeePassXC", "1Password"])
    assert t.run() == "gravada"
    [linha] = store.query("SELECT app FROM screen_log")
    assert linha["app"] == "code"


def test_nome_do_programa_vem_sem_extensao_e_sem_caixa(store, relogio):
    for nome in ("KEEPASSXC.EXE", "keepassxc", "1Password.app"):
        t = montar(store, relogio, app=lambda n=nome: n, apps=["keepassxc.exe", "1password"])
        assert t.run() == "excluida", nome


def test_sem_como_saber_o_programa_nao_captura_quando_ha_lista(store, relogio):
    assert montar(store, relogio, app=lambda: None, apps=["keepassxc"]).run() == "sem_app"
    assert store.screen_count() == 0
    assert (
        montar(store, relogio, app=lambda: None, apps=["keepassxc"], permitir=True).run()
        == "gravada"
    )
    # sem lista de programas, não consulta o programa em foco
    chamou = []
    t = montar(store, relogio, app=lambda: chamou.append(1) or None, apps=[])
    t.run()
    assert chamou == []


def test_app_em_foco_por_sistema():
    achar = {"xdotool": "/usr/bin/xdotool", "pwsh": "/usr/bin/pwsh"}.get
    assert app_em_foco(platform="win32", runner=runner_de("chrome\n"), which=achar) == "chrome"
    assert app_em_foco(platform="darwin", runner=runner_de("Safari"), which=achar) == "Safari"
    assert (
        app_em_foco(platform="linux", runner=runner_de("abc"), which=achar) is None
    )  # pid inválido
    assert app_em_foco(platform="linux", runner=runner_de("1", 1), which=achar) is None
    assert app_em_foco(platform="linux", runner=runner_de("1"), which=lambda _: None) is None


# ── E5.1 ────────────────────────────────────────────────────────────────────
@pytest.fixture
def c(tmp_path):
    settings = Settings(
        data_dir=tmp_path / "d", admin_token=TOKEN, jobs_enabled=False, _env_file=None
    )
    with TestClient(create_app(settings), base_url="http://127.0.0.1") as cli:
        yield cli


def test_busca_da_memoria_da_tela_na_interface(c, relogio):
    m = c.app.state.orion.memory
    m.add_screen("Reunião do TCC marcada para quarta com o orientador", "Calendar", "chrome")
    m.add_screen("Receita de bolo de cenoura com cobertura", "Receitas", "firefox")
    assert c.get("/tela/busca", params={"q": "orientador"}).status_code == 401
    r = c.get("/tela/busca", params={"q": "orientador"}, headers=AUTH).json()["resultados"]
    assert len(r) == 1 and r[0]["janela"] == "Calendar" and r[0]["app"] == "chrome"
    assert "[orientador]" in r[0]["trecho"] and r[0]["quando"] > 0
    assert c.get("/tela/busca", params={"q": "bolo", "dias": 1}, headers=AUTH).json()["resultados"]
    assert c.get("/tela/busca", params={"q": "x"}, headers=AUTH).status_code == 422  # curto demais
    assert c.get("/tela/busca", params={"q": "inexistente"}, headers=AUTH).json() == {
        "resultados": []
    }
