from playwright.sync_api import expect

from orion.policy import PathGuard

from .test_capabilities import TOKEN_INIT


def test_review_reject_copy_originals_and_accessibility(abrir, novo_backend, tmp_path):
    folder = tmp_path / "authorized"
    folder.mkdir()
    (folder / "nota.md").write_text("Original permanece")
    url, app, _, _ = novo_backend()
    s = app.state.orion
    s.memory.new_session("web")
    s.policy.path_guard = PathGuard(safe_roots=(folder,))
    s.file_plans.guard = s.policy.path_guard
    page = abrir("#/fontes", url=url, init=TOKEN_INIT, axe=True)
    page.get_by_role("button", name="Planejar cópias", exact=True).click()
    dialog = page.locator('#dialog-root [role="dialog"]')
    dialog.get_by_label("Pasta autorizada").fill(str(folder))
    dialog.get_by_role("button", name="Preparar plano", exact=True).click()
    card = page.locator("[data-file-plan]")
    expect(card).to_contain_text("nota.md → Organizados/Textos/nota.md")
    assert not (folder / "Organizados").exists()
    card.get_by_role("button", name="Revisar cópias", exact=True).click()
    dialog.get_by_role("button", name="Pedir aprovação", exact=True).click()
    card.get_by_role("button", name="Rejeitar cópias", exact=True).click()
    expect(card.get_by_role("button", name="Revisar cópias", exact=True)).to_be_visible()
    assert not (folder / "Organizados").exists()
    card.get_by_role("button", name="Revisar cópias", exact=True).click()
    dialog.get_by_role("button", name="Pedir aprovação", exact=True).click()
    card.get_by_role("button", name="Criar cópias revisadas", exact=True).click()
    dialog.get_by_role("button", name="Confirmar cópias", exact=True).click()
    expect(card).to_contain_text("Cópias criadas. Originais preservados.")
    assert (folder / "nota.md").read_bytes() == (folder / "Organizados/Textos/nota.md").read_bytes()
    page.reload()
    expect(page.locator("[data-file-plan]")).to_contain_text("1 concluída(s)")
    page.set_viewport_size({"width": 700, "height": 780})
    page.wait_for_function(
        '() => getComputedStyle(document.querySelector("#view-fontes")).opacity === "1"'
    )
    assert not page.evaluate("async () => (await axe.run(document)).violations")
