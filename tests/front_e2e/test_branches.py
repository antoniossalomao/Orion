from playwright.sync_api import expect

from .test_capabilities import TOKEN_INIT


def test_edit_request_creates_draft_path_preserves_original_without_execution(abrir, novo_backend):
    url, app, _, _ = novo_backend()
    state = app.state.orion
    original = state.memory.new_session("web", "Ensaio")
    state.memory.add_message(original.id, "user", "Pedido original")
    state.memory.add_message(original.id, "assistant", "Resposta preservada")
    page = abrir("#/chat", url=url, init=TOKEN_INIT)
    page.get_by_role("button", name="Editar em novo caminho").click()
    page.get_by_role("textbox", name="Pedido revisado").fill("Pedido corrigido")
    page.get_by_role("button", name="Criar caminho").click()
    expect(page.locator("#composer-input")).to_have_value("Pedido corrigido")
    expect(page.locator("#chat-col")).not_to_contain_text("Resposta preservada")
    page.get_by_role("button", name="Caminhos", exact=True).click()
    page.get_by_role("combobox", name="Caminho da conversa").select_option(original.id)
    page.get_by_role("button", name="Abrir caminho").click()
    expect(page.locator("#chat-col")).to_contain_text("Pedido original")
    expect(page.locator("#chat-col")).to_contain_text("Resposta preservada")
    assert len(state.memory.history(original.id)) == 2
