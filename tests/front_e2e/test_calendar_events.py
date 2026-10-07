import httpx
from playwright.sync_api import expect

from orion.calendar import Binding
from tests.extensions.test_calendar_events import EventConnector

from .test_capabilities import TOKEN_INIT


def test_event_product_review_reject_then_create_exactly_once(abrir, novo_backend):
    url, app, _, _ = novo_backend()
    s = app.state.orion
    session = s.memory.new_session("web")
    s.memory.add_message(session.id, "user", "Preparar reunião, sem criar até revisar")
    conn = EventConnector("personal")
    s.mcp.connections["agenda"] = conn
    from .conftest import TOKEN

    response = httpx.post(
        url + "/calendar/bind",
        headers={"Authorization": "Bearer " + TOKEN},
        json=Binding(connection_id="agenda", account="work", reviewed_read_only=True).model_dump(),
    )
    assert response.status_code == 200, response.text
    page = abrir("#/integracoes", url=url, init=TOKEN_INIT, axe=True)
    page.get_by_role("tab", name="Conexões MCP").click()
    page.get_by_role("button", name="Propor evento", exact=True).click()
    page.get_by_role("textbox", name="Título", exact=True).fill("Reunião revisada")
    page.get_by_role("textbox", name="Início com fuso", exact=True).fill(
        "2026-10-07T09:00:00-03:00"
    )
    page.get_by_role("textbox", name="Fim com fuso", exact=True).fill("2026-10-07T10:00:00-03:00")
    page.get_by_role("button", name="Salvar proposta").click()
    card = page.locator("[data-event-proposal]")
    expect(card).to_contain_text("Reunião revisada")
    expect(card).to_contain_text("America/Sao_Paulo")
    assert conn.calls == []
    card.get_by_role("button", name="Revisar criação").click()
    page.get_by_role("dialog").get_by_role("button", name="Pedir aprovação").click()
    card.get_by_role("button", name="Rejeitar criação").click()
    expect(card.get_by_role("button", name="Revisar criação")).to_be_visible()
    assert conn.calls == []
    card.get_by_role("button", name="Revisar criação").click()
    page.get_by_role("dialog").get_by_role("button", name="Pedir aprovação").click()
    card.get_by_role("button", name="Confirmar criação").click()
    dialog = page.get_by_role("dialog")
    expect(dialog).to_contain_text("Reunião revisada")
    expect(dialog).to_contain_text("work")
    expect(dialog).to_contain_text("Sem participantes ou convites.")
    dialog.get_by_role("button", name="Confirmar criação").click()
    expect(card).to_contain_text("Evento criado.")
    assert len(conn.calls) == 1
    page.reload()
    page.get_by_role("tab", name="Conexões MCP").click()
    expect(card).to_contain_text("Evento criado.")
    assert len(conn.calls) == 1
    page.set_viewport_size({"width": 700, "height": 780})
    page.wait_for_function(
        '() => getComputedStyle(document.querySelector("#view-integracoes")).opacity === "1"'
    )
    assert not page.evaluate("async () => (await axe.run(document)).violations")
    page.screenshot(path="/workspace/artifacts/orion-c42-agenda-700.png")
