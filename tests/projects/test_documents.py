import io

from fastapi.testclient import TestClient
from pypdf import PdfWriter

from orion.app import create_app
from orion.config import Settings
from orion.memory.scope import data_scope
from orion.projects import Projects
from tests.projects.test_projects import AUTH, TOKEN


def test_document_ingestion_scope_limits_invalid_recover_original(tmp_path):
    settings = Settings(data_dir=tmp_path, admin_token=TOKEN, jobs_enabled=False, _env_file=None)
    with TestClient(create_app(settings), base_url="http://127.0.0.1") as c:
        s = c.app.state.orion
        a, b = [Projects(s.memory).create(n)["id"] for n in ("A", "B")]
        assert c.get("/documents").status_code == 401
        params = {"name": "nota.md", "project_id": a}
        raw = b"# AMBER\n\nCanario-documento escopado."
        row = c.post("/documents", params=params, content=raw, headers=AUTH).json()
        assert row["status"] == "ready", row
        with data_scope(b, include_personal=False):
            assert not s.memory.search("Canario-documento")
        with data_scope(a, include_personal=False):
            assert s.memory.search("Canario-documento")
        assert (
            c.get(
                f"/documents/{row['id']}/download", params={"project_id": b}, headers=AUTH
            ).status_code
            == 404
        )
        assert (
            c.get(
                f"/documents/{row['id']}/download", params={"project_id": a}, headers=AUTH
            ).content
            == raw
        )
        assert (
            c.post(
                "/documents", params={**params, "name": "../key.txt"}, content=b"x", headers=AUTH
            ).status_code
            == 422
        )
        assert (
            c.post(
                "/documents", params=params, content=b"x" * (6 * 1024 * 1024 + 1), headers=AUTH
            ).status_code
            == 413
        )
        invalid = c.post(
            "/documents", params={**params, "name": "broken.pdf"}, content=b"not pdf", headers=AUTH
        ).json()
        assert invalid["status"] == "error"
        assert (
            c.post(
                f"/documents/{invalid['id']}/retry", params={"project_id": a}, headers=AUTH
            ).json()["status"]
            == "error"
        )
        assert (
            c.get(
                f"/documents/{invalid['id']}/download", params={"project_id": a}, headers=AUTH
            ).content
            == b"not pdf"
        )
        writer = PdfWriter()
        writer.add_blank_page(100, 100)
        buffer = io.BytesIO()
        writer.write(buffer)
        scanned = c.post(
            "/documents",
            params={**params, "name": "scan.pdf"},
            content=buffer.getvalue(),
            headers=AUTH,
        ).json()
        assert scanned["error"] == "pdf_requires_ocr"
        from pypdf import PdfReader

        from orion.document_extract import extract

        writer = PdfWriter()
        page = writer.add_blank_page(100, 100)
        from pypdf.generic import DecodedStreamObject, DictionaryObject, NameObject

        font = DictionaryObject(
            {
                NameObject("/Type"): NameObject("/Font"),
                NameObject("/Subtype"): NameObject("/Type1"),
                NameObject("/BaseFont"): NameObject("/Helvetica"),
            }
        )
        page[NameObject("/Resources")] = DictionaryObject(
            {NameObject("/Font"): DictionaryObject({NameObject("/F1"): font})}
        )
        stream = DecodedStreamObject()
        stream.set_data(b"BT /F1 12 Tf 10 50 Td (PDF AMBER) Tj ET")
        page[NameObject("/Contents")] = writer._add_object(stream)
        buffer = io.BytesIO()
        writer.write(buffer)
        assert len(PdfReader(buffer).pages) == 1
        valid = c.post(
            "/documents",
            params={**params, "name": "text.pdf"},
            content=buffer.getvalue(),
            headers=AUTH,
        ).json()
        assert valid["status"] == "ready", valid
        assert "PDF AMBER" in extract(buffer.getvalue(), ".pdf")
    with TestClient(create_app(settings), base_url="http://127.0.0.1") as c:
        assert len(c.get("/documents", params={"project_id": a}, headers=AUTH).json()) == 4
