import json

import pytest
from playwright.sync_api import expect

from .test_capabilities import TOKEN_INIT


def package(tmp_path):
    root = tmp_path / "package"
    (root / "skills/revisar").mkdir(parents=True)
    (root / "manifest.json").write_text(
        json.dumps(
            {
                "id": "pesquisa",
                "name": "Orion Pesquisa",
                "version": "1.0.0",
                "description": "Compare fontes e organize uma pesquisa com evidências.",
                "license": "MIT",
                "skills": ["skills/revisar"],
                "capabilities": ["native:buscar_memoria"],
            }
        )
    )
    (root / "skills/revisar/SKILL.md").write_text(
        "---\nname: revisar\ndescription: Revisar fontes pesquisa\n---\nCompare fontes e cite evidências."
    )
    return root


@pytest.mark.parametrize("width,theme", [(1440, "noite"), (700, "grafite"), (700, "contraste")])
def test_catalog_keyboard_permissions_skills_and_deactivate(
    abrir, novo_backend, tmp_path, width, theme
):
    url, _, _, _ = novo_backend()
    root = package(tmp_path)
    page = abrir("#/integracoes", url=url, init=TOKEN_INIT, viewport=(width, 900), axe=True)
    page.evaluate("theme => Orion.prefs.set('theme', theme)", theme)
    expect(page.get_by_role("heading", name="Seu Orion pode ir além")).to_be_visible()
    page.get_by_text("Pacote de desenvolvimento", exact=True).click()
    page.get_by_role("button", name="Instalar por pasta").click()
    page.get_by_role("textbox", name="Caminho absoluto da pasta").fill(str(root))
    page.get_by_role("button", name="Instalar", exact=True).click()
    card = page.locator('[data-plugin="pesquisa"]')
    expect(card).to_contain_text("Desativado")
    card.get_by_role("button", name="Ativar", exact=True).click()
    modal = page.get_by_role("dialog")
    expect(modal).to_contain_text("revisão")
    modal.get_by_role("checkbox", name="native:buscar_memoria").check()
    modal.get_by_role("button", name="Ativar", exact=True).click()
    expect(card).to_contain_text("Ativo")
    tab = page.get_by_role("tab", name="Plugins", exact=True)
    tab.focus()
    page.keyboard.press("ArrowRight")
    expect(page.get_by_role("tab", name="Skills", exact=True)).to_be_focused()
    expect(page.get_by_role("button", name="Usar no chat")).to_be_visible()
    assert page.evaluate("document.documentElement.scrollWidth <= innerWidth")
    result = page.evaluate(
        "async () => (await axe.run(document, {runOnly: {type: 'tag', values: ['wcag2a','wcag2aa']}})).violations"
    )
    assert not result, result
    page.get_by_role("button", name="Usar no chat").click()
    expect(page.locator("#composer-input")).to_have_value("/pesquisa:revisar ")
    expect(page.locator("#composer-input")).to_be_focused()
    page.locator("#composer-input").fill("/pesquisa:revisar meu rascunho")
    page.locator('.sb-item[data-view="integracoes"]').click()
    page.get_by_role("tab", name="Plugins", exact=True).click()
    card.get_by_role("button", name="Desativar").click()
    expect(card).to_contain_text("Desativado")
    page.locator('.sb-item[data-view="chat"]').click()
    expect(page.locator("#composer-input")).to_have_value("/pesquisa:revisar meu rascunho")


def test_plugin_api_error_can_retry_without_repeated_popup(abrir, novo_backend):
    url, _, _, _ = novo_backend()
    page = abrir("#/integracoes", url=url, init=TOKEN_INIT, http_ok=True)
    expect(page.get_by_role("heading", name="Seu Orion pode ir além")).to_be_visible()
    page.route(
        "**/plugins", lambda route: route.fulfill(status=503, json={"detail": "fixture_offline"})
    )
    page.get_by_role("tab", name="Skills", exact=True).click()
    page.get_by_role("tab", name="Plugins", exact=True).click()
    expect(page.get_by_role("button", name="Tentar novamente")).to_be_visible()
    page.unroute("**/plugins")
    page.get_by_role("button", name="Tentar novamente").click()
    expect(page.get_by_role("heading", name="Seu Orion pode ir além")).to_be_visible()
