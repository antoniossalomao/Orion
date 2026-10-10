import httpx
import pytest

from orion.transcribe import MAX_AUDIO, TranscribeError, Transcriber


def com(handler, **kw):
    return Transcriber(
        "CHAVE-SECRETA", client=httpx.AsyncClient(transport=httpx.MockTransport(handler)), **kw
    )


async def test_envia_multipart_no_formato_da_openai_e_devolve_o_texto():
    vistos = []

    def h(req: httpx.Request):
        vistos.append(req)
        return httpx.Response(200, json={"text": "  olá mundo  "})

    t = com(h, base_url="https://api.exemplo.test/openai/v1/", model="whisper-x", language="pt")
    assert await t.transcribe(b"audio-bytes", "a.ogg", "audio/ogg") == "olá mundo"
    (req,) = vistos
    assert str(req.url) == "https://api.exemplo.test/openai/v1/audio/transcriptions"
    assert req.headers["authorization"] == "Bearer CHAVE-SECRETA"
    assert req.headers["content-type"].startswith("multipart/form-data")
    corpo = req.content
    assert b'filename="a.ogg"' in corpo and b"audio-bytes" in corpo and b"whisper-x" in corpo


@pytest.mark.parametrize("status", [401, 429, 500])
async def test_erro_http_nao_vaza_a_chave_nem_o_corpo(status):
    t = com(lambda req: httpx.Response(status, text="CHAVE-SECRETA no corpo"))
    with pytest.raises(TranscribeError) as e:
        await t.transcribe(b"x")
    assert f"HTTP {status}" in str(e.value) and "CHAVE" not in str(e.value)


async def test_falha_de_rede_resposta_estranha_e_audio_vazio_ou_grande():
    def cai(req):
        raise httpx.ConnectError(f"falhou em {req.url} com CHAVE-SECRETA")

    with pytest.raises(TranscribeError) as e:
        await com(cai).transcribe(b"x")
    assert (
        "ConnectError" in str(e.value)
        and "CHAVE" not in str(e.value)
        and "http" not in str(e.value)
    )
    with pytest.raises(TranscribeError, match="inválida"):
        await com(lambda r: httpx.Response(200, text="não é json")).transcribe(b"x")
    with pytest.raises(TranscribeError, match="nada"):
        await com(lambda r: httpx.Response(200, json={"text": "   "})).transcribe(b"x")
    with pytest.raises(TranscribeError, match="vazio"):
        await com(lambda r: httpx.Response(200)).transcribe(b"")
    with pytest.raises(TranscribeError, match="grande"):
        await com(lambda r: httpx.Response(200)).transcribe(b"x" * (MAX_AUDIO + 1))


def test_chave_vazia_e_recusada():
    with pytest.raises(ValueError):
        Transcriber("")
