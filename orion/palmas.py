"""Duas palmas abrem o Orion (E2.2, regra 51): um detector de palmas no mesmo microfone da palavra
de ativação.

Roda sobre os mesmos quadros de 80 ms (16 kHz, 16 bits, mono) que `orion.wake` já lê; o áudio só
existe no quadro atual: nada é gravado, guardado nem enviado. Uma palma é um **pico curto**:

1. o pico do quadro passa de `razao` vezes o ruído de fundo (e de um piso absoluto, para a
   digitação e o teclado não contarem);
2. o quadro **seguinte** cai para menos de 40% da energia dele. Porta batendo, móvel arrastado e
   música alta têm cauda: o quadro seguinte continua alto, e não vale.

Duas palmas com 150 a 700 ms entre elas disparam, depois de uma janela de 700 ms sem terceira. Uma
terceira palma dentro dessa janela é aplauso ou batida: cancela e o detector fica surdo por
1,5 s. Contagem de tempo por quadros (determinística): os testes usam sinais sintéticos.
"""

from __future__ import annotations

import time
from collections import deque
from collections.abc import Callable

from .wake import FRAME_S, rms

PICO_MINIMO = 4000.0  # em amostras de 16 bits (máx. 32768): palma de verdade passa fácil
QUEDA = 0.4  # o quadro seguinte tem de ter menos que isso da energia do candidato
AQUECIMENTO = 25  # quadros (2 s) em que o ruído de fundo aprende depressa
ENTRE_MIN_S = 0.15
ENTRE_MAX_S = 0.70
SURDO_S = 1.5  # depois de aplauso/batida
RUIDO_INICIAL = 150.0


def pico(frame: bytes) -> float:
    import sys
    from array import array

    amostras = array("h")
    amostras.frombytes(frame[: len(frame) // 2 * 2])
    if sys.byteorder == "big":
        amostras.byteswap()
    return float(max((abs(a) for a in amostras), default=0))


class ClapDetector:
    """`feed(frame)` devolve True quando duas palmas acabaram de ser confirmadas."""

    def __init__(
        self,
        razao: float = 6.0,
        *,
        relogio: Callable[[], float] = time.time,
        pico_minimo: float = PICO_MINIMO,
    ) -> None:
        self._razao = razao
        self._piso = pico_minimo
        self._relogio = relogio
        self._ruido = RUIDO_INICIAL
        self.reset()
        # (epoch, pico/ruído) de cada palma confirmada: o painel mostra os da última hora
        self.picos: deque[tuple[float, float]] = deque(maxlen=200)

    def reset(self) -> None:
        self._n = 0  # quadros vistos
        self._candidato: tuple[float, int] | None = None  # (energia, índice do quadro)
        self._palmas: list[int] = []  # quadros das palmas confirmadas da sequência atual
        self._surdo_ate = -1

    def ultimos_picos(self, janela_s: float = 3600.0) -> list[tuple[float, float]]:
        corte = self._relogio() - janela_s
        return [p for p in self.picos if p[0] >= corte]

    # ── detecção ──────────────────────────────────────────────────────────
    def feed(self, frame: bytes) -> bool:
        n, self._n = self._n, self._n + 1
        energia, topo = rms(frame), pico(frame)
        if self._candidato is not None:
            e0, i0 = self._candidato
            self._candidato = None
            # voltou ao ruído de fundo (ou caiu a menos de 40% do pico): foi pico curto, não cauda
            if energia < QUEDA * e0 or energia < 1.3 * self._ruido:
                self._confirmar(i0, e0)
        disparou = False
        if len(self._palmas) == 2 and (n - self._palmas[1]) * FRAME_S > ENTRE_MAX_S:
            self._palmas.clear()
            disparou = True
        elif len(self._palmas) == 1 and (n - self._palmas[0]) * FRAME_S > ENTRE_MAX_S:
            self._palmas.clear()
        limiar = max(self._piso, self._razao * self._ruido)
        if n >= self._surdo_ate and topo >= limiar:
            self._candidato = (energia, n)
        else:
            if n < AQUECIMENTO:
                peso = 0.2
            else:
                peso = 0.05 if energia < self._ruido * 2 + 1 else 0.002
            self._ruido += peso * (energia - self._ruido)
        return disparou

    def _confirmar(self, i: int, energia: float) -> None:
        if i < self._surdo_ate:
            return
        if self._palmas and (i - self._palmas[-1]) * FRAME_S < ENTRE_MIN_S:
            return  # eco/ressalto da mesma palma
        self._palmas.append(i)
        self.picos.append((self._relogio(), round(energia / max(self._ruido, 1.0), 1)))
        if len(self._palmas) == 3:
            self._palmas.clear()
            self._surdo_ate = i + int(SURDO_S / FRAME_S)


# ── o que as palmas fazem ─────────────────────────────────────────────────

ACOES = ("abrir", "abrir_e_ouvir")


def acao_das_palmas(
    acao: str,
    *,
    abrir_ponte: Callable[[], bool],
    abrir_navegador: Callable[[], object],
    bloqueado: Callable[[], str],
) -> Callable[[], bool | None]:
    """A função que a escuta chama quando acontecem duas palmas. Devolve True se a escuta deve
    começar um turno de voz agora (`abrir_e_ouvir`), False se só abriu, None se estava bloqueada.

    Regra 51: palma só **abre**. Nunca aprova nem executa ferramenta; o único efeito é abrir a
    interface (pela ponte, se houver, senão no navegador) e, se você pediu, ouvir uma fala.
    `bloqueado()` devolve o motivo ("pânico", "não perturbe") ou "" — nesses casos nada acontece.
    `rotina:<nome>` só existe a partir da E9.1: até lá faz o mesmo que `abrir`.
    """
    ouvir = acao == "abrir_e_ouvir"

    def ao_bater() -> bool | None:
        if bloqueado():
            return None
        if not abrir_ponte():
            abrir_navegador()
        return ouvir

    return ao_bater
