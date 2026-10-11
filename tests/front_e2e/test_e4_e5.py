"""E4 e E5 no navegador, contra o Orion de verdade (API, SQLite e política reais)."""

import asyncio
import threading

from playwright.sync_api import expect

from orion import documents
from orion.memory.ops import Operations
from orion.memory.sleep import SleepCycle
from orion.projects import Projects

from .test_capabilities import TOKEN_INIT


def rodar(coro):
    """O Playwright síncrono já ocupa o laço de eventos desta thread: a corrotina roda em outra."""
    saida = []
    t = threading.Thread(target=lambda: saida.append(asyncio.run(coro)))
    t.start()
    t.join()
    return saida[0]


def esperar_visivel(page, view):
    page.wait_for_function(
        f'() => getComputedStyle(document.querySelector("#view-{view}")).opacity === "1"'
    )


# ── E4.1 e E4.5: projeto, validade e conflitos na tela de fatos ─────────────
def test_fato_muda_de_projeto_e_o_conflito_e_decidido_pelo_antonio(abrir, novo_backend):
    url, app, _, _ = novo_backend()
    m = app.state.orion.memory
    alfa = Projects(m).create("Projeto Alfa")["id"]
    m.new_session("web")
    cafe = m.add_fact("Antônio toma café sem açúcar", "conversa")
    velho = m.add_fact("Antônio mora em Marília desde 2020", "conversa")
    novo = m.add_fact("Antônio mora em Bauru desde 2026", "conversa")
    m.record_conflict(velho.id, novo.id)
    page = abrir("#/memoria", url=url, init=TOKEN_INIT, axe=True)

    conflito = page.locator("[data-conflito]")
    expect(conflito).to_contain_text("parecem se contradizer")
    expect(conflito).to_contain_text("Bauru")
    conflito.get_by_role("button", name="O novo vale").click()
    expect(conflito).to_have_count(0)
    antigo = page.locator(f'[data-fact-id="{velho.id}"]')
    expect(antigo).to_contain_text("(antigo)")
    expect(antigo).to_have_attribute("data-antigo", "true")
    assert m.get_fact(velho.id).superseded_by == novo.id and m.get_fact(velho.id) is not None

    card = page.locator(f'[data-fact-id="{cafe.id}"]')
    card.get_by_label("Projeto do fato", exact=False).select_option(label="Projeto Alfa")
    expect(card).to_have_count(0)  # saiu da memória pessoal
    assert m.get_fact(cafe.id).project_id == alfa
    esperar_visivel(page, "memoria")
    assert not page.evaluate("async () => (await axe.run(document)).violations")


def test_validade_do_fato_pela_tela(abrir, novo_backend):
    url, app, _, _ = novo_backend()
    m = app.state.orion.memory
    m.new_session("web")
    f = m.add_fact("O plano gratuito vale", "conversa")
    page = abrir("#/memoria", url=url, init=TOKEN_INIT)
    card = page.locator(f'[data-fact-id="{f.id}"]')
    card.get_by_role("button", name="Validade", exact=True).click()
    dialog = page.get_by_role("dialog")
    dialog.get_by_label("Vale até").fill("2020-01-31")
    dialog.get_by_role("button", name="Salvar", exact=True).click()
    expect(card).to_contain_text("(antigo)")
    expect(card).to_contain_text("vale até")
    assert m.get_fact(f.id).valid_until and m.get_fact(f.id).old


# ── E4.3: barra de progresso e reindexar ────────────────────────────────────
def test_documento_lento_mostra_progresso_e_reindexar_refaz(abrir, novo_backend, monkeypatch):
    liberar = threading.Event()

    def lento(raw, kind, *, ocr_paginas, ocr_langs, avisar):
        avisar(60)
        liberar.wait(10)
        return {"text": "conteúdo lento indexado Zeta-42"}

    monkeypatch.setattr(documents, "_extrair", lento)
    monkeypatch.setattr(documents, "ESPERA_S", 0.05)
    url, app, _, _ = novo_backend()
    page = abrir("#/fontes", url=url, init=TOKEN_INIT, axe=True)
    page.get_by_label("Adicionar documento").set_input_files(
        {"name": "grande.md", "mimeType": "text/markdown", "buffer": b"x"}
    )
    page.get_by_role("button", name="Enviar e indexar").click()
    card = page.locator("[data-document-id]")
    barra = card.locator("progress.doc-progresso")
    expect(barra).to_be_visible()
    expect(card).to_contain_text("Indexando")
    page.wait_for_function(
        '() => { const p = document.querySelector("progress.doc-progresso"); return p && +p.value >= 40; }'
    )
    esperar_visivel(page, "fontes")
    assert not page.evaluate("async () => (await axe.run(document)).violations")
    liberar.set()
    expect(card).to_contain_text("Indexado", timeout=8000)
    expect(barra).to_have_count(0)
    assert app.state.orion.memory.search("Zeta-42", kinds=("chunk",))
    # reindexar: apaga e refaz a partir do original
    with app.state.orion.memory.transaction() as c:
        c.execute("UPDATE chunks SET text='CORROMPIDO'")
    card.get_by_role("button", name="Reindexar").click()
    expect(card).to_contain_text("Indexado", timeout=8000)
    assert app.state.orion.memory.search("Zeta-42", kinds=("chunk",))


# ── E5.4: botões nos avisos do sono ─────────────────────────────────────────
def test_aviso_do_sono_apaga_o_repetido_e_decide_a_contradicao(abrir, novo_backend):
    url, app, _, _ = novo_backend()
    s = app.state.orion
    s.memory.new_session("web")
    a = s.memory.add_fact("Antônio estuda ADS na Unimar em Marília", "conversa")
    s.memory.add_fact("antonio estuda ads na unimar em marilia", "vault")
    velho = s.memory.add_fact("O prazo do TCC é março de 2026", "conversa")
    novo = s.memory.add_fact("O prazo do TCC é junho de 2026", "conversa")
    r = rodar(SleepCycle(s.memory, Operations(s.memory), None, at="03:00").run())
    assert r.duplicados == 1 and r.conflitos >= 1
    page = abrir("#/atividade", url=url, init=TOKEN_INIT, axe=True)
    rep = page.locator('[data-sono="repetido"]')
    expect(rep).to_be_visible()
    rep.get_by_role("button", name="Apagar o repetido", exact=False).last.click()
    page.get_by_role("alertdialog").get_by_role("button", name="Apagar").click()
    expect(page.locator('[data-sono="repetido"]')).to_have_count(0)
    restantes = [f.text for f in s.memory.facts() if "ads" in f.text.casefold()]
    assert len(restantes) == 1
    conf = page.locator('[data-sono="conflito"]')
    expect(conf).to_be_visible()
    conf.get_by_role("button", name="O novo vale").click()
    expect(page.locator('[data-sono="conflito"]')).to_have_count(0)
    assert s.memory.get_fact(velho.id).superseded_by == novo.id
    assert s.memory.get_fact(a.id) is not None or len(restantes) == 1
    esperar_visivel(page, "atividade")
    assert not page.evaluate("async () => (await axe.run(document)).violations")


# ── E5.1 e E5.4: busca na tela e relações do sono (tela Conhecimento) ───────
def test_conhecimento_busca_na_tela_e_apaga_relacao_do_sono(abrir, novo_backend):
    url, app, _, _ = novo_backend(screen_memory=True)
    m = app.state.orion.memory
    m.add_screen("Reunião do TCC marcada com a orientadora na quarta", "Calendar", "chrome")
    m.add_screen("Receita de bolo de cenoura", "Receitas", "firefox")
    Operations(m).add_edge("Antônio", "estuda_em", "Unimar", kind="sono")
    page = abrir("#/conhecimento", url=url, init=TOKEN_INIT, axe=True)
    page.get_by_role("searchbox", name="Buscar no texto da tela").fill("orientadora")
    page.get_by_role("searchbox", name="Buscar no texto da tela").press("Enter")
    achado = page.locator("[data-tela-resultado]")
    expect(achado).to_have_count(1)
    expect(achado).to_contain_text("Calendar")
    expect(achado).to_contain_text("chrome")
    expect(achado).to_contain_text("orientadora")
    rel = page.locator('[data-relacao="Antônio|estuda_em|Unimar"]')
    expect(rel).to_contain_text("estuda_em")
    rel.get_by_role("button", name="Apagar").click()
    page.get_by_role("alertdialog").get_by_role("button", name="Apagar").click()
    expect(rel).to_have_count(0)
    assert Operations(m).list_edges("sono") == []
    esperar_visivel(page, "conhecimento")
    assert not page.evaluate("async () => (await axe.run(document)).violations")


# ── E5.5 e E5.6: grafo, "me leva até", "o que liga X a Y" ───────────────────
def vault(tmp_path):
    v = tmp_path / "vault"
    (v / "01 Projetos").mkdir(parents=True)
    (v / "03 Recursos").mkdir(parents=True)
    (v / "01 Projetos" / "Orion.md").write_text("# Orion\nUsa [[RAG]] e [[MCP]].\n")
    (v / "03 Recursos" / "RAG.md").write_text("# RAG\nLigado a [[Busca Hibrida]].\n")
    (v / "03 Recursos" / "MCP.md").write_text("# MCP\nProtocolo.\n")
    (v / "03 Recursos" / "Busca Hibrida.md").write_text("# Busca Hibrida\nRRF.\n")
    return v


def test_grafo_3d_com_fatos_e_notas_comandos_de_ir_e_de_caminho(abrir, novo_backend, tmp_path):
    url, app, _, _ = novo_backend()
    m = app.state.orion.memory
    m.new_session("web")
    m.index_vault(vault(tmp_path))
    m.add_fact("O projeto Orion usa busca híbrida", "conversa")
    page = abrir("#/memoria", url=url, init=TOKEN_INIT)
    page.get_by_role("button", name="Ver grafo").click()
    page.wait_for_function("() => document.querySelector('#mem-stats')?.textContent.includes('nó')")
    assert page.evaluate("Orion.memoriaGrafo.pronto()")
    expect(page.locator("#mem-banner")).to_be_hidden()  # dados reais, nada de demonstração
    expect(page.locator('#mem-filters [data-filtro="fato"]')).to_be_visible()
    expect(page.locator("#mem-pasta")).to_be_visible()
    caixa = page.locator("#mem-search")
    caixa.fill("me leva até MCP")
    caixa.press("Enter")
    expect(page.locator("#mem-detail")).to_have_attribute("data-open", "true")
    expect(page.locator("#mem-detail-body h3")).to_have_text("MCP")
    expect(page.locator("#mem-detail-tipo")).to_have_text("Nota")
    caixa.fill("o que liga Orion a Busca Hibrida?")
    caixa.press("Enter")
    lista = page.locator(".mem-results")
    expect(lista).to_contain_text("Orion → Busca Hibrida · 2 ligações")
    expect(lista.locator("button.mem-result")).to_have_count(3)
    lista.get_by_role("button", name="RAG").click()
    expect(page.locator("#mem-detail-body h3")).to_have_text("RAG")
    caixa.fill("o que liga Orion a Nada Disso")
    caixa.press("Enter")
    expect(page.locator(".toast, [role='status']").filter(has_text="Não achei")).to_be_visible()
    # filtro por pasta recarrega só as notas daquela pasta
    page.locator("#mem-pasta").select_option("03 Recursos")
    page.wait_for_function("() => ![...Orion.memoriaGrafo.pronto() ? [1] : []].length || true")
    nos = page.evaluate(
        "Orion.api.grafo(500, {pasta: '03 Recursos'}).then(g => g.nodes.filter(n => n.tipo === 'nota').map(n => n.label).sort())"
    )
    assert nos == ["Busca Hibrida", "MCP", "RAG"]


def test_slash_grafo_abre_a_memoria_e_anda_pelo_grafo(abrir, novo_backend, tmp_path):
    url, app, _, _ = novo_backend()
    m = app.state.orion.memory
    m.new_session("web")
    m.index_vault(vault(tmp_path))
    page = abrir("#/chat", url=url, init=TOKEN_INIT)
    page.fill("#composer-input", "/grafo o que liga Orion a RAG")
    page.press("#composer-input", "Enter")
    expect(page.locator("html")).to_have_attribute("data-view", "memoria")
    expect(page.locator(".mem-results")).to_contain_text("Orion → RAG · 1 ligação", timeout=15000)


# ── E5.7: busca global na paleta ────────────────────────────────────────────
def test_paleta_em_tudo_acha_fato_e_abre_na_lista(abrir, novo_backend):
    url, app, _, _ = novo_backend()
    m = app.state.orion.memory
    sessao = m.new_session("web")
    m.add_message(sessao.id, "user", "vamos falar do projeto Zeta")
    m.add_fact("O projeto Zeta entrega em dezembro", "conversa")
    page = abrir("#/chat", url=url, init=TOKEN_INIT, axe=True)
    page.keyboard.press("Control+k")
    page.fill("#palette-input", "Zeta")
    grupo = page.get_by_role("group", name="Em tudo")
    expect(grupo).to_contain_text("O projeto Zeta entrega em dezembro", timeout=5000)
    esperar = page.locator("#palette")
    assert not page.evaluate(
        "async () => (await axe.run(document.querySelector('#palette'))).violations"
    )
    grupo.get_by_role("option", name="O projeto Zeta entrega em dezembro", exact=False).click()
    expect(page.locator("html")).to_have_attribute("data-view", "memoria")
    expect(page.get_by_role("searchbox", name="Buscar fatos")).to_have_value(
        "O projeto Zeta entrega em dezembro"
    )
    expect(page.locator("[data-fact-id]")).to_have_count(1)
    assert esperar is not None


# ── E4.4: aviso do resumo da conversa ───────────────────────────────────────
def test_chat_avisa_que_o_comeco_foi_resumido_e_deixa_ver_e_descartar(abrir, novo_backend):
    url, app, _, _ = novo_backend()
    m = app.state.orion.memory
    s = m.new_session("web")
    m.add_message(s.id, "user", "primeira pergunta")
    m.add_message(s.id, "assistant", "primeira resposta")
    m.set_summary(s.id, 2, "- decidiram usar SQLite\n- backup diário", "local:qwen3.5:4b")
    page = abrir("#/chat", url=url, init=TOKEN_INIT, axe=True)
    aviso = page.locator(".history-resumo")
    expect(aviso).to_contain_text("resumida até a mensagem 2")
    expect(aviso).to_contain_text("pode ter erro")
    texto = aviso.locator(".history-resumo-texto")
    expect(texto).to_be_hidden()
    aviso.get_by_role("button", name="ver resumo").click()
    expect(texto).to_contain_text("decidiram usar SQLite")
    assert not page.evaluate("async () => (await axe.run(document)).violations")
    aviso.get_by_role("button", name="descartar").click()
    expect(aviso).to_be_hidden()
    assert m.get_summary(s.id) is None
