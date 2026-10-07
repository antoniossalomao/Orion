from playwright.sync_api import expect

from .test_capabilities import TOKEN_INIT


def test_restaurar_paginar_fontes_e_exportar(abrir, novo_backend, tmp_path):
    url, app, _, _ = novo_backend()
    memory = app.state.orion.memory
    session = memory.new_session("web", "Histórico extenso")
    for i in range(125):
        memory.add_message(
            session.id,
            "assistant" if i % 2 else "user",
            f"Mensagem {i:03}",
            provenance={"memoria": [{"fonte": "<fonte>.md"}]} if i % 2 else None,
        )
    page = abrir("#/chat", url=url, init=TOKEN_INIT, axe=True)
    expect(page.locator(".msg")).to_have_count(50)
    expect(page.locator(".msg").last).to_contain_text("Mensagem 124")
    expect(page.locator(".history-sources").first).to_contain_text("<fonte>.md")
    assert page.locator("fonte").count() == 0
    page.get_by_role("button", name="Carregar mensagens anteriores").click()
    expect(page.locator(".msg")).to_have_count(100)
    page.get_by_role("button", name="Carregar mensagens anteriores").click()
    expect(page.locator(".msg")).to_have_count(125)
    expect(page.locator(".msg").first).to_contain_text("Mensagem 000")
    expect(page.get_by_role("button", name="Carregar mensagens anteriores")).to_have_count(0)
    assert page.locator(".msg time[datetime]").count() == 125
    with page.expect_download() as download:
        page.evaluate("Orion.acoes.exportar()")
    path = tmp_path / "conversa.md"
    download.value.save_as(path)
    texto = path.read_text()
    assert texto.index("Mensagem 000") < texto.index("Mensagem 124")
    page.wait_for_timeout(500)
    assert page.evaluate("async () => (await axe.run(document)).violations.map(v => v.id)") == []
    page.reload()
    page.wait_for_selector("html[data-pronto='true']")
    expect(page.locator(".msg")).to_have_count(50)


def test_limpar_preserva_registro_e_outra_conversa(abrir, novo_backend):
    url, app, _, _ = novo_backend()
    m = app.state.orion.memory
    outra = m.new_session("web", "Outra")
    m.add_message(outra.id, "user", "Outra mensagem")
    atual = m.new_session("web", "Atual")
    m.add_message(atual.id, "user", "Contexto a limpar")
    fato = m.add_fact("Memória permanente", "manual")
    page = abrir("#/chat", url=url, init=TOKEN_INIT)
    expect(page.locator(".msg-user")).to_contain_text("Contexto a limpar")
    page.evaluate("() => { Orion.acoes.limpar(); }")
    page.get_by_role("button", name="Limpar", exact=True).click()
    expect(page.locator(".msg")).to_have_count(0)
    assert m.history(atual.id)[0].text == "Contexto a limpar"
    assert m.history(outra.id)[0].text == "Outra mensagem"
    assert m.facts() == [fato]
    page.get_by_role("button", name="Ver registro completo").click()
    expect(page.locator(".msg-user")).to_contain_text("Contexto a limpar")
    page.fill("#composer-input", "rascunho")
    expect(page.locator("#btn-send")).to_be_disabled()
    page.get_by_role("button", name="Voltar à conversa atual").click()
    expect(page.locator(".msg")).to_have_count(0)
    expect(page.locator("#btn-send")).to_be_enabled()
    page.reload()
    page.wait_for_selector("html[data-pronto='true']")
    expect(page.locator(".msg")).to_have_count(0)
    page.click(f'.conv[data-id="{outra.id}"]')
    expect(page.locator(".msg-user")).to_contain_text("Outra mensagem")


def test_importada_exporta_conversa_visualizada_e_bloqueia_envio(abrir, novo_backend, tmp_path):
    url, app, _, caminhos = novo_backend()
    m = app.state.orion.memory
    atual = m.new_session("web", "Ativa")
    m.add_message(atual.id, "user", "Texto da ativa")
    antiga, _ = m.import_session("legado", "web", "Importada", 1)
    m.import_message(antiga, "m", "user", "Texto da importada", 2)
    page = abrir("#/chat", url=url, init=TOKEN_INIT)
    expect(page.locator(".msg-user")).to_contain_text("Texto da ativa")
    page.click(f'.conv[data-id="{antiga}"]')
    expect(page.locator(".msg-user")).to_contain_text("Texto da importada")
    page.fill("#composer-input", "não enviar")
    expect(page.locator("#btn-send")).to_be_disabled()
    page.keyboard.press("Enter")
    assert "/chat" not in caminhos
    with page.expect_download() as download:
        page.evaluate("Orion.acoes.exportar()")
    path = tmp_path / "importada.md"
    download.value.save_as(path)
    assert "Texto da importada" in path.read_text() and "Texto da ativa" not in path.read_text()
    assert m.selected_session("web").id == atual.id
    page.click(f'.conv[data-id="{atual.id}"]')
    expect(page.locator(".msg-user")).to_contain_text("Texto da ativa")
    page.fill("#composer-input", "Enviar agora")
    expect(page.locator("#btn-send")).to_be_enabled()


def test_falha_ao_carregar_sessao_bloqueia_envio_sem_desabilitar_historico(abrir, novo_backend):
    url, app, _, _ = novo_backend()
    m = app.state.orion.memory
    outra = m.new_session("web", "Outra")
    m.add_message(outra.id, "user", "Outra mensagem")
    atual = m.new_session("web", "Atual")
    m.add_message(atual.id, "user", "Mensagem atual")
    page = abrir("#/chat", url=url, init=TOKEN_INIT, http_ok=True)
    expect(page.locator(".msg-user")).to_contain_text("Mensagem atual")
    page.route(
        f"**/historico?sessao={outra.id}*",
        lambda r: r.fulfill(status=404, json={"detail": "sessão inexistente"}),
    )
    page.click(f'.conv[data-id="{outra.id}"]')
    expect(page.locator(".toast")).to_contain_text("Não consegui abrir")
    page.fill("#composer-input", "Não enviar na sessão errada")
    expect(page.locator("#btn-send")).to_be_disabled()
    assert page.evaluate("Orion.api.suporta('history')") is True
    page.unroute(f"**/historico?sessao={outra.id}*")
    page.click(f'.conv[data-id="{outra.id}"]')
    expect(page.locator(".msg-user")).to_contain_text("Outra mensagem")
    page.fill("#composer-input", "Pedido na conversa correta")
    expect(page.locator("#btn-send")).to_be_enabled()
