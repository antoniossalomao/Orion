"""E4.2 e E4.3: OCR de PDF escaneado, progresso da indexação e reindexar."""

import io
import threading
import time

from fastapi.testclient import TestClient
from PIL import Image
from pypdf import PdfWriter

from orion import documents
from orion.app import create_app
from orion.config import Settings
from orion.document_extract import MIN_TEXTO_PAGINA, extract, ocr_imagem
from orion.memory import MemoryStore
from tests.projects.test_projects import AUTH, TOKEN


def pdf_so_imagem(paginas=2) -> bytes:
    """PDF "escaneado": só uma imagem por página, nenhum texto."""
    imagens = [Image.new("RGB", (120, 80), (255 - 40 * i, 255, 255)) for i in range(paginas)]
    buf = io.BytesIO()
    imagens[0].save(buf, "PDF", save_all=True, append_images=imagens[1:])
    return buf.getvalue()


def pdf_com_texto() -> bytes:
    w = PdfWriter()
    w.add_blank_page(200, 200)
    buf = io.BytesIO()
    w.write(buf)
    return buf.getvalue()


def test_pdf_so_imagem_sem_ocr_continua_pedindo_ocr():
    try:
        extract(pdf_so_imagem(), ".pdf")
    except ValueError as e:
        assert str(e) == "pdf_requires_ocr"
    else:
        raise AssertionError("devia pedir OCR")


def test_pdf_so_imagem_vira_texto_com_o_ocr_e_respeita_o_teto():
    lidas, andamento = [], []

    def ocr(img):
        lidas.append(img.name)
        assert img.is_file()  # a imagem existe enquanto o OCR lê
        return f"texto da pagina {len(lidas)} com palavras suficientes"

    texto = extract(pdf_so_imagem(3), ".pdf", ocr=ocr, max_ocr_pages=2, progress=andamento.append)
    assert "pagina 1" in texto and "pagina 2" in texto and "pagina 3" not in texto
    assert len(lidas) == 2  # ORION_OCR_MAX_PAGES
    assert andamento == [33, 67, 100]
    assert not any(
        __import__("pathlib").Path(p).exists() for p in lidas
    )  # pasta temporária apagada


def pdf_texto_real() -> bytes:
    from reportlab.pdfgen import canvas

    buf = io.BytesIO()
    c = canvas.Canvas(buf)
    c.drawString(72, 700, "Pagina com texto de verdade, nada de imagem aqui.")
    c.save()
    return buf.getvalue()


def test_pagina_com_texto_nao_gasta_ocr():
    chamou = []
    texto = extract(
        pdf_texto_real(), ".pdf", ocr=lambda i: chamou.append(i) or "x" * 50, max_ocr_pages=5
    )
    assert chamou == [] and "texto de verdade" in texto
    assert len("Pagina com texto de verdade") > MIN_TEXTO_PAGINA


def test_ocr_imagem_cai_para_ingles_sem_o_idioma():
    chamadas = []

    class R:
        def __init__(self, rc, out=b""):
            self.returncode, self.stdout = rc, out

    def runner(argv, **kw):
        chamadas.append(argv[-1])
        return R(1) if argv[-1] == "por+eng" else R(0, "olá mundo".encode())

    assert ocr_imagem(__import__("pathlib").Path("x.png"), tesseract="/bin/t", runner=runner) == (
        "olá mundo"
    )
    assert chamadas == ["por+eng", "eng"]


def cliente(tmp_path, **kw):
    s = Settings(data_dir=tmp_path, admin_token=TOKEN, jobs_enabled=False, _env_file=None, **kw)
    return TestClient(create_app(s), base_url="http://127.0.0.1")


def test_upload_pequeno_volta_pronto_e_a_lista_traz_o_progresso(tmp_path):
    with cliente(tmp_path) as c:
        r = c.post("/documents", params={"name": "a.md"}, content=b"# Oi\n\ntexto", headers=AUTH)
        assert r.json()["status"] == "ready" and r.json()["progress"] == 100
        [linha] = c.get("/documents", headers=AUTH).json()
        assert linha["progress"] == 100
        assert c.get(f"/documents/{r.json()['id']}", headers=AUTH).json()["status"] == "ready"
        assert c.get("/documents/naoexiste", headers=AUTH).status_code == 404


def test_upload_lento_segue_em_segundo_plano_e_o_front_acompanha_o_progresso(tmp_path, monkeypatch):
    liberar = threading.Event()

    def lento(raw, kind, *, ocr_paginas, ocr_langs, avisar):
        avisar(40)
        liberar.wait(5)
        return {"text": "conteúdo lento mas indexado"}

    monkeypatch.setattr(documents, "_extrair", lento)
    monkeypatch.setattr(documents, "ESPERA_S", 0.05)
    with cliente(tmp_path) as c:
        r = c.post("/documents", params={"name": "g.md"}, content=b"x", headers=AUTH).json()
        assert r["status"] == "processing"  # voltou antes de terminar
        for _ in range(100):
            if c.get(f"/documents/{r['id']}", headers=AUTH).json()["progress"] >= 30:
                break
            time.sleep(0.02)
        meio = c.get(f"/documents/{r['id']}", headers=AUTH).json()
        assert meio["status"] == "processing" and 30 <= meio["progress"] < 100
        # enquanto indexa, tentar de novo é recusado em vez de duplicar o trabalho
        assert c.post(f"/documents/{r['id']}/retry", headers=AUTH).status_code == 409
        liberar.set()
        for _ in range(200):
            fim = c.get(f"/documents/{r['id']}", headers=AUTH).json()
            if fim["status"] == "ready":
                break
            time.sleep(0.02)
        assert fim["status"] == "ready" and fim["progress"] == 100
        s = c.app.state.orion
        assert s.memory.search("conteúdo lento", kinds=("chunk",))


def test_reindexar_refaz_trechos_e_vetores_a_partir_do_original(tmp_path):
    with cliente(tmp_path) as c:
        s = c.app.state.orion
        r = c.post(
            "/documents", params={"name": "n.md"}, content=b"# T\n\nCanario-reindexar", headers=AUTH
        ).json()
        with s.memory.transaction() as t:  # estraga o índice: só reindexar do original conserta
            t.execute("UPDATE chunks SET text='CORROMPIDO'")
        assert not s.memory.search("Canario-reindexar", kinds=("chunk",))
        out = c.post(f"/documents/{r['id']}/reindex", headers=AUTH)
        assert out.status_code == 200 and out.json()["status"] == "ready"
        assert s.memory.search("Canario-reindexar", kinds=("chunk",))
        assert not s.memory.query("SELECT 1 FROM chunks WHERE text='CORROMPIDO'")
        assert c.post("/documents/zzz/reindex", headers=AUTH).status_code == 404


def test_pdf_escaneado_sem_tesseract_da_erro_claro_e_reindexar_tenta_de_novo(tmp_path):
    with cliente(tmp_path) as c:
        r = c.post(
            "/documents", params={"name": "scan.pdf"}, content=pdf_so_imagem(), headers=AUTH
        ).json()
        assert r["status"] == "error" and r["error"] == "pdf_requires_ocr"
        assert r["progress"] == 100


def test_queda_no_meio_da_indexacao_vira_erro_e_nao_barra_parada(tmp_path):
    m = MemoryStore(tmp_path / "q.db")
    try:
        with m.transaction() as c:
            c.execute(
                "INSERT INTO uploads(id,name,kind,raw,status,progress,created_at)"
                " VALUES ('u1','a.md','.md',x'41','processing',40,1),"
                " ('u2','b.md','.md',x'41','ready',100,1)"
            )
        assert documents.recuperar_interrompidos(m) == 1
        linhas = {r["id"]: r for r in m.query("SELECT id,status,error,progress FROM uploads")}
        assert linhas["u1"]["status"] == "error" and linhas["u1"]["error"] == "document_interrupted"
        assert linhas["u1"]["progress"] == 100 and linhas["u2"]["status"] == "ready"
    finally:
        m.close()


def test_pdf_escaneado_com_tesseract_de_verdade_na_cadeia_vira_texto_pesquisavel(
    tmp_path, monkeypatch
):
    """Ponta a ponta: servidor → extrator isolado → "tesseract" (um script que imita o programa)
    → texto indexado e achado na busca. A prova do PDF só de imagem da E4."""
    import os
    import sys

    import pytest

    if sys.platform == "win32":
        pytest.skip("o tesseract falso é um script de shell")
    bin_ = tmp_path / "bin"
    bin_.mkdir()
    falso = bin_ / "tesseract"
    falso.write_text(
        '#!/bin/sh\necho "Contrato de aluguel assinado em Marilia, cláusula Zeta-77"\n'
    )
    falso.chmod(0o755)
    monkeypatch.setenv("PATH", f"{bin_}{os.pathsep}{os.environ['PATH']}")
    with cliente(tmp_path / "dados") as c:
        r = c.post(
            "/documents", params={"name": "scan.pdf"}, content=pdf_so_imagem(2), headers=AUTH
        ).json()
        assert r["status"] == "ready", r
        achados = c.app.state.orion.memory.search("Zeta-77", kinds=("chunk",))
        assert achados and "Contrato de aluguel" in achados[0].text
    with cliente(tmp_path / "dados2", ocr_max_pages=0) as c:  # ORION_OCR_MAX_PAGES=0 desliga
        r = c.post(
            "/documents", params={"name": "scan.pdf"}, content=pdf_so_imagem(2), headers=AUTH
        ).json()
        assert r["status"] == "error" and r["error"] == "pdf_requires_ocr"
