"""C34/C35: biblioteca de resultados — cópia própria, versões, origem e entrega segura."""

import json

import pytest
from fastapi.testclient import TestClient

from orion.agent import Agent
from orion.app import create_app
from orion.config import Settings
from orion.memory import MemoryStore
from orion.policy import ApprovalStore, PathGuard, PolicyEngine
from orion.policy.classes import Risk, ToolSpec
from orion.projects import Projects
from orion.resultados import Library, nome_seguro
from orion.tools import Tool, ToolRegistry, memory_tools
from tests.fakes import FakeGateway, chama, fala, pede

TOKEN = "token-de-teste-com-16+"
AUTH = {"Authorization": f"Bearer {TOKEN}"}


@pytest.fixture
def c(tmp_path):
    settings = Settings(data_dir=tmp_path / "d", admin_token=TOKEN, _env_file=None)
    with TestClient(
        create_app(settings, gateway_factory=lambda _: None), base_url="http://127.0.0.1"
    ) as cli:
        yield cli


def _gerar(c, tmp_path, nome="relatorio.md", conteudo="# Relatório\nolá", sessao=None):
    origem = tmp_path / "saida" / nome
    origem.parent.mkdir(exist_ok=True)
    origem.write_bytes(conteudo.encode() if isinstance(conteudo, str) else conteudo)
    lib = c.app.state.orion.biblioteca
    r = json.dumps({"ok": True, "path": str(origem), "tipo": "md"})
    return lib.registrar(sessao, "gerar_documento", r)


def test_exige_login(c):
    assert c.get("/resultados").status_code == 401
    assert c.get("/resultados/1/arquivo").status_code == 401
    assert c.delete("/resultados/1").status_code == 401


def test_nome_seguro_tira_caminho_e_caracteres_estranhos():
    assert nome_seguro("../../etc/passwd") == "passwd"
    assert nome_seguro("a b<c>.md") == "a b_c_.md"
    assert nome_seguro("") == "resultado"


def test_registra_copia_com_origem_e_a_copia_sobrevive_ao_original(c, tmp_path):
    m = c.app.state.orion.memory
    s = m.new_session("web", "Plano da semana")
    a = _gerar(c, tmp_path, sessao=s.id)
    assert a and a["version"] == 1 and a["stored"].endswith("-relatorio.md")
    (tmp_path / "saida" / "relatorio.md").unlink()  # o original some: a cópia fica
    lista = c.get("/resultados", headers=AUTH).json()["resultados"]
    assert lista[0]["nome"] == "relatorio.md" and lista[0]["conversa"] == "Plano da semana"
    assert lista[0]["previa"] == "texto" and lista[0]["ferramenta"] == "gerar_documento"
    assert c.get(f"/resultados/{a['id']}/texto", headers=AUTH).json()["texto"].startswith("# Relat")


def test_mesmo_nome_vira_a_versao_seguinte_e_a_antiga_fica(c, tmp_path):
    v1 = _gerar(c, tmp_path, conteudo="primeira")
    v2 = _gerar(c, tmp_path, conteudo="segunda versão")
    assert (v1["version"], v2["version"], v2["parent_id"]) == (1, 2, v1["id"])
    lista = c.get("/resultados", headers=AUTH).json()["resultados"]
    assert [r["versao"] for r in lista] == [2, 1] and lista[0]["anterior"] == v1["id"]
    assert c.get(f"/resultados/{v1['id']}/texto", headers=AUTH).json()["texto"] == "primeira"


def test_html_e_entregue_como_anexo_nunca_renderizado(c, tmp_path):
    a = _gerar(c, tmp_path, "pagina.html", "<script>alert(1)</script>")
    r = c.get(f"/resultados/{a['id']}/arquivo", headers=AUTH)
    assert r.status_code == 200 and r.headers["content-type"] == "application/octet-stream"
    assert "attachment" in r.headers["content-disposition"]
    assert r.headers["x-content-type-options"] == "nosniff"
    assert "sandbox" in r.headers["content-security-policy"]
    assert (
        c.get(f"/resultados/{a['id']}/arquivo?previa=true", headers=AUTH).headers["content-type"]
        == "application/octet-stream"
    )  # prévia inline só para imagem raster
    assert c.get(f"/resultados/{a['id']}/texto", headers=AUTH).status_code == 415


def test_imagem_raster_tem_previa_inline(c, tmp_path):
    d = c.app.state.orion
    imagens = d.settings.data_dir / "imagens"
    imagens.mkdir(parents=True, exist_ok=True)
    (imagens / "img-1.png").write_bytes(b"\x89PNG\r\n\x1a\n" + b"0" * 20)
    r = d.biblioteca.registrar(
        None, "gerar_imagem", json.dumps({"ok": True, "arquivo": "img-1.png"})
    )
    resp = c.get(f"/resultados/{r['id']}/arquivo?previa=true", headers=AUTH)
    assert resp.headers["content-type"] == "image/png" and resp.content.startswith(b"\x89PNG")
    lista = c.get("/resultados", headers=AUTH).json()["resultados"]
    assert lista[0]["previa"] == "imagem" and lista[0]["tipo"] == "imagem"


def test_ferramenta_com_erro_ou_caminho_falso_nao_registra_nada(c, tmp_path):
    lib: Library = c.app.state.orion.biblioteca
    assert lib.registrar(None, "gerar_documento", json.dumps({"erro": "x"})) is None
    assert lib.registrar(None, "gerar_documento", "não é json") is None
    assert (
        lib.registrar(None, "gerar_documento", json.dumps({"ok": True, "path": "/nao/existe"}))
        is None
    )
    assert (
        lib.registrar(None, "buscar_url", json.dumps({"ok": True, "path": "/etc/passwd"})) is None
    )
    # o modelo não consegue ler arquivo nenhum de volta: só o que foi copiado para a pasta
    assert (
        lib.registrar(None, "gerar_imagem", json.dumps({"ok": True, "arquivo": "../../etc/passwd"}))
        is None
    )
    assert c.get("/resultados", headers=AUTH).json()["total"] == 0


def test_apagar_remove_registro_e_arquivo_e_404_depois(c, tmp_path):
    a = _gerar(c, tmp_path)
    guardado = c.app.state.orion.biblioteca.pasta / a["stored"]
    assert guardado.is_file()
    assert c.delete(f"/resultados/{a['id']}", headers=AUTH).json() == {"ok": True}
    assert not guardado.exists()
    assert c.delete(f"/resultados/{a['id']}", headers=AUTH).status_code == 404
    assert c.get(f"/resultados/{a['id']}/arquivo", headers=AUTH).status_code == 404


def test_filtra_por_projeto_e_herda_o_projeto_da_conversa(c, tmp_path):
    m = c.app.state.orion.memory
    p = Projects(m).create("TCC")
    s = m.new_session("web", project_id=p["id"])
    _gerar(c, tmp_path, "a.md", sessao=s.id)
    _gerar(c, tmp_path, "b.md")
    assert c.get(f"/resultados?projeto={p['id']}", headers=AUTH).json()["total"] == 1
    assert c.get("/resultados", headers=AUTH).json()["total"] == 2


async def test_o_agente_registra_o_que_a_ferramenta_gerou(tmp_path):
    store = MemoryStore(tmp_path / "a.db")
    origem = tmp_path / "x" / "ata.md"

    def gerar_documento(path: str, conteudo: str):
        origem.parent.mkdir(exist_ok=True)
        origem.write_text(conteudo, encoding="utf-8")
        return {"ok": True, "path": str(origem), "tipo": "md"}

    ferramenta = Tool("gerar_documento", "x", {"type": "object", "properties": {}}, gerar_documento)
    policy = PolicyEngine(
        path_guard=PathGuard(protected_roots=(tmp_path / "p",), safe_roots=()),
        approvals=ApprovalStore(),
        tools={"gerar_documento": ToolSpec("gerar_documento", Risk.READ)},
    )
    lib = Library(store, tmp_path / "resultados")
    gw = FakeGateway(
        pede(chama("gerar_documento", path=str(origem), conteudo="# Ata")), fala("pronto")
    )
    agent = Agent(
        gateway=gw,
        tools=ToolRegistry([*memory_tools(store), ferramenta]),
        policy=policy,
        memory=store,
        library=lib,
    )
    [e async for e in agent.run("web", "faça a ata")]
    itens = store.list_artifacts()
    assert len(itens) == 1 and itens[0]["name"] == "ata.md" and itens[0]["session_id"]
    assert (tmp_path / "resultados" / itens[0]["stored"]).read_text(encoding="utf-8") == "# Ata"
    store.close()
