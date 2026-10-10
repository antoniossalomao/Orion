"""Palavra de ativação "Orion": um agente que escuta o microfone o tempo todo (regra 38).

**Como funciona.** Um laço numa thread lê o microfone em quadros de 80 ms (16 kHz, 16 bits, mono) e
passa cada quadro a um *detector* que roda **no próprio computador**. Enquanto a palavra não é
ouvida, o áudio só existe no quadro atual: nada é gravado em disco, guardado nem enviado. Ouviu
"Orion" → um bipe → grava a fala seguinte (até o silêncio) → entrega à mesma via do botão de
microfone (`turno_de_voz`: Whisper no Groq → turno do agente → edge-tts) e toca a resposta.

**Dois detectores** (nenhum validado com áudio real, ver ORION_MELHORIAS):
- `openwakeword`: um modelo `.onnx` treinado na palavra "orion". O projeto openWakeWord treina o
  modelo sozinho, com voz sintética (Colab oficial): **não precisa gravar amostras**.
- `vosk`: reconhecimento offline restrito à palavra (gramática `["orion", "[unk]"]`), sem treino,
  mas a palavra precisa existir no vocabulário do modelo baixado.

**O que a palavra de ativação NÃO faz** (regra 38): não aprova nada (aprovar é só por botão,
regra 2: a voz avisa e o cartão aparece), não abre ferramenta que a política não abriria e não
liga sozinha (`ORION_WAKE_ENABLED`). Tem teto de ativações por hora (uma TV que diga "Orion" não
vira rajada), pausa pela API/painel e cada ativação vai para o audit sem o áudio.
"""

from __future__ import annotations

import contextlib
import io
import json
import logging
import math
import os
import queue
import shutil
import subprocess
import sys
import tempfile
import threading
import time
import unicodedata
import wave
from array import array
from collections import deque
from collections.abc import Callable, Iterator
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Protocol

log = logging.getLogger("orion.wake")

SAMPLE_RATE = 16000
FRAME_SAMPLES = 1280  # 80 ms: o tamanho que o openWakeWord espera
FRAME_BYTES = FRAME_SAMPLES * 2
FRAME_S = FRAME_SAMPLES / SAMPLE_RATE
MAX_FALA_BYTES = 20 * 1024 * 1024  # o mesmo teto de `transcribe.MAX_AUDIO`


class Detector(Protocol):
    def feed(self, frame: bytes) -> bool:
        """True quando a palavra de ativação acabou de ser dita."""
        ...

    def reset(self) -> None: ...


class AudioSource(Protocol):
    def frames(self) -> Iterator[bytes]:
        """Quadros de `FRAME_BYTES` (PCM 16 bits, 16 kHz, mono), sem fim até `close`."""
        ...

    def drain(self) -> None:
        """Joga fora o que se acumulou enquanto o Orion falava."""
        ...

    def close(self) -> None: ...


def rms(frame: bytes) -> float:
    amostras = array("h")
    amostras.frombytes(frame[: len(frame) // 2 * 2])
    if sys.byteorder == "big":
        amostras.byteswap()
    if not amostras:
        return 0.0
    return math.sqrt(sum(a * a for a in amostras) / len(amostras))


def pcm_para_wav(pcm: bytes) -> bytes:
    """A fala gravada como WAV, que `tipo_do_audio` e o Whisper reconhecem."""
    saida = io.BytesIO()
    with wave.open(saida, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(SAMPLE_RATE)
        w.writeframes(pcm)
    return saida.getvalue()


@dataclass(frozen=True)
class WakeConfig:
    espera_s: float = 4.0  # depois da palavra, quanto esperar o começo da fala
    silencio_s: float = 1.0  # silêncio que encerra a fala
    max_s: float = 15.0  # teto de uma fala
    min_fala_s: float = 0.25  # menos que isso é ruído, não fala
    cauda_s: float = 0.3  # o resto da própria palavra "Orion" não conta como início de fala
    piso: float = 350.0  # energia mínima (RMS) para contar como fala
    fator_ruido: float = 3.0  # fala = fator × o ruído de fundo medido
    resfriamento_s: float = 2.0  # depois de um turno, ignora a palavra por este tempo
    max_por_hora: int = 30  # ativações por hora; acima disso a palavra é ignorada


@dataclass
class WakeStats:
    ouvindo: bool = False
    pausada: bool = False
    ativacoes: int = 0
    falas_vazias: int = 0  # ouviu a palavra e mais nada
    recusadas: int = 0  # palavra ouvida acima do teto por hora
    ultima: float | None = None  # epoch da última ativação
    ultimo_erro: str | None = None  # só o tipo da exceção


class WakeListener:
    """O laço de escuta. Síncrono e sem dependência de áudio real: a fonte e o detector entram por
    construtor, o que permite provar o comportamento com falsos (`tests/test_wake.py`)."""

    def __init__(
        self,
        source: AudioSource,
        detector: Detector,
        on_fala: Callable[[bytes], None],
        *,
        config: WakeConfig | None = None,
        bipe: Callable[[], None] | None = None,
        evento: Callable[[str, str], None] | None = None,
        relogio: Callable[[], float] = time.time,
    ) -> None:
        self._source, self._detector, self._on_fala = source, detector, on_fala
        self._cfg = config or WakeConfig()
        self._bipe, self._evento, self._relogio = bipe, evento, relogio
        self._pausa = threading.Event()
        self._parar = threading.Event()
        self._ativadas: deque[float] = deque()
        self._ruido = self._cfg.piso / self._cfg.fator_ruido
        self.stats = WakeStats()

    # ── controle ──────────────────────────────────────────────────────────
    def pausar(self) -> None:
        self._pausa.set()
        self.stats.pausada = True
        self._registrar("escuta", "pausada")

    def retomar(self) -> None:
        self._pausa.clear()
        self.stats.pausada = False
        self._registrar("escuta", "retomada")

    def parar(self) -> None:
        self._parar.set()

    def _registrar(self, tipo: str, motivo: str) -> None:
        if self._evento is None:
            return
        try:
            self._evento(tipo, motivo)
        except Exception:
            log.exception("registro do evento de voz falhou")

    # ── laço ──────────────────────────────────────────────────────────────
    def run(self) -> None:
        self.stats.ouvindo = True
        self._registrar("escuta", "início: microfone aberto; detecção só neste computador")
        try:
            quadros = iter(self._source.frames())
            for quadro in quadros:
                if self._parar.is_set():
                    break
                if self._pausa.is_set():
                    continue  # com a escuta pausada o quadro é descartado sem passar pelo detector
                self._acompanhar_ruido(quadro)
                if not self._detector.feed(quadro):
                    continue
                if not self._pode_ativar():
                    self._detector.reset()
                    continue
                self._atender(quadros)
        except Exception as e:  # noqa: BLE001 — microfone ausente, driver, modelo: o app segue sem a escuta
            self.stats.ultimo_erro = type(e).__name__
            log.warning("escuta da palavra de ativação terminou: %s", type(e).__name__)
        finally:
            self.stats.ouvindo = False
            with contextlib.suppress(Exception):
                self._source.close()
            self._registrar("escuta", "fim: microfone fechado")

    def _acompanhar_ruido(self, quadro: bytes) -> None:
        """Ruído de fundo = média móvel da energia. Quadro bem acima do ruído (alguém falando)
        quase não puxa a média; se o barulho do ambiente subir de vez, ela acaba acompanhando."""
        r = rms(quadro)
        peso = 0.05 if r < self._ruido * 2 + 1 else 0.002
        self._ruido += peso * (r - self._ruido)

    def _pode_ativar(self) -> bool:
        agora = self._relogio()
        while self._ativadas and agora - self._ativadas[0] > 3600:
            self._ativadas.popleft()
        if len(self._ativadas) >= self._cfg.max_por_hora:
            self.stats.recusadas += 1
            self._registrar("recusada", "teto de ativações por hora")
            return False
        return True

    def _limiar(self) -> float:
        return max(self._cfg.piso, self._ruido * self._cfg.fator_ruido)

    def _atender(self, quadros: Iterator[bytes]) -> None:
        self._ativadas.append(self._relogio())
        self.stats.ativacoes += 1
        self.stats.ultima = self._relogio()
        self._registrar("ativacao", "palavra ouvida; a fala seguinte vai à transcrição")
        if self._bipe is not None:
            with contextlib.suppress(Exception):
                self._bipe()
        fala = self._gravar(quadros)
        if fala is None:
            self.stats.falas_vazias += 1
        else:
            try:
                self._on_fala(pcm_para_wav(fala))
            except Exception as e:  # noqa: BLE001 — um turno com erro não derruba a escuta
                self.stats.ultimo_erro = type(e).__name__
                log.warning("turno de voz da ativação falhou: %s", type(e).__name__)
        self._detector.reset()
        self._source.drain()
        t0 = time.monotonic()
        for _ in quadros:  # resfriamento: o eco da própria resposta não reativa
            if time.monotonic() - t0 >= self._cfg.resfriamento_s or self._parar.is_set():
                break

    def _gravar(self, quadros: Iterator[bytes]) -> bytes | None:
        """Grava até o silêncio. None se ninguém falou (ou foi só um estalo)."""
        cfg, limiar = self._cfg, self._limiar()
        partes: list[bytes] = []
        t = fala_s = silencio_s = 0.0
        for quadro in quadros:
            if self._parar.is_set() or self._pausa.is_set():
                return None
            t += FRAME_S
            if t > cfg.cauda_s and rms(quadro) >= limiar:
                fala_s, silencio_s = fala_s + FRAME_S, 0.0
            elif fala_s:
                silencio_s += FRAME_S
            partes.append(quadro)
            if not fala_s and t >= cfg.espera_s:
                return None
            if fala_s and silencio_s >= cfg.silencio_s:
                break
            if t >= cfg.max_s or sum(map(len, partes)) >= MAX_FALA_BYTES:
                break
        return b"".join(partes) if fala_s >= cfg.min_fala_s else None


# ── detectores ────────────────────────────────────────────────────────────


class OpenWakeWordDetector:
    """Modelo `.onnx` treinado em "orion" (openWakeWord). Precisa de `pip install openwakeword`."""

    def __init__(self, modelo: Path | str, limiar: float = 0.5, *, fabrica: Any = None) -> None:
        if fabrica is None:
            from openwakeword.model import Model as fabrica  # type: ignore[import-not-found]
        self._m = fabrica(wakeword_models=[str(modelo)], inference_framework="onnx")
        self._limiar = limiar

    def feed(self, frame: bytes) -> bool:
        import numpy as np

        pontos = self._m.predict(np.frombuffer(frame, dtype=np.int16))
        return any(float(v) >= self._limiar for v in pontos.values())

    def reset(self) -> None:
        with contextlib.suppress(AttributeError):
            self._m.reset()


# Palavras que competem com "orion" na gramática do Vosk. Com a gramática só `["orion", "[unk]"]`
# o reconhecedor despeja qualquer fala em "orion" (25 de 30 frases sem a palavra ativaram, medido
# com voz sintética): ele precisa ter outras palavras "mais prováveis" para as falas comuns.
# Palavra fora do vocabulário do modelo é ignorada pelo Vosk (só aviso no log, que silenciamos).
CONCORRENTES = tuple(
    """o a os as um uma uns umas de do da dos das em no na nos nas ao aos à às por pelo pela para
    pra com sem sob sobre entre até desde contra durante após que quem qual quais como quando onde
    porque porquê mas e ou se não sim já ainda também muito muita mais menos pouco bem mal hoje
    amanhã ontem agora depois antes sempre nunca aqui ali lá aí isso isto aquilo ele ela eles elas
    eu tu você nós vocês me te lhe nos lhes meu minha meus minhas seu sua seus suas nosso nossa
    este esta esse essa aquele aquela tudo nada algo alguém ninguém todo toda todos todas cada
    outro outra mesmo mesma só ser estar ter haver fazer ir vir ver dar poder dever querer saber
    dizer falar pedir abrir fechar ligar desligar buscar procurar mostrar lembrar chamar mandar
    enviar ler escrever ouvir tocar parar começar terminar precisar comprar tem tenho temos é são
    foi era está estou estão vai vou vão pode posso quer quero faz faça abre abra liga ligue hora
    horas dia dias noite tarde manhã semana mês ano anos tempo vez coisa casa trabalho nome pessoa
    gente vida mundo lugar parte caso forma ponto lado fim começo meio problema pergunta resposta
    um dois três quatro cinco seis sete oito nove dez cem mil primeiro segundo terceiro último
    segunda terça quarta quinta sexta sábado domingo bom boa bons ruim grande pequeno novo velho
    certo errado fácil difícil rápido devagar obrigado obrigada por favor oi olá ok ei tchau
    origem ordem órgão orgão orientação oriente original orquestra orelha""".split()
)


def _sem_acento(palavra: str) -> str:
    return "".join(
        c for c in unicodedata.normalize("NFD", palavra.lower()) if unicodedata.category(c) != "Mn"
    )


class VoskDetector:
    """Reconhecimento offline restrito a um vocabulário pequeno: as palavras de ativação e
    `CONCORRENTES`. Precisa de `pip install vosk` e de uma pasta de modelo baixada por você (o
    Orion não baixa nada sozinho; testado com `vosk-model-small-pt-0.3`).

    Dispara quando a **última palavra do resultado parcial** é a de ativação: assim a escuta
    começa logo depois dela e o comando dito na mesma frase ("Orion, que horas são") é gravado."""

    def __init__(self, pasta: Path | str, palavras: list[str], *, fabrica: Any = None) -> None:
        self._palavras = {p.lower() for p in palavras}
        self._alvo = {_sem_acento(p) for p in palavras}
        vocabulario = sorted({*self._palavras, *CONCORRENTES})
        gramatica = json.dumps([*vocabulario, "[unk]"], ensure_ascii=False)
        if fabrica is None:
            from vosk import KaldiRecognizer, Model, SetLogLevel  # type: ignore[import-not-found]

            SetLogLevel(-1)  # "Ignoring word missing in vocabulary" para cada palavra de fora
            self._rec = KaldiRecognizer(Model(str(pasta)), SAMPLE_RATE, gramatica)
        else:
            self._rec = fabrica(str(pasta), gramatica)

    def feed(self, frame: bytes) -> bool:
        if self._rec.AcceptWaveform(frame):
            texto = str(json.loads(self._rec.Result()).get("text", ""))
        else:
            texto = str(json.loads(self._rec.PartialResult()).get("partial", ""))
        ultima = texto.split()[-1:] or [""]
        return _sem_acento(ultima[0]) in self._alvo

    def reset(self) -> None:
        self._rec.Reset()


def criar_detector(motor: str, modelo: Path | None, palavras: list[str], limiar: float) -> Detector:
    """O detector escolhido na configuração. `ValueError` com a mensagem para o painel se o
    modelo não existe; `ImportError` se falta a biblioteca."""
    if motor == "vosk":
        if modelo is None or not modelo.is_dir():
            raise ValueError("ORION_WAKE_MODEL precisa ser a pasta do modelo do Vosk")
        return VoskDetector(modelo, palavras)
    if modelo is None or not modelo.is_file():
        raise ValueError("ORION_WAKE_MODEL precisa ser o arquivo .onnx do modelo 'orion'")
    return OpenWakeWordDetector(modelo, limiar)


# ── microfone e som ───────────────────────────────────────────────────────


class SoundDeviceSource:
    """Microfone pelo PortAudio (`pip install sounddevice`). O callback só enfileira; a fila tem
    teto e perde o mais antigo, então um laço lento nunca cresce a memória."""

    def __init__(self, dispositivo: str = "", *, sd: Any = None) -> None:
        if sd is None:
            import sounddevice as sd  # type: ignore[import-not-found]
        self._sd = sd
        nome = dispositivo.strip()
        self._dispositivo: int | str | None = (
            None if not nome else int(nome) if nome.isdigit() else nome
        )
        self._fila: queue.Queue[bytes] = queue.Queue(maxsize=400)  # ~32 s
        self._fechado = threading.Event()

    def _callback(self, dados: Any, frames: int, tempo: Any, status: Any) -> None:
        try:
            self._fila.put_nowait(bytes(dados))
        except queue.Full:
            with contextlib.suppress(queue.Empty):
                self._fila.get_nowait()
            with contextlib.suppress(queue.Full):
                self._fila.put_nowait(bytes(dados))

    def frames(self) -> Iterator[bytes]:
        with self._sd.RawInputStream(
            samplerate=SAMPLE_RATE,
            channels=1,
            dtype="int16",
            blocksize=FRAME_SAMPLES,
            device=self._dispositivo,
            callback=self._callback,
        ):
            while not self._fechado.is_set():
                try:
                    yield self._fila.get(timeout=0.5)
                except queue.Empty:
                    continue

    def drain(self) -> None:
        with contextlib.suppress(queue.Empty):
            while True:
                self._fila.get_nowait()

    def close(self) -> None:
        self._fechado.set()


def argv_player(
    plataforma: str, arquivo: Path, *, existe: Callable[[str], str | None] = shutil.which
) -> tuple[list[str], dict[str, str]] | None:
    """(argv, env extra) para tocar um mp3. Caminho por variável de ambiente no Windows, nunca
    montado no script (regra 26). None se o sistema não tem tocador."""
    if plataforma == "darwin":
        return ["afplay", str(arquivo)], {}
    if plataforma.startswith("win"):
        script = (
            "Add-Type -AssemblyName presentationCore;"
            "$p=New-Object System.Windows.Media.MediaPlayer;"
            "$p.Open([uri]$env:ORION_AUDIO);$p.Play();"
            "while(-not $p.NaturalDuration.HasTimeSpan){Start-Sleep -Milliseconds 100};"
            "Start-Sleep -Milliseconds ([int]$p.NaturalDuration.TimeSpan.TotalMilliseconds+300)"
        )
        return ["powershell", "-NoProfile", "-NonInteractive", "-Command", script], {
            "ORION_AUDIO": str(arquivo)
        }
    for nome, extra in (
        ("ffplay", ["-nodisp", "-autoexit", "-loglevel", "quiet"]),
        ("mpg123", ["-q"]),
        ("mpv", ["--no-video", "--really-quiet"]),
    ):
        if existe(nome):
            return [nome, *extra, str(arquivo)], {}
    return None


def tocar_mp3(dados: bytes, *, plataforma: str = sys.platform, timeout_s: float = 180.0) -> bool:
    """Toca a resposta falada e espera terminar. False se o sistema não tem tocador."""
    with tempfile.NamedTemporaryFile(suffix=".mp3", delete=False) as f:
        f.write(dados)
        arquivo = Path(f.name)
    try:
        pronto = argv_player(plataforma, arquivo)
        if pronto is None:
            log.warning("sem tocador de áudio (instale ffplay, mpg123 ou mpv)")
            return False
        argv, extra = pronto
        env = {k: v for k, v in os.environ.items() if not k.startswith("ORION_")} | extra
        subprocess.run(argv, env=env, timeout=timeout_s, check=False, capture_output=True)  # noqa: S603
        return True
    except (OSError, subprocess.SubprocessError) as e:
        log.warning("não toquei a resposta: %s", type(e).__name__)
        return False
    finally:
        arquivo.unlink(missing_ok=True)


def bipe_com_sounddevice(sd: Any = None) -> Callable[[], None]:
    """Um tom curto de "estou ouvindo" (sem arquivo: a onda é gerada aqui)."""

    def tocar() -> None:
        nonlocal sd
        if sd is None:
            import sounddevice as sd_  # type: ignore[import-not-found]

            sd = sd_
        import numpy as np

        n = int(SAMPLE_RATE * 0.12)
        t = np.arange(n) / SAMPLE_RATE
        onda = (9000 * np.sin(2 * np.pi * 880 * t) * (1 - np.arange(n) / n)).astype(np.int16)
        sd.play(onda, SAMPLE_RATE, blocking=False)

    return tocar


@dataclass
class EscutaPronta:
    """O que o app guarda da escuta: o laço, a thread e, se não subiu, o motivo."""

    listener: WakeListener | None = None
    thread: threading.Thread | None = field(default=None, repr=False)
    motivo: str = ""


__all__ = [
    "FRAME_BYTES",
    "FRAME_S",
    "SAMPLE_RATE",
    "AudioSource",
    "Detector",
    "EscutaPronta",
    "OpenWakeWordDetector",
    "SoundDeviceSource",
    "VoskDetector",
    "WakeConfig",
    "WakeListener",
    "WakeStats",
    "argv_player",
    "bipe_com_sounddevice",
    "criar_detector",
    "pcm_para_wav",
    "rms",
    "tocar_mp3",
]
