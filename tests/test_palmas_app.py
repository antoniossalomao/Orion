"""Duas palmas ligadas ao app (regra 51): a ação, o pânico, a ponte, o audit e o painel."""

import json

from fastapi.testclient import TestClient

from orion import app as orion_app
from orion.app import create_app
from orion.config import Settings
from orion.wake import WakeConfig, WakeListener
from tests.fakes import FakeGateway, fala
from tests.test_voice import AUTH, TOKEN, FakeSpeaker, FakeTranscriber
from tests.test_wake import (
    PALMA,
    SILENCIO,
    FonteFalsa,
    PalmasFalsas,
)
from tests.test_wake_app import esperar

WS = "ws://127.0.0.1"


def app_com_palmas(tmp_path, monkeypatch, *, acao="abrir", quadros=None, voz=False, **extra):
    abertos: list[str] = []
    monkeypatch.setattr(orion_app.webbrowser, "open", lambda url: abertos.append(url) or True)
    ouvintes: list[WakeListener] = []

    def fabrica(settings, on_fala, evento, on_palmas=None):
        o = WakeListener(
            FonteFalsa(quadros if quadros is not None else [SILENCIO, PALMA, SILENCIO]),
            None,
            on_fala,
            config=WakeConfig(resfriamento_s=0.0),
            evento=evento,
            palmas=PalmasFalsas(),
            on_palmas=on_palmas,
        )
        ouvintes.append(o)
        return o, ""

    settings = Settings(
        data_dir=tmp_path / "d",
        admin_token=TOKEN,
        voice_enabled=voz,
        clap_enabled=True,
        clap_action=acao,
        _env_file=None,
        **extra,
    )
    app = create_app(
        settings,
        gateway_factory=lambda _: FakeGateway(fala("ok")),
        transcriber_factory=lambda _: FakeTranscriber(),
        speaker_factory=lambda _: FakeSpeaker(),
        wake_factory=fabrica,
    )
    return TestClient(app, base_url="http://127.0.0.1"), abertos, ouvintes


def test_duas_palmas_sem_ponte_abrem_o_navegador_e_vao_para_o_audit(tmp_path, monkeypatch):
    c, abertos, _ouvintes = app_com_palmas(tmp_path, monkeypatch)
    with c:
        assert esperar(lambda: abertos)
        assert abertos == [f"http://127.0.0.1:{c.app.state.orion.settings.port}/ui/#/chat"]
        ev = c.app.state.orion.ops.audit_recent(20)
        palmas = [e for e in ev if e["tool"] == "voz_palmas"]
        assert palmas and palmas[0]["action"] == "allow"
        assert "só abrem" in palmas[0]["reason"]  # nada de áudio no audit
        p = c.get("/painel", headers=AUTH).json()["voz"]["palmas"]
        assert p["pedida"] and p["acao"] == "abrir" and p["acionadas"] == 1 and p["recusadas"] == 0
        assert isinstance(p["picos_ultima_hora"], list)
        assert c.get("/approvals", headers=AUTH).json() == []  # palma nunca aprova nada


def test_com_a_ponte_conectada_o_comando_vai_para_ela_e_o_navegador_fica_quieto(
    tmp_path, monkeypatch
):
    c, abertos, _ouvintes = app_com_palmas(tmp_path, monkeypatch, quadros=[])
    with c:
        token = c.app.state.orion.auth.create_device_token("ponte", "t")
        with c.websocket_connect(
            WS + "/ws/ponte", headers={"Authorization": f"Bearer {token}"}
        ) as ws:
            # o laço de escuta já terminou (quadros vazios): chama a ação como ele chamaria
            from orion.palmas import acao_das_palmas

            ao_bater = acao_das_palmas(
                "abrir",
                abrir_ponte=lambda: c.app.state.orion.ponte.enviar(
                    {"cmd": "abrir", "rota": "#/chat"}
                ),
                abrir_navegador=lambda: abertos.append("navegador"),
                bloqueado=lambda: "",
            )
            assert ao_bater() is False
            assert json.loads(ws.receive_text()) == {"cmd": "abrir", "rota": "#/chat"}
        assert abertos == []


def test_em_panico_as_palmas_nao_fazem_nada_e_o_audit_registra_a_recusa(tmp_path, monkeypatch):
    c, _abertos, _ouvintes = app_com_palmas(tmp_path, monkeypatch, quadros=[])
    with c:
        c.app.state.orion.modos.entrar_panico("teste")  # o estado fica no banco (regra 48)
    # reinicia no mesmo banco com as palmas: já nasce em pânico
    c2, abertos2, ouvintes2 = app_com_palmas(tmp_path, monkeypatch)
    with c2:
        assert esperar(lambda: ouvintes2 and ouvintes2[0].stats.palmas_recusadas)
        assert abertos2 == []
        ev = c2.app.state.orion.ops.audit_recent(20)
        recusa = [e for e in ev if e["tool"] == "voz_palmas_recusadas"]
        assert recusa and recusa[0]["action"] == "deny"
        p = c2.get("/painel", headers=AUTH).json()["voz"]["palmas"]
        assert p["acionadas"] == 0 and p["recusadas"] == 1


def test_abrir_e_ouvir_exige_a_voz(tmp_path, monkeypatch):
    c, _abertos, ouvintes = app_com_palmas(tmp_path, monkeypatch, acao="abrir_e_ouvir", voz=False)
    with c:
        assert ouvintes == []  # nem criou o microfone
        e = c.get("/painel", headers=AUTH).json()["voz"]
        assert "ORION_VOICE_ENABLED" in e["escuta"]["ultimo_erro"]
        assert e["palmas"]["pedida"] and not e["palmas"]["ouvindo"]


def test_palmas_desligadas_por_padrao_e_acao_invalida_e_recusada(tmp_path):
    import pytest

    assert Settings(_env_file=None).clap_enabled is False
    with pytest.raises(ValueError, match="ORION_CLAP_ACTION"):
        Settings(clap_action="executar", _env_file=None)
    assert Settings(clap_action="rotina:bom-dia", _env_file=None).clap_action == "rotina:bom-dia"
