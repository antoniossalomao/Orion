from playwright.sync_api import expect

from .test_capabilities import TOKEN_INIT


def test_buscar_corpo_abrir_por_teclado_e_zero_resultados(abrir, novo_backend):
    url, app, _, _ = novo_backend()
    m = app.state.orion.memory
    sid = m.new_session("web", "Título sem o termo").id
    m.add_message(sid, "user", "O ornitorrinco está no corpo da mensagem")
    m.new_session("web", "Atual")
    m.add_message(m.new_session("telegram", "ornitorrinco").id, "user", "Canário de outro canal")
    page = abrir("#/chat", url=url, init=TOKEN_INIT, axe=True)
    page.fill("#sb-search", "ornitorrinco")
    expect(page.locator(".conv")).to_have_count(1)
    expect(page.locator(".conv-snippet")).to_contain_text("ornitorrinco")
    expect(page.locator("#sb-convs-list")).not_to_contain_text("Canário")
    page.keyboard.press("Enter")
    expect(page.locator(".conv")).to_be_focused()
    page.keyboard.press("Enter")
    expect(page.locator(".msg-user")).to_contain_text("ornitorrinco")
    assert m.selected_session("web").id == sid
    page.get_by_role("button", name="Opções de Título sem o termo").click()
    page.get_by_role("menuitem", name="Renomear").click()
    page.fill('[aria-label="Título da conversa"]', "Nome atualizado")
    page.keyboard.press("Enter")
    expect(page.locator(".conv-title")).to_have_text("Nome atualizado")
    page.fill("#sb-search", "sem resultado inexistente")
    expect(page.locator("#sb-convs-list")).to_contain_text("Nada encontrado")
    page.keyboard.press("Escape")
    expect(page.locator(".conv")).to_have_count(2)
    page.wait_for_timeout(500)
    assert page.evaluate("async () => (await axe.run(document)).violations.map(v => v.id)") == []


def test_paginar_busca_sem_repetir_conversas(abrir, novo_backend):
    url, app, _, _ = novo_backend(gateway=False)
    m = app.state.orion.memory
    for i in range(28):
        sid = m.new_session("web", f"Nota {i}").id
        m.add_message(sid, "user", f"ornitorrinco de número {i}")
    page = abrir("#/chat", url=url, init=TOKEN_INIT)
    page.fill("#sb-search", "ornitorrinco")
    expect(page.locator(".conv")).to_have_count(25)
    page.get_by_role("button", name="Mais conversas").click()
    expect(page.locator(".conv")).to_have_count(28)
    expect(page.get_by_role("button", name="Mais conversas")).to_have_count(0)
    assert len(set(page.locator(".conv").evaluate_all("(ns) => ns.map(n => n.dataset.id)"))) == 28
