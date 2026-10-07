from playwright.sync_api import expect

from .test_capabilities import TOKEN_INIT


def test_account_review_scope_and_revoke_preserves_history(abrir, novo_backend):
    url, app, _, _ = novo_backend()
    memory = app.state.orion.memory
    session = memory.new_session("web")
    memory.add_message(session.id, "user", "Conversa preservada")
    page = abrir("#/integracoes", url=url, init=TOKEN_INIT, axe=True)
    page.get_by_role("tab", name="Conexões MCP").click()
    page.get_by_role("button", name="Adicionar conta MCP", exact=True).click()
    page.get_by_role("textbox", name="Nome da conta").fill("Conta de ensaio")
    page.get_by_role("textbox", name="Servidor MCP da conta").fill("https://mcp.example/mcp")
    page.get_by_role("textbox", name="Escopos OAuth solicitados").fill("calendar.read")
    page.get_by_role("button", name="Adicionar conta", exact=True).click()
    card = page.locator("[data-account-id]")
    expect(card).to_contain_text("calendar.read")
    card.get_by_role("button", name="Revogar conta").click()
    page.get_by_role("dialog").get_by_role("button", name="Revogar", exact=True).click()
    expect(card).to_contain_text("revoked")
    assert memory.history(session.id)[0].text == "Conversa preservada"
    page.set_viewport_size({"width": 700, "height": 780})
    page.wait_for_function(
        '() => getComputedStyle(document.querySelector("#view-integracoes")).opacity === "1"'
    )
    assert not page.evaluate("async () => (await axe.run(document)).violations")
