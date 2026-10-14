"""E11 (visão e câmera): regras 64 (câmera), 65 (rosto) e 66 (presença).

Nada aqui toca câmera, modelo ou Bluetooth de verdade: fotos, saídas do YOLO, vetores e CLIs são
falsos. A prova com webcam real é do Antônio (ver ORION_MELHORIAS.md).
"""

import subprocess
from types import SimpleNamespace

import numpy as np
import pytest

from orion import saidas
from orion.camera import MAX_FOTO, CameraError, foto_jpeg
from orion.memory import MemoryStore
from orion.policy import DEFAULT_TOOLS
from orion.policy.classes import Risk
from orion.presenca import Presenca, Sinais, bloquear_tela, ocioso_segundos
from orion.rosto import LIMIAR, RostoError, Rostos
from orion.tools.camera import camera_tools
from orion.vigia import Vigia
from orion.visao_cli import argv_cli, descrever_por_cli, describe_ordenado, ordem
from orion.visao_local import (
    NUNCA_GUARDAR,
    RETENCAO_S,
    Avistamentos,
    Deteccao,
    Detector,
    decodificar_saida,
    nome_em_ingles,
    olhar_uma_vez,
)
from orion.vision import Vision, VisionError


@pytest.fixture
def memoria(tmp_path):
    m = MemoryStore(tmp_path / "m.db")
    yield m
    m.close()


# ── Regra 64: câmera ───────────────────────────────────────────────────────


def test_foto_usa_a_fonte_e_recusa_vazio_e_grande_demais():
    assert foto_jpeg(0, fonte=lambda d: b"\xff\xd8abc") == b"\xff\xd8abc"
    with pytest.raises(CameraError):
        foto_jpeg(0, fonte=lambda d: b"")
    with pytest.raises(CameraError):
        foto_jpeg(0, fonte=lambda d: b"x" * (MAX_FOTO + 1))


def test_erro_de_camera_nao_vaza_indice_nem_caminho():
    with pytest.raises(CameraError) as e:
        foto_jpeg(7, fonte=lambda d: b"")
    assert "7" not in str(e.value) and "/dev" not in str(e.value)


def test_ver_camera_descarta_a_foto_e_pede_a_visao(monkeypatch):
    chamadas = []

    class Visao:
        def describe(self, imagem, mime, pergunta="", transport=None):
            chamadas.append((imagem, mime, pergunta))
            return "um mouse preto"

    ferramentas = {
        t.name: t
        for t in camera_tools(
            device=0,
            vision=Visao(),
            ordem=["gemini"],
            avistamentos=None,
            foto=lambda d: b"\xff\xd8foto",
        )
    }
    r = ferramentas["ver_camera"].fn(pergunta="qual mouse?")
    assert r == {"ok": True, "descricao": "um mouse preto"}
    assert chamadas == [(b"\xff\xd8foto", "image/jpeg", "qual mouse?")]


def test_ver_camera_sem_visao_configurada_devolve_erro_sem_travar():
    ferramentas = {
        t.name: t
        for t in camera_tools(
            device=0, vision=None, ordem=["gemini"], avistamentos=None, foto=lambda d: b"\xff\xd8x"
        )
    }
    r = ferramentas["ver_camera"].fn()
    assert r["ok"] is False and "não configurada" in r["erro"]


def test_onde_vi_so_existe_com_a_vigilia_ligada(memoria):
    ferramentas = {
        t.name: t for t in camera_tools(device=0, vision=None, ordem=None, avistamentos=None)
    }
    assert "onde_vi" not in ferramentas
    av = Avistamentos(memoria, clock=lambda: 1_000_000.0)
    ferramentas = {
        t.name: t for t in camera_tools(device=0, vision=None, ordem=None, avistamentos=av)
    }
    r = ferramentas["onde_vi"].fn(objeto="celular")
    assert r["ok"] and r["vistos"] == [] and "nenhum" in r["nota"]


def test_ler_texto_imagem_valida_arquivo_e_usa_o_ocr(tmp_path):
    img = tmp_path / "print.png"
    img.write_bytes(b"\x89PNG")
    ferramentas = {
        t.name: t
        for t in camera_tools(
            device=0, vision=None, ordem=None, avistamentos=None,
            ocr=lambda arq, langs: "Hello world" if arq == img else None,
        )
    }  # fmt: skip
    ler = ferramentas["ler_texto_imagem"].fn
    assert ler(str(img)) == {"ok": True, "texto": "Hello world", "truncado": False}
    assert ler(str(tmp_path / "nao.png"))["ok"] is False
    txt = tmp_path / "x.txt"
    txt.write_text("oi")
    assert ler(str(txt))["ok"] is False


def test_preparar_imagem_flow_abre_site_e_devolve_prompt_sem_gerar():
    abertos = []
    ferramentas = {
        t.name: t
        for t in camera_tools(
            device=0, vision=None, ordem=None, avistamentos=None,
            abrir_navegador=lambda url: abertos.append(url) or True,
        )
    }  # fmt: skip
    r = ferramentas["preparar_imagem_flow"].fn(descricao="  um gato   azul ")
    assert r["ok"] and r["prompt"] == "um gato azul" and r["aberto"] is True
    assert abertos == [r["url"]]


def test_todas_as_ferramentas_novas_estao_na_politica():
    for nome in ("ver_camera", "onde_vi", "ler_texto_imagem", "preparar_imagem_flow"):
        assert nome in DEFAULT_TOOLS, nome
    assert DEFAULT_TOOLS["ver_camera"].risk is Risk.EXEC  # sempre confirma (regra 64)
    assert DEFAULT_TOOLS["ver_camera"].external is True
    assert DEFAULT_TOOLS["ler_texto_imagem"].read_path_arg == "path"


# ── Regra 64: detecção local e avistamentos ────────────────────────────────


def test_nome_em_portugues_vira_rotulo_coco():
    assert nome_em_ingles("Celular") == "cell phone"
    assert nome_em_ingles("  notebook ") == "laptop"
    assert nome_em_ingles("cadeira") == "chair"
    assert nome_em_ingles("zebra") == "zebra"


def _saida(caixas_classes):
    """Tensor (1, 84, N) no formato do YOLOv8: cx, cy, w, h e 80 scores."""
    n = len(caixas_classes)
    t = np.zeros((1, 84, n), dtype=np.float32)
    for i, ((cx, cy, w, h), classe, conf) in enumerate(caixas_classes):
        t[0, 0, i], t[0, 1, i], t[0, 2, i], t[0, 3, i] = cx, cy, w, h
        t[0, 4 + classe, i] = conf
    return t


def test_decodificar_remove_caixas_fracas_e_duplicadas_da_mesma_classe():
    saida = _saida(
        [
            ((100, 100, 50, 50), 39, 0.9),  # garrafa (índice 39) — a melhor
            ((102, 101, 50, 50), 39, 0.8),  # mesma garrafa, sobreposta: sai
            ((300, 300, 40, 40), 0, 0.95),  # pessoa, mas o filtro de guarda é depois
            ((10, 10, 20, 20), 39, 0.1),  # fraca: sai pelo limiar
        ]
    )
    dets = decodificar_saida(saida, escala=1.0, pad_x=0, pad_y=0)
    assert [d.label for d in dets] == ["person", "bottle"]
    assert dets[1].confidence == 0.9 and (dets[1].x, dets[1].y) == (100, 100)


def test_decodificar_desfaz_o_letterbox():
    saida = _saida([((320, 320, 100, 100), 56, 0.8)])  # cadeira no centro da entrada 640
    d = decodificar_saida(saida, escala=0.5, pad_x=0, pad_y=0)[0]
    assert d.label == "chair" and (d.x, d.y) == (640, 640)  # 320 / 0.5 = 640


def test_avistamentos_nunca_guardam_pessoa_e_apagam_depois_de_7_dias(memoria):
    relogio = {"t": 1_000_000.0}
    av = Avistamentos(memoria, clock=lambda: relogio["t"])
    dets = [Deteccao("person", 0.9, 1, 1, 5, 5), Deteccao("cup", 0.7, 3, 4, 5, 5)]
    assert av.registrar(dets) == 1
    assert "person" in NUNCA_GUARDAR
    assert [v["confianca"] for v in av.onde_vi("copo")] == [0.7]
    assert av.onde_vi("pessoa") == []  # nem consulta por pessoa
    relogio["t"] += RETENCAO_S + 1
    assert av.onde_vi("copo") == []  # passou da retenção
    av.registrar([])  # a faxina roda a cada gravação
    assert memoria.query("SELECT COUNT(*) AS n FROM object_sightings")[0]["n"] == 0


def test_avistamento_guarda_so_rotulo_e_numeros(memoria):
    Avistamentos(memoria).registrar([Deteccao("book", 0.5, 2, 3, 9, 9)])
    colunas = {r["name"] for r in memoria.query("PRAGMA table_info(object_sightings)")}
    assert colunas == {"id", "ts", "label", "confidence", "x", "y"}  # nada de imagem


class _SessaoFalsa:
    def __init__(self, saida):
        self.saida = saida
        self.entradas = []

    def get_inputs(self):
        return [SimpleNamespace(name="images")]

    def run(self, _saidas, dados):
        self.entradas.append(dados)
        return [self.saida]


def test_detector_redimensiona_com_letterbox_e_usa_o_nome_da_entrada(tmp_path):
    pytest.importorskip("cv2")  # extra 'camera'
    sessao = _SessaoFalsa(_saida([((320, 320, 80, 80), 39, 0.9)]))
    det = Detector(tmp_path / "m.onnx", sessao=sessao)
    bgr = np.zeros((480, 640, 3), dtype=np.uint8)  # paisagem: barras no topo e embaixo
    dets = det.detectar(bgr)
    entrada = sessao.entradas[0]["images"]
    assert entrada.shape == (1, 3, 640, 640) and entrada.dtype == np.float32
    assert dets and dets[0].label == "bottle"


def test_olhar_uma_vez_so_olha_com_presenca(memoria, monkeypatch):
    import orion.camera as camera_mod

    monkeypatch.setattr(camera_mod, "decodificar", lambda jpeg: np.zeros((8, 8, 3), np.uint8))
    chamadas = []
    det = SimpleNamespace(detectar=lambda bgr: chamadas.append(1) or [])
    av = Avistamentos(memoria)
    kw = {"foto": lambda: b"x", "detector": det, "avistamentos": av}
    assert olhar_uma_vez(**kw, pode_olhar=lambda: False) == 0
    assert chamadas == []
    assert olhar_uma_vez(**kw, pode_olhar=lambda: True) == 0
    assert chamadas == [1]


# ── Regra 65: rosto ─────────────────────────────────────────────────────────


class _Cofre(dict):
    def get_(self, nome):
        return self.get(nome)


def _rostos(memoria, vetor_por_foto):
    cofre = _Cofre()
    emb = lambda bgr: vetor_por_foto(bgr)  # noqa: E731
    return Rostos(memoria, emb, get=cofre.get_, set_=lambda n, v: cofre.__setitem__(n, v)), cofre


def _foto(i):
    return np.full((4, 4, 3), i, dtype=np.uint8)


def test_cadastro_guarda_vetores_cifrados_e_reconhece_o_dono(memoria):
    dono = np.array([1.0, 0.2, 0.0], dtype=np.float32)
    rostos, cofre = _rostos(memoria, lambda bgr: dono * (1 + 0.01 * int(bgr[0, 0, 0])))
    fotos = [_foto(i) for i in range(5)]
    assert rostos.cadastrar("Antônio", fotos) == 5
    assert "ORION_FACE_KEY" in cofre  # a chave foi para o cofre, não para o banco
    cru = memoria.query("SELECT embedding FROM faces")[0]["embedding"]
    assert bytes(cru) != dono.tobytes()  # está cifrado
    assert rostos.reconhecer(_foto(0)) is True
    estranho = np.array([0.0, 0.0, 1.0], dtype=np.float32)
    rostos2, _ = _rostos(memoria, lambda bgr: estranho)
    assert rostos2.reconhecer(_foto(0)) is False  # outra pessoa, mesmo banco


def test_cadastro_recusa_sem_rosto_e_sem_nome(memoria):
    rostos, _ = _rostos(memoria, lambda bgr: None)
    with pytest.raises(RostoError):
        rostos.cadastrar("Antônio", [_foto(i) for i in range(5)])
    rostos2, _ = _rostos(memoria, lambda bgr: np.ones(3))
    with pytest.raises(RostoError):
        rostos2.cadastrar("  ", [_foto(i) for i in range(5)])


def test_apagar_remove_vetores_e_chave(memoria):
    rostos, cofre = _rostos(memoria, lambda bgr: np.ones(3, dtype=np.float32))
    rostos.cadastrar("Antônio", [_foto(i) for i in range(4)])
    assert rostos.cadastrados() == [("Antônio", 4)]
    assert rostos.apagar() == 4
    assert memoria.query("SELECT COUNT(*) AS n FROM faces")[0]["n"] == 0
    assert "ORION_FACE_KEY" not in cofre or cofre["ORION_FACE_KEY"] == ""


def test_limiar_de_similaridade_e_razoavel():
    assert 0.3 < LIMIAR < 0.7


# ── Regra 66: presença ──────────────────────────────────────────────────────


def _presenca(relogio, away=600.0):
    eventos = []
    p = Presenca(
        away_s=away,
        on_chegou=lambda: eventos.append("chegou"),
        on_saiu=lambda: eventos.append("saiu"),
        clock=lambda: relogio["t"],
    )
    return p, eventos


def test_chega_com_tela_desbloqueada_e_sai_sem_sinal():
    relogio = {"t": 0.0}
    p, eventos = _presenca(relogio, away=600)
    livre = Sinais(tela_desbloqueada=True, ocioso_s=5, bt_perto=None, rosto=None)
    assert p.atualizar(livre) == "chegou"
    relogio["t"] = 300
    assert p.atualizar(Sinais(None, None, None, None)) is None  # ainda dentro da janela
    relogio["t"] = 700
    assert p.atualizar(Sinais(None, None, None, None)) == "saiu"
    assert p.atualizar(Sinais(None, None, None, None)) is None  # não repete o aviso
    assert eventos == ["chegou", "saiu"]


def test_ocioso_longo_nao_conta_como_presenca():
    relogio = {"t": 0.0}
    p, _ = _presenca(relogio, away=600)
    assert p.atualizar(Sinais(True, 1200, None, None)) is None
    assert p.presente is False


def test_celular_perto_ou_rosto_bastam_para_estar_presente():
    relogio = {"t": 0.0}
    p, eventos = _presenca(relogio)
    assert p.atualizar(Sinais(None, None, True, None)) == "chegou"
    relogio["t"] = 700
    assert p.atualizar(Sinais(None, None, None, None)) == "saiu"
    relogio["t"] = 10_000
    assert p.atualizar(Sinais(None, None, None, True)) == "chegou"
    assert eventos == ["chegou", "saiu", "chegou"]


def test_vigia_pausa_a_tela_so_se_foi_a_presenca_e_nao_olha_sem_presenca():
    relogio = {"t": 0.0}
    p, _ = _presenca(relogio, away=600)
    tela = SimpleNamespace(pausada=False, chamadas=[])
    tela.pausar = lambda ativa=True: (tela.chamadas.append(ativa), setattr(tela, "pausada", ativa))
    vistos = []
    sinais = {"s": Sinais(True, 1, None, None)}
    v = Vigia(
        presenca=p, sinais=lambda: sinais["s"], vigiar=lambda: vistos.append(1) or 2, tela=tela
    )
    assert v.tick() == 2 and vistos == [1]  # presente: olha
    relogio["t"] = 2000
    sinais["s"] = Sinais(None, None, None, None)
    assert v.tick() == 0 and tela.chamadas == [True]  # saiu: pausa a memória da tela
    assert len(vistos) == 1  # e não olhou
    relogio["t"] = 2001
    sinais["s"] = Sinais(True, 1, None, None)
    v.tick()
    assert tela.chamadas == [True, False]  # voltou: retoma
    sem_presenca = Vigia(presenca=None, sinais=lambda: sinais["s"], vigiar=lambda: 99)
    assert sem_presenca.tick() == 0  # câmera nunca olha sem presença (regra 64)


def test_bloqueio_de_tela_so_roda_quando_chamado_e_respeita_o_sistema():
    chamadas = []

    def runner(argv, **kw):
        chamadas.append(argv)
        return subprocess.CompletedProcess(argv, 0, b"", b"")

    achou = {"loginctl": "/x"}
    assert bloquear_tela(platform="linux", which=achou.get, runner=runner) is True
    assert chamadas == [["loginctl", "lock-session"]]
    assert bloquear_tela(platform="freebsd", which=lambda n: None, runner=runner) is False


def test_ocioso_sem_xprintidle_devolve_none():
    assert ocioso_segundos(platform="win32", which=lambda n: None) is None
    assert ocioso_segundos(platform="linux", which=lambda n: None) is None


# ── E11.2: visão pelas assinaturas ──────────────────────────────────────────


def test_ordem_ignora_nomes_invalidos_e_repeticoes():
    assert ordem("claude, gemini, claude, lixo") == ["claude", "gemini"]
    assert ordem("") == ["gemini", "claude", "codex", "gemini_cli"]


def test_argv_de_cada_cli_recebe_a_imagem_por_arquivo(tmp_path):
    img = tmp_path / "imagem.jpg"
    assert argv_cli("codex", img, "o que é?")[:3] == ["exec", "-i", str(img)]
    assert "--allowedTools" in argv_cli("claude", img, "x")
    assert argv_cli("gemini_cli", img, "y")[1] == "@imagem.jpg y"


def test_cli_roda_numa_pasta_temporaria_e_registra_a_saida(tmp_path, monkeypatch):
    registros = []
    monkeypatch.setattr(saidas, "medir", _medidor(registros))
    vistos = {}

    def runner(argv, cwd, **kw):
        vistos["cwd"] = cwd
        vistos["argv"] = argv
        return subprocess.CompletedProcess(argv, 0, b"um mouse", b"")

    texto = descrever_por_cli(
        "codex", b"\xff\xd8x", "image/jpeg", "o que é?",
        runner=runner, which=lambda n: f"/bin/{n}",
    )  # fmt: skip
    assert texto == "um mouse"
    assert vistos["argv"][0] == "/bin/codex"
    assert not vistos["cwd"].startswith(str(tmp_path / "Orion"))  # pasta temporária
    assert registros == [("cli:codex", True)]


def test_cli_que_falha_ou_nao_existe_vira_erro_da_visao():
    with pytest.raises(VisionError, match="não instalada"):
        descrever_por_cli("claude", b"x", "image/jpeg", which=lambda n: None)

    def falha(argv, **kw):
        return subprocess.CompletedProcess(argv, 2, b"", b"")

    with pytest.raises(VisionError, match="código 2"):
        descrever_por_cli("claude", b"x", "image/jpeg", runner=falha, which=lambda n: "/c")


def _medidor(registros):
    from contextlib import contextmanager

    @contextmanager
    def medir(provider, kind, **kw):
        m = saidas.Medida()
        try:
            yield m
        finally:
            registros.append((provider, m.ok))

    return medir


def test_ordem_cai_para_a_cli_quando_o_gateway_falha(monkeypatch):
    class Gateway:
        def describe(self, imagem, mime, pergunta="", transport=None):
            raise VisionError("HTTP 429")

    chamadas = []

    def runner(argv, **kw):
        chamadas.append(argv[0])
        return subprocess.CompletedProcess(argv, 0, b"resposta da CLI", b"")

    texto = describe_ordenado(
        b"\xff\xd8x", "image/jpeg", "?", vision=Gateway(),
        ordem_=["gemini", "claude"], runner=runner, which=lambda n: f"/bin/{n}",
    )  # fmt: skip
    assert texto == "resposta da CLI" and chamadas == ["/bin/claude"]


def test_ordem_sem_nada_disponivel_junta_as_falhas():
    with pytest.raises(VisionError) as e:
        describe_ordenado(b"x", "image/jpeg", "?", vision=None, ordem_=["gemini"])
    assert "não configurada" in str(e.value)


def test_gateway_de_visao_sem_resposta_levanta_erro_de_visao():
    # regressão: a classe Vision segue igual para quem chama só com o gateway
    with pytest.raises(VisionError):
        Vision([SimpleNamespace(name="x", base_url="http://127.0.0.1:9", model="m", api_key="",
                               timeout_s=0.5)]).describe(b"\xff", "image/jpeg")  # fmt: skip


def test_saida_sem_batch_tambem_e_aceita():
    saida = _saida([((320, 320, 40, 40), 56, 0.8)])
    assert decodificar_saida(saida[0], 1.0, 0, 0)[0].label == "chair"
