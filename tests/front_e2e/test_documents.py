from playwright.sync_api import expect

from .test_capabilities import TOKEN_INIT


def test_upload_error_retry_original_and_draft_preserved(abrir, novo_backend):
    url, _, _, _ = novo_backend()
    page = abrir("#/chat", url=url, init=TOKEN_INIT, axe=True)
    page.locator("#composer-input").fill("Meu rascunho preservado")
    page.evaluate("Orion.app.ir('fontes')")
    page.get_by_label("Adicionar documento").set_input_files(
        {"name": "nota.md", "mimeType": "text/markdown", "buffer": b"# AMBER fonte"}
    )
    page.get_by_role("button", name="Enviar e indexar").click()
    expect(page.locator("[data-document-id]")).to_contain_text("Indexado")
    page.get_by_label("Adicionar documento").set_input_files(
        {"name": "broken.pdf", "mimeType": "application/pdf", "buffer": b"broken"}
    )
    page.get_by_role("button", name="Enviar e indexar").click()
    expect(page.get_by_role("button", name="Tentar novamente")).to_be_visible()
    page.get_by_role("button", name="Tentar novamente").click()
    expect(page.get_by_role("button", name="Tentar novamente")).to_be_visible()
    page.set_viewport_size({"width": 700, "height": 780})
    page.wait_for_function(
        '() => getComputedStyle(document.querySelector("#view-fontes")).opacity === "1"'
    )
    assert not page.evaluate("async () => (await axe.run(document)).violations")
    page.evaluate("Orion.app.ir('chat')")
    expect(page.locator("#composer-input")).to_have_value("Meu rascunho preservado")
