from playwright.sync_api import expect

from .test_capabilities import TOKEN_INIT


def test_menu_teclado_polling_foco_e_persistencia(abrir, novo_backend):
    url, app, _, _ = novo_backend()
    m = app.state.orion.memory
    sid = m.new_session("web", "Minha conversa").id
    m.add_message(sid, "user", "Meu texto")
    page = abrir("#/chat", url=url, init=TOKEN_INIT, axe=True)
    expect(page.locator(".msg-user")).to_contain_text("Meu texto")
    mais = page.get_by_role("button", name="Opções de Minha conversa")
    mais.focus()
    page.keyboard.press("ArrowDown")
    expect(page.get_by_role("menuitem", name="Renomear")).to_be_focused()
    page.evaluate("Orion.sidebar.carregar()")
    expect(page.get_by_role("menuitem", name="Renomear")).to_be_focused()
    page.keyboard.press("Enter")
    expect(page.get_by_role("textbox", name="Título da conversa")).to_be_focused()
    page.fill('[aria-label="Título da conversa"]', "Nome persistente")
    page.keyboard.press("Enter")
    expect(page.locator(".conv-title")).to_have_text("Nome persistente")
    mais = page.get_by_role("button", name="Opções de Nome persistente")
    mais.click()
    page.get_by_role("menuitem", name="Fixar", exact=True).click()
    expect(page.locator(".conv-group").first).to_have_text("Fixadas")
    mais.focus()
    page.evaluate("Orion.sidebar.carregar()")
    expect(mais).to_be_focused()
    page.reload()
    page.wait_for_selector("html[data-pronto='true']")
    expect(page.locator(".conv-title")).to_have_text("Nome persistente")
    expect(page.locator(".conv-ro")).to_have_text("fixada")
    page.get_by_role("button", name="Opções de Nome persistente").click()
    page.get_by_role("menuitem", name="Arquivar").click()
    expect(page.locator(".msg-system")).to_contain_text("somente leitura")
    expect(page.locator(".conv-group")).to_have_text("Arquivadas")
    assert m.history(sid)[0].text == "Meu texto"
    assert m.selected_session("web") is None
    page.get_by_role("button", name="Opções de Nome persistente").click()
    page.get_by_role("menuitem", name="Restaurar").click()
    page.click(f'.conv[data-id="{sid}"]')
    expect(page.locator(".msg-user")).to_contain_text("Meu texto")
    page.fill("#composer-input", "posso continuar")
    expect(page.locator("#btn-send")).to_be_enabled()
    page.wait_for_timeout(500)
    assert page.evaluate("async () => (await axe.run(document)).violations.map(v => v.id)") == []
