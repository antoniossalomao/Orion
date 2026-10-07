import json

from playwright.sync_api import expect

from .conftest import TOKEN

TOKEN_INIT = f"localStorage.setItem('orion_token', JSON.stringify({json.dumps(TOKEN)}))"


def test_backend_novo_chat_sse_com_token_e_canal_web(abrir, novo_backend):
    url, _, gw, caminhos = novo_backend()
    page = abrir("#/chat", url=url, init=TOKEN_INIT)
    expect(page.locator("#conn-text")).to_have_text("Conectado")
    assert page.evaluate("Orion.api.estado().backend") == "orion"
    expect(page.locator("#model-btn")).to_be_disabled()
    expect(page.locator("#model-label")).to_have_text("Modelo do servidor")
    expect(page.locator("#btn-attach")).to_be_disabled()
    page.fill("#composer-input", "oi")
    with page.expect_request(lambda r: r.url == f"{url}/chat") as pedido:
        page.click("#btn-send")
    assert pedido.value.post_data_json == {"texto": "oi", "canal": "web"}
    assert pedido.value.headers["authorization"] == f"Bearer {TOKEN}"
    expect(page.locator(".msg-orion")).to_contain_text("Resposta do backend novo.")
    assert len(gw.chamadas) == 1
    assert "/sessoes" in caminhos and "/tts/mudo" not in caminhos


def test_backend_novo_aprovar_e_retomar_pelo_front(abrir, novo_backend):
    url, app, _, _ = novo_backend(approval=True)
    page = abrir("#/chat", url=url, init=TOKEN_INIT)
    page.fill("#composer-input", "Esqueça esse fato")
    page.click("#btn-send")
    aprovar = page.locator('[data-decisao="aprovar"]')
    expect(aprovar).to_be_enabled()
    assert len(app.state.orion.memory.facts()) == 1
    with page.expect_request(lambda r: r.url.endswith("/resume")) as pedido:
        aprovar.click()
    assert pedido.value.post_data_json == {"canal": "web"}
    expect(page.locator(".msg-orion").last).to_contain_text("Esqueci.")
    assert app.state.orion.memory.facts() == []


def test_api_online_sem_gateway_preserva_rascunho(abrir, novo_backend):
    url, _, gw, caminhos = novo_backend(gateway=False)
    page = abrir("#/chat", url=url, init=TOKEN_INIT)
    expect(page.locator("#conn-text")).to_have_text("Modelo indisponível")
    expect(page.locator("#conn-sub")).to_have_text("API disponível")
    page.fill("#composer-input", "Meu rascunho")
    page.keyboard.press("Enter")
    expect(page.locator("#composer-input")).to_have_value("Meu rascunho")
    expect(page.locator("#btn-send")).to_be_disabled()
    expect(page.locator("#chat-capabilities")).to_contain_text("rascunho")
    assert "/chat" not in caminhos and not gw.chamadas
    page.set_viewport_size({"width": 700, "height": 650})
    expect(page.locator("#composer-input")).to_be_in_viewport()
    assert page.evaluate("document.documentElement.scrollWidth <= innerWidth")
    assert (
        page.locator("#chat-capabilities").bounding_box()["y"]
        < page.locator("#composer-input").bounding_box()["y"]
    )


def test_backend_novo_sem_auth_explicita_acesso_indisponivel(abrir, novo_backend):
    url, _, _, caminhos = novo_backend(auth=False)
    page = abrir("#/chat", url=url)
    expect(page.locator("#chat-capabilities")).to_contain_text("acesso ao chat")
    page.evaluate("Orion.chat.carregarPendentes()")
    assert "/approvals" not in caminhos


def test_backend_novo_telas_sem_recursos_nao_sondam_endpoints(abrir, novo_backend):
    url, _, _, caminhos = novo_backend(gateway=False)
    page = abrir(url=url, init=TOKEN_INIT, axe=True)
    for rota, seletor in [
        ("integracoes", "#integ-grid"),
        ("memoria", "#mem-capabilities"),
        ("config", "#activity-capabilities"),
    ]:
        page.click(f'.sb-item[data-view="{rota}"]')
        if rota == "integracoes":
            page.get_by_role("tab", name="Voz e canais").click()
        expect(page.locator(seletor)).to_contain_text("indisponív")
    page.evaluate("""async () => {
        for (let i=0; i<3; i++) {
            await Orion.sidebar.carregar();
            await Orion.api.integracoes().catch(() => {});
            await Orion.api.metrics().catch(() => {});
        }
    }""")
    opcionais = {
        "/integracoes",
        "/metrics",
        "/stats",
        "/stats/historico",
        "/grafo/completo",
        "/memoria/categorias",
        "/tts/mudo",
    }
    assert not opcionais.intersection(caminhos)
    page.wait_for_function(
        "() => getComputedStyle(document.querySelector('#view-config')).opacity === '1'"
    )
    violacoes = page.evaluate("async () => (await axe.run(document)).violations.map(v => v.id)")
    assert violacoes == []


def test_404_opcional_legado_nao_repete_nem_muda_conexao(abrir):
    page = abrir(http_ok=True)
    chamadas = []
    page.route(
        "**/integracoes",
        lambda r: (
            chamadas.append(r.request.url),
            r.fulfill(status=404, json={"detail": "ausente"}),
        ),
    )
    page.click('.sb-item[data-view="integracoes"]')
    expect(page.locator("#integ-grid")).to_contain_text("indisponíveis")
    page.evaluate("""async () => {
        for (let i=0; i<3; i++) await Orion.api.integracoes().catch(() => {});
        await Orion.sidebar.verificar();
    }""")
    assert len(chamadas) == 1
    expect(page.locator("#conn-text")).to_have_text("Conectado")
    assert page.evaluate("Orion.api.suporta('integrations')") is False
    assert page.evaluate("Orion.api.suporta('notifications')") is False
    expect(page.locator(".toast")).to_have_count(0)


def test_trocar_backend_recalcula_capacidades(abrir, novo_backend):
    url, _, _, caminhos = novo_backend(gateway=False)
    page = abrir("#/chat", init=TOKEN_INIT)
    expect(page.locator("#model-btn")).to_be_enabled()
    # Proxy de mesma origem: o servidor real preserva sua política sem CORS aberto.
    base = page.url.split("/ui/")[0]
    page.route(
        "**/novo/**",
        lambda r: r.fulfill(response=r.fetch(url=url + r.request.url.split("/novo")[1])),
    )
    page.evaluate("url => Orion.prefs.set('base_url', url)", base + "/novo")
    page.wait_for_function("() => Orion.api.estado().backend === 'orion'")
    expect(page.locator("#model-btn")).to_be_disabled()
    page.evaluate("Orion.sidebar.carregar()")
    expect(page.locator("#sb-convs-list")).to_contain_text("Nenhuma conversa")
    assert "/sessoes" in caminhos


def test_backend_novo_criar_e_trocar_sessoes_pelo_front(abrir, novo_backend):
    url, app, _, _ = novo_backend()
    page = abrir("#/chat", url=url, init=TOKEN_INIT)
    expect(page.locator("#sb-new")).to_be_enabled()
    with page.expect_request(lambda r: r.method == "POST" and r.url.endswith("/sessoes")) as pedido:
        page.click("#sb-new")
    assert pedido.value.headers["authorization"] == f"Bearer {TOKEN}"
    expect(page.locator(".conv")).to_have_count(1)
    primeira = app.state.orion.memory.selected_session("web").id
    page.fill("#composer-input", "Mensagem da primeira conversa")
    page.click("#btn-send")
    expect(page.locator(".msg-orion")).to_contain_text("Resposta do backend novo.")
    page.wait_for_function("() => !Orion.chat.ocupado()")
    page.click("#sb-new")
    expect(page.locator(".conv")).to_have_count(2)
    segunda = app.state.orion.memory.selected_session("web").id
    assert primeira != segunda
    expect(page.locator(".msg-user")).to_have_count(0)
    page.click(f'.conv[data-id="{primeira}"]')
    expect(page.locator(".msg-user")).to_contain_text("Mensagem da primeira conversa")
    expect(page.locator(".msg-orion")).to_contain_text("Resposta do backend novo.")
    assert app.state.orion.memory.selected_session("web").id == primeira
    assert app.state.orion.memory.history(segunda) == []
