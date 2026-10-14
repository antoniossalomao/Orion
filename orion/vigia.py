"""Vigia do E11: liga a presença (regra 66) e a vigília da câmera (regra 64) ao ciclo dos jobs.

A cada rodada: lê os sinais locais, atualiza a presença e, só se houver presença, faz um quadro da
câmera e guarda os rótulos. Ao sair, pausa a memória da tela (se foi esta presença que a pausou);
ao chegar, retoma. O bloqueio da tela não tem gatilho automático: é `bloquear_tela`, sob pedido.
"""

from __future__ import annotations

from collections.abc import Callable
from typing import Any

from .presenca import Presenca, Sinais, sinais_do_sistema
from .screen_memory import tela_bloqueada


class Vigia:
    def __init__(
        self,
        *,
        presenca: Presenca | None,
        sinais: Callable[[], Sinais],
        vigiar: Callable[[], int] | None = None,
        tela: Any = None,
    ) -> None:
        self.presenca = presenca
        self._sinais = sinais
        self._vigiar = vigiar
        self._tela = tela
        self._pausou = False  # a pausa da memória da tela foi desta presença?

    def tick(self) -> int:
        """Devolve quantos rótulos novos foram guardados nesta rodada. Sem presença, não olha."""
        if self.presenca is None:
            return 0
        self.presenca.atualizar(self._sinais())
        if not self.presenca.presente:
            self._pausar_tela()
            return 0
        self._retomar_tela()
        return self._vigiar() if self._vigiar is not None else 0

    def _pausar_tela(self) -> None:
        if self._tela is not None and not getattr(self._tela, "pausada", True):
            self._tela.pausar(True)
            self._pausou = True

    def _retomar_tela(self) -> None:
        if self._tela is not None and self._pausou:
            self._tela.pausar(False)
            self._pausou = False


def sinais_padrao(*, bt_endereco: str = "") -> Callable[[], Sinais]:
    """Sinais do sistema: tela (regra 44, sem como saber = bloqueada) e Bluetooth; rosto fora."""

    def coletar() -> Sinais:
        return sinais_do_sistema(
            bt_endereco=bt_endereco,
            tela=tela_bloqueada,
            rosto=lambda: None,
        )

    return coletar
