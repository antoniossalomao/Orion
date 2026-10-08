"""Memória da tela (regra 44): OCR local, só texto, janelas excluídas, retenção e pausa."""

from pathlib import Path
from types import SimpleNamespace

import pytest
from fastapi.testclient import TestClient

from orion.app import create_app
from orion.config import Settings
from orion.memory import MemoryStore
from orion.policy import PathGuard, PolicyEngine
from orion.policy.engine import Action, Context, ToolCall
from orion.screen_memory import (
    TOOL_SPEC,
    ScreenMemory,
    capturador_de_tela,
    filtrar,
    ocr_tesseract,
    parse_lista,
    screen_tool,
    titulo_da_janela,
)

TOKEN = "token-de-teste-com-16+"
AUTH = {"Authorization": f"Bearer {TOKEN}"}
TEXTO = "Reunião do TCC amanhã às 14h\nRevisar o capítulo de metodologia\nEnviar para o orientador"


class Relogio:
    def __init__(self, t=1_791_000_000.0):
        self.t = t

    def __call__(self):
        return self.t


@pytest.fixture
def relogio():
    return Relogio()


@pytest.fixture
def store(tmp_path, relogio):
    s = MemoryStore(tmp_path / "t.db", clock=relogio)
    yield s
    s.close()


def _tela(store, relogio, *, texto=TEXTO, titulo="Obsidian - Plano", exclude=(), permitir=False):
    arquivos = []

    def capturar(destino: Path) -> bool:
        destino.write_bytes(b"\xff\xd8fake")
        arquivos.append(destino)
        return True

    t = ScreenMemory(
        store,
        capture=capturar,
        ocr=lambda img: texto if isinstance(texto, str) or texto is None else texto(img),
        title=lambda: titulo,
        interval_s=300,
        retention_days=7,
        exclude=list(exclude),
        allow_unknown_title=permitir,
        clock=relogio,
    )
    return t, arquivos


def test_grava_so_texto_e_apaga_a_imagem_sempre(store, relogio):
    t, arquivos = _tela(store, relogio)
    assert t.run() == "gravada"
    assert store.last_screen_text() == TEXTO
    assert len(arquivos) == 1 and not arquivos[0].exists()  # a imagem não sobrevive
    assert not arquivos[0].parent.exists()  # nem a pasta temporária


def test_a_imagem_some_mesmo_se_o_ocr_explodir(store, relogio):
    def ruim(img):
        raise RuntimeError("tesseract morreu")

    t, arquivos = _tela(store, relogio, texto=ruim)
    with pytest.raises(RuntimeError):
        t.run()
    assert not arquivos[0].exists()


def test_janela_excluida_nem_captura_e_sem_titulo_falha_fechada(store, relogio):
    t, arquivos = _tela(
        store, relogio, titulo="Nubank - Minha conta", exclude=parse_lista("banco; nubank;login")
    )
    assert t.run() == "excluida" and arquivos == [] and t.stats["excluidas"] == 1
    t2, arquivos2 = _tela(store, relogio, titulo=None)
    assert t2.run() == "sem_titulo" and arquivos2 == []
    t3, _ = _tela(store, relogio, titulo=None, permitir=True)
    assert t3.run() == "gravada"
    assert store.screen_count() == 1


def test_filtra_linhas_com_cara_de_segredo_e_repetidas_nao_regravam(store, relogio):
    bruto = (
        "Plano de estudo da semana de prova\n"
        "senha do wifi: abc12345\n"
        "meu cpf 123.456.789-09 está aqui\n"
        "cartão 4111 1111 1111 1111 vence\n"
        "api_key = sk-aaaaaaaaaaaaaaaaaaaaaaaaaaaa\n"
        "Terminar a lista de exercícios de cálculo"
    )
    limpo = filtrar(bruto)
    assert "Plano de estudo" in limpo and "exercícios" in limpo
    for proibido in ("wifi", "123.456", "4111", "sk-aaaa"):
        assert proibido not in limpo
    assert filtrar("curto") == ""
    t, _ = _tela(store, relogio, texto=bruto)
    assert t.run() == "gravada"
    relogio.t += 400
    assert t.run() == "igual" and store.screen_count() == 1


def test_ocr_ausente_e_pouco_texto_nao_gravam(store, relogio):
    t, _ = _tela(store, relogio, texto=None)
    assert t.run() == "ocr_indisponivel" and store.screen_count() == 0
    t2, _ = _tela(store, relogio, texto="ok")
    assert t2.run() == "pouco_texto"


def test_intervalo_pausa_e_retencao(store, relogio):
    t, _ = _tela(store, relogio)
    assert t.devida()
    t.run()
    assert not t.devida()  # intervalo de 5 min
    relogio.t += 301
    assert t.devida()
    t.pausar(True)
    assert t.pausada and not t.devida()
    t.pausar(False)
    assert t.devida()
    relogio.t += 8 * 86400  # 8 dias depois: o registro velho cai na poda da próxima rodada
    t.run()
    assert store.screen_count() <= 1 and all(
        r["ts"] >= relogio.t - 7 * 86400 for r in store.search_screen("metodologia")
    )


def test_busca_por_palavra_com_trecho_e_janela_de_dias(store, relogio):
    store.add_screen("Anotações sobre o orquestrador de agentes e o MCP", "Obsidian")
    relogio.t += 40 * 86400
    achou = store.search_screen("orquestrador", dias=90)
    assert len(achou) == 1 and "[orquestrador]" in achou[0]["trecho"]
    assert store.search_screen("orquestrador", dias=7) == []
    assert store.search_screen("") == []
    assert store.clear_screen() == 1 and store.screen_count() == 0


def test_ferramenta_e_leitura_externa_e_contamina(store, tmp_path):
    store.add_screen("rascunho do contrato de estágio", "Word")
    t = screen_tool(store)
    r = t.run({"consulta": "contrato"})
    assert "contrato" in r and "Word" in r
    motor = PolicyEngine(path_guard=PathGuard(protected_roots=(tmp_path / "p",), safe_roots=()))
    motor.register_tool(TOOL_SPEC)
    ctx = Context("s")
    assert motor.evaluate(ToolCall("buscar_tela", {"consulta": "x"}), ctx).action is Action.ALLOW
    motor.note_result(ToolCall("buscar_tela", {}), ctx)
    assert ctx.tainted  # texto de tela é conteúdo de terceiros


def test_comandos_por_sistema_sem_executar_nada_de_verdade():
    visto = []

    def runner(argv, **kw):
        visto.append(argv)
        return SimpleNamespace(returncode=0, stdout=b"Meu Documento - Word\n", stderr=b"")

    achar = {"xdotool", "tesseract", "grim"}
    which = lambda n: f"/usr/bin/{n}" if n in achar else None  # noqa: E731
    assert titulo_da_janela(platform="linux", runner=runner, which=which) == "Meu Documento - Word"
    assert visto[0][:2] == ["xdotool", "getactivewindow"]
    assert titulo_da_janela(platform="linux", runner=runner, which=lambda n: None) is None
    assert titulo_da_janela(platform="darwin", runner=runner, which=which) == "Meu Documento - Word"
    assert visto[-1][0] == "osascript"
    assert titulo_da_janela(platform="win32", runner=runner, which=lambda n: "pwsh")
    assert "GetForegroundWindow" in visto[-1][-1]
    assert ocr_tesseract(Path("/x.jpg"), runner=runner, which=which) == "Meu Documento - Word\n"
    assert visto[-1][1:] == [str(Path("/x.jpg")), "stdout", "-l", "por+eng"]
    assert ocr_tesseract(Path("/x.jpg"), runner=runner, which=lambda n: None) is None
    cap = capturador_de_tela(platform="linux", runner=runner, which=which)
    assert cap(Path("/nao/existe.jpg")) is False  # o comando "rodou", mas não gerou imagem


@pytest.fixture
def c(tmp_path):
    settings = Settings(data_dir=tmp_path / "d", admin_token=TOKEN, _env_file=None)

    def fabrica(s, memory):
        return ScreenMemory(
            memory,
            capture=lambda d: False,
            ocr=lambda i: None,
            title=lambda: "x",
        )

    with TestClient(
        create_app(settings, gateway_factory=lambda _: None, screen_factory=fabrica),
        base_url="http://127.0.0.1",
    ) as cli:
        yield cli


def test_api_pausa_retoma_e_mostra_so_contagens(c):
    assert c.get("/tela").status_code == 401
    assert c.get("/tela", headers=AUTH).json()["ligada"] is True
    assert c.post("/tela/pausa", json={"ativa": False}, headers=AUTH).json() == {"pausada": True}
    estado = c.get("/tela", headers=AUTH).json()
    assert estado["pausada"] is True and "texto" not in estado
    assert c.post("/tela/pausa", json={"ativa": True}, headers=AUTH).json() == {"pausada": False}


def test_desligada_por_padrao_e_api_diz_409(tmp_path):
    settings = Settings(data_dir=tmp_path / "d", admin_token=TOKEN, _env_file=None)
    assert settings.screen_memory is False
    with TestClient(
        create_app(settings, gateway_factory=lambda _: None), base_url="http://127.0.0.1"
    ) as cli:
        assert cli.get("/tela", headers=AUTH).json() == {"ligada": False}
        assert cli.post("/tela/pausa", json={"ativa": False}, headers=AUTH).status_code == 409
