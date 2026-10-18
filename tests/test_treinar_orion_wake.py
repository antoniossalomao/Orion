"""Critérios de publicação e separação dos dados do treino, sem baixar modelos."""

import importlib.util
import json
from pathlib import Path

import pytest

_path = Path(__file__).parents[1] / "scripts" / "treinar_orion_wake.py"
_spec = importlib.util.spec_from_file_location("treinar_orion_wake", _path)
assert _spec and _spec.loader
_module = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_module)


def test_sintetico_ou_amostra_curta_nao_publica_modelo_final():
    metrics = {
        "real_audio": False,
        "positive_clips": 100,
        "negative_seconds": 7200,
        "recall": 1.0,
        "false_alarms_per_hour": 0.0,
    }
    assert not _module.publishable(metrics)
    metrics["real_audio"] = True
    assert _module.publishable(metrics)
    for key, value in [
        ("positive_clips", 49),
        ("negative_seconds", 3599),
        ("recall", 0.94),
        ("false_alarms_per_hour", 0.51),
        ("recall", float("nan")),
    ]:
        assert not _module.publishable({**metrics, key: value})


def test_locutor_e_caminho_nao_vazam_entre_conjuntos(tmp_path):
    path = tmp_path / "corpus.json"
    clip = {"speaker": "antonio", "path": "voz.wav", "positive": True, "split": "train"}
    path.write_text(json.dumps({"synthetic": False, "clips": [clip, {**clip, "split": "test"}]}))
    with pytest.raises(ValueError, match="locutor"):
        _module.read_corpus(path)
    path.write_text(json.dumps({"synthetic": False, "clips": [{**clip, "path": "../fora.wav"}]}))
    with pytest.raises(ValueError, match="dentro"):
        _module.read_corpus(path)
