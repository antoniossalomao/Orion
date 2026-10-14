"""Presença (E11.6, regra 66): chegou, saiu. Só pausa, retoma e cumprimenta.

Sinais locais, do mais barato ao mais caro: tela desbloqueada com teclado/mouse recente, celular
por Bluetooth perto, rosto (E11.5) com a câmera ligada. Ninguém sai do computador com isso. Sem
sinal por `away_s` segundos, a presença acaba: a ação de saída (bloquear a tela e pausar as
capturas) é de quem chama; este módulo só decide e avisa a transição.
"""

from __future__ import annotations

import asyncio
import shutil
import subprocess
import sys
import time
from collections.abc import Callable
from dataclasses import dataclass
from typing import Any

from .tools.desktop import env_limpo


@dataclass(frozen=True)
class Sinais:
    tela_desbloqueada: bool | None  # None: este sistema não consegue dizer
    ocioso_s: float | None  # segundos desde o último teclado/mouse
    bt_perto: bool | None  # celular visto por Bluetooth agora
    rosto: bool | None  # rosto cadastrado na câmera agora


class Presenca:
    def __init__(
        self,
        *,
        away_s: float,
        on_chegou: Callable[[], None],
        on_saiu: Callable[[], None],
        clock: Callable[[], float] = time.monotonic,
    ) -> None:
        self.away_s = away_s
        self._on_chegou, self._on_saiu, self._clock = on_chegou, on_saiu, clock
        self.presente = False
        self._ultimo_sinal: float | None = None

    def atualizar(self, s: Sinais) -> str | None:
        """Devolve "chegou", "saiu" ou None (nada mudou). Os callbacks rodam na transição."""
        agora = self._clock()
        if self._tem_sinal(s):
            self._ultimo_sinal = agora
        presente = self._ultimo_sinal is not None and agora - self._ultimo_sinal < self.away_s
        if presente and not self.presente:
            self.presente = True
            self._on_chegou()
            return "chegou"
        if not presente and self.presente:
            self.presente = False
            self._on_saiu()
            return "saiu"
        return None

    def _tem_sinal(self, s: Sinais) -> bool:
        ativo = s.tela_desbloqueada is True and (s.ocioso_s is None or s.ocioso_s < self.away_s)
        return ativo or s.bt_perto is True or s.rosto is True


def ocioso_segundos(
    *,
    platform: str = sys.platform,
    which: Callable[[str], str | None] = shutil.which,
    runner: Callable[..., Any] = subprocess.run,
) -> float | None:
    """Segundos sem teclado/mouse. Hoje só Linux X11 (`xprintidle`); None no resto."""
    if platform.startswith("linux") and which("xprintidle"):
        try:
            r = runner(
                ["xprintidle"], env=env_limpo(), stdin=subprocess.DEVNULL,
                capture_output=True, timeout=5, check=False,
            )  # fmt: skip
            return int(r.stdout.decode().strip()) / 1000.0 if r.returncode == 0 else None
        except (OSError, subprocess.TimeoutExpired, ValueError):
            return None
    return None


def bloquear_tela(
    *,
    platform: str = sys.platform,
    which: Callable[[str], str | None] = shutil.which,
    runner: Callable[..., Any] = subprocess.run,
) -> bool:
    """Bloqueia a sessão. True se o comando foi disparado; quem chama confirma antes (regra 66)."""
    if platform == "win32":
        argv = ["rundll32.exe", "user32.dll,LockWorkStation"]
    elif platform == "darwin":
        argv = ["pmset", "displaysleepnow"]  # exige senha ao acordar, nas configurações do macOS
    elif which("loginctl"):
        argv = ["loginctl", "lock-session"]
    else:
        return False
    try:
        r = runner(
            argv, env=env_limpo(), stdin=subprocess.DEVNULL, capture_output=True,
            timeout=10, check=False,
        )  # fmt: skip
    except (OSError, subprocess.TimeoutExpired):
        return False
    return r.returncode == 0


def bt_perto(endereco: str, *, timeout_s: float = 6.0) -> bool | None:
    """Celular visto por Bluetooth agora (bleak, opcional). None se não há como dizer."""
    if not endereco:
        return None
    try:
        from bleak import BleakScanner  # type: ignore[import-not-found]
    except ImportError:
        return None

    async def procurar() -> bool:
        achado = await BleakScanner.find_device_by_address(endereco, timeout=timeout_s)
        return achado is not None

    try:
        return bool(asyncio.run(procurar()))
    except Exception:  # noqa: BLE001 — adaptador ausente ou sem permissão: sinal indisponível
        return None


def sinais_do_sistema(
    *, bt_endereco: str = "", tela: Callable[[], bool | None], rosto: Callable[[], bool | None]
) -> Sinais:
    """Coleta os sinais agora. A tela e o rosto vêm de quem chama (testáveis e opcionais)."""
    bloqueada = tela()
    desbloqueada = None if bloqueada is None else not bloqueada
    return Sinais(
        tela_desbloqueada=desbloqueada,
        ocioso_s=ocioso_segundos(),
        bt_perto=bt_perto(bt_endereco),
        # regra 65: o rosto só é comparado com a tela desbloqueada
        rosto=rosto() if desbloqueada else None,
    )


__all__ = [
    "Presenca",
    "Sinais",
    "bloquear_tela",
    "bt_perto",
    "ocioso_segundos",
    "sinais_do_sistema",
]
