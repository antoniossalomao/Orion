"""Vigia do E11: liga a presença (REGRAS.md: presença só pausa e retoma, cumpre e bloqueia só com
sua confirmação) e a vigília da câmera (REGRAS.md: a câmera só liga por pedido seu, e cada uso fica
registrado) ao ciclo dos jobs.

A cada rodada: lê os sinais locais, atualiza a presença e, só se houver presença, faz um quadro da
câmera e guarda os rótulos. Ao sair, pausa a memória da tela (se foi esta presença que a pausou);
ao chegar, retoma. Ao sair, bloqueia a tela só se `ORION_PRESENCE_LOCK` estiver ligado e você tiver
confirmado a configuração (REGRAS.md: presença só pausa e retoma, cumpre e bloqueia só com sua
confirmação).
"""

from __future__ import annotations

from collections.abc import Callable
from typing import Any

from .presenca import Presenca, Sinais, bloquear_tela, sinais_do_sistema
from .screen_memory import tela_bloqueada

CHAVE_BLOQUEIO = (
    "presence_lock_confirmado"  # contador no banco: 1 depois de `orion presenca confirmar`
)


def acoes_de_presenca(
    *,
    bloquear: bool,
    notificar: Callable[[str], None],
    auditar: Callable[[str], None],
    confirmado: Callable[[], bool],
    hora: Callable[[], str],
    bloqueador: Callable[[], bool] = bloquear_tela,
) -> tuple[Callable[[], None], Callable[[], None]]:
    """Ações da transição (REGRAS.md: presença só pausa e retoma, cumpre e bloqueia só com sua
    confirmação): cumprimento ao chegar; ao sair, bloqueio só se você
    confirmou a configuração. Sem confirmação, avisa uma vez e não bloqueia."""
    avisou = {"sem_confirmacao": False}

    def chegou() -> None:
        notificar(f"Oi, Antônio. Você voltou às {hora()}.")

    def saiu() -> None:
        if not bloquear:
            return
        if not confirmado():
            if not avisou["sem_confirmacao"]:
                avisou["sem_confirmacao"] = True
                notificar(
                    "Não bloqueei a tela ao sair: rode `orion presenca confirmar` para ligar."
                )
            return
        if bloqueador():
            auditar("tela bloqueada ao sair (presença)")
            avisou["sem_confirmacao"] = False

    return chegou, saiu


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


def sinais_padrao(
    *, bt_endereco: str = "", rosto: Callable[[], bool | None] | None = None
) -> Callable[[], Sinais]:
    """Sinais do sistema: tela (REGRAS.md: memória da tela: só texto, local e com opt-in, sem como
    saber = bloqueada), Bluetooth e rosto."""

    def coletar() -> Sinais:
        return sinais_do_sistema(
            bt_endereco=bt_endereco,
            tela=tela_bloqueada,
            rosto=rosto or (lambda: None),
        )

    return coletar
