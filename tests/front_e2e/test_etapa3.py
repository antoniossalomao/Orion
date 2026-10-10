"""E3: conversas e projetos no navegador. `novo_backend` = app real, gateway simulado."""

from playwright.sync_api import expect

from .test_capabilities import TOKEN_INIT


def test_arquivadas_busca_desarquiva_e_apaga(abrir, novo_backend):
    url, app, _, _ = novo_backend()
    m = app.state.orion.memory
    ativa = m.new_session("web", "Conversa ativa").id
    velha = m.new_session("web", "Orçamento da reforma").id
    outra = m.new_session("web", "Receita de bolo").id
    m.add_message(velha, "user", "quanto custa a reforma")
    for sid in (velha, outra):
        m.edit_session("web", sid, archived=True)
    page = abrir("#/arquivadas", url=url, init=TOKEN_INIT, axe=True)
    itens = page.locator("#arquivadas-corpo .arq-item")
    expect(itens).to_have_count(2)
    page.fill("#arquivadas-busca", "reforma")
    expect(itens).to_have_count(1)
    expect(itens.first).to_contain_text("Orçamento da reforma")
    page.fill("#arquivadas-busca", "")
    expect(itens).to_have_count(2)
    # desarquivar
    page.get_by_role("button", name="Desarquivar Receita de bolo").click()
    expect(itens).to_have_count(1)
    assert not m.get_session(outra).archived
    # apagar pede confirmação com o título; cancelar não apaga
    page.get_by_role("button", name="Apagar Orçamento da reforma").click()
    expect(page.get_by_role("alertdialog")).to_contain_text("Orçamento da reforma")
    page.keyboard.press("Escape")
    assert m.get_session(velha) is not None
    page.get_by_role("button", name="Apagar Orçamento da reforma").click()
    page.get_by_role("button", name="Apagar para sempre").click()
    expect(page.locator("#arquivadas-corpo")).to_contain_text("Nenhuma conversa arquivada")
    assert m.get_session(velha) is None and m.get_session(ativa) is not None
    page.wait_for_timeout(300)
    assert page.evaluate("async () => (await axe.run(document)).violations.map(v => v.id)") == []


def test_arquivadas_no_backend_de_mentira(abrir):
    page = abrir("#/conhecimento")
    page.evaluate("Orion.app.ir('arquivadas')")
    expect(page.locator("#view-arquivadas")).to_be_visible()
    expect(page.locator("#arquivadas-corpo")).to_contain_text("Nenhuma conversa arquivada")


def test_filtro_por_projeto_na_barra_lateral_e_selo(abrir, novo_backend):
    url, app, _, _ = novo_backend()
    m = app.state.orion.memory
    from orion.projects import Projects

    pid = Projects(m).create("Estágio")["id"]
    solta = m.new_session("web", "Conversa solta").id
    dentro = m.new_session("web", "Tarefa do estágio", project_id=pid).id
    page = abrir("#/chat", url=url, init=TOKEN_INIT, axe=True)
    convs = page.locator("#sb-convs-list .conv")
    expect(convs).to_have_count(2)
    seletor = page.get_by_label("Filtrar conversas por projeto")
    expect(seletor).to_be_visible()
    # selo com o nome curto do projeto só na conversa que está nele
    expect(page.locator(f'.conv[data-id="{dentro}"] .conv-projeto')).to_have_text("Estágio")
    expect(page.locator(f'.conv[data-id="{solta}"] .conv-projeto')).to_be_hidden()
    seletor.select_option("nenhum")
    expect(convs).to_have_count(1)
    expect(convs.first).to_contain_text("Conversa solta")
    seletor.select_option(pid)
    expect(convs).to_have_count(1)
    expect(convs.first).to_contain_text("Tarefa do estágio")
    # a escolha sobrevive ao recarregar (localStorage)
    page.reload()
    page.wait_for_selector("html[data-pronto='true']")
    expect(page.get_by_label("Filtrar conversas por projeto")).to_have_value(pid)
    expect(page.locator("#sb-convs-list .conv")).to_have_count(1)
    page.get_by_label("Filtrar conversas por projeto").select_option("todos")
    expect(page.locator("#sb-convs-list .conv")).to_have_count(2)
    page.wait_for_timeout(300)
    assert page.evaluate("async () => (await axe.run(document)).violations.map(v => v.id)") == []


def test_mover_conversa_para_projeto_por_slash_e_por_paleta(abrir, novo_backend):
    from orion.projects import Projects

    url, app, _, _ = novo_backend()
    m = app.state.orion.memory
    estagio = Projects(m).create("Estágio")["id"]
    Projects(m).create("Estudos")
    Projects(m).create("Estudos avançados")
    sid = m.new_session("web", "Para mover").id
    m.add_message(sid, "user", "oi")
    page = abrir("#/chat", url=url, init=TOKEN_INIT, axe=True)
    expect(page.locator(".msg-user")).to_contain_text("oi")
    campo = page.locator("#composer-input")
    # sugestão do slash
    campo.fill("/proj")
    expect(page.locator(".slash-item, [role=option]").first).to_contain_text("/projeto")
    # nome ambíguo não move nada
    campo.fill("/projeto estud")
    campo.press("Enter")
    expect(
        page.locator(".toast, [role=status]").filter(has_text="Mais de um projeto")
    ).to_be_visible()
    assert m.get_session(sid).project_id is None
    # nome único move (sem acento e sem caixa)
    campo.fill("/projeto estagio")
    campo.press("Enter")
    expect(page.locator(f'.conv[data-id="{sid}"] .conv-projeto')).to_have_text("Estágio")
    assert m.get_session(sid).project_id == estagio
    # "nenhum" tira do projeto
    campo.fill("/projeto nenhum")
    campo.press("Enter")
    expect(page.locator(f'.conv[data-id="{sid}"] .conv-projeto')).to_be_hidden()
    assert m.get_session(sid).project_id is None
    # paleta: comando existe e abre o diálogo de destino
    page.keyboard.press("Control+k")
    page.keyboard.type("mover conversa")
    expect(page.get_by_role("option", name="Mover conversa para projeto…")).to_be_visible()
    page.keyboard.press("Enter")
    dialogo = page.get_by_role("dialog")
    expect(dialogo).to_contain_text("Mover conversa")
    dialogo.get_by_label("Destino da conversa").select_option(label="Estágio")
    dialogo.get_by_role("button", name="Mover conversa").click()
    expect(page.locator(f'.conv[data-id="{sid}"] .conv-projeto')).to_have_text("Estágio")
    assert m.get_session(sid).project_id == estagio


def test_documento_muda_de_escopo_pelo_disponivel_em(abrir, novo_backend):
    from orion.memory.scope import data_scope
    from orion.projects import Projects

    url, app, _, _ = novo_backend()
    m = app.state.orion.memory
    pid = Projects(m).create("Estágio")["id"]
    page = abrir("#/fontes", url=url, init=TOKEN_INIT, axe=True)
    page.get_by_label("Adicionar documento").set_input_files(
        {
            "name": "manual.md",
            "mimeType": "text/markdown",
            "buffer": b"# Manual\n\nFrase-rara-do-manual.",
        }
    )
    page.get_by_role("button", name="Enviar e indexar").click()
    card = page.locator("[data-document-id]")
    expect(card).to_contain_text("Indexado")
    with data_scope(None, include_personal=True):
        assert m.search("Frase-rara-do-manual")
    page.get_by_label("Disponível em: manual.md").select_option(label="Estágio")
    expect(page.locator("#document-status")).to_contain_text("movido")
    expect(card).to_have_count(0)  # saiu da lista pessoal
    with data_scope(None, include_personal=True):
        assert not m.search("Frase-rara-do-manual")
    with data_scope(pid, include_personal=False):
        assert m.search("Frase-rara-do-manual")
    page.wait_for_timeout(300)
    assert page.evaluate("async () => (await axe.run(document)).violations.map(v => v.id)") == []


def test_editar_pedido_cria_versao_e_as_setas_so_trocam_a_exibicao(abrir, novo_backend):
    from tests.fakes import FakeGateway, fala

    gw = FakeGateway(fala("Resposta velha."), fala("Resposta nova."))
    url, app, _, _ = novo_backend(gateway_override=gw)
    m = app.state.orion.memory
    sid = m.active_session("web").id
    page = abrir("#/chat", url=url, init=TOKEN_INIT, axe=True)
    page.fill("#composer-input", "pergunta torta")
    page.keyboard.press("Enter")
    expect(page.locator(".msg-orion .prose, .msg-orion").last).to_contain_text("Resposta velha.")
    page.reload()
    page.wait_for_selector("html[data-pronto='true']")
    expect(page.locator(".msg-user")).to_contain_text("pergunta torta")
    page.get_by_role("button", name="Editar pedido (nova versão)").click()
    dialogo = page.get_by_role("dialog")
    dialogo.get_by_label("Pedido revisado").fill("pergunta certa")
    dialogo.get_by_role("button", name="Enviar nova versão").click()
    expect(page.locator(".msg-orion").last).to_contain_text("Resposta nova.")
    # o histórico foi recarregado: pedido novo, resposta nova, a antiga some
    expect(page.locator(".msg-user")).to_have_count(1)
    expect(page.locator(".msg-user")).to_contain_text("pergunta certa")
    expect(page.get_by_text("Resposta velha.")).to_have_count(0)
    nav = page.get_by_role("group", name="Versões do pedido")
    expect(nav).to_contain_text("2/2")
    page.get_by_role("button", name="Versão anterior do pedido").click()
    expect(page.locator(".msg-user .bubble")).to_have_text("pergunta torta")
    expect(nav).to_contain_text("versão antiga")
    page.get_by_role("button", name="Próxima versão do pedido").click()
    expect(page.locator(".msg-user .bubble")).to_have_text("pergunta certa")
    # nada foi apagado do banco, e o contexto do modelo só tem a versão nova
    assert m.query("SELECT count(*) FROM messages WHERE session_id=? AND superseded=1", (sid,))[0][0] == 2
    assert "pergunta torta" not in str(gw.chamadas[1])
    page.wait_for_timeout(300)
    assert page.evaluate("async () => (await axe.run(document)).violations.map(v => v.id)") == []
