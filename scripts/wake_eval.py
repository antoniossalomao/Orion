"""Mede o detector da palavra de ativação com as SUAS gravações, antes de ligar o microfone.

Grave (celular, gravador do Windows) frases curtas, converta para WAV 16 kHz mono 16 bits e
nomeie `pos_*.wav` (você dizendo "Orion", sozinho ou no começo de um pedido) e `neg_*.wav`
(fala normal sem a palavra, TV, música, conversa ao fundo). Quanto mais, melhor: 20 de cada é o
mínimo para ter alguma confiança.

    # converter (ffmpeg): ffmpeg -i entrada.m4a -ar 16000 -ac 1 -sample_fmt s16 pos_01.wav
    uv run --extra wake-vosk python scripts/wake_eval.py vosk <pasta-do-modelo> <pasta-dos-wavs>
    uv run --extra wake python scripts/wake_eval.py openwakeword <modelo.onnx> <pasta-dos-wavs> 0.5

Mostra quantas gravações `pos_` ativaram e quantas `neg_` ativaram sem dever. Troque o limiar
(openwakeword) até o falso alarme ser zero ou quase: ele custa mais que uma ativação perdida.
"""

from __future__ import annotations

import sys
import wave
from pathlib import Path

from orion.wake import FRAME_BYTES, OpenWakeWordDetector, VoskDetector


def quadros(arquivo: Path) -> list[bytes]:
    with wave.open(str(arquivo)) as w:
        if (w.getnchannels(), w.getsampwidth(), w.getframerate()) != (1, 2, 16000):
            raise SystemExit(f"{arquivo.name}: precisa ser WAV 16 kHz, mono, 16 bits")
        pcm = bytes(FRAME_BYTES * 6) + w.readframes(w.getnframes()) + bytes(FRAME_BYTES * 25)
    return [
        pcm[i : i + FRAME_BYTES].ljust(FRAME_BYTES, b"\0") for i in range(0, len(pcm), FRAME_BYTES)
    ]


def main(argv: list[str]) -> int:
    if len(argv) < 4 or argv[1] not in ("vosk", "openwakeword"):
        print(__doc__)
        return 2
    motor, modelo, pasta = argv[1], Path(argv[2]), Path(argv[3])
    if motor == "vosk":
        det = VoskDetector(modelo, ["orion", "órion"])
    else:
        det = OpenWakeWordDetector(modelo, float(argv[4]) if len(argv) > 4 else 0.5)
    arquivos = sorted(pasta.glob("*.wav"))
    pos = [a for a in arquivos if a.name.startswith("pos_")]
    neg = [a for a in arquivos if a.name.startswith("neg_")]
    if not pos or not neg:
        print("preciso de arquivos pos_*.wav e neg_*.wav na pasta")
        return 2

    def ativou(a: Path) -> bool:
        det.reset()
        return any(det.feed(q) for q in quadros(a))

    acertos = [a for a in pos if ativou(a)]
    falsos = [a for a in neg if ativou(a)]
    print(f"ativou em {len(acertos)}/{len(pos)} gravações com a palavra")
    print(f"ativou sem dever em {len(falsos)}/{len(neg)} gravações sem a palavra")
    for a in pos:
        if a not in acertos:
            print(f"  perdeu: {a.name}")
    for a in falsos:
        print(f"  falso alarme: {a.name}")
    return 0 if not falsos else 1


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
