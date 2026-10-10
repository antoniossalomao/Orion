"""E2.2 (regra 51): detector de palmas com sinais sintéticos (quadros de 80 ms, 16 kHz)."""

import math
import random
from array import array

from orion.palmas import ClapDetector
from orion.wake import FRAME_S, FRAME_SAMPLES


def _quadro(amostras) -> bytes:
    a = array("h", [max(-32768, min(32767, int(x))) for x in amostras])
    return a.tobytes()


def silencio(nivel=100, semente=1):
    r = random.Random(semente)
    return _quadro(r.uniform(-nivel, nivel) for _ in range(FRAME_SAMPLES))


def palma(amp=20000, semente=2):
    """Pico curto: cai a menos de 1/e em 80 amostras (5 ms); o resto do quadro é ruído baixo."""
    r = random.Random(semente)
    return _quadro(
        amp * math.exp(-n / 80) * math.sin(n * 0.9) + r.uniform(-100, 100)
        for n in range(FRAME_SAMPLES)
    )


def porta(amp=15000):
    """Batida com cauda: energia alta por vários quadros (a palma não tem isso)."""
    return _quadro(amp * math.exp(-n / 3000) * math.sin(n * 0.3) for n in range(FRAME_SAMPLES))


def tecla(amp=2500, semente=3):
    r = random.Random(semente)
    return _quadro(
        amp * math.exp(-n / 40) * math.sin(n * 1.1) + r.uniform(-100, 100)
        for n in range(FRAME_SAMPLES)
    )


def roda(eventos: dict[int, bytes], total: int, **kw) -> list[int]:
    """Alimenta `total` quadros (silêncio onde não há evento) e devolve em quais o detector disparou."""
    d = ClapDetector(**kw)
    saida = []
    for i in range(total):
        if d.feed(eventos.get(i, silencio(semente=i + 10))):
            saida.append(i)
    return saida


def test_duas_palmas_disparam_uma_vez_depois_da_janela_sem_terceira():
    disparos = roda({10: palma(), 15: palma()}, 40)
    assert len(disparos) == 1
    # a janela de 700 ms (≈ 9 quadros) vem depois da segunda palma, não antes
    assert 15 + 8 <= disparos[0] <= 15 + 11
    assert (disparos[0] - 15) * FRAME_S >= 0.7


def test_uma_palma_sozinha_nao_dispara():
    assert roda({10: palma()}, 40) == []


def test_ressalto_da_mesma_palma_nao_conta_como_segunda():
    # dois quadros seguidos (80 ms) < 150 ms: é a mesma palma com eco
    assert roda({10: palma(), 11: palma(semente=5)}, 40) == []


def test_palmas_muito_separadas_nao_formam_par():
    assert roda({10: palma(), 10 + 13: palma()}, 60) == []  # ≈ 1 s entre elas


def test_tres_palmas_seguidas_sao_aplauso_e_deixam_o_detector_surdo():
    ev = {10: palma(), 14: palma(), 18: palma()}
    assert roda(ev, 50) == []
    # logo depois do aplauso (menos de 1,5 s), mais um par também é ignorado...
    ev2 = {**ev, 24: palma(), 29: palma()}
    assert roda(ev2, 60) == []
    # ...e passado o tempo, um par novo volta a funcionar
    ev3 = {**ev, 60: palma(), 65: palma()}
    assert len(roda(ev3, 90)) == 1


def test_porta_batendo_tem_cauda_e_nao_conta():
    assert roda({10: porta(), 11: porta(), 12: porta(), 20: porta(), 21: porta()}, 40) == []


def test_digitacao_fica_abaixo_do_piso_absoluto():
    ev = {i: tecla(semente=i) for i in range(5, 30, 3)}
    assert roda(ev, 40) == []


def test_ruido_de_fundo_alto_exige_palma_proporcionalmente_mais_forte():
    # ambiente com ruído ~1500 de energia: o limiar sobe para 6 × ruído
    barulho = {i: silencio(nivel=2600, semente=i) for i in range(0, 80)}
    fraca = {**barulho, 40: palma(amp=7000), 45: palma(amp=7000)}
    forte = {**barulho, 40: palma(amp=30000), 45: palma(amp=30000)}
    assert roda(fraca, 80) == []
    assert len(roda(forte, 80)) == 1


def test_razao_configuravel():
    # amplitude 5000 passa do piso, mas com razão 150 (limiar ≈ 150 × ruído ≈ 7000) não passa
    ev = {10: palma(amp=5000), 15: palma(amp=5000)}
    assert len(roda(ev, 40)) == 1
    assert roda(ev, 40, razao=150.0) == []


def test_picos_confirmados_ficam_registrados_para_calibrar():
    t = [1000.0]
    d = ClapDetector(relogio=lambda: t[0])
    for i in range(40):
        t[0] += FRAME_S
        d.feed(palma() if i in (10, 15) else silencio(semente=i + 10))
    assert len(d.picos) == 2
    assert all(razao > 1 for _, razao in d.picos)
    assert len(d.ultimos_picos(3600)) == 2
    t[0] += 4000
    assert d.ultimos_picos(3600) == []


def test_reset_esquece_a_sequencia():
    d = ClapDetector()
    for i in range(12):
        d.feed(palma() if i == 10 else silencio(semente=i))
    d.reset()
    saida = [d.feed(palma() if i == 5 else silencio(semente=i)) for i in range(30)]
    assert not any(saida)
