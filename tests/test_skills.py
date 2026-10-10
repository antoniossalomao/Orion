"""B1/B2: skills no formato Agent Skills, carregadas aos poucos, sem executar nada."""

import pytest

from orion.agent import Agent
from orion.gateway import ToolCallRequest
from orion.memory import MemoryStore
from orion.policy import ApprovalStore, PathGuard, PolicyEngine
from orion.policy.engine import Action, Context, ToolCall
from orion.skills import TOOL_SPEC, SkillCatalog, skill_tool
from orion.tools import ToolRegistry, memory_tools
from tests.fakes import FakeGateway, fala, pede


def _skill(raiz, nome, desc="Faz X quando pedirem Y", corpo="Passo 1.\nPasso 2.", front=None):
    d = raiz / nome
    d.mkdir(parents=True)
    f = front if front is not None else f"---\nname: {nome}\ndescription: {desc}\n---\n"
    (d / "SKILL.md").write_text(f + corpo, encoding="utf-8")
    return d


def test_lista_so_nome_e_descricao_e_carrega_o_corpo_sob_demanda(tmp_path):
    _skill(tmp_path, "revisar-pr", "Revisa PR quando o Antônio mandar um link", "SEGREDO-DO-CORPO")
    cat = SkillCatalog(tmp_path)
    bloco = cat.prompt_block()
    assert "revisar-pr: Revisa PR" in bloco and "SEGREDO-DO-CORPO" not in bloco
    r = cat.load("revisar-pr")
    assert r["instrucoes"] == "SEGREDO-DO-CORPO"
    assert "erro" in cat.load("nao-existe") and cat.load("nao-existe")["disponiveis"] == [
        "revisar-pr"
    ]


def test_rejeita_nome_invalido_diferente_da_pasta_e_descricao_vazia(tmp_path):
    _skill(tmp_path, "Maiusculo", front="---\nname: Maiusculo\ndescription: x\n---\n")
    _skill(tmp_path, "a", front="---\nname: b\ndescription: x\n---\n")
    _skill(tmp_path, "c", front="---\nname: c\ndescription:\n---\n")
    _skill(tmp_path, "d", front="sem frontmatter\n")
    cat = SkillCatalog(tmp_path)
    assert cat.skills == {} and len(cat.rejeitadas) == 4


def test_corpo_gigante_e_rejeitado_e_link_simbolico_nao_vale(tmp_path):
    _skill(tmp_path, "grande", corpo="x" * 25_000)
    fora = tmp_path.parent / "fora-skill"
    _skill(fora, "ladrao")
    (tmp_path / "ladrao").symlink_to(fora / "ladrao")
    cat = SkillCatalog(tmp_path)
    assert cat.skills == {}
    assert {r.pasta for r in cat.rejeitadas} == {"grande", "ladrao"}


def test_scripts_nao_sao_executados_apenas_avisados_e_references_so_listadas(tmp_path):
    d = _skill(tmp_path, "com-script")
    (d / "scripts").mkdir()
    (d / "scripts" / "x.sh").write_text("touch /tmp/NAO-DEVE-EXISTIR")
    (d / "references").mkdir()
    (d / "references" / "guia.md").write_text("CONTEUDO-DA-REFERENCIA")
    r = SkillCatalog(tmp_path).load("com-script")
    assert r["referencias_na_pasta"] == ["guia.md"] and "CONTEUDO-DA-REFERENCIA" not in str(r)
    assert any("scripts" in a for a in r["avisos"])


def test_pasta_inexistente_e_catalogo_vazio(tmp_path):
    cat = SkillCatalog(tmp_path / "nao-existe")
    assert cat.prompt_block() == "" and cat.skills == {}


def test_skill_e_leitura_na_politica_e_nao_libera_nada_a_mais(tmp_path):
    motor = PolicyEngine(
        path_guard=PathGuard(protected_roots=(tmp_path / "p",), safe_roots=()),
        approvals=ApprovalStore(),
    )
    motor.register_tool(TOOL_SPEC)
    assert (
        motor.evaluate(ToolCall("carregar_skill", {"nome": "x"}), Context("s")).action
        is Action.ALLOW
    )
    # mesmo depois de carregar uma skill, escrita numa sessão contaminada segue pedindo aval
    ctx = Context("s", tainted=True)
    assert motor.evaluate(ToolCall("salvar_memoria", {"texto": "x"}), ctx).action is Action.CONFIRM


@pytest.fixture
def store(tmp_path):
    s = MemoryStore(tmp_path / "a.db")
    yield s
    s.close()


async def test_agente_poe_so_o_indice_no_prompt_e_a_ferramenta_entrega_o_corpo(tmp_path, store):
    _skill(tmp_path / "sk", "resumo-semanal", "Monta o resumo da semana", "CORPO-COMPLETO")
    cat = SkillCatalog(tmp_path / "sk")
    policy = PolicyEngine(
        path_guard=PathGuard(protected_roots=(tmp_path / "p",), safe_roots=()),
        approvals=ApprovalStore(),
    )
    policy.register_tool(TOOL_SPEC)
    gw = FakeGateway(
        pede(ToolCallRequest("c1", "carregar_skill", {"nome": "resumo-semanal"})), fala("feito")
    )
    agent = Agent(
        gateway=gw,
        tools=ToolRegistry([*memory_tools(store), skill_tool(cat)]),
        policy=policy,
        memory=store,
        skills=cat,
    )
    [e async for e in agent.run("web", "faça o resumo da semana")]
    sistema = gw.chamadas[0][0]["content"]
    assert "resumo-semanal: Monta o resumo da semana" in sistema and "CORPO-COMPLETO" not in sistema
    retorno = gw.chamadas[1][-1]["content"]
    assert "CORPO-COMPLETO" in retorno
