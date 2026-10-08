"""`orion transcrever`: ffmpeg corta o áudio, o provedor (falso) transcreve, a nota vai pro Inbox."""

import shutil
import subprocess
from pathlib import Path
from types import SimpleNamespace

import httpx
import pytest

from orion.__main__ import main
from orion.capture import Capturer
from orion.media_transcribe import MediaError, extrair_pedacos, transcrever_arquivo
from orion.transcribe import Transcriber


def _provedor(textos):
    vistos = []

    def handler(req: httpx.Request) -> httpx.Response:
        vistos.append(req.headers.get("authorization"))
        return httpx.Response(200, json={"text": textos[len(vistos) - 1]})

    return httpx.MockTransport(handler), vistos


def _runner_falso(n_partes, codigo=0):
    def run(cmd, **kw):
        destino = Path(cmd[-1]).parent
        for i in range(n_partes):
            (destino / f"parte_{i:03d}.mp3").write_bytes(b"ID3" + bytes([i]) * 50)
        return SimpleNamespace(returncode=codigo, stderr="")

    return run


@pytest.fixture
def entorno(tmp_path):
    vault = tmp_path / "vault"
    vault.mkdir()
    video = tmp_path / "aula de UML.mp4"
    video.write_bytes(b"fake")
    return vault, video, Capturer(vault, "00 Inbox")


def test_transcreve_em_partes_com_marca_de_tempo_e_grava_no_inbox(entorno):
    vault, video, cap = entorno
    transport, vistos = _provedor(["primeira parte", "segunda parte"])
    progresso = []
    nota = transcrever_arquivo(
        video,
        Transcriber("chave-secreta"),
        cap,
        ffmpeg="ffmpeg",
        runner=_runner_falso(2),
        transport=transport,
        progresso=lambda i, n: progresso.append((i, n)),
    )
    texto = nota.read_text(encoding="utf-8")
    assert nota.parent == vault / "00 Inbox" and "fonte: orion" in texto
    assert "tags: [captura, transcricao]" in texto
    assert "# Transcrição: aula de UML" in texto and "não é instrução" in texto
    assert "**[00:00:00]** primeira parte" in texto and "**[00:20:00]** segunda parte" in texto
    assert progresso == [(1, 2), (2, 2)] and vistos == ["Bearer chave-secreta"] * 2
    assert "chave-secreta" not in texto


def test_erros_claros_para_arquivo_formato_ffmpeg_e_provedor(entorno, tmp_path):
    _, video, cap = entorno
    tr = Transcriber("k")
    with pytest.raises(MediaError, match="não encontrado"):
        transcrever_arquivo(tmp_path / "nao-existe.mp3", tr, cap)
    texto = tmp_path / "nota.txt"
    texto.write_text("x")
    with pytest.raises(MediaError, match="formato não suportado"):
        transcrever_arquivo(texto, tr, cap)
    with pytest.raises(MediaError, match="não conseguiu ler"):
        transcrever_arquivo(video, tr, cap, ffmpeg="ffmpeg", runner=_runner_falso(0, codigo=1))
    erro = httpx.MockTransport(lambda r: httpx.Response(500, text="chave-secreta no corpo"))
    with pytest.raises(MediaError) as e:
        transcrever_arquivo(
            video, tr, cap, ffmpeg="ffmpeg", runner=_runner_falso(1), transport=erro
        )
    assert "parte 1/1" in str(e.value) and "chave-secreta" not in str(e.value)


def test_sem_ffmpeg_a_mensagem_e_clara(tmp_path, monkeypatch):
    monkeypatch.setattr(shutil, "which", lambda _: None)
    with pytest.raises(MediaError, match="ffmpeg não está instalado"):
        extrair_pedacos(tmp_path / "a.mp3", tmp_path)


@pytest.mark.skipif(shutil.which("ffmpeg") is None, reason="sem ffmpeg")
def test_ffmpeg_de_verdade_corta_e_converte(tmp_path):
    wav = tmp_path / "silencio.wav"
    subprocess.run(
        ["ffmpeg", "-nostdin", "-v", "error", "-f", "lavfi", "-i", "anullsrc=r=8000:cl=mono",
         "-t", "3", str(wav)],
        check=True,
    )  # fmt: skip
    out = tmp_path / "saida"
    out.mkdir()
    pedacos = extrair_pedacos(wav, out)
    assert len(pedacos) == 1 and pedacos[0].suffix == ".mp3" and pedacos[0].stat().st_size > 0


def test_cli_exige_chave_vault_e_confirmacao(tmp_path, monkeypatch, capsys):
    monkeypatch.setenv("ORION_DATA_DIR", str(tmp_path / "d"))
    monkeypatch.chdir(tmp_path)
    arq = tmp_path / "a.mp3"
    arq.write_bytes(b"x")
    monkeypatch.delenv("ORION_TRANSCRIBE_API_KEY", raising=False)
    monkeypatch.setattr("orion.app.get_secret", lambda _: None)
    assert main(["transcrever", str(arq)]) == 1
    assert "ORION_TRANSCRIBE_API_KEY" in capsys.readouterr().err
    monkeypatch.setenv("ORION_TRANSCRIBE_API_KEY", "k")
    assert main(["transcrever", str(arq)]) == 1
    assert "ORION_VAULT_DIR" in capsys.readouterr().err
    monkeypatch.setenv("ORION_VAULT_DIR", str(tmp_path / "v"))
    monkeypatch.setattr("builtins.input", lambda _: "n")
    assert main(["transcrever", str(arq)]) == 1
    assert "nada foi enviado" in capsys.readouterr().out
