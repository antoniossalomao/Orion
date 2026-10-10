from playwright.sync_api import expect

from .test_capabilities import TOKEN_INIT


def test_activity_persistent_read_decision_and_no_reconnection_popup(abrir, novo_backend):
    url, app, _, _ = novo_backend()
    state = app.state.orion
    session = state.memory.new_session("web")
    fact = state.memory.add_fact("Nota de atividade", "ensaio")
    from orion.policy import Context, ToolCall

    state.policy.evaluate(ToolCall("esquecer_fato", {"id": fact.id}), Context(session.id))
    state.ops.notify("lembrete", "AMBER aviso único", ref="ensaio:1")
    page = abrir("#/atividade", url=url, init=TOKEN_INIT, axe=True)
    row = page.locator("[data-notification-id]")
    expect(row).to_contain_text("AMBER aviso único")
    expect(page.get_by_role("button", name="Revisar decisão")).to_be_visible()
    row.get_by_role("button", name="Marcar como lido").click()
    expect(row).to_contain_text("Lido")
    page.reload()
    expect(row).to_contain_text("Lido")
    expect(row.get_by_role("button")).to_have_count(0)
    page.get_by_role("combobox", name="Filtrar atividade").select_option("unread")
    expect(row).to_have_count(0)
    expect(page.get_by_role("button", name="Revisar decisão")).to_be_visible()
    page.set_viewport_size({"width": 700, "height": 780})
    page.wait_for_function(
        '() => getComputedStyle(document.querySelector("#view-atividade")).opacity === "1"'
    )
    assert not page.evaluate("async () => (await axe.run(document)).violations")
