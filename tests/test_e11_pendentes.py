"""E11, o que faltava: fone (classes do modelo), cumprimento, bloqueio confirmado, rosto na
presença, vídeo ao vivo, legenda e a confirmação pela CLI. Tudo com falsos (sem câmera, sem
microfone, sem Vosk). Ver também test_e11_visao.py."""

import asyncio
import json
from types import SimpleNamespace

import numpy as np
import pytest

from orion.memory import MemoryStore
from orion.presenca import Sinais, sinais_do_sistema
from orion.tools.camera import descrever_camera
from orion.vigia import CHAVE_BLOQUEIO, acoes_de_presenca
from orion.visao_local import (
    Avistamentos,
    Deteccao,
    decodificar_saida,
    nome_em_ingles,
    nomes_do_modelo,
)


@pytest.fixture
def memoria(tmp_path):
    m = MemoryStore(tmp_path / "m.db")
    yield m
    m.close()


# ── fone: as classes vêm do modelo ─────────────────────────────────────────


class _SessaoMeta:
    def __init__(self, names):
        self._names = names

    def get_modelmeta(self):
        return SimpleNamespace(custom_metadata_map={"names": self._names} if self._names else {})


def test_nomes_vem_dos_metadados_do_modelo():
    sessao = _SessaoMeta("{0: 'headphones', 1: 'cup'}")
    assert nomes_do_modelo(sessao) == ["headphones", "cup"]


def test_modelo_sem_metadados_usa_o_padrao():
    assert nomes_do_modelo(_SessaoMeta(None)) is None
    assert nomes_do_modelo(_SessaoMeta("isto não é um dict")) is None


def test_saida_com_classes_do_modelo_rotula_fone():
    t = np.zeros((1, 84, 1), dtype=np.float32)
    t[0, 0, 0], t[0, 1, 0], t[0, 2, 0], t[0, 3, 0] = 100, 100, 40, 40
    t[0, 4, 0] = 0.9  # índice 0 = "headphones" neste modelo
    dets = decodificar_saida(t, 1.0, 0, 0, nomes=["headphones", "cup"])
    assert [d.label for d in dets] == ["headphones"]


def test_fone_em_portugues_vira_headphones():
    assert nome_em_ingles("fone") == "headphones"
    assert nome_em_ingles("fone de ouvido") == "headphones"


def test_onde_vi_fone_responde_com_hora(memoria):
    av = Avistamentos(memoria, clock=lambda: 2_000_000.0)
    av.registrar([Deteccao("headphones", 0.8, 5, 5, 9, 9)])
    vistos = av.onde_vi("fone")
    assert len(vistos) == 1 and vistos[0]["confianca"] == 0.8


# ── presença: cumprimento, bloqueio confirmado, rosto ─────────────────────


def _acoes(*, bloquear, confirmado, bloqueou=None):
    avisos, auditoria = [], []
    chegou, saiu = acoes_de_presenca(
        bloquear=bloquear,
        notificar=avisos.append,
        auditar=auditoria.append,
        confirmado=lambda: confirmado,
        hora=lambda: "09:30",
        bloqueador=(lambda: bloqueou.append(1) or True) if bloqueou is not None else (lambda: True),
    )
    return chegou, saiu, avisos, auditoria


def test_cumprimento_ao_chegar():
    chegou, _, avisos, _ = _acoes(bloquear=False, confirmado=False)
    chegou()
    assert avisos == ["Oi, Antônio. Você voltou às 09:30."]


def test_sair_sem_bloqueio_ligado_nao_faz_nada():
    bloqueou = []
    _, saiu, avisos, _ = _acoes(bloquear=False, confirmado=True, bloqueou=bloqueou)
    saiu()
    assert bloqueou == [] and avisos == []


def test_bloqueio_sem_confirmacao_avisa_uma_vez_e_nao_bloqueia():
    bloqueou = []
    _, saiu, avisos, _ = _acoes(bloquear=True, confirmado=False, bloqueou=bloqueou)
    saiu()
    saiu()
    assert bloqueou == []
    assert len(avisos) == 1 and "orion presenca confirmar" in avisos[0]


def test_bloqueio_confirmado_bloqueia_e_audita():
    bloqueou = []
    _, saiu, _, auditoria = _acoes(bloquear=True, confirmado=True, bloqueou=bloqueou)
    saiu()
    assert bloqueou == [1]
    assert auditoria == ["tela bloqueada ao sair (presença)"]


def test_rosto_so_e_consultado_com_tela_desbloqueada():
    chamadas = []

    def rosto():
        chamadas.append(1)
        return True

    bloqueada = sinais_do_sistema(tela=lambda: True, rosto=rosto)
    assert bloqueada.rosto is None and chamadas == []
    livre = sinais_do_sistema(tela=lambda: False, rosto=rosto)
    assert livre.rosto is True and chamadas == [1]
    desconhecida = sinais_do_sistema(tela=lambda: None, rosto=rosto)
    assert desconhecida.rosto is None  # sem como saber a tela, não compara


def test_rosto_sozinho_mantem_presenca():
    from orion.presenca import Presenca

    relogio = {"t": 0.0}
    p = Presenca(
        away_s=600, on_chegou=lambda: None, on_saiu=lambda: None, clock=lambda: relogio["t"]
    )
    assert p.atualizar(Sinais(None, None, None, True)) == "chegou"


def test_confirmacao_pela_cli_grava_e_desfaz(tmp_path):
    from orion.__main__ import _presenca
    from orion.config import Settings

    settings = Settings(data_dir=tmp_path, presence_lock=True)
    _presenca(settings, SimpleNamespace(acao="confirmar"))
    m = MemoryStore(settings.db_path)
    try:
        assert m.counter_get(CHAVE_BLOQUEIO) == 1
    finally:
        m.close()
    _presenca(settings, SimpleNamespace(acao="desfazer"))
    m = MemoryStore(settings.db_path)
    try:
        assert m.counter_get(CHAVE_BLOQUEIO) == 0
    finally:
        m.close()


# ── /camera: a mesma descrição para ferramenta, Telegram e paleta ─────────


def test_descrever_camera_usa_a_visao_e_apaga_a_foto():
    recebidas = []

    class Visao:
        def describe(self, imagem, mime, pergunta="", transport=None):
            recebidas.append((len(imagem), mime, pergunta))
            return "uma garrafa"

    texto = descrever_camera(0, Visao(), ["gemini"], "o que é?", foto=lambda d: b"\xff\xd8abc")
    assert texto == "uma garrafa"
    assert recebidas == [(5, "image/jpeg", "o que é?")]


# ── vídeo ao vivo: quadros da câmera na sessão, indicador para o navegador ─


class _WS:
    def __init__(self, mensagens, atraso_final=0.0):
        self._fila = [json.dumps({"cmd": c}) for c in mensagens]
        self.enviados = []
        self._atraso = atraso_final

    async def receive(self):
        if self._fila:
            await asyncio.sleep(0.1)  # dá tempo de a câmera mandar quadros antes do próximo comando
            return {"type": "websocket.receive", "text": self._fila.pop(0)}
        await asyncio.sleep(self._atraso)
        return {"type": "websocket.disconnect"}

    async def send_text(self, texto):
        self.enviados.append(json.loads(texto))

    async def send_bytes(self, dados):
        pass


class _Sessao:
    def __init__(self):
        self.quadros = []

    async def send_realtime_input(self, **kw):
        if "video" in kw:
            self.quadros.append(kw["video"].data)

    async def receive(self):
        await asyncio.Event().wait()  # o Live fica aberto sem resposta até a subida parar
        yield None


class _Conexao:
    def __init__(self, sessao):
        self._s = sessao

    async def __aenter__(self):
        return self._s

    async def __aexit__(self, *a):
        return False


def test_camera_ligada_envia_quadros_e_avisa_o_navegador(monkeypatch):
    import orion.voice as voice

    monkeypatch.setattr(voice, "INTERVALO_QUADRO_S", 0.01)
    ws = _WS(["camera_on", "camera_off", "stop"], atraso_final=0.2)
    sessao = _Sessao()
    fotos = []

    def camera():
        fotos.append(1)
        return b"\xff\xd8quadro"

    motivo = asyncio.run(voice.ponte_ao_vivo(ws, _Conexao(sessao), max_s=5, camera=camera))
    assert motivo in {"navegador", "tempo", "erro"}
    assert sessao.quadros and sessao.quadros[0] == b"\xff\xd8quadro"
    indicadores = [m for m in ws.enviados if m.get("type") == "camera"]
    assert indicadores[0] == {"type": "camera", "on": True}
    assert indicadores[-1] == {"type": "camera", "on": False}


def test_camera_pedida_sem_ORION_CAMERA_avisa_erro():
    import orion.voice as voice

    ws = _WS(["camera_on", "stop"], atraso_final=0.05)
    asyncio.run(voice.ponte_ao_vivo(ws, _Conexao(_Sessao()), max_s=5, camera=None))
    assert any(m.get("type") == "error" and "ORION_CAMERA" in m["msg"] for m in ws.enviados)


# ── legenda: Vosk falso, fluxo falso ───────────────────────────────────────


class _Rec:
    def __init__(self, falas):
        self._falas = list(falas)
        self._ultimo = ""

    def AcceptWaveform(self, pcm):
        if not self._falas:
            return False
        self._ultimo = self._falas.pop(0)
        return True

    def Result(self):
        return json.dumps({"text": self._ultimo})


def test_legenda_guarda_so_as_ultimas_falas_em_memoria():
    from orion.legenda import MAX_LINHAS, Legenda

    leg = Legenda(modelo=None, reconhecedor=_Rec([f"fala {i}" for i in range(MAX_LINHAS + 5)]))
    for _ in range(MAX_LINHAS + 5):
        leg.alimentar(b"\x00\x00")
    linhas = leg.ultimas()
    assert len(linhas) == MAX_LINHAS and linhas[-1] == f"fala {MAX_LINHAS + 4}"


def test_legenda_rodar_le_o_fluxo_e_para_quando_pedido():
    from orion.legenda import Legenda

    class Fluxo:
        def __init__(self, leg):
            self.leg, self.leituras = leg, 0

        def __enter__(self):
            return self

        def __exit__(self, *a):
            return False

        def read(self, n):
            self.leituras += 1
            if self.leituras >= 3:
                self.leg.parar()
            return b"\x01\x00" * n, None

    rec = _Rec(["oi mundo", "tudo bem", ""])
    leg = Legenda(modelo=None, reconhecedor=rec)
    leg._fonte = lambda: Fluxo(leg)
    leg.rodar()
    assert leg.ultimas() == ["oi mundo", "tudo bem"]


def test_legenda_sem_modelo_erro_claro(tmp_path):
    from orion.legenda import Legenda, LegendaError

    with pytest.raises(LegendaError, match="ORION_SUBTITLE_MODEL"):
        Legenda(tmp_path / "nao-existe").alimentar(b"\x00\x00")
