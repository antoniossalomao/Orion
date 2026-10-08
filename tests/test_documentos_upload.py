"""C38/C32: documentos enviados pela interface entram na memória; os de projeto ficam no projeto."""

import io

import pytest
from fastapi.testclient import TestClient

from orion.app import create_app
from orion.config import Settings
from orion.memory import MemoryStore

TOKEN = "token-de-teste-com-16+"
AUTH = {"Authorization": f"Bearer {TOKEN}"}
TEXTO = (
    "A norma ABNT NBR 6023 define como citar referências bibliográficas em trabalhos "
    "acadêmicos, com autor, título, edição e ano de publicação."
)


@pytest.fixture
def c(tmp_path):
    settings = Settings(data_dir=tmp_path / "d", admin_token=TOKEN, _env_file=None)
    with TestClient(
        create_app(settings, gateway_factory=lambda _: None), base_url="http://127.0.0.1"
    ) as cli:
        yield cli


def mem(c):
    return c.app.state.orion.memory


def enviar(c, nome, conteudo, **dados):
    return c.post(
        "/memoria/documentos",
        files={"arquivo": (nome, conteudo)},
        data={k: str(v) for k, v in dados.items()},
        headers=AUTH,
    )


def test_exige_login(c):
    assert c.get("/memoria/documentos").status_code == 401
    assert c.post("/memoria/documentos", files={"arquivo": ("a.txt", b"x")}).status_code == 401
    assert c.delete("/memoria/documentos/1").status_code == 401


def test_envia_texto_lista_acha_na_busca_e_apaga(c):
    r = enviar(c, "norma abnt.md", TEXTO.encode())
    assert r.status_code == 201
    corpo = r.json()
    assert (
        corpo["resultado"] == "new" and corpo["trechos"] >= 1 and corpo["caracteres"] == len(TEXTO)
    )
    lista = c.get("/memoria/documentos", headers=AUTH).json()["documentos"]
    assert [d["nome"] for d in lista] == ["norma abnt.md"] and lista[0]["projeto_id"] is None
    assert any("NBR 6023" in h.text for h in mem(c).search("referências bibliográficas"))
    # mesmo conteúdo de novo: nada refeito
    assert enviar(c, "norma abnt.md", TEXTO.encode()).json()["resultado"] == "same"
    assert c.delete(f"/memoria/documentos/{corpo['id']}", headers=AUTH).json() == {"ok": True}
    assert c.delete(f"/memoria/documentos/{corpo['id']}", headers=AUTH).status_code == 404
    assert not any("NBR 6023" in h.text for h in mem(c).search("referências bibliográficas"))


def test_docx_e_lido_e_tipo_invalido_vazio_e_sem_texto_sao_recusados(c):
    from docx import Document

    buf = io.BytesIO()
    doc = Document()
    doc.add_paragraph(TEXTO)
    doc.save(buf)
    assert enviar(c, "trabalho.docx", buf.getvalue()).status_code == 201
    assert enviar(c, "programa.exe", b"MZ....").status_code == 415
    assert enviar(c, "../../etc/passwd.txt", b"").status_code == 422
    assert enviar(c, "curto.txt", b"oi").status_code == 422
    assert enviar(c, "quebrado.pdf", b"isto nao e um pdf").status_code == 422
    # o nome com caminho vira só o nome do arquivo
    assert enviar(c, "../../etc/outro.txt", TEXTO.encode()).status_code == 201
    nomes = {d["nome"] for d in c.get("/memoria/documentos", headers=AUTH).json()["documentos"]}
    assert nomes == {"trabalho.docx", "outro.txt"}


def test_arquivo_grande_demais_e_recusado(c, monkeypatch):
    monkeypatch.setattr("orion.app.MAX_ARQUIVO", 100)
    assert enviar(c, "grande.txt", b"x" * 500).status_code == 413


def test_documento_de_projeto_so_aparece_no_contexto_do_projeto(c):
    m = mem(c)
    p = m.create_project("TCC")
    assert enviar(c, "tcc.md", TEXTO.encode(), projeto_id=p.id).status_code == 201
    assert enviar(c, "tcc.md", TEXTO.encode(), projeto_id=999).status_code == 404

    def achou(**kw):
        return [h for h in m.search("referências bibliográficas", **kw) if h.kind == "chunk"]

    assert achou(project_id=p.id) and achou()  # o projeto vê; a busca explícita (padrão) vê tudo
    assert achou(project_id=None) == []  # conversa sem projeto: não entra no contexto automático
    assert achou(project_id=p.id + 1) == []  # outro projeto também não
    assert c.get("/memoria/documentos", headers=AUTH).json()["documentos"][0]["projeto_id"] == p.id


def test_documento_global_aparece_em_qualquer_projeto(c):
    m = mem(c)
    p = m.create_project("TCC")
    enviar(c, "geral.md", TEXTO.encode())
    assert [h for h in m.search("NBR 6023", project_id=p.id) if h.kind == "chunk"]
    assert [h for h in m.search("NBR 6023", project_id=None) if h.kind == "chunk"]


def test_apagar_o_projeto_mantem_o_documento_como_global(c):
    m = mem(c)
    p = m.create_project("Velho")
    enviar(c, "x.md", TEXTO.encode(), projeto_id=p.id)
    m.delete_project(p.id)
    assert c.get("/memoria/documentos", headers=AUTH).json()["documentos"][0]["projeto_id"] is None


def test_nota_do_vault_nao_e_apagavel_por_aqui(c):
    m = mem(c)
    m.index_document("Projetos/x.md", "x", TEXTO)
    doc = m.query("SELECT id FROM documents WHERE source='Projetos/x.md'")[0]["id"]
    assert c.delete(f"/memoria/documentos/{doc}", headers=AUTH).status_code == 404
    assert c.get("/memoria/documentos", headers=AUTH).json()["total"] == 0


def test_migracao_v8_para_v9(tmp_path):
    db = tmp_path / "m.db"
    m = MemoryStore(db)
    m.index_document("upload:a.md", "a", TEXTO)
    m._conn.executescript(
        "DROP TABLE artifacts;"
        "ALTER TABLE documents DROP COLUMN project_id;"
        "UPDATE meta SET value='8' WHERE key='schema_version';"
    )
    m._conn.close()
    m2 = MemoryStore(db)
    assert m2.list_documents()[0]["project_id"] is None
    assert m2.index_document("upload:b.md", "b", TEXTO + " outro", project_id=None) == "new"
