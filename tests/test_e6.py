"""E6: fontes e atividade no chat, diff e limpeza da biblioteca, pesquisa pela interface."""

import json
from datetime import datetime

import pytest
from fastapi.testclient import TestClient

from orion.agent import Agent, _links
from orion.app import create_app
from orion.capture import Capturer
from orion.config import Settings
from orion.gateway import ToolCallRequest
from orion.memory import MemoryStore
from orion.policy import ApprovalStore, PathGuard, PolicyEngine
from orion.research import NightResearch, assuntos_salvos, salvar_assuntos
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


def _gerar(c, tmp_path, nome="relatorio.md", conteudo="# Relatório\nolá"):
    origem = tmp_path / "saida" / nome
    origem.parent.mkdir(exist_ok=True)
    origem.write_bytes(conteudo.encode() if isinstance(conteudo, str) else conteudo)
    r = json.dumps({"ok": True, "path": str(origem), "tipo": "md"})
    return c.app.state.orion.biblioteca.registrar(None, "gerar_documento", r)


# ── E6.1: PDF inline só para a prévia ─────────────────────────────────────
def test_pdf_tem_previa_inline_e_sem_previa_continua_anexo(c, tmp_path):
    a = _gerar(c, tmp_path, "contrato.pdf", b"%PDF-1.4\n%fim")
    lista = c.get("/resultados", headers=AUTH).json()["resultados"]
    assert lista[0]["previa"] == "pdf"
    r = c.get(f"/resultados/{a['id']}/arquivo?previa=true", headers=AUTH)
    assert r.headers["content-type"] == "application/pdf"
    assert r.headers["content-disposition"].startswith("inline")
    assert r.headers["x-content-type-options"] == "nosniff"
    assert r.headers["content-security-policy"] == "default-src 'none'"
    r2 = c.get(f"/resultados/{a['id']}/arquivo", headers=AUTH)
    assert "attachment" in r2.headers["content-disposition"]


def test_html_nunca_vira_inline_nem_com_previa(c, tmp_path):
    a = _gerar(c, tmp_path, "p.html", "<script>1</script>")
    r = c.get(f"/resultados/{a['id']}/arquivo?previa=true", headers=AUTH)
    assert "attachment" in r.headers["content-disposition"]
    assert r.headers["content-type"] == "application/octet-stream"


# ── E6.2: diff ────────────────────────────────────────────────────────────
def test_diff_entre_versoes(c, tmp_path):
    v1 = _gerar(c, tmp_path, conteudo="a\nb\nc\n")
    v2 = _gerar(c, tmp_path, conteudo="a\nB\nc\nd\n")
    r = c.get(f"/resultados/{v2['id']}/diff?com={v1['id']}", headers=AUTH).json()
    assert (r["de"], r["para"]) == (v1["id"], v2["id"])
    assert r["adicionadas"] == 2 and r["removidas"] == 1 and not r["iguais"]
    assert "-b" in r["linhas"] and "+B" in r["linhas"] and "+d" in r["linhas"]


def test_diff_de_versoes_iguais_e_erros(c, tmp_path):
    v1 = _gerar(c, tmp_path, conteudo="x")
    v2 = _gerar(c, tmp_path, conteudo="x")
    assert c.get(f"/resultados/{v2['id']}/diff?com={v1['id']}", headers=AUTH).json()["iguais"]
    assert c.get(f"/resultados/{v2['id']}/diff?com=999", headers=AUTH).status_code == 404
    img = _gerar(c, tmp_path, "f.png", b"\x89PNG\r\n\x1a\n")
    assert c.get(f"/resultados/{img['id']}/diff?com={v1['id']}", headers=AUTH).status_code == 415
    assert c.get(f"/resultados/{v2['id']}/diff", headers=AUTH).status_code == 422
    assert c.get(f"/resultados/{v2['id']}/diff?com={v1['id']}").status_code == 401


# ── E6.3: limpeza em lote ─────────────────────────────────────────────────
def test_limpar_mantem_as_n_mais_novas_de_cada_nome(c, tmp_path):
    for i in range(4):
        _gerar(c, tmp_path, "a.md", f"a{i}")
    for i in range(2):
        _gerar(c, tmp_path, "b.md", f"b{i}")
    lib = c.app.state.orion.biblioteca
    sim = c.post("/resultados/limpar", json={"manter": 2, "simular": True}, headers=AUTH).json()
    assert sim["total"] == 2 and sim["apagados"] == 0 and sim["bytes"] == 4
    assert len(c.get("/resultados", headers=AUTH).json()["resultados"]) == 6
    r = c.post("/resultados/limpar", json={"manter": 2}, headers=AUTH).json()
    assert r["apagados"] == 2
    restantes = c.get("/resultados", headers=AUTH).json()["resultados"]
    assert sorted((x["nome"], x["versao"]) for x in restantes) == [
        ("a.md", 3), ("a.md", 4), ("b.md", 1), ("b.md", 2),
    ]  # fmt: skip
    arquivos = {p.name for p in lib.pasta.iterdir()}
    assert len(arquivos) == 4  # o arquivo também sai da pasta


def test_limpar_so_um_nome_e_valida_o_corpo(c, tmp_path):
    for i in range(3):
        _gerar(c, tmp_path, "a.md", f"a{i}")
        _gerar(c, tmp_path, "b.md", f"b{i}")
    r = c.post("/resultados/limpar", json={"manter": 1, "nome": "a.md"}, headers=AUTH).json()
    assert r["apagados"] == 2
    nomes = [x["nome"] for x in c.get("/resultados", headers=AUTH).json()["resultados"]]
    assert nomes.count("a.md") == 1 and nomes.count("b.md") == 3
    for ruim in ({"manter": 0}, {"manter": 1, "extra": 1}, {}):
        assert c.post("/resultados/limpar", json=ruim, headers=AUTH).status_code == 422
    assert c.post("/resultados/limpar", json={"manter": 1}).status_code == 401


# ── E6.4: o que o turno usou ──────────────────────────────────────────────
def test_links_sem_pontuacao_e_sem_repetir():
    t = 'veja https://a.com/x. e (https://b.org/y), de novo https://a.com/x; "https://c.io"'
    assert _links(t) == ["https://a.com/x", "https://b.org/y", "https://c.io"]
    assert _links("sem link, ftp://x.y e javascript:alert(1)") == []


@pytest.fixture
def store(tmp_path):
    s = MemoryStore(tmp_path / "a.db")
    yield s
    s.close()


@pytest.fixture
def policy(tmp_path):
    guard = PathGuard(
        protected_roots=(tmp_path / "Orion",), safe_roots=(tmp_path / "Documents",), system_roots=()
    )
    return PolicyEngine(path_guard=guard, approvals=ApprovalStore())


async def test_resposta_traz_o_que_usou_e_os_links_das_fontes(store, policy):
    web = Tool(
        "buscar_url",
        "x",
        {"type": "object", "properties": {}},
        lambda: {"texto": "Notícia em https://exemplo.com/a e https://exemplo.com/b."},
    )
    agent = Agent(
        gateway=FakeGateway(pede(chama("buscar_url")), pede(chama("listar_fatos")), fala("ok")),
        tools=ToolRegistry([*memory_tools(store), web]),
        policy=policy,
        memory=store,
        servidor_mcp=lambda n: "docs" if n == "buscar_url" else None,
    )
    eventos = [e async for e in agent.run("web", "pesquise")]
    prov = next(e for e in eventos if e.kind == "done").data["provenance"]
    assert prov["usou"] == [
        {"nome": "buscar_url", "origem": "mcp:docs", "estado": "completed"},
        {"nome": "listar_fatos", "origem": "nativa", "estado": "completed"},
    ]
    assert prov["fontes"] == ["https://exemplo.com/a", "https://exemplo.com/b"]
    gravada = store.history(store.active_session("web").id)[-1].provenance
    assert gravada["usou"] == prov["usou"] and gravada["fontes"] == prov["fontes"]


async def test_resposta_sem_ferramenta_nao_tem_usou_nem_fontes(store, policy):
    agent = Agent(
        gateway=FakeGateway(fala("oi")),
        tools=ToolRegistry(memory_tools(store)),
        policy=policy,
        memory=store,
    )
    eventos = [e async for e in agent.run("web", "oi")]
    prov = next(e for e in eventos if e.kind == "done").data["provenance"]
    assert "usou" not in prov and "fontes" not in prov


def test_origem_de_skill_mcp_e_nativa(store, policy):
    agent = Agent(
        gateway=FakeGateway(),
        tools=ToolRegistry([]),
        policy=policy,
        memory=store,
        servidor_mcp=lambda n: "gh" if n.startswith("gh__") else None,
    )

    def ch(ferramenta, **a):
        return ToolCallRequest("1", ferramenta, a)

    assert agent._origem_de(ch("carregar_skill", nome="Estilo-X")) == "skill:estilo-x"
    assert agent._origem_de(ch("gh__issues")) == "mcp:gh"
    assert agent._origem_de(ch("ler_arquivo")) == "nativa"


# ── E6.5: pesquisa pela interface ─────────────────────────────────────────
def _ts(h, m=0, dia=8):
    return datetime(2026, 10, dia, h, m).timestamp()


def _pesquisa(c, tmp_path, at="03:00", texto="resumo"):
    from orion.agent import AgentEvent

    estado = c.app.state.orion
    vault = tmp_path / "vault"
    vault.mkdir(exist_ok=True)
    chamadas = []

    async def run_turn(canal, t):
        chamadas.append(t)
        yield AgentEvent("text", {"text": texto})
        yield AgentEvent("done", {})

    estado.pesquisa = NightResearch(
        memory=estado.memory,
        run_turn=run_turn,
        capturer=Capturer(vault, "00 Inbox", clock=lambda: _ts(3, 5)),
        assuntos=lambda: assuntos_salvos(estado.memory, estado.settings.research_topics)[0],
        at=at,
        clock=lambda: _ts(3, 5),
    )
    return estado.pesquisa, chamadas


def test_assuntos_padrao_vem_do_env_e_a_interface_sobrescreve(c):
    r = c.get("/pesquisa/assuntos", headers=AUTH).json()
    assert r["origem"] == "padrao" and len(r["assuntos"]) == 3 and r["max"] == 5
    assert r["max_tamanho"] == 200 and r["disponivel"] is False and r["hora"] is None
    r = c.put("/pesquisa/assuntos", json={"assuntos": ["  zkML  ", "Pix"]}, headers=AUTH).json()
    assert r["assuntos"] == ["zkML", "Pix"] and r["origem"] == "interface"
    assert c.get("/pesquisa/assuntos", headers=AUTH).json()["assuntos"] == ["zkML", "Pix"]
    r = c.put("/pesquisa/assuntos", json={"assuntos": []}, headers=AUTH).json()
    assert r["assuntos"] == [] and r["origem"] == "interface"  # vazio é uma escolha


@pytest.mark.parametrize(
    "ruim",
    [
        [f"a{i}" for i in range(6)],
        ["x" * 201],
        ["   "],
        ["a;b"],
    ],
)
def test_assuntos_invalidos_sao_recusados_sem_cortar(c, ruim):
    r = c.put("/pesquisa/assuntos", json={"assuntos": ruim}, headers=AUTH)
    assert r.status_code == 422
    assert c.get("/pesquisa/assuntos", headers=AUTH).json()["origem"] == "padrao"


def test_assuntos_exigem_login_e_corpo_estrito(c):
    assert c.get("/pesquisa/assuntos").status_code == 401
    assert c.put("/pesquisa/assuntos", json={"assuntos": []}).status_code == 401
    assert c.get("/pesquisa/relatorios").status_code == 401
    assert c.post("/pesquisa/rodar").status_code == 401
    r = c.put("/pesquisa/assuntos", json={"assuntos": [], "x": 1}, headers=AUTH)
    assert r.status_code == 422


def test_valor_guardado_estragado_cai_no_padrao(store):
    store.meta_set("research:topics", "isto não é json")
    assert assuntos_salvos(store, "a;b") == (["a", "b"], "padrao")
    store.meta_set("research:topics", json.dumps([1, 2]))
    assert assuntos_salvos(store, "a;b") == (["a", "b"], "padrao")
    salvar_assuntos(store, ["z"])
    assert assuntos_salvos(store, "a;b") == (["z"], "interface")


def test_relatorios_sem_pesquisa_ligada_e_lista_vazia(c):
    assert c.get("/pesquisa/relatorios", headers=AUTH).json() == {"total": 0, "relatorios": []}
    r = c.post("/pesquisa/rodar", headers=AUTH)
    assert r.status_code == 409


def test_rodar_agora_grava_relatorio_com_os_assuntos_da_interface(c, tmp_path):
    pesq, chamadas = _pesquisa(c, tmp_path, at="")
    c.put("/pesquisa/assuntos", json={"assuntos": ["edge AI"]}, headers=AUTH)
    assert pesq.devida() is False  # sem hora não roda sozinha
    r = c.post("/pesquisa/rodar", headers=AUTH)
    assert r.status_code == 202 and r.json() == {"iniciada": True}
    # o TestClient espera a tarefa quando a próxima requisição passa pelo laço
    lista = c.get("/pesquisa/relatorios", headers=AUTH).json()
    assert lista["total"] == 1 and chamadas and "edge AI" in chamadas[0]
    item = lista["relatorios"][0]
    assert item["arquivo"].startswith("00 Inbox/") and item["data"] == "2026-10-08"
    assert "pesquisa-noturna" in item["nome"] and item["bytes"] > 0
    # rodar agora não gasta a rodada da noite
    assert c.app.state.orion.memory.counter_get("pesquisa:ultimo") == 0


def test_rodar_agora_respeita_panico_e_cota(c, tmp_path):
    _pesquisa(c, tmp_path)
    estado = c.app.state.orion
    estado.custos.pode_usar = lambda *a, **k: False
    assert c.post("/pesquisa/rodar", headers=AUTH).status_code == 429
    estado.custos.pode_usar = lambda *a, **k: True
    estado.modos.entrar_panico("teste")
    assert c.post("/pesquisa/rodar", headers=AUTH).status_code == 409


def test_rodar_sem_assunto_e_recusado(c, tmp_path):
    _pesquisa(c, tmp_path)
    c.put("/pesquisa/assuntos", json={"assuntos": []}, headers=AUTH)
    assert c.post("/pesquisa/rodar", headers=AUTH).status_code == 422


def test_lista_so_tem_relatorios_da_pesquisa(c, tmp_path):
    pesq, _ = _pesquisa(c, tmp_path)
    pasta = tmp_path / "vault" / "00 Inbox"
    pasta.mkdir(parents=True)
    (pasta / "2026-10-08 0300 - captura.md").write_text("x")
    (pasta / "2026-10-07 0300 - pesquisa-noturna-aviso.md").write_text("y")
    (pasta / "pesquisa-noturna.txt").write_text("z")
    nomes = [r["nome"] for r in pesq.relatorios()]
    assert nomes == ["2026-10-07 0300 - pesquisa-noturna-aviso"]


async def test_arquivo_gerado_no_turno_vira_cartao_na_proveniencia(store, policy, tmp_path):
    from orion.resultados import Library

    arquivo = tmp_path / "Documents" / "ata.md"
    arquivo.parent.mkdir()
    arquivo.write_text("# Ata")
    gerar = Tool(
        "gerar_documento",
        "x",
        {"type": "object", "properties": {"path": {"type": "string"}}},
        lambda path: {"ok": True, "path": path, "tipo": "md"},
    )
    biblioteca = Library(store, tmp_path / "resultados")
    for _ in range(2):  # a 2ª vez vira a versão 2 do mesmo arquivo
        agent = Agent(
            gateway=FakeGateway(pede(chama("gerar_documento", path=str(arquivo))), fala("Gerei.")),
            tools=ToolRegistry([gerar]),
            policy=policy,
            memory=store,
            library=biblioteca,
        )
        eventos = [e async for e in agent.run("web", "gere a ata")]
        prov = next(e for e in eventos if e.kind == "done").data["provenance"]
    assert prov["resultados"] == [{"id": 2, "nome": "ata.md", "versao": 2}]
    assert prov["usou"][0]["nome"] == "gerar_documento"
