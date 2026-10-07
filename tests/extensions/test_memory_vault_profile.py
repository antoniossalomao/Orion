from fastapi.testclient import TestClient

from orion.app import create_app
from orion.config import Settings
from orion.memory.scope import data_scope
from orion.projects import Projects
from tests.fakes import FakeGateway, chama, fala, pede
from tests.projects.test_projects import AUTH, TOKEN


def test_memory_vault_sources_project_canaries_no_write_or_new_mcp(tmp_path):
    gateway = FakeGateway(
        pede(chama("buscar_memoria", consulta="AMBER contexto")),
        fala("Retomada com fonte."),
        pede(chama("buscar_memoria", consulta="AMBER contexto")),
        fala("Síntese com fonte."),
        pede(chama("salvar_memoria", texto="WRITE_INFERIDA", fonte="inferência")),
        fala("A escrita fora do escopo foi recusada."),
    )
    app = create_app(
        Settings(data_dir=tmp_path / "data", admin_token=TOKEN, jobs_enabled=False, _env_file=None),
        gateway_factory=lambda _: gateway,
    )
    with TestClient(app, base_url="http://127.0.0.1") as c:
        s = app.state.orion
        vault = tmp_path / "vault"
        vault.mkdir()
        note = vault / "nota.md"
        note.write_text("# AMBER contexto\n\nDecisão registrada com fonte.")
        before = note.read_bytes()
        a = Projects(s.memory).create("A", root=str(vault))["id"]
        b = Projects(s.memory).create("B")["id"]
        with data_scope(a, include_personal=False):
            s.memory.index_document("nota.md", "Nota A", note.read_text())
            s.memory.add_fact("AMBER decisão do projeto", "nota.md")
        with data_scope(b, include_personal=False):
            s.memory.add_fact("BLUE privada", "nota B")
        s.memory.add_fact("PRIVATE pessoal", "nota pessoal")
        session = s.memory.new_session("web", project_id=a)
        package = c.post("/plugins/builtin/orion-memoria-vault", headers=AUTH).json()
        review = {
            "digest": package["selected_digest"],
            "capabilities": package["capabilities"],
            "scope": "project:" + a,
        }
        assert (
            c.post("/plugins/orion-memoria-vault/activate", headers=AUTH, json=review).json()[
                "state"
            ]
            == "active"
        )
        for skill in ["retomar-contexto", "sintetizar-notas"]:
            response = c.post(
                "/chat",
                headers=AUTH,
                json={"texto": "AMBER contexto", "skills": ["orion-memoria-vault:" + skill]},
            )
            assert response.status_code == 200
        response = c.post(
            "/chat",
            headers=AUTH,
            json={
                "texto": "Sintetize as notas",
                "skills": ["orion-memoria-vault:sintetizar-notas"],
            },
        )
        assert response.status_code == 200
        assert s.memory.query("SELECT count(*) FROM facts WHERE text='WRITE_INFERIDA'")[0][0] == 0
        inputs = str(gateway.chamadas)
        assert "BLUE privada" not in inputs and "PRIVATE pessoal" not in inputs
        assert "nota.md" in inputs and "Decisão registrada com fonte." in inputs
        assert all(
            schema["function"]["name"] in {"buscar_memoria", "listar_fatos"}
            for tools in gateway.ferramentas
            for schema in tools
        )
        assert note.read_bytes() == before
        assert not s.mcp.connections
        assert s.memory.get_session(session.id).project_id == a
        assert c.post("/plugins/orion-memoria-vault/deactivate", headers=AUTH).status_code == 200
        with data_scope(a, include_personal=False):
            assert s.memory.search("AMBER")
        assert note.read_bytes() == before
