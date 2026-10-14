"""E11 no navegador, contra o Orion de verdade: a página da legenda (sem login, com a legenda
desligada) e o botão de câmera na voz ao vivo (mensagem de voz desligada)."""

from playwright.sync_api import expect

from .test_capabilities import TOKEN_INIT


def test_legenda_pede_login_quando_nao_ha_sessao(abrir, novo_backend):
    url, _, _, _ = novo_backend()
    # o 401 de /legenda é esperado (sem sessão): http_ok deixa o console mostrar esse erro
    page = abrir(pagina="/ui/legenda.html", url=url, http_ok=True)
    expect(page.locator("#linhas")).to_contain_text("Entre na interface do Orion")
    assert page.evaluate("() => !!window.OrionLegendaLogica")  # o script da lógica carregou


def test_botao_de_camera_na_voz_avisa_que_precisa_da_voz_ligada(abrir, novo_backend):
    url, _, _, _ = novo_backend()
    page = abrir("#/chat", url=url, init=TOKEN_INIT)
    botao = page.locator("#composer-box .voice-camera-btn")
    expect(botao).to_have_attribute("aria-pressed", "false")
    botao.click()
    expect(page.get_by_text("Ligue a voz ao vivo antes de ligar a câmera.")).to_be_visible()
    expect(botao).to_have_attribute("aria-pressed", "false")
