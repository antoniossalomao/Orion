# /// script
# requires-python = ">=3.12"
# dependencies = [
#   "numpy==2.2.6", "onnx==1.18.0", "onnxruntime==1.22.1",
#   "edge-tts==7.2.3", "truststore==0.10.4",
# ]
# ///
"""Treino local reproduzível do classificador Orion para o frontend oww-rs 0.3.3.

Uso: uv run scripts/treinar_orion_wake.py sintetizar PASTA
     uv run scripts/treinar_orion_wake.py treinar CORPUS.json BUNDLE
     uv run scripts/treinar_orion_wake.py avaliar CORPUS_REAL.json BUNDLE

Sintetizar envia somente frases fixas à Microsoft (edge-tts), não grava o microfone.
Os sintéticos geram um candidato. Só uma avaliação separada com áudio real pode
publicar status validated: recall >=95%, <=0,5 falso/h, >=50 positivos e >=1 h negativo.
CORPUS.json contém synthetic (bool) e clips: path, split (train/validation/test),
speaker (grupo sem vazamento entre splits), positive (bool), start_s/end_s (positivos).
WAV: PCM16 mono 16 kHz. O corpus, o áudio e o treino nunca vão para provedor.
"""

from __future__ import annotations

import argparse
import asyncio
import hashlib
import io
import json
import os
import shutil
import subprocess
import tarfile
import urllib.request
import wave
from collections import deque
from pathlib import Path

FRONTEND_SHA = "d2a492188104214331630dee9125e20b13b7d54cc375f28e01163d1490884ac7"
RATE = 16000
HOP = 1280


def read_corpus(path: Path) -> dict:
    """REGRAS.md: conteúdo externo é dado; corpus só descreve arquivos de áudio."""
    corpus = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(corpus.get("synthetic"), bool) or not corpus.get("clips"):
        raise ValueError("Corpus exige synthetic e clips não vazios")
    speakers: dict[str, str] = {}
    for clip in corpus["clips"]:
        split = clip["split"]
        if split not in {"train", "validation", "test"}:
            raise ValueError("Split inválido")
        speaker = clip["speaker"]
        if speaker in speakers and speakers[speaker] != split:
            raise ValueError("Um locutor/grupo não pode aparecer em splits diferentes")
        speakers[speaker] = split
        if not isinstance(clip["positive"], bool):
            raise ValueError("Rótulo positive precisa ser booleano")
        resolved = (path.parent / clip["path"]).resolve()
        if not resolved.is_relative_to(path.parent.resolve()):
            raise ValueError("Áudio precisa ficar dentro da pasta do corpus")
        clip["file"] = resolved
    return corpus


def publishable(metrics: dict) -> bool:
    """Um candidato sintético nunca é promovido silenciosamente a modelo final."""
    return (
        metrics["real_audio"] is True
        and metrics["positive_clips"] >= 50
        and metrics["negative_seconds"] >= 3600
        and 0.95 <= metrics["recall"] <= 1
        and 0 <= metrics["false_alarms_per_hour"] <= 0.5
    )


def frontend(directory: Path):
    import numpy as np
    import onnxruntime as ort

    directory.mkdir(parents=True, exist_ok=True)
    names = ["melspectrogram.onnx", "embedding_model.onnx"]
    if not all((directory / n).exists() for n in names):
        # URL fixa; valida o pacote inteiro antes de ler só os dois membros esperados.
        with urllib.request.urlopen(
            "https://crates.io/api/v1/crates/oww-rs/0.3.3/download", timeout=60
        ) as response:
            data = response.read(50 * 1024 * 1024)
        if hashlib.sha256(data).hexdigest() != FRONTEND_SHA:
            raise ValueError("SHA-256 do frontend openWakeWord não confere")
        with tarfile.open(fileobj=io.BytesIO(data)) as archive:
            for name in names:
                member = archive.extractfile(f"oww-rs-0.3.3/models/{name}")
                if member is None:
                    raise ValueError("Frontend incompleto")
                (directory / name).write_bytes(member.read())
    options = ort.SessionOptions()
    options.intra_op_num_threads = 1
    sessions = [
        ort.InferenceSession(str(directory / n), options, providers=["CPUExecutionProvider"])
        for n in names
    ]

    def features(audio):
        mel, embedding = sessions
        lookback = np.zeros(480, dtype=np.float32)
        mels = deque([np.zeros((8, 32), dtype=np.float32) for _ in range(10)], maxlen=10)
        vectors = deque([np.zeros(96, dtype=np.float32) for _ in range(16)], maxlen=16)
        for offset in range(0, len(audio), HOP):
            chunk = np.zeros(HOP, dtype=np.float32)
            part = audio[offset : offset + HOP]
            chunk[: len(part)] = part
            data = np.concatenate([lookback, chunk])[None, :]
            lookback = chunk[-480:]
            result = mel.run(None, {mel.get_inputs()[0].name: data})[0].reshape(8, 32)
            mels.append(result / 10 + 2)
            data = np.concatenate(mels)[4:].reshape(1, 76, 32, 1)
            vectors.append(
                embedding.run(None, {embedding.get_inputs()[0].name: data})[0].reshape(96)
            )
            yield np.stack(vectors).copy()

    return features


def read_audio(path: Path):
    import numpy as np

    with wave.open(str(path), "rb") as audio:
        if (audio.getframerate(), audio.getnchannels(), audio.getsampwidth()) != (RATE, 1, 2):
            raise ValueError(f"WAV precisa ser PCM16 mono 16 kHz: {path}")
        return (
            np.frombuffer(audio.readframes(audio.getnframes()), dtype="<i2").astype(np.float32)
            / 32768
        )


def synthesize(directory: Path):
    import truststore

    # O edge-tts fixa certifi; a truststore mantém também as CAs do sistema/ambiente.
    truststore.inject_into_ssl()
    import edge_tts

    directory.mkdir(parents=True, exist_ok=True)
    ffmpeg = shutil.which("ffmpeg")
    if ffmpeg is None:
        raise ValueError("ffmpeg necessário para preparar WAVs")
    voices = [
        ("pt-BR-AntonioNeural", "train"),
        ("pt-BR-FranciscaNeural", "train"),
        ("pt-PT-DuarteNeural", "validation"),
        ("pt-PT-RaquelNeural", "test"),
    ]
    phrases = [
        ("Orion", True),
        ("Órion", True),
        ("Ei Orion", True),
        ("O rio", False),
        ("A origem", False),
        ("Oriente", False),
        ("Bom dia", False),
        ("Que horas são", False),
        ("Abra a janela", False),
        ("Não quero nada", False),
        ("A música está tocando", False),
    ]
    clips = []
    for voice, split in voices:
        for i, (phrase, positive) in enumerate(phrases):
            stem = f"{voice}-{i}"
            mp3, wav = directory / f"{stem}.mp3", directory / f"{stem}.wav"
            asyncio.run(
                edge_tts.Communicate(
                    phrase,
                    voice,
                    proxy=os.environ.get("HTTPS_PROXY"),
                    connect_timeout=15,
                    receive_timeout=30,
                ).save(str(mp3))
            )
            subprocess.run(  # noqa: S603 — argv fixo, ffmpeg absoluto, sem shell.
                [
                    ffmpeg,
                    "-nostdin",
                    "-v",
                    "error",
                    "-y",
                    "-i",
                    str(mp3),
                    "-af",
                    "adelay=640,apad=pad_dur=1.6",
                    "-ar",
                    str(RATE),
                    "-ac",
                    "1",
                    "-c:a",
                    "pcm_s16le",
                    str(wav),
                ],
                check=True,
                timeout=30,
            )
            mp3.unlink()
            import numpy as np

            audio = read_audio(wav)
            active = np.flatnonzero(np.abs(audio) > 0.01)
            if not len(active):
                raise ValueError("TTS não produziu fala audível")
            clips.append(
                {
                    "path": wav.name,
                    "split": split,
                    "speaker": voice,
                    "positive": positive,
                    "start_s": max(0.0, float(active[0]) / RATE - 0.04),
                    "end_s": float(active[-1]) / RATE + 0.04,
                }
            )
    output = directory / "corpus.json"
    output.write_text(json.dumps({"synthetic": True, "clips": clips}, indent=2), encoding="utf-8")
    print(output)


def train(corpus_path: Path, bundle: Path):
    import numpy as np
    import onnx
    from onnx import TensorProto, helper, numpy_helper

    corpus = read_corpus(corpus_path)
    bundle.mkdir(parents=True, exist_ok=True)
    features = frontend(bundle / "frontend")
    x, y = [], []
    for clip in corpus["clips"]:
        if clip["split"] != "train":
            continue
        for i, frame in enumerate(features(read_audio(clip["file"]))):
            end = (i + 1) * 0.08
            label = clip["positive"] and end >= clip["end_s"] and end - 1.28 <= clip["start_s"]
            x.append(frame.reshape(-1))
            y.append(float(label))
    if not x or sum(y) < 10 or len(y) - sum(y) < 10:
        raise ValueError("Treino exige janelas positivas e negativas suficientes")
    data = np.asarray(x, dtype=np.float32)
    labels = np.asarray(y, dtype=np.float32)[:, None]
    mean, scale = data.mean(axis=0), np.maximum(data.std(axis=0), 0.01)
    data = (data - mean) / scale
    rng = np.random.default_rng(42)
    w1 = (rng.standard_normal((1536, 32)) * 0.02).astype(np.float32)
    b1 = np.zeros(32, dtype=np.float32)
    w2 = (rng.standard_normal((32, 1)) * 0.02).astype(np.float32)
    b2 = np.zeros(1, dtype=np.float32)
    weights = np.where(labels > 0, len(y) / (2 * sum(y)), len(y) / (2 * (len(y) - sum(y))))
    for _ in range(60):
        for indices in np.array_split(rng.permutation(len(y)), max(1, len(y) // 64)):
            batch, target = data[indices], labels[indices]
            hidden = np.maximum(batch @ w1 + b1, 0)
            prediction = 1 / (1 + np.exp(-np.clip(hidden @ w2 + b2, -30, 30)))
            delta = ((prediction - target) * weights[indices] / len(indices)).astype(np.float32)
            gradient = (delta @ w2.T) * (hidden > 0)
            w2 -= 0.03 * hidden.T @ delta
            b2 -= 0.03 * delta.sum(axis=0)
            w1 -= 0.03 * batch.T @ gradient
            b1 -= 0.03 * gradient.sum(axis=0)
    constants = {
        "shape": np.array([1, 1536], dtype=np.int64),
        "mean": mean,
        "scale": scale,
        "w1": w1,
        "b1": b1,
        "w2": w2,
        "b2": b2,
    }
    nodes = [
        helper.make_node("Reshape", ["input", "shape"], ["flat"]),
        helper.make_node("Sub", ["flat", "mean"], ["centered"]),
        helper.make_node("Div", ["centered", "scale"], ["normalized"]),
        helper.make_node("MatMul", ["normalized", "w1"], ["dense"]),
        helper.make_node("Add", ["dense", "b1"], ["biased"]),
        helper.make_node("Relu", ["biased"], ["hidden"]),
        helper.make_node("MatMul", ["hidden", "w2"], ["score"]),
        helper.make_node("Add", ["score", "b2"], ["logit"]),
        helper.make_node("Sigmoid", ["logit"], ["probability"]),
    ]
    graph = helper.make_graph(
        nodes,
        "Orion",
        [helper.make_tensor_value_info("input", TensorProto.FLOAT, [1, 16, 96])],
        [helper.make_tensor_value_info("probability", TensorProto.FLOAT, [1, 1])],
        [numpy_helper.from_array(value, name) for name, value in constants.items()],
    )
    model = helper.make_model(graph, opset_imports=[helper.make_opsetid("", 13)], ir_version=8)
    onnx.checker.check_model(model)
    onnx.save(model, bundle / "orion.onnx")
    data = (bundle / "orion.onnx").read_bytes()
    manifest = {
        "keyword": "Orion",
        "status": "candidate",
        "sha256": hashlib.sha256(data).hexdigest(),
        "bytes": len(data),
        "threshold": 0.5,
        "input_shape": [1, 16, 96],
        "metrics": None,
        "training_speakers": sorted(
            {c["speaker"] for c in corpus["clips"] if c["split"] == "train"}
        ),
        "synthetic_training": corpus["synthetic"],
        "seed": 42,
    }
    (bundle / "orion.json").write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    print("Candidato criado. Avalie com corpus separado e teste o microfone: orion wake-test.")


def evaluate(corpus_path: Path, bundle: Path):
    import onnxruntime as ort

    corpus = read_corpus(corpus_path)
    manifest_path = bundle / "orion.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    if hashlib.sha256((bundle / "orion.onnx").read_bytes()).hexdigest() != manifest["sha256"]:
        raise ValueError("Hash do modelo não confere")
    features = frontend(bundle / "frontend")
    session = ort.InferenceSession(str(bundle / "orion.onnx"), providers=["CPUExecutionProvider"])
    positives = hits = false_alarms = 0
    negative_seconds = 0.0
    training = set(manifest["training_speakers"])
    for clip in corpus["clips"]:
        if clip["split"] != "test":
            continue
        if clip["speaker"] in training:
            raise ValueError("Avaliação contaminada por locutor usado no treino")
        audio = read_audio(clip["file"])
        history: deque[float] = deque(maxlen=12)
        detections = 0
        last = -2.0
        for i, frame in enumerate(features(audio)):
            probability = float(session.run(None, {"input": frame[None, :, :]})[0].reshape(-1)[0])
            history.append(probability)
            above = [v for v in history if v > manifest["threshold"]]
            time_s = (i + 1) * 0.08
            if probability < 0.1 and len(above) > 3 and time_s - last > 2.0:
                detections += 1
                last = time_s
        if clip["positive"]:
            positives += 1
            hits += int(detections > 0)
        else:
            false_alarms += detections
            negative_seconds += len(audio) / RATE
    if not positives or not negative_seconds:
        raise ValueError("Teste exige positivos e áudio negativo")
    metrics = {
        "recall": hits / positives,
        "false_alarms_per_hour": false_alarms * 3600 / negative_seconds,
        "positive_clips": positives,
        "negative_seconds": negative_seconds,
        "real_audio": not corpus["synthetic"],
    }
    manifest["metrics"] = metrics
    manifest["status"] = "validated" if publishable(metrics) else "candidate"
    manifest_path.write_text(json.dumps(manifest, indent=2), encoding="utf-8")
    print(json.dumps({"status": manifest["status"], **metrics}, indent=2))


def main():
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("sintetizar").add_argument("directory", type=Path)
    for command in ["treinar", "avaliar"]:
        sub = commands.add_parser(command)
        sub.add_argument("corpus", type=Path)
        sub.add_argument("bundle", type=Path)
    args = parser.parse_args()
    if args.command == "sintetizar":
        synthesize(args.directory)
    elif args.command == "treinar":
        train(args.corpus, args.bundle)
    else:
        evaluate(args.corpus, args.bundle)


if __name__ == "__main__":
    main()
