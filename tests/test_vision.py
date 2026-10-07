"""Visão: cliente do gateway (imagem + pergunta → texto) e as três ferramentas, sob a política."""

import json
import subprocess

import httpx
import pytest

from orion.gateway import Endpoint
from orion.policy import Action, Context, PathGuard, PolicyEngine, Risk, ToolCall
from orion.tools.vision import MANTER, argv_captura, vision_tools
from orion.vision import MAX_IMAGEM, MAX_RESPOSTA, Vision, VisionError

CHAVE = "sk-chave-secreta-da-visao-0123456789"
JPG = b"\xff\xd8\xff\xe0imagem-de-mentira"


def ep(nome="gw", url="http://gateway.local/v1", modelo="modelo-visao"):
    return Endpoint(nome, url, modelo, CHAVE)


def resposta(texto):
    return httpx.Response(200, json={"choices": [{"message": {"content": texto}}]})


class Gateway:
    """API de visão de mentira: guarda o corpo recebido."""

    def __init__(self, *respostas):
        self.respostas = list(respostas) or [resposta("Uma janela do terminal.")]
        self.reqs: list[httpx.Request] = []

    def __call__(self, req: httpx.Request) -> httpx.Response:
        self.reqs.append(req)
        r = self.respostas.pop(0) if len(self.respostas) > 1 else self.respostas[0]
        if isinstance(r, Exception):
            raise r
        return r

    def corpo(self, i=0):
        return json.loads(self.reqs[i].content)


# ── cliente ───────────────────────────────────────────────────────────────────
def test_manda_a_imagem_e_a_pergunta_e_devolve_o_texto():
    gw = Gateway()
    texto = Vision([ep()]).describe(JPG, "image/jpeg", "o que é isto?", httpx.MockTransport(gw))
    assert texto == "Uma janela do terminal."
    corpo = gw.corpo()
    assert corpo["model"] == "modelo-visao" and corpo["stream"] is False
    partes = corpo["messages"][0]["content"]
    assert partes[0] == {"type": "text", "text": "o que é isto?"}
    assert partes[1]["image_url"]["url"].startswith("data:image/jpeg;base64,")
    assert gw.reqs[0].headers["authorization"] == f"Bearer {CHAVE}"
    assert gw.reqs[0].url.path == "/v1/chat/completions"


def test_pergunta_vazia_usa_a_padrao():
    gw = Gateway()
    Vision([ep()]).describe(JPG, "image/jpeg", "  ", httpx.MockTransport(gw))
    assert "Descreva" in gw.corpo()["messages"][0]["content"][0]["text"]


def test_cai_para_o_proximo_endpoint_e_o_erro_nao_leva_url_nem_chave():
    gw = Gateway(httpx.ConnectError(f"falhou em http://gateway.local/v1 com {CHAVE}"))
    with pytest.raises(VisionError) as e:
        Vision([ep()]).describe(JPG, "image/jpeg", "", httpx.MockTransport(gw))
    assert CHAVE not in str(e.value) and "gateway.local" not in str(e.value)
    assert "ConnectError" in str(e.value)

    gw = Gateway(httpx.Response(429), resposta("ok pelo segundo"))
    texto = Vision([ep("a"), ep("b")]).describe(JPG, "image/jpeg", "", httpx.MockTransport(gw))
    assert texto == "ok pelo segundo" and len(gw.reqs) == 2


@pytest.mark.parametrize(
    "r",
    [
        httpx.Response(500, text=f"erro {CHAVE}"),
        httpx.Response(200, json={"choices": []}),
        httpx.Response(200, text="não é json"),
        resposta("   "),
    ],
)
def test_resposta_ruim_vira_erro_claro_sem_vazar_o_corpo(r):
    with pytest.raises(VisionError) as e:
        Vision([ep()]).describe(JPG, "image/jpeg", "", httpx.MockTransport(Gateway(r)))
    assert CHAVE not in str(e.value)


def test_imagem_vazia_ou_grande_demais_nem_sai_do_computador():
    gw = Gateway()
    for dado in (b"", b"x" * (MAX_IMAGEM + 1)):
        with pytest.raises(VisionError):
            Vision([ep()]).describe(dado, "image/jpeg", "", httpx.MockTransport(gw))
    assert gw.reqs == []


def test_aceita_partes_de_conteudo_e_corta_resposta_enorme():
    partes = httpx.Response(
        200, json={"choices": [{"message": {"content": [{"type": "text", "text": "a"}]}}]}
    )
    assert (
        Vision([ep()]).describe(JPG, "image/jpeg", "", httpx.MockTransport(Gateway(partes))) == "a"
    )
    grande = resposta("z" * (MAX_RESPOSTA + 500))
    out = Vision([ep()]).describe(JPG, "image/jpeg", "", httpx.MockTransport(Gateway(grande)))
    assert len(out) == MAX_RESPOSTA


# ── captura ───────────────────────────────────────────────────────────────────
def test_argv_de_captura_por_sistema(tmp_path):
    d = tmp_path / "t.jpg"
    todos = lambda n: f"/usr/bin/{n}"  # noqa: E731
    assert argv_captura("darwin", d, todos)[0] == ["screencapture", "-x", "-t", "jpg", str(d)]
    argv, env = argv_captura("win32", d, lambda n: n if n == "pwsh" else None)
    assert argv[0] == "pwsh" and "CopyFromScreen" in argv[-1]
    assert env["CAPTURA_DESTINO"] == str(d) and str(d) not in argv[-1]  # destino por variável
    assert argv_captura("linux", d, todos)[0][0] == "grim"
    assert argv_captura("linux", d, lambda n: n if n == "scrot" else None)[0][0] == "scrot"
    assert argv_captura("linux", d, lambda n: n if n == "import" else None)[0][:3] == [
        "import", "-window", "root",
    ]  # fmt: skip
    assert argv_captura("linux", d, lambda n: None) is None
    assert argv_captura("win32", d, lambda n: None) is None


class Camera:
    """`runner` de mentira: grava a imagem onde o comando mandou, como o programa de verdade."""

    def __init__(self, codigo=0, grava=True, levanta=None):
        self.codigo, self.grava, self.levanta = codigo, grava, levanta
        self.chamadas: list[tuple[list[str], dict]] = []

    def __call__(self, argv, **kw):
        self.chamadas.append((argv, kw))
        if self.levanta:
            raise self.levanta
        if self.grava:
            from pathlib import Path

            Path(argv[-1]).write_bytes(JPG)
        return subprocess.CompletedProcess(
            argv, self.codigo, b"", b"sem display" if self.codigo else b""
        )


def montar(
    tmp_path, gw=None, camera=None, tempo=None, achar=lambda n: f"/usr/bin/{n}", plat="linux"
):
    gw = gw or Gateway()
    camera = camera or Camera()
    relogio = iter(range(1000, 100000)) if tempo is None else tempo
    ts = vision_tools(
        Vision([ep()]), tmp_path / "capturas",
        platform=plat, runner=camera, which=achar, clock=lambda: next(relogio),
        transport=httpx.MockTransport(gw),
    )  # fmt: skip
    return {t.name: t for t in ts}, gw, camera


def roda(tool, **args):
    return json.loads(tool.run(args))


def test_capturar_tela_grava_na_pasta_do_orion_e_devolve_o_caminho(tmp_path):
    ts, _, camera = montar(tmp_path)
    r = roda(ts["capturar_tela"])
    assert r["ok"] and r["bytes"] == len(JPG)
    assert r["path"].startswith(str(tmp_path / "capturas")) and r["path"].endswith(".jpg")
    env = camera.chamadas[0][1]["env"]
    assert not any(k.startswith("ORION_") for k in env)


def test_capturar_falha_com_motivo_quando_nao_ha_comando_display_ou_imagem(tmp_path):
    ts, *_ = montar(tmp_path, achar=lambda n: None)
    assert "grim, scrot" in roda(ts["capturar_tela"])["erro"]
    ts, *_ = montar(tmp_path, camera=Camera(codigo=1, grava=False))
    assert "sem display" in roda(ts["capturar_tela"])["erro"]
    ts, *_ = montar(tmp_path, camera=Camera(grava=False))  # saiu 0 mas não gerou arquivo
    assert roda(ts["capturar_tela"])["ok"] is False
    ts, *_ = montar(tmp_path, camera=Camera(levanta=subprocess.TimeoutExpired("grim", 1)))
    assert "TimeoutExpired" in roda(ts["capturar_tela"])["erro"]


def test_so_as_ultimas_capturas_ficam_no_disco(tmp_path):
    ts, *_ = montar(tmp_path)
    for _ in range(MANTER + 5):
        assert roda(ts["capturar_tela"])["ok"]
    assert len(list((tmp_path / "capturas").glob("tela-*.jpg"))) == MANTER


# ── explicar_tela e analisar_imagem ───────────────────────────────────────────
def test_explicar_tela_captura_pergunta_e_apaga_a_imagem(tmp_path):
    ts, gw, _ = montar(tmp_path)
    r = roda(ts["explicar_tela"], pergunta="tem erro na tela?")
    assert r == {"ok": True, "descricao": "Uma janela do terminal."}
    assert gw.corpo()["messages"][0]["content"][0]["text"] == "tem erro na tela?"
    assert list((tmp_path / "capturas").glob("*")) == []  # a tela não fica guardada


def test_explicar_tela_apaga_a_imagem_mesmo_quando_o_modelo_falha(tmp_path):
    ts, *_ = montar(tmp_path, gw=Gateway(httpx.Response(500)))
    r = roda(ts["explicar_tela"])
    assert r["ok"] is False and "HTTP 500" in r["erro"]
    assert list((tmp_path / "capturas").glob("*")) == []


def test_analisar_imagem_valida_o_arquivo_antes_de_enviar(tmp_path):
    ts, gw, _ = montar(tmp_path)
    assert "não encontrado" in roda(ts["analisar_imagem"], path=str(tmp_path / "x.png"))["erro"]
    (tmp_path / "a.txt").write_text("oi")
    assert "formato" in roda(ts["analisar_imagem"], path=str(tmp_path / "a.txt"))["erro"]
    (tmp_path / "grande.png").write_bytes(b"x" * (MAX_IMAGEM + 1))
    assert "passa de" in roda(ts["analisar_imagem"], path=str(tmp_path / "grande.png"))["erro"]
    assert gw.reqs == []
    foto = tmp_path / "foto.PNG"
    foto.write_bytes(b"\x89PNGfoto")
    r = roda(ts["analisar_imagem"], path=str(foto), pergunta="o que é?")
    assert r["ok"] and gw.corpo()["messages"][0]["content"][1]["image_url"]["url"].startswith(
        "data:image/png;base64,"
    )
    assert foto.exists()  # a imagem do usuário nunca é apagada


# ── política ──────────────────────────────────────────────────────────────────
@pytest.fixture
def politica(tmp_path):
    return PolicyEngine(path_guard=PathGuard(protected_roots=(tmp_path / "p",), safe_roots=()))


def decide(p, nome, args=None):
    return p.evaluate(ToolCall(nome, args or {}), Context("s"))


def test_classes_de_risco_da_visao(politica):
    assert decide(politica, "capturar_tela").risk is Risk.WRITE
    assert decide(politica, "capturar_tela").action is Action.ALLOW  # fica no audit
    d = decide(politica, "explicar_tela")
    assert d.risk is Risk.EXEC and d.action is Action.CONFIRM  # a tela inteira vai ao modelo
    assert decide(politica, "analisar_imagem", {"path": "/tmp/f.png"}).action is Action.ALLOW


def test_analisar_imagem_de_segredo_confirma_e_o_resultado_e_externo(politica):
    assert decide(politica, "analisar_imagem", {"path": "/home/x/.ssh/id_rsa.png"}).action is (
        Action.CONFIRM
    )
    ctx = Context("s")
    politica.note_result(ToolCall("analisar_imagem", {"path": "/tmp/f.png"}), ctx)
    assert ctx.tainted  # texto lido de uma imagem é dado, e a sessão passa a confirmar escrita
