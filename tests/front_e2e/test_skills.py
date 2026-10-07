from playwright.sync_api import expect

from orion.extensions.skill_runtime import SkillSource

from .test_capabilities import TOKEN_INIT


def fonte(tmp_path):
    root = tmp_path / "skills"
    skill = root / "revisar"
    skill.mkdir(parents=True)
    (skill / "SKILL.md").write_text("""---
name: revisar
description: Revisar orçamento projeto
metadata:
  version: "1.0"
---
Revisar dados fornecidos com clareza.
""")
    return SkillSource(root=root, namespace="pesquisa", origin="pacote de teste", enabled=True)


def test_skill_slash_keyboard_provenance_and_unknown_draft(abrir, novo_backend, tmp_path):
    url, app, gw, _ = novo_backend(skills=[fonte(tmp_path)])
    page = abrir("#/chat", url=url, init=TOKEN_INIT)
    page.wait_for_function("() => Orion.composer.skills().length === 1")
    campo = page.locator("#composer-input")
    campo.fill("/pes")
    expect(page.locator(".slash-menu [role=option]")).to_contain_text("pesquisa:revisar")
    campo.press("Tab")
    expect(campo).to_have_value("/pesquisa:revisar ")
    campo.fill("/pesquisa:ausente mantenha rascunho")
    campo.press("Enter")
    expect(campo).to_have_value("/pesquisa:ausente mantenha rascunho")
    assert not gw.chamadas
    campo.fill("/pesquisa:revisar orçamento do projeto")
    expect(page.locator("#composer-box .composer-skill")).to_contain_text("pacote de teste · 1.0")
    campo.press("Enter")
    expect(page.locator(".msg-user .attach-note")).to_contain_text("pesquisa:revisar")
    expect(page.locator(".msg-orion .prose")).to_contain_text("Resposta do backend novo")
    session = app.state.orion.memory.active_session("web")
    assert (
        app.state.orion.memory.history(session.id)[-1].provenance["skills"][0]["version"] == "1.0"
    )
    assert any("Revisar dados" in m["content"] for m in gw.chamadas[0])


def test_palette_preserves_draft_and_unavailable_skill_preserves_input(
    abrir, novo_backend, tmp_path
):
    url, app, _, _ = novo_backend(skills=[fonte(tmp_path)])
    page = abrir("#/chat", url=url, init=TOKEN_INIT, http_ok=True)
    page.wait_for_function("() => Orion.composer.skills().length === 1")
    campo = page.locator("#composer-input")
    campo.fill("meu texto antes de escolher")
    page.keyboard.press("Control+k")
    page.locator("#palette-input").fill("pesquisa:revisar")
    page.locator("#palette-input").press("Enter")
    expect(campo).to_have_value("/pesquisa:revisar meu texto antes de escolher")
    # Servidor revoga enquanto o client ainda tem catálogo; HTTP recusa restaura draft.
    app.state.orion.skills.enabled.clear()
    campo.press("Enter")
    expect(page.locator(".msg-orion")).to_contain_text("skill_disabled")
    expect(campo).to_have_value("/pesquisa:revisar meu texto antes de escolher")
