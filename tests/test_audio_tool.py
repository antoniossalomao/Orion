import json

import httpx

from orion.tools.audio import audio_tools
from orion.transcribe import Transcriber


def montar(handler):
    t = Transcriber("CHAVE-SECRETA")
    ferramenta = audio_tools(t, httpx.MockTransport(handler))[0]
    return ferramenta


def roda(ferramenta, **args):
    return json.loads(ferramenta.run(args))


def test_transcreve_arquivo_local_enviando_nome_tipo_e_chave_certos(tmp_path):
    vistos = []

    def api(req: httpx.Request):
        vistos.append(req)
        return httpx.Response(200, json={"text": "reunião às nove"})

    audio = tmp_path / "gravação.m4a"
    audio.write_bytes(b"AUDIO")
    r = roda(montar(api), path=str(audio))
    assert r == {"ok": True, "path": str(audio), "caracteres": 15, "texto": "reunião às nove"}
    (req,) = vistos
    assert req.headers["authorization"] == "Bearer CHAVE-SECRETA" and b"AUDIO" in req.content
    assert b"audio/mp4" in req.content or b"audio/x-m4a" in req.content or b"m4a" in req.content


def test_recusa_ausente_formato_estranho_e_erro_da_api_sem_vazar_a_chave(tmp_path):
    f = montar(lambda req: httpx.Response(429, text="CHAVE-SECRETA estourou"))
    assert "não encontrado" in roda(f, path=str(tmp_path / "x.mp3"))["erro"]
    (tmp_path / "a.exe").write_bytes(b"MZ")
    assert "não suportado" in roda(f, path=str(tmp_path / "a.exe"))["erro"]
    (tmp_path / "b.mp3").write_bytes(b"x")
    r = roda(f, path=str(tmp_path / "b.mp3"))
    assert r["erro"] == "a transcrição falhou (HTTP 429)" and "CHAVE" not in json.dumps(r)


def test_so_entra_no_registro_com_a_chave_e_a_politica_trata_como_externo(tmp_path):
    from orion.memory import MemoryStore
    from orion.policy import DEFAULT_TOOLS
    from orion.tools import default_registry

    s = MemoryStore(tmp_path / "m.db")
    try:
        sem = default_registry(s, desktop=True).names()
        com = default_registry(s, desktop=True, transcriber=Transcriber("k" * 20)).names()
        assert "transcrever_audio" not in sem and "transcrever_audio" in com
    finally:
        s.close()
    spec = DEFAULT_TOOLS["transcrever_audio"]
    assert spec.external and spec.read_path_arg == "path"
