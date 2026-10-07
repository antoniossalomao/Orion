import base64
import io

import pytest
from fastapi.testclient import TestClient
from PIL import Image

from orion.app import create_app
from orion.artifacts import MAX_CONTENT, ArtifactError, Artifacts, Payload
from orion.config import Settings
from orion.memory import MemoryStore
from orion.projects import Projects
from tests.projects.test_projects import AUTH, TOKEN


def test_artifacts_versions_scope_provenance_conflict_restart_download(tmp_path):
    settings = Settings(data_dir=tmp_path, admin_token=TOKEN, jobs_enabled=False, _env_file=None)
    with TestClient(create_app(settings), base_url="http://127.0.0.1") as client:
        memory = client.app.state.orion.memory
        project = Projects(memory).create("A")["id"]
        other = Projects(memory).create("B")["id"]
        session = memory.new_session("web", project_id=project)
        message = memory.add_message(
            session.id, "assistant", "resultado", provenance={"fontes": ["fixture"]}
        )
        payload = {
            "title": "Análise",
            "session_id": session.id,
            "content": "# AMBER",
            "message_id": message.id,
        }
        assert client.get("/artifacts").status_code == 401
        result = client.post(
            "/artifacts", params={"project_id": project}, json=payload, headers=AUTH
        )
        assert result.status_code == 200
        id_ = result.json()["id"]
        assert client.post("/plugins/builtin/orion-pesquisa", headers=AUTH).status_code == 200
        assert client.delete("/plugins/orion-pesquisa", headers=AUTH).status_code == 200
        other_session = memory.new_session("web", project_id=other)
        assert (
            client.post(
                "/artifacts",
                params={"project_id": project},
                json={**payload, "session_id": other_session.id},
                headers=AUTH,
            ).status_code
            == 409
        )
        assert client.get(f"/artifacts/{id_}", headers=AUTH).status_code == 404
        assert (
            client.get(f"/artifacts/{id_}", params={"project_id": other}, headers=AUTH).status_code
            == 404
        )
        url = f"/artifacts/{id_}/versions?project_id={project}"
        assert (
            client.post(url, json={**payload, "content": "# BLUE"}, headers=AUTH).status_code == 409
        )
        revised = client.post(
            url, json={**payload, "content": "# revisão", "expected_version": 1}, headers=AUTH
        )
        assert revised.status_code == 200 and revised.json()["version"] == 2
        assert (
            client.post(url, json={**payload, "expected_version": 1}, headers=AUTH).status_code
            == 409
        )
        versions = client.get(url, headers=AUTH).json()
        assert len(versions) == 2 and versions[0]["provenance"] == {"fontes": ["fixture"]}
        assert (
            client.post(
                "/artifacts", json={**payload, "title": "../escape"}, headers=AUTH
            ).status_code
            == 422
        )
        assert client.get("/artifacts/../../secret/download", headers=AUTH).status_code == 404
    with TestClient(create_app(settings), base_url="http://127.0.0.1") as client:
        downloaded = client.get(
            f"/artifacts/{id_}/download?project_id={project}&version=1", headers=AUTH
        )
        assert downloaded.content == b"# AMBER"
        assert downloaded.headers["x-content-type-options"] == "nosniff"
        assert downloaded.headers["content-disposition"].startswith("attachment;")
        assert not client.get("/artifacts", headers=AUTH).json()
        assert (
            client.get("/artifacts", params={"project_id": project}, headers=AUTH).json()[0]["id"]
            == id_
        )


def test_artifact_images_normalized_limits_and_transaction_rollback(tmp_path):
    memory = MemoryStore(tmp_path / "a.db")
    try:
        session = memory.new_session("web")
        store = Artifacts(memory)
        image = io.BytesIO()
        Image.new("RGB", (2, 2), "red").save(image, format="JPEG")
        payload = Payload(
            title="Imagem",
            kind="image",
            content=base64.b64encode(image.getvalue()).decode(),
            session_id=session.id,
        )
        result = store.save(payload)
        data = store.read(result["id"], None)[1]["content"]
        assert data.startswith(b"\x89PNG")
        with Image.open(io.BytesIO(data)) as normalized:
            assert normalized.size == (2, 2) and not normalized.info
        with pytest.raises(ArtifactError, match="image_invalid"):
            store.save(
                payload.model_copy(
                    update={"content": base64.b64encode(b'<svg onload="alert(1)">').decode()}
                )
            )
        with pytest.raises(ArtifactError, match="size_limit"):
            store.save(
                Payload(title="Too large", content="a" * (MAX_CONTENT + 1), session_id=session.id)
            )
        assert len(store.list()) == 1
        with memory.transaction() as db:
            db.execute("UPDATE artifact_versions SET content=X'00'")
        with pytest.raises(ArtifactError, match="integrity_failed"):
            store.read(result["id"], None)
    finally:
        memory.close()
