import pytest

from orion.agent import Agent
from orion.extensions.skill_runtime import SkillReference, SkillRuntime, SkillSource
from orion.extensions.skills import SkillError
from orion.memory import MemoryStore
from orion.policy import PathGuard, PolicyEngine
from orion.tools.registry import Tool, ToolRegistry
from tests.fakes import FakeGateway, chama, fala, pede


def runtime(tmp_path, *, enabled=True, allowed="buscar_memoria"):
    root = tmp_path / "skills"
    skill = root / "financas"
    skill.mkdir(parents=True)
    (skill / "references").mkdir()
    (skill / "references/manual.md").write_text(
        "referência carregada apenas quando pedida", encoding="utf-8"
    )
    (skill / "SKILL.md").write_text(
        f"""---
name: financas
description: Analisar relatórios finanças orçamento
allowed-tools: {allowed}
metadata:
  version: "1.0"
---
Trabalho auxiliar. [manual](references/manual.md)
""",
        encoding="utf-8",
    )
    return SkillRuntime([SkillSource(root=root, namespace="pesquisa", enabled=enabled)])


def test_relevance_explicit_references_budget_and_disabled(tmp_path):
    rt = runtime(tmp_path)
    assert not rt.select("que horas são").data
    selection = rt.select("analisar relatórios de finanças")
    assert selection.skills[0]["id"] == "pesquisa:financas"
    assert "referência carregada" not in selection.data[0].text
    selection = rt.select(
        "oi",
        ["pesquisa:financas"],
        [SkillReference(skill="pesquisa:financas", path="references/manual.md")],
    )
    assert "referência carregada" in selection.data[-1].text
    assert (
        sum(len(x.text.encode()) for x in rt.select("oi", ["pesquisa:financas"], budget=20).data)
        <= 20
    )
    with pytest.raises(SkillError, match="skill_reference_not_selected"):
        rt.select(
            "oi",
            references=[SkillReference(skill="pesquisa:financas", path="references/manual.md")],
        )
    with pytest.raises(SkillError, match="skill_reference_not_declared"):
        rt.select(
            "oi",
            ["pesquisa:financas"],
            [SkillReference(skill="pesquisa:financas", path="SKILL.md")],
        )
    rt.enabled.clear()
    assert not rt.select("relatórios finanças").data
    with pytest.raises(SkillError, match="skill_disabled"):
        rt.select("oi", ["pesquisa:financas"])


async def test_allowed_tools_only_restrict_and_cannot_grant_policy_permission(tmp_path):
    rt = runtime(tmp_path, allowed="controlar_janela admin_bypass")
    selection = rt.select("oi", ["pesquisa:financas"])
    store = MemoryStore(tmp_path / "memory.db")
    try:
        calls = []
        registry = ToolRegistry(
            [
                Tool(name, name, {"type": "object"}, lambda: calls.append(1))
                for name in ("controlar_janela", "admin_bypass", "salvar_memoria")
            ]
        )
        policy = PolicyEngine(
            path_guard=PathGuard(protected_roots=(), safe_roots=(), system_roots=())
        )
        gw = FakeGateway(
            pede(chama("admin_bypass"), chama("controlar_janela"), chama("salvar_memoria")),
            fala("aguardando"),
        )
        agent = Agent(gateway=gw, tools=registry, policy=policy, memory=store)
        events = [e async for e in agent.run("web", "execute", selection=selection)]
        assert not calls
        assert [e.data["decision"] for e in events if e.kind == "tool"] == [
            "deny",
            "confirm",
            "deny",
        ]
        assert len(gw.ferramentas[0]) == 2
        session = store.active_session("web")
        assert store.history(session.id)[-1].provenance["skills"][0]["version"] == "1.0"
    finally:
        store.close()


def test_skill_catalog_authenticated_and_does_not_load_body(tmp_path):
    from fastapi.testclient import TestClient

    from orion.app import create_app
    from orion.config import Settings

    runtime(tmp_path)
    root = tmp_path / "skills"
    # Catálogo continua sendo só metadados: corpo inválido ainda não é carregado.
    path = root / "financas/SKILL.md"
    path.write_bytes(path.read_bytes() + b"\xff")
    settings = Settings(
        data_dir=tmp_path / "dados",
        jobs_enabled=False,
        _env_file=None,
        admin_token="catalogo-skill-fixture16",
        skill_sources=[SkillSource(root=root, namespace="pesquisa", enabled=True)],
    )
    with TestClient(create_app(settings), base_url="http://127.0.0.1") as client:
        assert client.get("/skills").status_code == 401
        data = client.get(
            "/skills", headers={"Authorization": "Bearer catalogo-skill-fixture16"}
        ).json()
        assert data[0]["id"] == "pesquisa:financas" and "text" not in data[0]
        assert str(root) not in client.get("/capabilities").text
