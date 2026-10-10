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
