"""E5.4 a E5.7: relações do sono, grafo da memória, caminho entre dois nós, wikilinks e busca global."""

import pytest
from fastapi.testclient import TestClient

from orion.app import create_app
from orion.config import Settings
from orion.grafo import achar_no, caminho, montar
from orion.memory import MemoryStore
from orion.memory.ops import Operations
from orion.memory.scope import data_scope
from orion.memory.text import wikilinks
from orion.projects import Projects
from tests.test_screen_memory import AUTH, TOKEN


@pytest.fixture
def mem(tmp_path):
    s = MemoryStore(tmp_path / "g.db")
    yield s
    s.close()


@pytest.fixture
def vault(tmp_path):
    v = tmp_path / "vault"
    (v / "01 Projetos" / "Orion").mkdir(parents=True)
    (v / "03 Recursos" / "Conceitos").mkdir(parents=True)
    (v / "01 Projetos" / "Orion" / "Orion.md").write_text(
        "---\ntype: projeto\n---\n# Orion\nUsa [[RAG]] e [[MCP|protocolo]]. Veja [[Nota que não existe]].\n"
    )
    (v / "03 Recursos" / "Conceitos" / "RAG.md").write_text(
        "# RAG\nBusca híbrida. Ligada a [[MCP]].\n"
    )
    (v / "03 Recursos" / "Conceitos" / "MCP.md").write_text("# MCP\nProtocolo.\n")
    (v / "03 Recursos" / "Conceitos" / "Ilha.md").write_text("# Ilha\nSem links.\n")
    return v


def test_wikilinks_pega_alias_secao_e_pasta():
    assert wikilinks("[[A]] [[B|apelido]] [[C#sec]] [[pasta/D]] ![[E.png]] [[ ]]") == {
        "A",
        "B",
        "C",
        "D",
        "E.png",
    }


def test_indexar_o_vault_cria_arestas_de_wikilink_e_refaz_a_cada_vez(mem, vault):
    mem.index_vault(vault)
    arestas = {
        (r["src"], r["dst"]) for r in mem.query("SELECT src, dst FROM edges WHERE kind='vault'")
    }
    assert arestas == {
        ("doc:01 Projetos/Orion/Orion.md", "doc:03 Recursos/Conceitos/RAG.md"),
        ("doc:01 Projetos/Orion/Orion.md", "doc:03 Recursos/Conceitos/MCP.md"),
        ("doc:03 Recursos/Conceitos/RAG.md", "doc:03 Recursos/Conceitos/MCP.md"),
    }
    (vault / "03 Recursos" / "Conceitos" / "RAG.md").write_text("# RAG\nSem link agora.\n")
    mem.index_vault(vault)
    assert mem.query("SELECT count(*) FROM edges WHERE kind='vault'")[0][0] == 2
    # projeto não mexe nas arestas do vault pessoal
    a = Projects(mem).create("A")["id"]
    with data_scope(a, include_personal=False):
        mem.index_vault(vault)
    assert mem.query("SELECT count(*) FROM edges WHERE kind='vault'")[0][0] == 2


def test_grafo_junta_fatos_sono_importado_e_notas(mem, vault):
    ops = Operations(mem)
    mem.index_vault(vault)
    f = mem.add_fact("Antônio estuda na Unimar em Marília", "m")
    ops.add_edge("Antônio", "estuda_em", "Unimar", kind="sono")
    s = mem.new_session("web")
    msg = mem.add_message(s.id, "user", "Em que cidade eu moro?")
    ops.add_edge(f"message:{msg.id}", "sobre", "topic:marília")
    ops.add_edge("message:99999", "sobre", "topic:fantasma")
    g = montar(mem, limite=None)
    ids = {n["id"]: n for n in g["nodes"]}
    assert ids[f"fato:{f.id}"]["tipo"] == "fato" and ids[f"fato:{f.id}"]["ref"] == f.id
    assert ids["entidade:Unimar"]["tipo"] == "topico" and ids["topic:marília"]["tipo"] == "topico"
    assert ids["doc:03 Recursos/Conceitos/RAG.md"]["tipo"] == "nota"
    assert ids["doc:03 Recursos/Conceitos/RAG.md"]["pasta"] == "03 Recursos/Conceitos"
    pares = {(lk["source"], lk["target"], lk["rel"]) for lk in g["links"]}
    assert ("entidade:Antônio", "entidade:Unimar", "estuda_em") in pares
    assert (f"fato:{f.id}", "entidade:Unimar", "cita") in pares  # o fato cita a Unimar
    assert (f"fato:{f.id}", "topic:marília", "cita") in pares
    assert "message:99999" not in ids and "topic:fantasma" not in ids  # fala que não existe
    assert ids[f"message:{msg.id}"]["tipo"] == "evento"
    assert all(lk["source"] in ids and lk["target"] in ids for lk in g["links"])


def test_grafo_do_legado_traz_as_falas_com_ator_e_data(mem):
    s = mem.new_session("web")
    m1 = mem.add_message(s.id, "user", "Em que cidade eu moro?")
    m2 = mem.add_message(s.id, "assistant", "Marília-SP.")
    ops = Operations(mem)
    ops.add_edge(f"message:{m1.id}", "precedeu", f"message:{m2.id}")
    ops.add_edge(f"message:{m1.id}", "sobre", "topic:cidade")
    g = montar(mem, limite=None)
    por = {n["id"]: n for n in g["nodes"]}
    assert (
        por[f"message:{m1.id}"]["tipo"] == "evento" and por[f"message:{m1.id}"]["ator"] == "Antônio"
    )
    assert por[f"message:{m2.id}"]["ator"] == "Orion" and por[f"message:{m2.id}"]["ts"] > 0
    assert por["topic:cidade"]["label"] == "cidade"


def test_projeto_so_ve_os_proprios_fatos_e_documentos(mem, vault):
    mem.index_vault(vault)
    Operations(mem).add_edge("Antônio", "usa", "Python", kind="sono")
    a = Projects(mem).create("A")["id"]
    mem.add_fact("fato pessoal", "m")
    with data_scope(a, include_personal=False):
        mem.add_fact("fato do projeto A", "m")
        mem.index_document("spec.md", "Spec", "texto da spec")
    g = montar(mem, project_id=a, limite=None)
    rotulos = {n["label"] for n in g["nodes"]}
    assert rotulos == {"fato do projeto A", "Spec"} and g["links"] == []
    pessoal = {n["label"] for n in montar(mem, limite=None)["nodes"]}
    assert "fato do projeto A" not in pessoal and "Spec" not in pessoal


def test_limite_corta_nos_menos_ligados_e_avisa(mem):
    ops = Operations(mem)
    for i in range(30):
        mem.add_fact(f"fato avulso número {i} sobre assuntos variados", "m")
    for i in range(5):
        ops.add_edge("Centro", f"r{i}", f"Folha{i}", kind="sono")
    g = montar(mem, limite=10)
    assert g["truncado"] is True and len(g["nodes"]) == 10
    assert {"entidade:Centro", "entidade:Folha0"} <= {n["id"] for n in g["nodes"]}
    assert not montar(mem, limite=None)["truncado"]


def test_filtro_por_pasta_do_vault(mem, vault):
    mem.index_vault(vault)
    g = montar(mem, limite=None, pasta="03 Recursos")
    docs = {n["id"] for n in g["nodes"] if n["tipo"] == "nota"}
    assert docs == {f"doc:03 Recursos/Conceitos/{n}.md" for n in ("RAG", "MCP", "Ilha")}
    assert all("Orion" not in lk["source"] for lk in g["links"])


def test_achar_no_e_caminho(mem, vault):
    mem.index_vault(vault)
    g = montar(mem, limite=None)
    nos = g["nodes"]
    assert achar_no(nos, "rag")["id"] == "doc:03 Recursos/Conceitos/RAG.md"
    assert achar_no(nos, "orion")["label"] == "Orion"
    assert achar_no(nos, "Ilha")["label"] == "Ilha"
    assert achar_no(nos, "xyzzy") is None and achar_no(nos, "  ") is None
    rota = caminho(g, "doc:01 Projetos/Orion/Orion.md", "doc:03 Recursos/Conceitos/MCP.md")
    assert rota == ["doc:01 Projetos/Orion/Orion.md", "doc:03 Recursos/Conceitos/MCP.md"]
    assert caminho(g, "doc:03 Recursos/Conceitos/Ilha.md", "doc:03 Recursos/Conceitos/RAG.md") == []
    assert caminho(g, "a", "a") == ["a"]


@pytest.fixture
def c(tmp_path, vault):
    settings = Settings(
        data_dir=tmp_path / "d",
        admin_token=TOKEN,
        jobs_enabled=False,
        vault_dir=vault,
        _env_file=None,
    )
    with TestClient(create_app(settings), base_url="http://127.0.0.1") as cli:
        yield cli


def test_rotas_do_grafo_exigem_login_e_servem_o_formato_do_front(c, vault):
    m = c.app.state.orion.memory
    m.index_vault(vault)
    for rota in (
        "/grafo/completo",
        "/grafo/no?q=rag",
        "/grafo/caminho?de=a&para=b",
        "/busca?q=rag",
    ):
        assert c.get(rota).status_code == 401
    g = c.get("/grafo/completo", headers=AUTH).json()
    assert set(g) == {"nodes", "links", "truncado"} and g["nodes"]
    assert set(g["links"][0]) == {"source", "target", "rel"}
    assert c.get("/grafo/completo", params={"limite": 5}, headers=AUTH).status_code == 422
    assert c.get("/grafo/no", params={"q": "mcp"}, headers=AUTH).json()["no"]["label"] == "MCP"
    assert c.get("/grafo/no", params={"q": "zzzz"}, headers=AUTH).json() == {"no": None}


def test_o_que_liga_x_a_y(c, vault):
    c.app.state.orion.memory.index_vault(vault)
    r = c.get("/grafo/caminho", params={"de": "Orion", "para": "MCP"}, headers=AUTH).json()
    assert [n["label"] for n in r["caminho"]] == ["Orion", "MCP"]
    assert r["ligacoes"][0]["rel"] == "liga"
    sem = c.get("/grafo/caminho", params={"de": "Orion", "para": "Ilha"}, headers=AUTH).json()
    assert sem["caminho"] == [] and sem["de"]["label"] == "Orion"
    nada = c.get("/grafo/caminho", params={"de": "Orion", "para": "nada"}, headers=AUTH)
    assert (
        nada.status_code == 200
        and nada.json()["para"] is None
        and nada.json()["de"]["label"] == "Orion"
    )
    assert nada.json()["caminho"] == []


def test_relacoes_do_sono_listam_e_apagam_so_as_do_sono(c):
    ops = c.app.state.orion.ops
    ops.add_edge("Antônio", "estuda_em", "Unimar", kind="sono")
    ops.add_edge("topic:a", "conecta", "topic:b", kind="rem")
    lista = c.get("/memoria/relacoes", headers=AUTH).json()
    assert [(r["src"], r["rel"], r["dst"]) for r in lista] == [("Antônio", "estuda_em", "Unimar")]
    q = {"src": "Antônio", "rel": "estuda_em", "dst": "Unimar"}
    assert c.delete("/memoria/relacoes", params=q).status_code == 401
    assert c.delete("/memoria/relacoes", params=q, headers=AUTH).json() == {"ok": True}
    assert c.get("/memoria/relacoes", headers=AUTH).json() == []
    assert c.delete("/memoria/relacoes", params=q, headers=AUTH).status_code == 404
    outra = {
        "src": "topic:a",
        "rel": "conecta",
        "dst": "topic:b",
    }  # grafo importado não sai por aqui
    assert c.delete("/memoria/relacoes", params=outra, headers=AUTH).status_code == 404
    assert ops.neighbors("topic:a")


def test_busca_global_agrupa_por_fonte_com_ate_5_itens(c, vault):
    s = c.app.state.orion
    m = s.memory
    m.index_vault(vault)
    sessao = m.active_session("web")
    m.add_message(sessao.id, "user", "vamos falar do projeto Zeta hoje")
    for i in range(8):
        m.add_fact(f"o projeto Zeta tem a fase {i}", "m")
    m.add_screen("tela com o projeto Zeta aberto no editor", "Editor", "code")
    with m.transaction() as t:
        t.execute(
            "INSERT INTO library_results(kind,name,stored,bytes,tool,created_at)"
            " VALUES ('documento','Plano Zeta.docx','x',1,'gerar_documento',1)"
        )
    r = c.get("/busca", params={"q": "Zeta"}, headers=AUTH).json()
    por = {g["fonte"]: g for g in r["grupos"]}
    assert set(por) == {"conversas", "fatos", "tela", "resultados"}
    assert len(por["fatos"]["itens"]) == 5
    assert por["tela"]["itens"][0]["janela"] == "Editor"
    assert por["resultados"]["itens"][0]["nome"] == "Plano Zeta.docx"
    docs = c.get("/busca", params={"q": "híbrida"}, headers=AUTH).json()["grupos"]
    assert docs[0]["fonte"] == "documentos" and "RAG" in docs[0]["itens"][0]["fonte"]
    assert c.get("/busca", params={"q": "x"}, headers=AUTH).status_code == 422
    assert c.get("/busca", params={"q": "nadaaquicomcerteza"}, headers=AUTH).json()["grupos"] == []


def test_busca_global_respeita_o_projeto(c):
    s = c.app.state.orion
    m = s.memory
    a = Projects(m).create("A")["id"]
    m.add_fact("segredo pessoal Omega", "m")
    with data_scope(a, include_personal=False):
        m.add_fact("segredo do projeto Omega", "m")
    sa = m.new_session("web", project_id=a)
    m.add_message(sa.id, "user", "conversa do projeto sobre Omega")
    pessoal = c.get("/busca", params={"q": "Omega"}, headers=AUTH).json()["grupos"]
    textos = str(pessoal)
    assert "pessoal Omega" in textos and "projeto Omega" not in textos
    assert "conversas" not in {g["fonte"] for g in pessoal}
    do_projeto = c.get("/busca", params={"q": "Omega", "projeto": a}, headers=AUTH).json()["grupos"]
    fontes = {g["fonte"] for g in do_projeto}
    assert fontes == {"conversas", "fatos"} and "pessoal Omega" not in str(do_projeto)
