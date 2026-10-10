import json

from fastapi.testclient import TestClient

from orion.app import create_app
from orion.config import Settings
from orion.research import ResearchConfig
from orion.research.http import Response
from tests.fakes import FakeGateway, chama, fala, pede

TOKEN = "fixture-research-profile-admin"
AUTH = {"Authorization": f"Bearer {TOKEN}"}


def test_bundled_profile_two_skills_empty_and_contradictory_sources(monkeypatch, tmp_path):
    sources = [
        {
            "title": "Fonte A",
            "url": "https://example.com/a",
            "description": "O valor publicado é 10.",
        },
        {
            "title": "Fonte B",
            "url": "https://example.com/b",
            "description": "O valor publicado é 20 para outro período.",
        },
    ]
    responses = [sources, []]
    calls = []

    def controlled_get(self, url, **kwargs):
        calls.append(url)
        return Response(200, {}, json.dumps({"web": {"results": responses.pop(0)}}).encode(), url)

    monkeypatch.setattr("orion.research.provider.get_secret", lambda _: "fixture-only-key")
    monkeypatch.setattr("orion.research.http.PublicHTTP.get", controlled_get)
    gateway = FakeGateway(
        pede(chama("pesquisar_internet", consulta="fontes A B")),
        fala("As fontes usam períodos diferentes."),
        pede(chama("pesquisar_internet", consulta="sem evidência")),
        fala("Não foi possível concluir."),
    )
    app = create_app(
        Settings(
            data_dir=tmp_path,
            admin_token=TOKEN,
            jobs_enabled=False,
            research=ResearchConfig(enabled=True),
            _env_file=None,
        ),
        gateway_factory=lambda _: gateway,
    )
    with TestClient(app, base_url="http://127.0.0.1") as client:
        assert client.get("/plugins/available").status_code == 401
        assert client.post("/plugins/builtin/unknown", headers=AUTH).status_code == 422
        listed = client.get("/plugins/available", headers=AUTH).json()[0]
        row = client.post("/plugins/builtin/orion-pesquisa", headers=AUTH).json()
        assert row["state"] == "disabled" and row["origin"] == "orion"
        review = {"digest": row["selected_digest"], "capabilities": listed["capabilities"]}
        assert (
            client.post("/plugins/orion-pesquisa/activate", json=review, headers=AUTH).json()[
                "state"
            ]
            == "active"
        )
        skills = client.get("/skills", headers=AUTH).json()
        assert len(skills) == 2 and all(skill["enabled"] for skill in skills)
        runtime = app.state.orion.skills
        assert not runtime.select("bom dia escreva um poema").skills
        for name in ["comparar-fontes", "pesquisar-assunto"]:
            result = client.post(
                "/chat",
                json={"texto": "Compare as duas fontes", "skills": [f"orion-pesquisa:{name}"]},
                headers=AUTH,
            )
            assert result.status_code == 200 and '"provenance"' in result.text
        assert len(calls) == 2 and all(
            url.startswith("https://api.search.brave.com/res/v1/web/search?") for url in calls
        )
        tool_messages = [
            message["content"]
            for turn in gateway.chamadas
            for message in turn
            if message["role"] == "tool"
        ]
        assert any(
            "https://example.com/a" in message and "https://example.com/b" in message
            for message in tool_messages
        )
        assert any('"fontes": []' in message for message in tool_messages)
        schemas = [schema["function"]["name"] for schema in gateway.ferramentas[0]]
        assert set(schemas) == {"pesquisar_internet", "buscar_url"}
        assert (
            app.state.orion.extensions.store.get("orion-pesquisa")["active_digest"]
            == row["selected_digest"]
        )


def test_profile_waits_when_provider_is_disabled(tmp_path):
    app = create_app(
        Settings(data_dir=tmp_path, admin_token=TOKEN, jobs_enabled=False, _env_file=None),
        gateway_factory=lambda _: FakeGateway(),
    )
    with TestClient(app, base_url="http://127.0.0.1") as client:
        row = client.post("/plugins/builtin/orion-pesquisa", headers=AUTH).json()
        result = client.post(
            "/plugins/orion-pesquisa/activate",
            json={"digest": row["selected_digest"], "capabilities": row["capabilities"]},
            headers=AUTH,
        ).json()
        assert (
            result["state"] == "waiting_connection" and result["error"] == "capability_unavailable"
        )
        assert not client.get("/skills", headers=AUTH).json()
