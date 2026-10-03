"""O app serve a própria interface em /ui/ (mesma origem → funciona no celular, sem CORS)."""

import base64
import hashlib
import re

import pytest
from fastapi.testclient import TestClient

from orion.app import FRONT_DIR, create_app
from orion.config import Settings


@pytest.fixture
def client(tmp_path):
    s = Settings(data_dir=tmp_path / "dados", _env_file=None)
    with TestClient(create_app(s), base_url="http://127.0.0.1") as c:
        yield c


def test_ui_serve_o_index_e_os_assets(client):
    r = client.get("/ui/")
    assert r.status_code == 200 and "<title>Orion</title>" in r.text
    assert client.get("/ui/js/app.js").status_code == 200
    assert client.get("/ui/css/tokens.css").status_code == 200
    assert client.get("/ui/orion.svg").headers["content-type"].startswith("image/svg")


@pytest.mark.parametrize(
    "caminho", ["orion_app.py", "ponte.py", "__pycache__/x.pyc", "nao-existe.js"]
)
def test_ui_nao_serve_codigo_python_nem_o_que_nao_existe(client, caminho):
    assert client.get(f"/ui/{caminho}").status_code == 404


def test_api_tem_prioridade_sobre_o_mount(client):
    assert client.get("/health").json()["status"] == "ok"


def test_serve_ui_desligavel(tmp_path):
    s = Settings(data_dir=tmp_path / "d2", serve_ui=False, _env_file=None)
    with TestClient(create_app(s), base_url="http://127.0.0.1") as c:
        assert c.get("/ui/").status_code == 404


def test_csp_do_index_libera_exatamente_o_script_inline_de_preferencias():
    """Mexeu no script que aplica o tema antes da 1ª pintura? Atualize o hash da CSP no index.html."""
    html = (FRONT_DIR / "index.html").read_text(encoding="utf-8")
    inline = re.findall(r"<script>(.*?)</script>", html, re.S)
    assert len(inline) == 1, "só o script de preferências pode ser inline (a CSP não deixa outros)"
    hash_real = base64.b64encode(hashlib.sha256(inline[0].encode()).digest()).decode()
    csp = re.search(r'http-equiv="Content-Security-Policy" content="([^"]+)"', html).group(1)
    assert f"'sha256-{hash_real}'" in csp
    assert "script-src 'self'" in csp and "'unsafe-inline'" not in csp.split("style-src")[0]
    assert "object-src 'none'" in csp and "base-uri 'none'" in csp
