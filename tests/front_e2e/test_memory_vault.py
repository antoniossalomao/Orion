from playwright.sync_api import expect

from tests.fakes import FakeGateway

from .test_capabilities import TOKEN_INIT


def test_memory_vault_builtin_install_use_source_and_no_write(abrir, novo_backend):
    url, app, _, _ = novo_backend(gateway_override=FakeGateway())
    s = app.state.orion
    session = s.memory.new_session("web")
    s.memory.add_message(session.id, "user", "Sintetizar notas")
    s.memory.add_message(
        session.id,
        "assistant",
        "Síntese preservada",
        provenance={"memoria": [{"fonte": "upload:" + "a" * 32 + "/nota.md"}]},
    )
    page = abrir("#/integracoes", url=url, init=TOKEN_INIT, axe=True)
    suggested = page.locator("article").filter(
        has=page.get_by_role("heading", name="Orion Memória e Vault", exact=True)
    )
    suggested.get_by_role("button", name="Adicionar", exact=True).click()
    card = page.locator('[data-plugin="orion-memoria-vault"]')
    card.get_by_role("button", name="Ativar", exact=True).click()
    dialog = page.get_by_role("dialog")
    for name in ["native:buscar_memoria", "native:listar_fatos"]:
        dialog.get_by_role("checkbox", name=name, exact=True).check()
    dialog.get_by_role("button", name="Ativar", exact=True).click()
    expect(card).to_contain_text("Ativo")
    page.get_by_role("tab", name="Skills", exact=True).click()
    skill = page.locator("article").filter(
        has=page.get_by_role("heading", name="retomar-contexto", exact=True)
    )
    skill.get_by_role("button", name="Usar no chat", exact=True).click()
    expect(page.locator("#composer-input")).to_have_value("/orion-memoria-vault:retomar-contexto ")
    page.locator("summary").filter(has_text="Fontes e atividade").click()
    expect(page.locator(".history-sources")).to_contain_text("Memória: nota.md")
    expect(page.locator(".history-sources")).not_to_contain_text("a" * 32)
    assert len(s.memory.history(session.id)) == 2
    page.set_viewport_size({"width": 700, "height": 780})
    page.wait_for_function(
        '() => getComputedStyle(document.querySelector("#view-chat")).opacity === "1"'
    )
    assert not page.evaluate("async () => (await axe.run(document)).violations")
