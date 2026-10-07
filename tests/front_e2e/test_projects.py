from playwright.sync_api import expect

from .test_capabilities import TOKEN_INIT


def create(page, name, instructions):
    page.get_by_role("button", name="Novo projeto", exact=True).click()
    dialog = page.get_by_role("dialog")
    dialog.get_by_role("textbox", name="Nome do projeto", exact=True).fill(name)
    dialog.get_by_role("textbox", name="Instruções do projeto", exact=True).fill(instructions)
    dialog.get_by_role("button", name="Criar projeto", exact=True).click()
    expect(page.locator("#project-detail h3")).to_have_text(name)


def test_projects_keyboard_context_move_drafts_archive_and_narrow(abrir, novo_backend):
    url, app, _, _ = novo_backend()
    page = abrir("#/projetos", url=url, init=TOKEN_INIT, axe=True)
    expect(page.locator("#project-status")).to_contain_text("começar")
    create(page, "Pesquisa A", "AMBER · sempre citar fontes")
    page.get_by_role("button", name="Usar este projeto", exact=True).focus()
    page.keyboard.press("Enter")
    expect(page.locator("#project-context")).to_have_text("Projeto · Pesquisa A")
    page.locator("#composer-input").fill("rascunho A")
    session_a = page.evaluate("Orion.historico.sessao()")
    page.get_by_role("link", name="Projetos", exact=True).click()
    create(page, "Pesquisa B", "BLUE · comparar evidências")
    page.get_by_role("button", name="Usar este projeto", exact=True).click()
    expect(page.locator("#project-context")).to_have_text("Projeto · Pesquisa B")
    expect(page.locator("#composer-input")).to_have_value("")
    page.locator("#composer-input").fill("rascunho B")
    page.get_by_role("link", name="Projetos", exact=True).click()
    page.get_by_role("button", name="Pesquisa A", exact=True).click()
    page.get_by_role("button", name="Usar este projeto", exact=True).click()
    expect(page.locator("#composer-input")).to_have_value("rascunho A")
    assert page.evaluate("Orion.historico.sessao()") == session_a
    page.locator("#sb-new").click()
    page.wait_for_function("(sid) => Orion.historico.sessao() !== sid", arg=session_a)
    expect(page.locator("#project-context")).to_have_text("Projeto · Pesquisa A")
    page.get_by_role("link", name="Projetos", exact=True).click()
    page.get_by_role("button", name="Conversar sem projeto", exact=True).click()
    expect(page.locator("#project-context")).to_have_text("Pessoal · sem projeto")
    page.evaluate("(sid) => { Orion.projects.mover(sid); }", session_a)
    dialog = page.get_by_role("dialog")
    project_b = next(
        p
        for p in app.state.orion.memory.query("SELECT id,name FROM projects")
        if p["name"] == "Pesquisa B"
    )
    dialog.get_by_role("combobox", name="Destino da conversa").select_option(
        "project:" + project_b["id"]
    )
    with page.expect_response(lambda r: r.url.endswith("/projects/sessions/" + session_a)) as moved:
        dialog.get_by_role("button", name="Mover conversa", exact=True).click()
    assert moved.value.ok
    expect(page.locator("#dialog-root [role=dialog]")).to_have_count(0)
    page.get_by_role("link", name="Projetos", exact=True).click()
    page.get_by_role("button", name="Pesquisa B", exact=True).click()
    expect(page.locator("#project-detail")).to_contain_text("BLUE")
    assert app.state.orion.memory.get_session(session_a).project_id == project_b["id"]
    page.set_viewport_size({"width": 700, "height": 780})
    page.wait_for_function(
        '() => getComputedStyle(document.querySelector("#view-projetos")).opacity === "1"'
    )
    assert page.evaluate("document.documentElement.scrollWidth <= innerWidth")
    assert not page.evaluate(
        "async () => (await axe.run(document, {runOnly: {type:'tag', values:['wcag2a','wcag2aa']}})).violations"
    )
    page.screenshot(path="/workspace/artifacts/orion-c33-projects-700.png", full_page=True)
    page.get_by_role("button", name="Arquivar projeto", exact=True).click()
    page.locator("#project-archived").check()
    expect(page.get_by_role("button", name="Pesquisa B", exact=True)).to_be_visible()
    expect(page.get_by_role("button", name="Restaurar projeto", exact=True)).to_be_visible()
