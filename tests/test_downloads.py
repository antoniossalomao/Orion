"""E9.4 (REGRAS.md: automação sua vale pelo arquivo concedido): organizador de Downloads com regras suas, concedidas pelo hash."""

import os
from pathlib import Path

import pytest
from fastapi.testclient import TestClient

from orion.app import create_app
from orion.config import Settings
from orion.downloads import EXEMPLO, Downloads
from orion.jobs import JobRunner
from orion.memory import MemoryStore
from orion.memory.ops import Operations
from orion.policy import PathGuard
from tests.fakes import FakeGateway, fala

TOKEN = "token-de-teste-com-16+"
AUTH = {"Authorization": f"Bearer {TOKEN}"}
AGORA = 1_800_000_000.0


@pytest.fixture
def mundo(tmp_path):
    baixados = tmp_path / "Downloads"
    docs = tmp_path / "Documents"
    baixados.mkdir()
    docs.mkdir()
    m = MemoryStore(tmp_path / "d.db")
    guard = PathGuard(
        protected_roots=(tmp_path / "Orion",), safe_roots=(baixados, docs), system_roots=()
    )
    arq = tmp_path / "regras.yaml"
    d = Downloads(arq, m, guard, pasta_padrao=baixados, clock=lambda: AGORA)
    yield d, baixados, docs, arq, m
    m.close()


def arquivo(pasta, nome, conteudo="x", idade=3600):
    p = pasta / nome
    p.write_text(conteudo)
    os.utime(p, (AGORA - idade, AGORA - idade))
    return p


REGRAS = """
regras:
  - {extensoes: [pdf], padrao: "fatura*", destino: DOCS/Faturas}
  - {extensoes: [jpg, PNG], destino: DOCS/Imagens}
"""


def regras(arq, docs, texto=REGRAS):
    arq.write_text(texto.replace("DOCS", str(docs)), encoding="utf-8")


def test_sem_regras_ou_sem_concessao_nada_se_move(mundo):
    d, baixados, docs, arq, _ = mundo
    arquivo(baixados, "fatura-10.pdf")
    assert d.rodar().status == "sem_regras" and d.estado()["existe"] is False
    regras(arq, docs)
    assert d.estado()["estado"] == "sem_concessao"
    assert d.rodar().status == "nao_concedida"
    assert (baixados / "fatura-10.pdf").exists()
    d.conceder()
    regras(arq, docs, REGRAS + "# editado\n")
    assert d.estado()["estado"] == "mudou" and d.rodar().status == "nao_concedida"
    d.conceder()
    assert d.rodar().status == "ok"


def test_move_so_o_que_casa_e_nunca_sobrescreve(mundo):
    d, baixados, docs, arq, _ = mundo
    regras(arq, docs)
    d.conceder()
    arquivo(baixados, "fatura-10.pdf", "novo")
    arquivo(baixados, "outro.pdf")  # extensão casa, o padrão não
    arquivo(baixados, "foto.PNG")
    arquivo(baixados, "foto.jpg")
    arquivo(baixados, "livro.epub")
    (docs / "Faturas").mkdir()
    (docs / "Faturas" / "fatura-10.pdf").write_text("antigo")
    (baixados / "subpasta").mkdir()
    arquivo(baixados, ".oculto.pdf")
    r = d.rodar()
    assert sorted(Path(m["para"]).name for m in r.movidos) == [
        "fatura-10 (2).pdf", "foto.PNG", "foto.jpg"
    ]  # fmt: skip
    assert (docs / "Faturas" / "fatura-10.pdf").read_text() == "antigo"  # nunca sobrescreveu
    assert (docs / "Faturas" / "fatura-10 (2).pdf").read_text() == "novo"
    assert sorted(p.name for p in baixados.iterdir()) == [
        ".oculto.pdf",
        "livro.epub",
        "outro.pdf",
        "subpasta",
    ]
    assert r.ficaram == 2


def test_nao_mexe_em_download_em_andamento_nem_em_arquivo_recente_nem_em_link(mundo, tmp_path):
    d, baixados, docs, arq, _ = mundo
    regras(arq, docs)
    d.conceder()
    arquivo(baixados, "fatura-1.pdf.crdownload")
    arquivo(baixados, "fatura-2.pdf", idade=5)  # ainda sendo escrito
    alvo = arquivo(tmp_path, "segredo.pdf")
    (baixados / "fatura-3.pdf").symlink_to(alvo)
    r = d.rodar()
    assert r.movidos == [] and alvo.exists() and (baixados / "fatura-3.pdf").is_symlink()


def test_destino_proibido_e_recusado_pela_politica_de_escrita(mundo, tmp_path):
    d, baixados, docs, arq, _ = mundo
    arq.write_text(
        f"regras:\n  - {{extensoes: [pdf], destino: {tmp_path / 'Orion'}}}\n"
        f"  - {{extensoes: [ps1], destino: {docs}}}\n  - {{extensoes: [txt], destino: /etc}}\n",
        encoding="utf-8",
    )
    d.conceder()
    arquivo(baixados, "a.pdf")
    arquivo(baixados, "b.ps1")
    arquivo(baixados, "c.txt")
    r = d.rodar()
    assert r.movidos == [] and len(r.recusados) == 3
    assert any("núcleo" in x for x in r.recusados) and any("sensível" in x for x in r.recusados)
    assert not (tmp_path / "Orion").exists()


def test_simular_nao_move_e_desfazer_volta_so_o_que_esta_no_lugar(mundo):
    d, baixados, docs, arq, _ = mundo
    regras(arq, docs)
    d.conceder()
    a = arquivo(baixados, "fatura-1.pdf")
    b = arquivo(baixados, "fatura-2.pdf")
    sim = d.rodar(simular=True)
    assert len(sim.movidos) == 2 and a.exists() and d.historico() == []
    d.rodar()
    assert not a.exists() and not b.exists() and len(d.historico()) == 2
    r = d.desfazer("ultimo")
    assert len(r["desfeitos"]) == 1 and (a.exists() ^ b.exists())
    # o outro: alguém ocupou o lugar de origem, então não volta (e nada é sobrescrito)
    pendente = b if not b.exists() else a
    pendente.write_text("ocupado")
    r = d.desfazer("hoje")
    assert r["desfeitos"] == [] and any("ocupado" in p for p in r["pulados"])
    assert pendente.read_text() == "ocupado"
    pendente.unlink()
    assert len(d.desfazer("hoje")["desfeitos"]) == 1
    assert d.desfazer("hoje") == {"desfeitos": [], "pulados": []}
    with pytest.raises(ValueError):
        d.desfazer("tudo")


def test_arquivo_de_regras_invalido_explica_e_nao_concede(mundo):
    d, baixados, _docs, arq, _ = mundo
    for ruim in (
        "regras: []",
        "regras: [{destino: x}]\nextra: 1",
        "[1, 2]",
        "regras: [{padrao: 'a/b', destino: x}]",
    ):
        arq.write_text(ruim, encoding="utf-8")
        assert d.estado()["erro"] and d.estado()["estado"] == "sem_concessao"
        with pytest.raises(ValueError):
            d.conceder()
    arq.write_text("regras: [{destino: x}]", encoding="utf-8")  # regra sem condição não casa nada
    d.conceder()
    arquivo(baixados, "qualquer.bin")
    assert d.rodar().movidos == []


def test_exemplo_e_valido(mundo):
    d, _, _, arq, _ = mundo
    arq.write_text(EXEMPLO, encoding="utf-8")
    assert d.estado()["erro"] is None and d.estado()["regras"] == 2


def test_job_roda_o_organizador_e_respeita_o_panico(mundo):
    d, baixados, docs, arq, m = mundo
    regras(arq, docs)
    d.conceder()
    arquivo(baixados, "fatura-1.pdf")
    rel_tempo = [AGORA]

    class Modos:
        def __init__(self, p):
            self.p = p

        def panico(self):
            return self.p

        def nao_perturbe(self):
            return False

    jobs = JobRunner(m, Operations(m), downloads=d, modos=Modos(True), clock=lambda: rel_tempo[0])
    rel = type("R", (), {"downloads": 0, "erros": []})()
    jobs._passos_sincronos(rel)
    assert rel.downloads == 0 and (baixados / "fatura-1.pdf").exists()  # pânico: nada se mexe
    jobs._modos = Modos(False)
    rel_tempo[0] += 3600
    jobs._passos_sincronos(rel)
    assert rel.downloads == 1 and not (baixados / "fatura-1.pdf").exists()


def test_rotas_da_api(tmp_path):
    settings = Settings(
        data_dir=tmp_path / "d", admin_token=TOKEN, jobs_enabled=False, _env_file=None
    )
    pasta = tmp_path / "Downloads"
    pasta.mkdir()
    (tmp_path / "d").mkdir()
    with TestClient(
        create_app(settings, gateway_factory=lambda _: FakeGateway(fala("oi"))),
        base_url="http://127.0.0.1",
    ) as c:
        assert c.get("/downloads").status_code == 401
        e = c.get("/downloads", headers=AUTH).json()
        assert e["existe"] is False and e["estado"] == "sem_concessao"
        assert c.post("/downloads/conceder", headers=AUTH).status_code == 422
        (tmp_path / "d" / "regras_downloads.yaml").write_text(
            f"pasta: {pasta}\nregras:\n  - {{extensoes: [pdf], destino: {pasta}/pdfs}}\n",
            encoding="utf-8",
        )
        assert c.post("/downloads/conceder", headers=AUTH).json()["estado"] == "concedida"
        assert c.post("/downloads/desfazer", json={"escopo": "x"}, headers=AUTH).status_code == 422
        r = c.post("/downloads/rodar?simular=true", headers=AUTH).json()
        assert r["status"] in ("ok", "erro") and r["simulado"] is True
        assert c.post("/downloads/desfazer", json={}, headers=AUTH).json() == {
            "desfeitos": [],
            "pulados": [],
        }
        assert c.post("/downloads/revogar", headers=AUTH).json() == {"ok": True}
        assert c.post("/downloads/rodar", headers=AUTH).json()["status"] == "nao_concedida"
