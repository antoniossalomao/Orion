import json
from pathlib import Path

import pytest
from fastapi.testclient import TestClient

from orion.app import create_app, gateway_from_settings
from orion.config import Settings
from tests.fakes import FakeGateway, chama, fala, pede

TOKEN = "token-de-teste-com-16+"
AUTH = {"Authorization": f"Bearer {TOKEN}"}


def cliente(tmp_path, *roteiros, gateway=True):
    settings = Settings(data_dir=tmp_path / "d", admin_token=TOKEN, _env_file=None)
    gw = FakeGateway(*roteiros)
    app = create_app(settings, gateway_factory=(lambda _: gw) if gateway else (lambda _: None))
    return TestClient(app, base_url="http://127.0.0.1"), gw


def linhas_sse(resp) -> list[str]:
    return [ln[6:] for ln in resp.iter_lines() if ln.startswith("data: ")]


def eventos(resp) -> list[dict | str]:
    return [json.loads(x) if x != "[DONE]" else x for x in linhas_sse(resp)]


def test_chat_exige_token_e_valida_o_corpo(tmp_path):
    with cliente(tmp_path, fala("x"))[0] as c:
        assert c.post("/chat", json={"texto": "oi"}).status_code == 401
        for corpo in ({"texto": ""}, {"texto": "x" * 8001}, {"texto": "oi", "canal": "../x"}, {}):
            assert c.post("/chat", json=corpo, headers=AUTH).status_code == 422


def test_chat_503_sem_gateway_e_health_informa(tmp_path):
    with cliente(tmp_path, gateway=False)[0] as c:
        assert c.post("/chat", json={"texto": "oi"}, headers=AUTH).status_code == 503
        assert c.get("/health").json()["components"]["gateway"] is False


def test_chat_em_streaming_no_formato_do_legado(tmp_path):
    c, gw = cliente(tmp_path, fala("Olá, Antônio."))
    with c:
        assert c.get("/health").json()["components"]["gateway"] is True
        with c.stream(
            "POST", "/chat", json={"texto": "oi", "canal": "telegram"}, headers=AUTH
        ) as r:
            assert r.headers["content-type"].startswith("text/event-stream")
            ev = eventos(r)
    assert ev[:2] == [{"text": "Olá, Antônio."}, {"tier": "omni/modelo-x"}]
    assert "provenance" in ev[2] and ev[-1] == "[DONE]"
    assert gw.chamadas[0][-1] == {"role": "user", "content": "oi"}


def test_o_telegram_do_legado_le_o_novo_chat_sem_mudar(tmp_path, monkeypatch):
    """O endpoint novo é um substituto direto: o bot de hoje extrai o texto certo."""
    monkeypatch.syspath_prepend(str(Path(__file__).parents[1] / "Orion_Ollama"))
    import orion_telegram

    c, _ = cliente(
        tmp_path,
        fala("Resposta "),
    )
    with c, c.stream("POST", "/chat", json={"texto": "oi"}, headers=AUTH) as r:
        assert orion_telegram._extrair_texto(r.iter_lines()) == "Resposta"


def test_fluxo_de_aprovacao_ponta_a_ponta_pelo_http(tmp_path):
    c, _ = cliente(
        tmp_path, pede(chama("esquecer_fato", id=1)), fala("Aguardando você."), fala("Esqueci.")
    )
    with c:
        c.app.state.orion.memory.add_fact("Antônio gosta de café", "manual")
        with c.stream("POST", "/chat", json={"texto": "esqueça isso"}, headers=AUTH) as r:
            ev = eventos(r)
        aprov = next(e["approval"] for e in ev if isinstance(e, dict) and "approval" in e)
        assert aprov["tool"] == "esquecer_fato" and aprov["args"] == {"id": 1}
        assert len(c.app.state.orion.memory.facts()) == 1

        (pendente,) = c.get("/approvals", headers=AUTH).json()
        assert pendente["id"] == aprov["id"]
        # aprovar sem token não vale; com token vale
        assert (
            c.post(f"/approvals/{aprov['id']}/decide", json={"approved": True}).status_code == 401
        )
        c.post(
            f"/approvals/{aprov['id']}/decide",
            json={"approved": True, "channel": "web"},
            headers=AUTH,
        )

        with c.stream(
            "POST", f"/approvals/{aprov['id']}/resume", json={"canal": "web"}, headers=AUTH
        ) as r:
            ev2 = eventos(r)
        assert {"text": "Esqueci."} in ev2 and ev2[-1] == "[DONE]"
        assert c.app.state.orion.memory.facts() == []
        # uso único: retomar de novo só devolve erro
        with c.stream(
            "POST", f"/approvals/{aprov['id']}/resume", json={"canal": "web"}, headers=AUTH
        ) as r:
            assert "error" in eventos(r)[0]


def test_resume_exige_token_e_aprovacao_existente(tmp_path):
    c, _ = cliente(tmp_path)
    with c:
        assert c.post("/approvals/x/resume", json={}).status_code == 401
        with c.stream("POST", "/approvals/inexistente/resume", json={}, headers=AUTH) as r:
            ev = eventos(r)
        assert "error" in ev[0] and ev[-1] == "[DONE]"


def test_erro_interno_do_turno_vira_evento_e_nao_derruba_o_stream(tmp_path):
    c, _ = cliente(tmp_path, RuntimeError("bug"))
    with c, c.stream("POST", "/chat", json={"texto": "oi"}, headers=AUTH) as r:
        ev = eventos(r)
    assert ev[0] == {"error": "falha interna no turno"} and ev[-1] == "[DONE]"


@pytest.mark.parametrize(
    ("url", "modelo", "esperado"),
    [("", "", False), ("http://x/v1", "", False), ("http://x/v1", "m", True)],
)
def test_gateway_so_liga_com_url_e_modelo(url, modelo, esperado):
    s = Settings(gateway_url=url, gateway_model=modelo, gateway_api_key="k", _env_file=None)
    assert (gateway_from_settings(s) is not None) is esperado
