"""E7.6 e E7.8: skill forçada no turno, `orion estilo` e o prompt `estilo-antonio` do servidor."""

import json
import os
import sys

import pytest
from fastapi.testclient import TestClient
from mcp import Client, StdioServerParameters

from orion import estilo
from orion.agent import Agent
from orion.app import create_app
from orion.config import Settings
from orion.mcp_servidor import gerar_token
from orion.memory import MemoryStore
from orion.policy import ApprovalStore, PathGuard, PolicyEngine
from orion.skills import SkillCatalog, skill_tool
from orion.tools import ToolRegistry
from tests.fakes import FakeGateway, fala

TOKEN = "token-de-teste-com-16+"
AUTH = {"Authorization": f"Bearer {TOKEN}"}


def _skill(raiz, nome="resumir", corpo="Resuma em 3 linhas.", desc="Resume textos."):
    pasta = raiz / nome
    pasta.mkdir(parents=True)
    (pasta / "SKILL.md").write_text(
        f"---\nname: {nome}\ndescription: {desc}\n---\n\n{corpo}\n", encoding="utf-8"
    )


@pytest.fixture
def agente(tmp_path):
    store = MemoryStore(tmp_path / "a.db")
    _skill(tmp_path / "skills")
    catalogo = SkillCatalog(tmp_path / "skills")
    policy = PolicyEngine(
        path_guard=PathGuard(protected_roots=(), safe_roots=(tmp_path,), system_roots=()),
        approvals=ApprovalStore(),
    )
    gw = FakeGateway(fala("Resumi."))
    ag = Agent(
        gateway=gw,
        tools=ToolRegistry([skill_tool(catalogo)]),
        policy=policy,
        memory=store,
        skills=catalogo,
    )
    yield ag, gw, store
    store.close()


async def test_skill_forcada_entra_como_se_carregar_skill_tivesse_sido_chamada(agente):
    ag, gw, _ = agente
    eventos = [e async for e in ag.run("web", "resuma isto", skill_forcada="resumir")]
    msgs = gw.chamadas[0]
    chamada = next(m for m in msgs if m.get("tool_calls"))
    assert chamada["tool_calls"][0]["function"]["name"] == "carregar_skill"
    resultado = next(m for m in msgs if m["role"] == "tool")
    assert json.loads(resultado["content"])["instrucoes"] == "Resuma em 3 linhas."
    ordem = [m["role"] for m in msgs]
    assert ordem.index("user") < ordem.index("tool")  # depois do pedido, antes da resposta
    prov = next(e for e in eventos if e.kind == "done").data["provenance"]
    assert prov["usou"][0] == {
        "nome": "carregar_skill",
        "origem": "skill:resumir",
        "estado": "completed",
    }


async def test_skill_inexistente_e_erro_e_nada_e_gravado(agente):
    ag, gw, store = agente
    eventos = [e async for e in ag.run("web", "oi", skill_forcada="nao-existe")]
    assert [e.kind for e in eventos] == ["error"] and eventos[0].data[
        "message"
    ] == "skill_not_found"
    assert gw.chamadas == [] and store.history(store.active_session("web").id) == []


def test_rotas_do_catalogo_e_do_chat_com_skill(tmp_path):
    _skill(tmp_path / "d" / "skills", "revisar", "Revise com cuidado.", "Revisa código.")
    app = create_app(
        Settings(data_dir=tmp_path / "d", admin_token=TOKEN, jobs_enabled=False, _env_file=None),
        gateway_factory=lambda _: FakeGateway(fala("ok"), fala("ok2")),
    )
    with TestClient(app, base_url="http://127.0.0.1") as c:
        assert c.get("/skills/catalogo").status_code == 401
        r = c.get("/skills/catalogo", headers=AUTH).json()
        assert r == {"total": 1, "skills": [{"nome": "revisar", "descricao": "Revisa código."}]}
        ok = c.post("/chat", json={"texto": "x", "skill": "revisar"}, headers=AUTH)
        assert ok.status_code == 200 and '"usou"' in ok.text and "skill:revisar" in ok.text
        ruim = c.post("/chat", json={"texto": "x", "skill": "Revisar!"}, headers=AUTH)
        assert ruim.status_code == 422
        falta = c.post("/chat", json={"texto": "x", "skill": "outra"}, headers=AUTH)
        assert "skill_not_found" in falta.text


# ── E7.8: orion estilo ────────────────────────────────────────────────────────
class FalsoDelegador:
    def __init__(self, saida=None, ok=True):
        self.chamadas, self.saida, self.ok = [], saida, ok

    def delegate(self, tarefa, pasta, agente=None):
        self.chamadas.append((tarefa, pasta, agente))
        if not self.ok:
            return {"ok": False, "erro": "limite diário atingido"}
        return {"ok": True, "saida": self.saida or ("## Nomes\n" + "snake_case em tudo. " * 10)}


def test_estilo_gera_skill_pendente_que_o_catalogo_ainda_nao_ve(tmp_path):
    repo = tmp_path / "meu-repo"
    repo.mkdir()
    skills = tmp_path / "skills"
    d = FalsoDelegador(
        "```markdown\n## Nomes\nsnake_case. " + "x" * 100 + "\napi_key = abcdefgh12345\n```"
    )
    arquivo = estilo.gerar([repo], d, skills)
    texto = arquivo.read_text(encoding="utf-8")
    assert arquivo == skills / ".pendentes" / "estilo-antonio" / "SKILL.md"
    assert texto.startswith("---\nname: estilo-antonio\n") and "# Estilo em `meu-repo`" in texto
    assert "api_key" not in texto and "```" not in texto  # linha com cara de segredo e cerca saem
    assert d.chamadas[0][1] == str(repo) and d.chamadas[0][2] == "claude"
    assert "estilo-antonio" not in SkillCatalog(skills).skills  # nasce desativada


def test_estilo_ativar_move_para_o_catalogo_e_nao_sobrescreve(tmp_path):
    repo = tmp_path / "r"
    repo.mkdir()
    skills = tmp_path / "skills"
    estilo.gerar([repo], FalsoDelegador(), skills)
    destino = estilo.ativar(skills)
    assert destino == skills / "estilo-antonio" / "SKILL.md"
    cat = SkillCatalog(skills)
    assert "estilo-antonio" in cat.skills and cat.rejeitadas == []
    estilo.gerar([repo], FalsoDelegador(), skills)
    with pytest.raises(estilo.EstiloError, match="já existe"):
        estilo.ativar(skills)


def test_estilo_erros_sao_claros(tmp_path):
    with pytest.raises(estilo.EstiloError, match="ao menos uma"):
        estilo.gerar([], FalsoDelegador(), tmp_path)
    with pytest.raises(estilo.EstiloError, match="inexistente"):
        estilo.gerar([tmp_path / "nada"], FalsoDelegador(), tmp_path)
    repo = tmp_path / "r"
    repo.mkdir()
    with pytest.raises(estilo.EstiloError, match="limite diário"):
        estilo.gerar([repo], FalsoDelegador(ok=False), tmp_path)
    with pytest.raises(estilo.EstiloError, match="curta demais"):
        estilo.gerar([repo], FalsoDelegador("ok"), tmp_path)
    with pytest.raises(estilo.EstiloError, match="não há skill pendente"):
        estilo.ativar(tmp_path / "vazio")


def test_estilo_muitos_repos_cabem_no_limite_da_skill(tmp_path):
    repos = []
    for i in range(5):
        r = tmp_path / f"r{i}"
        r.mkdir()
        repos.append(r)
    d = FalsoDelegador("## Nomes\n" + "y" * 9000)
    texto = estilo.gerar(repos, d, tmp_path / "s").read_text(encoding="utf-8")
    assert len(texto) <= 20_000 and texto.count("# Estilo em `r") == 5
    skills = tmp_path / "s"
    estilo.ativar(skills)
    assert SkillCatalog(skills).rejeitadas == []  # cabe no MAX_CORPO


async def test_prompt_do_servidor_mcp_so_existe_com_a_skill_ativa(tmp_path):
    caminho = tmp_path / "dados" / "orion.db"
    caminho.parent.mkdir()
    m = MemoryStore(caminho)
    token = gerar_token(m)
    skills = caminho.parent / "skills"
    env = {k: v for k, v in os.environ.items() if not k.startswith("ORION_")}
    env.update(ORION_DATA_DIR=str(caminho.parent), ORION_MCP_SERVER_TOKEN=token)
    params = StdioServerParameters(
        command=sys.executable, args=["-m", "orion", "mcp-servidor"], env=env
    )
    try:
        async with Client(params, read_timeout_seconds=30) as cli:
            assert [p.name for p in (await cli.list_prompts()).prompts] == ["estilo-antonio"]
            vazio = await cli.get_prompt("estilo-antonio")
            assert "não está ativa" in vazio.messages[0].content.text
            r = tmp_path / "r"
            r.mkdir()
            estilo.gerar([r], FalsoDelegador(), skills)
            estilo.ativar(skills)
            p = await cli.get_prompt("estilo-antonio")
            assert "snake_case" in p.messages[0].content.text
            assert "name: estilo-antonio" not in p.messages[0].content.text  # só o corpo
    finally:
        m.close()
