"""S3: limite do /upload e checagem de Origin do /ws/voice."""

import pytest
from fastapi import FastAPI
from fastapi.testclient import TestClient
from origem import ORIGENS_PERMITIDAS, origem_permitida
from routers.misc import MAX_UPLOAD_BYTES, MiscRouter
from starlette.websockets import WebSocketDisconnect


@pytest.fixture
def cliente(tmp_path):
    chamadas = []

    async def voz(ws, _chave):
        await ws.accept()
        chamadas.append("voz")
        await ws.close()

    r = MiscRouter(
        pasta_uploads=str(tmp_path),
        voice_session=voz,
        gemini_api_key="x",
        increment_voice_live=lambda: None,
        decrement_voice_live=lambda: None,
    )
    app = FastAPI()
    app.include_router(r.router)
    return TestClient(app), tmp_path, chamadas


def test_upload_normal_e_sanitiza_nome(cliente):
    c, pasta, _ = cliente
    r = c.post("/upload", files={"file": ("../../x y.png", b"abc", "image/png")})
    assert r.status_code == 200 and r.json()["bytes"] == 3
    assert "/" not in r.json()["nome"] and len(list(pasta.iterdir())) == 1


def test_upload_acima_do_limite_e_recusado_sem_gravar(cliente):
    c, pasta, _ = cliente
    r = c.post("/upload", files={"file": ("g.bin", b"0" * (MAX_UPLOAD_BYTES + 1))})
    assert r.status_code == 413
    assert list(pasta.iterdir()) == []


def test_ws_voice_recusa_origem_de_site_externo(cliente):
    c, _, chamadas = cliente
    with pytest.raises(WebSocketDisconnect):
        with c.websocket_connect("/ws/voice", headers={"origin": "https://evil.example"}):
            pass
    assert chamadas == []


@pytest.mark.parametrize("origem", [None, "null", "http://127.0.0.1:8000"])
def test_ws_voice_aceita_origens_locais(cliente, origem):
    c, _, chamadas = cliente
    headers = {"origin": origem} if origem else {}
    with c.websocket_connect("/ws/voice", headers=headers):
        pass
    assert chamadas == ["voz"]


def test_politica_de_origem():
    assert origem_permitida(None) and origem_permitida("null")
    assert not origem_permitida("https://evil.example")
    assert not origem_permitida("http://127.0.0.1:8001")
    assert None in ORIGENS_PERMITIDAS
