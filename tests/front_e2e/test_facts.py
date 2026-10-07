from playwright.sync_api import expect

from .test_capabilities import TOKEN_INIT


def test_correct_fact_source_reopen_and_forget_only_after_approval(abrir, novo_backend):
    url, app, _, _ = novo_backend()
    memory = app.state.orion.memory
    memory.new_session("web")
    fact = memory.add_fact("AMBER fato original", "Nota de ensaio")
    page = abrir("#/memoria", url=url, init=TOKEN_INIT, axe=True)
    card = page.locator(f'[data-fact-id="{fact.id}"]')
    expect(card).to_contain_text("Nota de ensaio")
    page.get_by_role("searchbox", name="Buscar fatos").fill("AMBER")
    card.get_by_role("button", name="Corrigir", exact=True).click()
    dialog = page.get_by_role("dialog")
    dialog.get_by_role("textbox", name="Texto do fato").fill("AMBER fato corrigido")
    dialog.get_by_role("button", name="Pedir aprovação", exact=True).click()
    approval = page.locator("[data-fact-approval]")
    expect(approval).to_contain_text("AMBER fato corrigido")
    assert memory.facts()[0].text == "AMBER fato original"
    approval.get_by_role("button", name="Aprovar e executar").click()
    expect(card).to_contain_text("AMBER fato corrigido")
    expect(card).to_contain_text("Nota de ensaio")
    page.reload()
    expect(card).to_contain_text("AMBER fato corrigido")
    card.get_by_role("button", name="Esquecer", exact=True).click()
    page.get_by_role("dialog").get_by_role("button", name="Pedir aprovação").click()
    expect(approval).to_contain_text("Remoção definitiva")
    approval.get_by_role("button", name="Rejeitar").click()
    expect(approval).to_have_count(0)
    assert memory.facts()
    card.get_by_role("button", name="Esquecer", exact=True).click()
    page.get_by_role("dialog").get_by_role("button", name="Pedir aprovação").click()
    approval.get_by_role("button", name="Aprovar e executar").click()
    expect(card).to_have_count(0)
    assert not memory.facts()
    page.set_viewport_size({"width": 700, "height": 780})
    page.wait_for_function(
        '() => getComputedStyle(document.querySelector("#view-memoria")).opacity === "1"'
    )
    assert page.evaluate("document.documentElement.scrollWidth <= innerWidth")
    assert not page.evaluate("async () => (await axe.run(document)).violations")
