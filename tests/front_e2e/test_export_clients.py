from playwright.sync_api import expect

from .test_capabilities import TOKEN_INIT


def test_external_client_issue_once_list_and_revoke(abrir, novo_backend):
    url, app, _, _ = novo_backend()
    page = abrir("#/config", url=url, init=TOKEN_INIT, axe=True)
    page.get_by_role("button", name="Autorizar cliente externo", exact=True).click()
    dialog = page.locator('#dialog-root [role="dialog"]')
    dialog.get_by_label("Nome do cliente").fill("Cliente de ensaio")
    dialog.get_by_role("checkbox", name="Ler fatos", exact=True).check()
    dialog.get_by_role("button", name="Gerar credencial", exact=True).click()
    expect(dialog.get_by_label("Credencial do cliente")).to_have_value(
        __import__("re").compile(r"^orion_client_")
    )
    dialog.get_by_role("button", name="Guardei a credencial", exact=True).click()
    card = page.locator("[data-export-client]")
    expect(card).to_contain_text("Cliente de ensaio")
    expect(card).not_to_contain_text("orion_client_")
    card.get_by_role("button", name="Revogar acesso", exact=True).click()
    dialog.get_by_role("button", name="Revogar acesso", exact=True).click()
    expect(card).to_contain_text("Revogado")
    assert app.state.orion.export_credentials.listing()[0]["revoked"] == 1
    page.set_viewport_size({"width": 700, "height": 780})
    page.wait_for_function(
        '() => getComputedStyle(document.querySelector("#view-config")).opacity === "1"'
    )
    v = page.evaluate(
        "async () => (await axe.run(document)).violations.map(x=>[x.id,x.nodes.map(n=>n.target)])"
    )
    assert not v, v
