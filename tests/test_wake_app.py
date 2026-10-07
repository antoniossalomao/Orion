"""Palavra de ativação ligada ao app (regra 38): da palavra ouvida ao turno do agente e à fala."""

import time

from fastapi.testclient import TestClient

from orion import app as orion_app
from orion.app import create_app
from orion.config import Settings
from orion.wake import WakeConfig, WakeListener
from tests.fakes import FakeGateway, fala
from tests.test_voice import AUTH, TOKEN, FakeSpeaker, FakeTranscriber
from tests.test_wake import FALA, GATILHO, SILENCIO, DetectorFalso, FonteFalsa, segundos

ROTEIRO = [GATILHO] + [FALA] * 12 + [SILENCIO] * segundos(1.3)


def app_com_escuta(tmp_path, monkeypatch, *, quadros=None, voz=True, wake=True, transcriber=True):
    tocados: list[bytes] = []
    monkeypatch.setattr(orion_app, "tocar_mp3", tocados.append)
    ouvinte: list[WakeListener] = []

    def fabrica(settings, on_fala, evento):
        o = WakeListener(
            FonteFalsa(quadros if quadros is not None else ROTEIRO),
            DetectorFalso(),
            on_fala,
            config=WakeConfig(resfriamento_s=0.0),
            evento=evento,
        )
        ouvinte.append(o)
        return o, ""

    settings = Settings(
        data_dir=tmp_path / "d",
        admin_token=TOKEN,
        voice_enabled=voz,
        wake_enabled=wake,
        _env_file=None,
    )
    gw = FakeGateway(fala("São três horas."))
    app = create_app(
        settings,
        gateway_factory=lambda _: gw,
        transcriber_factory=lambda _: FakeTranscriber() if transcriber else None,
        speaker_factory=lambda _: FakeSpeaker(),
        wake_factory=fabrica,
    )
    return TestClient(app, base_url="http://127.0.0.1"), gw, tocados, ouvinte


def esperar(cond, s=5.0):
    fim = time.monotonic() + s
    while time.monotonic() < fim:
        if cond():
            return True
        time.sleep(0.02)
    return False


def test_palavra_ouvida_vira_turno_do_agente_com_fala_tocada_e_audit(tmp_path, monkeypatch):
    c, gw, tocados, _ = app_com_escuta(tmp_path, monkeypatch)
    with c:
        assert esperar(lambda: tocados)
        assert tocados == [b"MP3-FALSO"]
        assert gw.chamadas and "que horas são" in str(gw.chamadas[0])
        estado = c.app.state.orion
        ev = [e["tool"] for e in estado.ops.audit_recent(20)]
        assert "voz_ativacao" in ev and "voz_escuta" in ev
        v = c.get("/painel", headers=AUTH).json()["voz"]["escuta"]
        assert v["pedida"] and v["ativacoes"] == 1 and v["ultimo_erro"] is None


def test_audit_da_ativacao_nao_leva_audio_nem_texto_falado(tmp_path, monkeypatch):
    c, _gw, tocados, _ = app_com_escuta(tmp_path, monkeypatch)
    with c:
        assert esperar(lambda: tocados)
        todos = str(c.app.state.orion.ops.audit_recent(50))
        assert "que horas são" not in todos and "MP3" not in todos


def test_a_palavra_nao_aprova_nada_e_o_canal_e_o_da_voz(tmp_path, monkeypatch):
    c, _gw, tocados, _ = app_com_escuta(tmp_path, monkeypatch)
    with c:
        assert esperar(lambda: tocados)
        assert c.get("/approvals", headers=AUTH).json() == []


def test_pausar_pela_api_tira_o_detector_do_circuito(tmp_path, monkeypatch):
    c, _gw, _t, ouvinte = app_com_escuta(tmp_path, monkeypatch, quadros=[])
    with c:
        assert c.post("/voz/escuta", json={"ativa": False}).status_code == 401
        r = c.post("/voz/escuta", json={"ativa": False}, headers=AUTH)
        assert r.json() == {"pausada": True} and ouvinte[0].stats.pausada
        painel = c.get("/painel", headers=AUTH).json()["voz"]["escuta"]
        assert painel["pausada"] is True
        assert c.post("/voz/escuta", json={"ativa": True}, headers=AUTH).json() == {
            "pausada": False
        }


def test_sem_voz_ligada_a_escuta_nao_sobe_e_o_painel_diz_por_que(tmp_path, monkeypatch):
    c, _gw, _t, ouvinte = app_com_escuta(tmp_path, monkeypatch, voz=False)
    with c:
        assert ouvinte == []  # nem chegou a criar o microfone
        e = c.get("/painel", headers=AUTH).json()["voz"]["escuta"]
        assert e["pedida"] and not e["ouvindo"] and "ORION_VOICE_ENABLED" in e["ultimo_erro"]
        r = c.post("/voz/escuta", json={"ativa": False}, headers=AUTH)
        assert r.status_code == 409


def test_sem_chave_de_transcricao_a_escuta_nao_sobe(tmp_path, monkeypatch):
    c, *_, ouvinte = app_com_escuta(tmp_path, monkeypatch, transcriber=False)
    with c:
        assert ouvinte == []
        e = c.get("/painel", headers=AUTH).json()["voz"]["escuta"]
        assert "ORION_TRANSCRIBE_API_KEY" in e["ultimo_erro"]


def test_desligada_por_padrao(tmp_path, monkeypatch):
    c, *_, ouvinte = app_com_escuta(tmp_path, monkeypatch, wake=False)
    with c:
        assert ouvinte == []
        e = c.get("/painel", headers=AUTH).json()["voz"]["escuta"]
        assert e == {
            "pedida": False,
            "ouvindo": False,
            "pausada": False,
            "ativacoes": 0,
            "ultima": None,
            "ultimo_erro": None,
        }
