"""Painel de privacidade (N2): o que saiu do computador, por dia e provedor, sem o conteúdo."""

from datetime import datetime

from fastapi.testclient import TestClient

from orion.app import _privacidade, create_app
from orion.config import Settings
from orion.memory import MemoryStore

TOKEN = "token-de-teste-com-16+"
AUTH = {"Authorization": f"Bearer {TOKEN}"}
AGORA = datetime(2026, 10, 9, 15).timestamp()


def test_agrupa_por_dia_e_provedor_e_deixa_o_local_de_fora(tmp_path):
    store = MemoryStore(tmp_path / "p.db")
    try:
        add = store.add_external_call
        add(provider="gateway:padrão", kind="chat", ok=True, latency_ms=1, bytes_out=100, ts=AGORA)
        add(
            provider="gateway:visao",
            kind="vision",
            ok=True,
            latency_ms=1,
            bytes_out=5000,
            content_kind="imagem",
            ts=AGORA - 60,
        )
        add(
            provider="groq",
            kind="transcribe",
            ok=False,
            latency_ms=1,
            bytes_out=900,
            content_kind="audio",
            ts=AGORA - 86400,
        )
        add(provider="ollama", kind="chat", ok=True, latency_ms=1, bytes_out=777, ts=AGORA)
        add(provider="brave", kind="search", ok=True, latency_ms=1, ts=AGORA - 10 * 86400)
        p = _privacidade(store, 7, AGORA)
    finally:
        store.close()
    assert p["dias"] == 7 and len(p["serie"]) == 7
    assert p["serie"][-1]["dia"] == "2026-10-09" and p["serie"][0]["dia"] == "2026-10-03"
    hoje, ontem = p["serie"][-1], p["serie"][-2]
    assert (hoje["envios"], hoje["bytes"]) == (2, 5100)
    assert hoje["provedores"]["gateway:visao"]["tipos"] == {"imagem": 1}
    assert ontem["provedores"]["groq"] == {"envios": 1, "bytes": 900, "tipos": {"audio": 1}}
    assert p["provedores"] == ["gateway:padrão", "gateway:visao", "groq"]  # sem ollama nem brave
    assert [x["provedor"] for x in p["hoje"]] == ["gateway:padrão", "gateway:visao"]
    assert p["hoje"][1] == {
        "hora": "14:59:00",
        "provedor": "gateway:visao",
        "tipo": "vision",
        "conteudo": "imagem",
        "bytes": 5000,
        "ok": True,
    }


def test_rota_exige_login_e_limita_os_dias(tmp_path):
    settings = Settings(data_dir=tmp_path / "d", admin_token=TOKEN, _env_file=None)
    with TestClient(
        create_app(settings, gateway_factory=lambda _: None), base_url="http://127.0.0.1"
    ) as c:
        assert c.get("/privacidade").status_code == 401
        assert c.get("/privacidade?dias=0", headers=AUTH).status_code == 422
        r = c.get("/privacidade?dias=3", headers=AUTH)
    assert r.status_code == 200 and len(r.json()["serie"]) == 3 and r.json()["hoje"] == []
