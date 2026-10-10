"""Palavra de ativação (regra 38): o laço de escuta, com microfone e detector de mentira."""

import array
import io
import json
import math
import threading
import wave

import pytest

from orion.wake import (
    FRAME_BYTES,
    FRAME_S,
    OpenWakeWordDetector,
    SoundDeviceSource,
    VoskDetector,
    WakeConfig,
    WakeListener,
    argv_player,
    criar_detector,
    pcm_para_wav,
    rms,
)

SILENCIO = bytes(FRAME_BYTES)


def tom(amp: int) -> bytes:
    n = FRAME_BYTES // 2
    return array.array("h", (int(amp * math.sin(i / 3)) for i in range(n))).tobytes()


FALA, GATILHO = tom(6000), tom(1)  # `GATILHO` é só um quadro que o detector falso reconhece


class FonteFalsa:
    """Entrega os quadros dados e depois fecha (o laço termina)."""

    def __init__(self, quadros):
        self._q, self.drenou, self.fechou = list(quadros), 0, False

    def frames(self):
        yield from self._q

    def drain(self):
        self.drenou += 1

    def close(self):
        self.fechou = True


class DetectorFalso:
    """Dispara quando recebe exatamente `GATILHO`."""

    def __init__(self):
        self.vistos, self.resets = 0, 0

    def feed(self, frame):
        self.vistos += 1
        return frame is GATILHO

    def reset(self):
        self.resets += 1


def escuta(quadros, **kw):
    fala, eventos = [], []
    cfg = kw.pop("config", WakeConfig(resfriamento_s=0.0))
    det = kw.pop("detector", DetectorFalso())
    fonte = FonteFalsa(quadros)
    ouvinte = WakeListener(
        fonte, det, fala.append, config=cfg, evento=lambda t, m: eventos.append((t, m)), **kw
    )
    return ouvinte, fonte, det, fala, eventos


def segundos(s: float) -> int:
    return round(s / FRAME_S)


def test_sem_a_palavra_nada_e_gravado_nem_enviado():
    o, fonte, det, fala, eventos = escuta([FALA] * 50 + [SILENCIO] * 50)
    o.run()
    assert fala == [] and o.stats.ativacoes == 0
    assert det.vistos == 100 and fonte.fechou  # cada quadro só passou pelo detector
    assert [t for t, _ in eventos] == ["escuta", "escuta"]  # início e fim, sem ativação


def test_palavra_depois_fala_depois_silencio_entrega_um_wav_da_fala():
    quadros = (
        [SILENCIO] * 5 + [GATILHO] + [FALA] * 10 + [SILENCIO] * segundos(1.2) + [SILENCIO] * 20
    )
    o, fonte, _det, fala, eventos = escuta(quadros)
    o.run()
    assert o.stats.ativacoes == 1 and len(fala) == 1
    with wave.open(io.BytesIO(fala[0])) as w:
        assert (w.getnchannels(), w.getsampwidth(), w.getframerate()) == (1, 2, 16000)
        assert 10 * FRAME_S <= w.getnframes() / 16000 <= 12 * FRAME_S + 1.2
    assert ("ativacao", "palavra ouvida; a fala seguinte vai à transcrição") in eventos
    assert fonte.drenou == 1  # o que se acumulou enquanto o Orion falava é descartado


def test_palavra_sem_fala_depois_nao_chama_o_agente():
    o, *_, fala, _ev = escuta([GATILHO] + [SILENCIO] * segundos(5))
    o.run()
    assert fala == [] and o.stats.ativacoes == 1 and o.stats.falas_vazias == 1


def test_a_cauda_da_propria_palavra_nao_inicia_a_fala():
    cauda = [GATILHO] + [FALA] * 3  # o fim de "Orion" ainda soando (0,24 s)
    o, *_, fala, _ev = escuta(cauda + [SILENCIO] * segundos(5))
    o.run()
    assert fala == [] and o.stats.falas_vazias == 1
    comando = cauda + [SILENCIO] * 6 + [FALA] * 8 + [SILENCIO] * segundos(1.2)
    o2, *_, fala2, _ev2 = escuta(comando)
    o2.run()
    assert len(fala2) == 1


def test_estalo_curto_nao_vira_fala():
    cfg = WakeConfig(resfriamento_s=0.0, min_fala_s=0.3)
    o, *_, fala, _ev = escuta([GATILHO, FALA] + [SILENCIO] * segundos(5), config=cfg)
    o.run()
    assert fala == [] and o.stats.falas_vazias == 1


def test_fala_longa_e_cortada_no_teto():
    cfg = WakeConfig(resfriamento_s=0.0, max_s=2.0)
    o, *_, fala, _ev = escuta([GATILHO] + [FALA] * 200, config=cfg)
    o.run()
    with wave.open(io.BytesIO(fala[0])) as w:
        assert w.getnframes() / 16000 <= 2.2


def test_teto_por_hora_ignora_a_palavra_e_registra_a_recusa():
    cfg = WakeConfig(resfriamento_s=0.0, max_por_hora=2)
    quadros = ([GATILHO] + [FALA] * 10 + [SILENCIO] * segundos(1.2)) * 4
    o, _f, det, fala, eventos = escuta(quadros, config=cfg)
    o.run()
    assert len(fala) == 2 and o.stats.recusadas >= 1
    assert any(t == "recusada" for t, _ in eventos)
    assert det.resets >= 3


def test_pausada_o_detector_nem_recebe_o_quadro():
    o, _f, det, fala, eventos = escuta([GATILHO] * 5)
    o.pausar()
    o.run()
    assert det.vistos == 0 and fala == [] and o.stats.pausada
    assert ("escuta", "pausada") in eventos


def test_retomar_volta_a_ouvir():
    o, _f, det, _fala, _ev = escuta([SILENCIO] * 3)
    o.pausar()
    o.retomar()
    o.run()
    assert det.vistos == 3 and not o.stats.pausada


def test_erro_do_turno_nao_derruba_a_escuta():
    chamadas = []

    def quebra(wav):
        chamadas.append(wav)
        raise RuntimeError("segredo: chave-123")

    quadros = ([GATILHO] + [FALA] * 10 + [SILENCIO] * segundos(1.2)) * 2
    fonte, det = FonteFalsa(quadros), DetectorFalso()
    o = WakeListener(fonte, det, quebra, config=WakeConfig(resfriamento_s=0.0))
    o.run()
    assert len(chamadas) == 2  # a segunda ativação ainda foi atendida
    assert o.stats.ultimo_erro == "RuntimeError" and "chave" not in repr(o.stats)


def test_falha_do_microfone_vira_so_o_tipo_da_excecao():
    class Quebrada(FonteFalsa):
        def frames(self):
            raise OSError("PortAudio: device /dev/snd/secreto")
            yield b""

    o = WakeListener(Quebrada([]), DetectorFalso(), lambda w: None)
    o.run()
    assert o.stats.ultimo_erro == "OSError" and not o.stats.ouvindo


def test_bipe_toca_na_ativacao_e_falha_do_bipe_nao_atrapalha():
    toques = []

    def bipe():
        toques.append(1)
        raise RuntimeError("sem saída de som")

    o, *_, fala, _ev = escuta([GATILHO] + [FALA] * 10 + [SILENCIO] * segundos(1.2), bipe=bipe)
    o.run()
    assert toques == [1] and len(fala) == 1


def test_parar_encerra_o_laco():
    o, fonte, *_ = escuta([SILENCIO] * 10)
    o.parar()
    o.run()
    assert fonte.fechou and not o.stats.ouvindo


def test_ruido_de_fundo_alto_sobe_o_limiar_de_fala():
    cfg = WakeConfig(resfriamento_s=0.0, piso=300.0)
    o, *_ = escuta([], config=cfg)
    baixo = o._limiar()
    for _ in range(200):
        o._acompanhar_ruido(tom(400))
    assert o._limiar() > baixo


# ── peças ─────────────────────────────────────────────────────────────────


def test_rms_e_wav():
    assert rms(SILENCIO) == 0 and rms(tom(1000)) > 500 and rms(b"") == 0
    with wave.open(io.BytesIO(pcm_para_wav(tom(1000) * 2))) as w:
        assert w.getnframes() == FRAME_BYTES


def test_openwakeword_dispara_acima_do_limiar_e_reseta():
    class Modelo:
        def __init__(self, **kw):
            self.kw, self.score, self.resets = kw, 0.1, 0

        def predict(self, x):
            assert x.dtype.name == "int16" and len(x) == FRAME_BYTES // 2
            return {"orion": self.score}

        def reset(self):
            self.resets += 1

    d = OpenWakeWordDetector("orion.onnx", 0.6, fabrica=Modelo)
    assert d._m.kw == {"wakeword_models": ["orion.onnx"], "inference_framework": "onnx"}
    assert d.feed(SILENCIO) is False
    d._m.score = 0.7
    assert d.feed(SILENCIO) is True
    d.reset()
    assert d._m.resets == 1


def test_vosk_a_gramatica_tem_concorrentes_e_so_dispara_na_ultima_palavra():
    class Rec:
        def __init__(self, pasta, gramatica):
            self.gramatica, self.texto, self.final = json.loads(gramatica), "", False

        def AcceptWaveform(self, _):
            return self.final

        def Result(self):
            return json.dumps({"text": self.texto})

        def PartialResult(self):
            return json.dumps({"partial": self.texto})

        def Reset(self):
            self.texto = ""

    d = VoskDetector("modelo", ["Orion", "órion"], fabrica=Rec)
    g = d._rec.gramatica
    # só "orion" + [unk] faria qualquer fala virar "orion": precisa de concorrentes
    assert {"orion", "órion", "horas", "origem", "[unk]"} <= set(g) and len(g) > 100
    d._rec.texto = "que horas são"
    assert d.feed(SILENCIO) is False
    d._rec.texto = "ei órion"  # o modelo escreve com acento; o alvo ignora o acento
    assert d.feed(SILENCIO) is True
    d._rec.texto = "orion que horas"  # já passou da palavra: quem dispara é o parcial anterior
    assert d.feed(SILENCIO) is False
    d._rec.final, d._rec.texto = True, "ordem do dia orion"
    assert d.feed(SILENCIO) is True
    d.reset()
    assert d._rec.texto == ""


def test_criar_detector_recusa_modelo_ausente(tmp_path):
    with pytest.raises(ValueError, match=r"\.onnx"):
        criar_detector("openwakeword", tmp_path / "nao-existe.onnx", ["orion"], 0.5)
    with pytest.raises(ValueError, match="pasta"):
        criar_detector("vosk", None, ["orion"], 0.5)


def test_fonte_do_microfone_enfileira_e_perde_o_mais_antigo():
    class Sd:
        def RawInputStream(self, **kw):
            self.kw = kw
            return self

        def __enter__(self):
            return self

        def __exit__(self, *a):
            return False

    sd = Sd()
    f = SoundDeviceSource("3", sd=sd)
    gerador = f.frames()
    for i in range(450):
        f._callback(bytes([i % 256]) * 4, 0, None, None)
    assert f._fila.qsize() == 400
    primeiro = next(gerador)
    assert primeiro[0] == 50 % 256  # os 50 mais antigos foram descartados
    assert sd.kw["device"] == 3 and sd.kw["samplerate"] == 16000 and sd.kw["dtype"] == "int16"
    f.drain()
    assert f._fila.qsize() == 0
    f.close()
    gerador.close()


def test_player_por_sistema_sem_montar_script_com_o_caminho(tmp_path):
    arq = tmp_path / "fala; calc.mp3"
    mac = argv_player("darwin", arq)
    assert mac == (["afplay", str(arq)], {})
    argv, env = argv_player("win32", arq)
    assert env == {"ORION_AUDIO": str(arq)} and str(arq) not in " ".join(argv)
    assert argv_player("linux", arq, existe=lambda n: "/usr/bin/mpg123" if n == "mpg123" else None)[
        0
    ] == ["mpg123", "-q", str(arq)]
    assert argv_player("linux", arq, existe=lambda n: None) is None


def test_o_laco_roda_em_thread_e_para_quando_pedido():
    class Infinita(FonteFalsa):
        def frames(self):
            while True:
                yield SILENCIO

    o = WakeListener(Infinita([]), DetectorFalso(), lambda w: None)
    t = threading.Thread(target=o.run, daemon=True)
    t.start()
    o.parar()
    t.join(2)
    assert not t.is_alive()


# ── duas palmas (regra 51) ────────────────────────────────────────────────


class PalmasFalsas:
    """Dispara quando recebe exatamente `PALMA`."""

    def __init__(self):
        self.resets = 0

    def feed(self, frame):
        return frame is PALMA

    def reset(self):
        self.resets += 1


PALMA = tom(2)  # outro quadro-sinal, distinto do GATILHO


def com_palmas(quadros, on_palmas, *, so_palmas=True, **kw):
    fala, eventos = [], []
    fonte = FonteFalsa(quadros)
    ouvinte = WakeListener(
        fonte,
        None if so_palmas else DetectorFalso(),
        fala.append,
        config=WakeConfig(resfriamento_s=0.0),
        evento=lambda t, m: eventos.append((t, m)),
        palmas=PalmasFalsas(),
        on_palmas=on_palmas,
        **kw,
    )
    return ouvinte, fala, eventos


def test_palmas_abrem_sem_ouvir_e_so_com_palmas_ligadas():
    chamadas = []
    ouvinte, fala, eventos = com_palmas(
        [SILENCIO, PALMA, SILENCIO], lambda: chamadas.append(1) or False
    )
    ouvinte.run()
    assert chamadas == [1] and fala == []  # não gravou fala nenhuma
    assert ouvinte.stats.palmas == 1 and ouvinte.stats.ativacoes == 0
    assert ("palmas", "duas palmas; só abrem o Orion, não aprovam nada") in eventos


def test_palmas_com_abrir_e_ouvir_gravam_a_fala_seguinte():
    quadros = [PALMA] + [FALA] * 12 + [SILENCIO] * segundos(1.3)
    ouvinte, fala, eventos = com_palmas(quadros, lambda: True)
    ouvinte.run()
    assert len(fala) == 1 and fala[0][:4] == b"RIFF"
    assert ouvinte.stats.palmas == 1 and ouvinte.stats.ativacoes == 1
    assert any(t == "ativacao" and "palmas" in m for t, m in eventos)


def test_palmas_bloqueadas_pelo_panico_ou_nao_perturbe_nao_fazem_nada():
    ouvinte, _fala, eventos = com_palmas([PALMA, SILENCIO], lambda: None)
    ouvinte.run()
    assert ouvinte.stats.palmas == 0 and ouvinte.stats.palmas_recusadas == 1
    assert ("palmas_recusadas", "ação bloqueada (pânico ou não perturbe)") in eventos


def test_teto_de_palmas_por_hora():
    chamadas = []
    ouvinte, _, eventos = com_palmas(
        [PALMA, SILENCIO] * 5, lambda: chamadas.append(1) or False, max_palmas_por_hora=3
    )
    ouvinte.run()
    assert len(chamadas) == 3 and ouvinte.stats.palmas == 3 and ouvinte.stats.palmas_recusadas == 2
    assert ("palmas_recusadas", "teto de palmas por hora") in eventos


def test_palmas_pausadas_com_a_escuta_nao_chegam_ao_detector():
    chamadas = []
    ouvinte, _, _ = com_palmas([PALMA, SILENCIO], lambda: chamadas.append(1) or False)
    ouvinte.pausar()
    ouvinte.run()
    assert chamadas == []


def test_palavra_e_palmas_dividem_o_mesmo_microfone():
    chamadas = []
    quadros = [PALMA, GATILHO] + [FALA] * 12 + [SILENCIO] * segundos(1.3)
    ouvinte, fala, _ = com_palmas(quadros, lambda: chamadas.append(1) or False, so_palmas=False)
    ouvinte.run()
    assert chamadas == [1] and len(fala) == 1  # as palmas abriram e a palavra gravou a fala


def test_erro_na_acao_das_palmas_nao_derruba_a_escuta():
    def quebra():
        raise RuntimeError("sem navegador")

    ouvinte, _, _ = com_palmas([PALMA, SILENCIO, SILENCIO], quebra)
    ouvinte.run()
    assert ouvinte.stats.ultimo_erro == "RuntimeError" and ouvinte.stats.palmas == 0


# ── o que as palmas fazem ─────────────────────────────────────────────────


def test_acao_das_palmas_abre_pela_ponte_ou_pelo_navegador_e_respeita_bloqueios():
    from orion.palmas import acao_das_palmas

    log = []

    def monta(acao, ponte_ok=True, bloqueio=""):
        return acao_das_palmas(
            acao,
            abrir_ponte=lambda: log.append("ponte") or ponte_ok,
            abrir_navegador=lambda: log.append("navegador"),
            bloqueado=lambda: bloqueio,
        )

    assert monta("abrir")() is False and log == ["ponte"]
    log.clear()
    assert monta("abrir", ponte_ok=False)() is False and log == ["ponte", "navegador"]
    log.clear()
    assert monta("abrir_e_ouvir")() is True
    log.clear()
    assert monta("abrir", bloqueio="pânico")() is None and log == []  # nada é feito
    assert monta("rotina:bom-dia")() is False  # rotinas só chegam na E9.1: faz o mesmo que abrir
