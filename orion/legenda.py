"""Legenda ao vivo do áudio do sistema (E11.7), com Vosk local.

Opt-in e pesado (`ORION_SUBTITLES`, modelo Vosk em português em `ORION_SUBTITLE_MODEL`, que você
coloca). O áudio do sistema (loopback do dispositivo `ORION_SUBTITLE_DEVICE`) é lido em blocos e
passa direto pelo reconhecedor: nada é gravado e nada sai do computador (regra 64). Só o texto
das últimas falas fica em memória, e a página `/ui/legenda.html` lê esse texto.
"""

from __future__ import annotations

import json
import threading
from collections import deque
from collections.abc import Callable
from pathlib import Path
from typing import Any

RATE = 16000  # o Vosk espera PCM 16 kHz mono
BLOCO = 4000  # amostras por leitura (0,25 s)
MAX_LINHAS = 20  # só as falas recentes ficam em memória


class LegendaError(RuntimeError):
    """Falha que o dono pode ver. Nunca leva caminho de áudio nem conteúdo."""


class Legenda:
    def __init__(
        self,
        modelo: Path,
        *,
        dispositivo: str = "",
        reconhecedor: Any = None,
        fonte: Callable[[], Any] | None = None,
    ) -> None:
        self.modelo = modelo
        self.dispositivo = dispositivo
        self._reconhecedor = reconhecedor
        self._fonte = fonte  # abre um fluxo de áudio (testes injetam um falso)
        self._linhas: deque[str] = deque(maxlen=MAX_LINHAS)
        self._parar = threading.Event()
        self._trava = threading.Lock()

    def _rec(self) -> Any:
        if self._reconhecedor is None:
            if not self.modelo.is_dir():
                raise LegendaError("modelo Vosk não encontrado (ORION_SUBTITLE_MODEL)")
            try:
                from vosk import (  # type: ignore[import-not-found]
                    KaldiRecognizer,
                    Model,
                    SetLogLevel,
                )
            except ImportError:
                raise LegendaError("instale o extra 'wake-vosk' (vosk)") from None
            SetLogLevel(-1)
            self._reconhecedor = KaldiRecognizer(Model(str(self.modelo)), RATE)
        return self._reconhecedor

    def alimentar(self, pcm: bytes) -> str | None:
        """Passa um bloco de PCM 16 bits. Devolve a fala quando ela termina (ou None)."""
        rec = self._rec()
        if not rec.AcceptWaveform(pcm):
            return None
        texto = str(json.loads(rec.Result()).get("text", "")).strip()
        if texto:
            with self._trava:
                self._linhas.append(texto[:300])
        return texto or None

    def ultimas(self) -> list[str]:
        with self._trava:
            return list(self._linhas)

    def rodar(self) -> None:
        """Laço de leitura (bloqueante; roda numa thread daemon). Para com `parar()`."""
        fluxo = self._abrir_fluxo()
        with fluxo:
            while not self._parar.is_set():
                dados, _ = fluxo.read(BLOCO)
                self.alimentar(bytes(dados))

    def parar(self) -> None:
        self._parar.set()

    def _abrir_fluxo(self) -> Any:
        if self._fonte is not None:
            return self._fonte()
        try:
            import sounddevice  # type: ignore[import-not-found]
        except ImportError:
            raise LegendaError("instale o extra 'wake' (sounddevice)") from None
        dispositivo: Any = self.dispositivo or None
        if isinstance(dispositivo, str) and dispositivo.isdigit():
            dispositivo = int(dispositivo)
        return sounddevice.RawInputStream(
            samplerate=RATE, channels=1, dtype="int16", blocksize=BLOCO, device=dispositivo
        )


def iniciar_thread(legenda: Legenda) -> threading.Thread:
    """Roda a legenda em segundo plano; erro vai para o log, sem derrubar o servidor."""
    import logging

    log = logging.getLogger("orion.legenda")

    def alvo() -> None:
        try:
            legenda.rodar()
        except (LegendaError, OSError) as e:
            log.warning("legenda parada: %s", e)

    t = threading.Thread(target=alvo, name="orion-legenda", daemon=True)
    t.start()
    return t
