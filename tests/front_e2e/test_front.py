"""Front do Orion no navegador de verdade: fluxos, teclado, segurança, janela estreita e acessibilidade."""

from __future__ import annotations

import json
import re

import pytest
from playwright.sync_api import expect

from .conftest import AXE, SENHA, TOKEN

ROTAS = ["", "#/chat", "#/memoria", "#/integracoes", "#/config", "#/painel"]
VIEWS = ["home", "chat", "memoria", "integracoes", "config", "painel"]
TEMAS = ["noite", "grafite", "contraste"]


def enviar(page, texto: str) -> None:
    caixa = (
        "#home-input"
        if page.evaluate("document.documentElement.dataset.view") == "home"
        else "#composer-input"
    )
    page.fill(caixa, texto)
    page.keyboard.press("Enter")


def ultima_resposta(page):
    return page.locator(".msg-orion").last


def esperar_fim(page, timeout: int = 20000) -> None:
    expect(ultima_resposta(page)).to_have_attribute("data-streaming", "false", timeout=timeout)


# ── carregar e navegar ──────────────────────────────────────────────────────────
def test_home_carrega_com_saudacao_status_e_ceu(abrir):
    page = abrir()
    expect(page).to_have_title("Orion")
    expect(page.locator("#home-saudacao")).to_have_text(
        re.compile(r"^(Bom dia|Boa tarde|Boa noite|Boa madrugada)$")
    )
    expect(page.locator("#conn-text")).to_have_text("Conectado")
    expect(page.locator("#home-status")).to_contain_text("Cérebro conectado")
    assert page.locator("#sky canvas").count() == 1, "a constelação (WebGL) não subiu"


def test_navegacao_por_hash_botao_voltar_e_views_ocultas_inertes(abrir):
    page = abrir()
    for rota, view in zip(ROTAS[1:], VIEWS[1:], strict=True):
        page.click(f'.sb-item[data-view="{view}"]')
        expect(page.locator("html")).to_have_attribute("data-view", view)
        assert page.url.endswith(rota)
        inertes = page.evaluate(
            "[...document.querySelectorAll('.view')].filter(v => v.inert).map(v => v.dataset.view)"
        )
        assert sorted(inertes) == sorted(v for v in VIEWS if v != view), (
            "view oculta ainda alcançável por Tab"
        )
        expect(page.locator(f'.sb-item[data-view="{view}"]')).to_have_attribute(
            "aria-current", "page"
        )
    page.go_back()
    expect(page.locator("html")).to_have_attribute("data-view", VIEWS[-2])


def test_deep_link_abre_direto_na_tela(abrir):
    page = abrir("#/config")
    expect(page.locator("html")).to_have_attribute("data-view", "config")
    expect(page.locator("#tb-title")).to_have_text("Configurações")


def test_atalhos_de_teclado(abrir):
    page = abrir()
    page.keyboard.press("Alt+3")
    expect(page.locator("html")).to_have_attribute("data-view", "memoria")
    page.keyboard.press("Alt+2")
    expect(page.locator("html")).to_have_attribute("data-view", "chat")
    page.keyboard.press("Control+b")
    expect(page.locator("html")).to_have_attribute("data-sb", "collapsed")
    expect(page.locator("#sb-toggle")).to_have_attribute("aria-expanded", "false")
    page.keyboard.press("Control+b")
    expect(page.locator("html")).to_have_attribute("data-sb", "expanded")
    page.locator("#main").focus()
    page.keyboard.press("/")
    expect(page.locator("#composer-input")).to_be_focused()
    page.keyboard.press("Escape")  # fora de um chat ocupado, só solta o foco do campo
    page.locator("#main").focus()
    page.keyboard.press("?")
    expect(page.locator("html")).to_have_attribute("data-view", "config")
    expect(page.locator("#kbd-list")).to_contain_text("Paleta de comandos")


def test_paleta_de_comandos(abrir):
    page = abrir()
    page.keyboard.press("Control+k")
    expect(page.locator("#palette")).to_have_attribute("data-open", "true")
    expect(page.locator("#palette-input")).to_be_focused()
    assert page.evaluate("document.querySelector('#app').inert") is True
    page.keyboard.type("config")
    expect(page.locator('#palette-list [role="option"]').first).to_contain_text("Configurações")
    expect(page.locator("#palette-input")).to_have_attribute("aria-activedescendant", "pal-0")
    page.keyboard.press("Enter")
    expect(page.locator("html")).to_have_attribute("data-view", "config")
    expect(page.locator("#palette")).to_be_hidden()
    assert page.evaluate("document.querySelector('#app').inert") is False
    # Esc fecha e devolve o foco a quem abriu
    page.locator("#palette-btn").focus()
    page.keyboard.press("Control+k")
    page.keyboard.press("Escape")
    expect(page.locator("#palette")).to_be_hidden()
    expect(page.locator("#palette-btn")).to_be_focused()
    # comando de tema muda o tema de verdade
    page.keyboard.press("Control+k")
    page.keyboard.type("grafite")
    page.keyboard.press("Enter")
    expect(page.locator("html")).to_have_attribute("data-theme", "grafite")


# ── chat ────────────────────────────────────────────────────────────────────────
def test_chat_streaming_renderiza_markdown_tabela_e_codigo(abrir):
    page = abrir()
    enviar(page, "Explique a diferença entre diagrama de classes e de sequência, com exemplo")
    expect(page.locator("html")).to_have_attribute("data-view", "chat")
    expect(page.locator(".msg-user .bubble").last).to_contain_text("diagrama de classes")
    esperar_fim(page)
    prose = ultima_resposta(page).locator(".prose")
    expect(prose.locator("table")).to_have_count(1)
    expect(prose.locator(".md-code-lang")).to_have_text("Python")
    expect(prose.locator("blockquote")).to_have_count(1)
    expect(prose.locator("a.md-link").first).to_have_attribute(
        "rel", "noopener noreferrer nofollow"
    )
    # copiar código usa o clipboard de verdade
    prose.locator(".md-copy").click()
    expect(prose.locator(".md-copy")).to_have_text("Copiado")
    assert "import json" in page.evaluate("navigator.clipboard.readText()")
    # fim da resposta: ações aparecem, botão volta a "enviar", estado do céu volta a "idle"
    expect(ultima_resposta(page).locator('[data-acao="copiar"]')).to_be_visible()
    expect(page.locator("#btn-send")).to_have_attribute("data-mode", "send")
    expect(page.locator("html")).to_have_attribute("data-estado", "idle")


def test_streaming_rende_no_maximo_uma_vez_por_quadro(abrir):
    page = abrir("#/chat")
    page.evaluate("""() => { window.__m = 0; window.__f = 0;
        const raf = () => { window.__f++; requestAnimationFrame(raf); }; raf();
        new MutationObserver(() => window.__m++).observe(document.getElementById('chat-col'), { childList: true, subtree: true, characterData: true }); }""")
    enviar(page, "Explique a diferença entre diagrama de classes e de sequência, com exemplo")
    esperar_fim(page)
    mutacoes, quadros = page.evaluate("[window.__m, window.__f]")
    # a resposta chega em ~100 pedaços; cada renderização é agendada no próximo quadro
    assert 2 <= mutacoes <= quadros + 2, f"{mutacoes} renderizações em {quadros} quadros"


def test_ceu_fora_da_home_fica_em_no_maximo_20_quadros_por_segundo(abrir):
    page = abrir("#/config")
    page.wait_for_timeout(600)
    antes = page.evaluate("Orion.sky.quadros()")
    page.wait_for_timeout(3000)
    por_segundo = (page.evaluate("Orion.sky.quadros()") - antes) / 3
    assert por_segundo <= 22, f"{por_segundo:.1f} quadros/s atrás de uma tela opaca"


def test_composer_multilinha_historico_e_estado_do_botao(abrir):
    page = abrir("#/chat")
    caixa = page.locator("#composer-input")
    expect(page.locator("#btn-send")).to_be_disabled()
    caixa.fill("linha 1")
    altura_1 = caixa.evaluate("e => e.getBoundingClientRect().height")
    page.keyboard.press("Shift+Enter")
    page.keyboard.type("linha 2")
    assert caixa.input_value() == "linha 1\nlinha 2"
    assert caixa.evaluate("e => e.getBoundingClientRect().height") > altura_1 + 10, (
        "o campo deveria crescer com as linhas"
    )
    expect(page.locator("#btn-send")).to_be_enabled()
    page.keyboard.press("Enter")
    expect(page.locator(".msg-user .bubble").last).to_have_text("linha 1\nlinha 2")
    esperar_fim(page)
    caixa.focus()
    page.keyboard.press("ArrowUp")
    assert caixa.input_value() == "linha 1\nlinha 2", "↑ repete a mensagem anterior"


def test_parar_resposta_em_andamento(abrir):
    page = abrir("#/chat")
    enviar(page, "responda bem lento por favor")
    expect(page.locator("#btn-send")).to_have_attribute("data-mode", "stop")
    expect(ultima_resposta(page).locator(".thinking")).to_be_visible()
    page.click("#btn-send")
    expect(page.locator("#btn-send")).to_have_attribute("data-mode", "send", timeout=5000)
    expect(page.locator(".msg-orion")).to_have_count(
        0
    )  # sem texto e sem nada útil: a bolha vazia some


def test_erro_do_cerebro_mostra_alerta_e_tentar_de_novo(abrir):
    page = abrir("#/chat")
    enviar(page, "isso vai dar falha")
    erro = page.locator(".msg-error")
    expect(erro).to_have_count(1)
    expect(erro).to_contain_text("nenhum modelo respondeu")
    expect(erro).to_have_attribute("role", "alert")
    erro.get_by_role("button", name="Tentar de novo").click()
    expect(page.locator(".msg-error")).to_have_count(2)


def test_aprovacao_aprovar_executa_e_marca_o_cartao(abrir):
    page = abrir("#/chat")
    enviar(page, "apague os arquivos antigos")
    cartao = page.locator(".approval").first
    expect(cartao).to_have_attribute("data-estado", "pendente")
    expect(cartao).to_contain_text("Remove-Item")
    expect(page.locator(".tool-chip").first).to_have_attribute("data-estado", "espera")
    esperar_fim(page)
    cartao.get_by_role("button", name="Aprovar e executar").click()
    expect(cartao).to_have_attribute("data-estado", "aprovada")
    expect(page.locator(".msg-orion").last).to_contain_text("Feito. Removi a pasta", timeout=15000)
    esperar_fim(page)
    expect(cartao.locator(".approval-state")).to_have_text("Aprovada · executada.")
    expect(cartao.get_by_role("button", name="Aprovar e executar")).to_be_disabled()


def test_aprovar_antes_da_resposta_terminar_nao_mistura_as_mensagens(abrir):
    page = abrir("#/chat")
    enviar(page, "apague os arquivos antigos, devagar")
    # o cartão chega e o texto final ainda demora 1,5 s: clica sem esperar o fim da resposta
    page.locator(".approval").get_by_role("button", name="Aprovar e executar").click()
    expect(page.locator(".msg-orion")).to_have_count(2, timeout=15000)
    esperar_fim(page)
    primeira, segunda = page.locator(".msg-orion .prose").all_inner_texts()
    assert "Feito" not in primeira and "Feito. Removi a pasta" in segunda
    assert primeira.startswith("Preciso da sua aprovação") and primeira.endswith("cartão acima.")


def test_aprovacao_negar_nao_executa(abrir):
    page = abrir("#/chat")
    enviar(page, "apague os arquivos antigos")
    cartao = page.locator(".approval").last
    esperar_fim(page)
    cartao.get_by_role("button", name="Negar").click()
    expect(cartao).to_have_attribute("data-estado", "negada")
    expect(cartao.locator(".approval-state")).to_contain_text("nada foi executado")
    expect(page.locator(".msg-orion")).to_have_count(1)  # nenhuma retomada


def test_aprovacao_mostra_o_comando_inteiro_e_argumento_cortado_so_pode_ser_negado(abrir):
    page = abrir("#/chat")
    enviar(page, "apague os arquivos antigos")
    normal = page.locator(".approval").last
    esperar_fim(page)
    expect(normal.get_by_role("button", name="Aprovar e executar")).to_be_visible()
    expect(normal.locator(".approval-warn")).to_have_count(0)

    enviar(page, "apague tudo, comando enorme")
    cartao = page.locator(".approval").last
    esperar_fim(page)
    expect(cartao.locator(".approval-warn")).to_contain_text("só dá para negar")
    expect(cartao.get_by_role("button", name="Aprovar e executar")).to_have_count(0)
    # o texto não é cortado pela interface (o limite é o do servidor, que avisou): 1990 "x" aparecem
    assert cartao.locator(".approval-args").inner_text().count("x") >= 1990
    cartao.get_by_role("button", name="Negar").click()
    expect(cartao).to_have_attribute("data-estado", "negada")


def test_historico_de_conversas_abre_e_somente_leitura_avisa(abrir):
    page = abrir()
    page.locator("#sb-convs-list .conv", has_text="Dúvida de UML").click()
    expect(page.locator("html")).to_have_attribute("data-view", "chat")
    expect(page.locator(".msg").first).to_be_visible()
    page.locator("#sb-convs-list .conv", has_text="conversas antigas").click()
    expect(page.locator(".msg-system")).to_contain_text("somente leitura")
    # busca na lista: por trecho, sem acento
    page.fill("#sb-search", "memoria")
    expect(page.locator("#sb-convs-list .conv")).to_have_count(1)
    page.fill("#sb-search", "zzzz")
    expect(page.locator("#sb-convs-list")).to_contain_text("Nada encontrado")


def test_anexo_sobe_e_aparece_na_mensagem(abrir, tmp_path):
    page = abrir("#/chat")
    img = tmp_path / "print.png"
    img.write_bytes(
        bytes.fromhex(
            "89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c4890000000d49444154789c6360000002000001e221bc330000000049454e44ae426082"
        )
    )
    page.set_input_files("#file-input", str(img))
    chip = page.locator(".attach-chip")
    expect(chip).to_have_attribute("data-estado", "ok")
    expect(page.locator("#btn-send")).to_be_enabled()  # anexo sozinho já permite enviar
    page.locator("#composer-input").fill("o que é isso?")
    page.keyboard.press("Enter")
    expect(page.locator(".msg-user .attach-note").last).to_contain_text("print.png")
    expect(page.locator(".attach-chip")).to_have_count(0)
    # tipo não suportado vira aviso, não erro
    pdf = tmp_path / "x.pdf"
    pdf.write_bytes(b"%PDF-1.4")
    esperar_fim(page)
    page.set_input_files("#file-input", str(pdf))
    expect(page.locator(".toast").last).to_contain_text("tipo não suportado")


def test_botao_mais_recentes_aparece_ao_subir(abrir):
    page = abrir("#/chat")
    for i in range(3):
        enviar(page, f"pergunta longa número {i} para encher a tela")
        esperar_fim(page)
    rolagem = page.locator("#chat-scroll")
    rolagem.evaluate("e => { e.scrollTo({top: 0, behavior: 'instant'}); }")
    expect(page.locator("#scroll-down")).to_have_attribute("data-show", "true")
    page.click("#scroll-down")
    expect(page.locator("#scroll-down")).to_have_attribute("data-show", "false", timeout=3000)


# ── comandos, busca, foco, citar e afins ────────────────────────────────────────
def test_comandos_de_barra_completam_executam_e_nao_vao_ao_modelo(abrir):
    page = abrir("#/chat")
    caixa = page.locator("#composer-input")
    menu = page.locator("#composer-box .slash-menu")
    itens = page.locator("#composer-box .slash-item")
    caixa.fill("/")
    expect(menu).to_have_attribute("data-open", "true")
    expect(itens.first).to_contain_text("/nova")
    # prefixo + Enter COMPLETA; Tab também; argumento fixo executa
    caixa.fill("/mo")
    page.keyboard.press("Enter")
    assert caixa.input_value() == "/modelo "
    page.keyboard.type("claude")
    page.keyboard.press("Enter")
    expect(page.locator("#model-label")).to_have_text("Claude")
    assert caixa.input_value() == ""
    caixa.fill("/tema gra")
    expect(itens).to_have_count(1)
    page.keyboard.press("Enter")
    expect(page.locator("html")).to_have_attribute("data-theme", "grafite")
    caixa.fill("/tem")
    page.keyboard.press("Tab")
    assert caixa.input_value() == "/tema "
    expect(itens).to_have_count(3)
    page.keyboard.press("ArrowDown")
    page.keyboard.press("Enter")  # item destacado: "grafite"
    expect(page.locator("html")).to_have_attribute("data-theme", "grafite")
    assert page.locator(".msg-user").count() == 0, "um comando foi parar no chat"
    # Esc fecha o menu sem apagar o texto e sem sair do campo
    caixa.fill("/n")
    page.keyboard.press("Escape")
    expect(menu).to_have_attribute("data-open", "false")
    expect(caixa).to_be_focused()
    # comando desconhecido ou inválido avisa e preserva o texto
    caixa.fill("/xyz")
    page.keyboard.press("Enter")
    expect(page.locator(".toast").last).to_contain_text("Comando desconhecido")
    assert caixa.input_value() == "/xyz"
    caixa.fill("/modelo gpt")
    page.keyboard.press("Enter")
    expect(page.locator(".toast").last).to_contain_text("Argumento inválido")


def test_pilha_de_toasts_tem_limite_e_nao_trava_a_pagina(abrir):
    page = abrir("#/chat")
    for i in range(6):
        page.evaluate(f"Orion.ui.toast('aviso {i}', {{ ms: 0 }})")
    expect(page.locator(".toast")).to_have_count(3)  # só os 3 mais recentes
    expect(page.locator(".toast").last).to_contain_text("aviso 5")
    page.evaluate(
        "Orion.ui.toast('mesmo id', { id: 'x', ms: 0 }); Orion.ui.toast('mesmo id', { id: 'x', ms: 0 })"
    )
    expect(page.locator('.toast[data-id="x"]')).to_have_count(1)


def test_barra_dupla_envia_a_barra_literal_e_caminho_nao_e_comando(abrir):
    page = abrir("#/chat")
    page.fill("#composer-input", "//nova coisa")
    page.keyboard.press("Enter")
    expect(page.locator(".msg-user .bubble").last).to_have_text("/nova coisa")
    esperar_fim(page)
    page.fill("#composer-input", "/home/antonio/notas.txt tem o quê?")
    page.keyboard.press("Enter")
    expect(page.locator(".msg-user .bubble").last).to_have_text(
        "/home/antonio/notas.txt tem o quê?"
    )


def test_comando_de_barra_tambem_funciona_no_inicio_e_durante_a_resposta(abrir):
    page = abrir()
    page.fill("#home-input", "/tema contraste")
    page.keyboard.press("Enter")
    expect(page.locator("html")).to_have_attribute("data-theme", "contraste")
    expect(page.locator("html")).to_have_attribute("data-view", "home")
    page.evaluate("Orion.prefs.set('theme', 'noite')")
    page.click('.sb-item[data-view="chat"]')
    enviar(page, "responda bem lento por favor")
    page.fill("#composer-input", "/tema grafite")  # com a resposta em andamento, comando passa
    page.keyboard.press("Enter")
    expect(page.locator("html")).to_have_attribute("data-theme", "grafite")


def test_busca_na_conversa_marca_conta_e_navega(abrir):
    page = abrir("#/chat")
    enviar(page, "Explique a diferença entre diagrama de classes e de sequência, com exemplo")
    esperar_fim(page)
    page.keyboard.press("Control+f")
    expect(page.locator("#find-bar")).to_be_visible()
    expect(page.locator("#find-input")).to_be_focused()
    page.keyboard.type("MEMORIA")  # sem acento e sem diferenciar maiúsculas
    expect(page.locator("#find-count")).to_contain_text("1 de")
    total = page.evaluate("CSS.highlights.get('busca').size")
    assert total >= 2
    page.keyboard.press("Enter")
    expect(page.locator("#find-count")).to_contain_text(f"2 de {total}")
    page.keyboard.press("Shift+Enter")
    page.keyboard.press("Shift+Enter")
    expect(page.locator("#find-count")).to_contain_text(f"{total} de {total}")  # volta pelo fim
    page.fill("#find-input", "zzzz")
    expect(page.locator("#find-count")).to_have_text("Nada encontrado")
    page.keyboard.press("Escape")
    expect(page.locator("#find-bar")).to_be_hidden()
    assert page.evaluate("CSS.highlights.has('busca')") is False
    expect(page.locator("#composer-input")).to_be_focused()


def test_busca_por_comando_de_barra(abrir):
    page = abrir("#/chat")
    enviar(page, "Explique a diferença entre diagrama de classes e de sequência, com exemplo")
    esperar_fim(page)
    page.fill("#composer-input", "/buscar tabela")
    page.keyboard.press("Enter")
    expect(page.locator("#find-input")).to_have_value("tabela")
    expect(page.locator("#find-count")).to_contain_text(" de ")


def test_modo_foco_esconde_as_barras_e_esc_volta(abrir):
    page = abrir("#/chat")
    page.keyboard.press("Control+.")
    expect(page.locator("html")).to_have_attribute("data-foco", "true")
    expect(page.locator("#sidebar")).to_be_hidden()
    expect(page.locator("#tb-title")).to_be_hidden()
    assert (
        page.locator("#topbar").evaluate("e => e.getBoundingClientRect().height") < 44
    )  # só a faixa de arrastar
    expect(page.locator("#composer-input")).to_be_visible()
    page.keyboard.press("Escape")
    expect(page.locator("html")).not_to_have_attribute("data-foco", "true")
    expect(page.locator("#sidebar")).to_be_visible()
    page.fill("#composer-input", "/foco")
    page.keyboard.press("Enter")
    expect(page.locator("html")).to_have_attribute("data-foco", "true")


def test_resposta_mostra_o_tempo_que_levou(abrir):
    page = abrir("#/chat")
    enviar(page, "oi")
    esperar_fim(page)
    expect(ultima_resposta(page).locator(".msg-dur")).to_have_text(
        re.compile(r"^\d+(,\d)? (ms|s)$")
    )


def test_copiar_a_ultima_resposta_e_a_conversa(abrir):
    page = abrir("#/chat")
    enviar(page, "oi")
    esperar_fim(page)
    page.keyboard.press("Control+Shift+C")
    expect(page.locator(".toast").last).to_contain_text("Resposta copiada")
    assert (
        page.evaluate("navigator.clipboard.readText()")
        == "Entendido, Antônio. Estou pronto para o que vier."
    )
    page.fill("#composer-input", "/copiar conversa")
    page.keyboard.press("Enter")
    expect(page.locator(".toast").last).to_contain_text("Conversa copiada")
    assert page.evaluate("navigator.clipboard.readText()").startswith("# Conversa com o Orion")


def test_citar_trecho_selecionado_leva_ao_campo(abrir):
    page = abrir("#/chat")
    enviar(page, "Explique a diferença entre diagrama de classes e de sequência, com exemplo")
    esperar_fim(page)
    page.evaluate(
        """() => { const p = document.querySelector('.msg-orion .prose p'); const r = document.createRange();
        r.selectNodeContents(p); const s = getSelection(); s.removeAllRanges(); s.addRange(r); }"""
    )
    page.dispatch_event(".msg-orion .prose p", "mouseup")
    expect(page.locator("#citar-btn")).to_be_visible()
    page.click("#citar-btn")
    expect(page.locator("#composer-input")).to_be_focused()
    assert (
        page.locator("#composer-input")
        .input_value()
        .startswith("> Claro. Aqui está o plano da fase 0")
    )
    expect(page.locator("#citar-btn")).to_be_hidden()
    # sem seleção, o atalho avisa em vez de falhar calado
    page.keyboard.press("Control+Shift+Q")
    expect(page.locator(".toast").last).to_contain_text("Selecione um trecho")


def test_rascunho_e_por_conversa(abrir):
    page = abrir("#/chat")
    caixa = page.locator("#composer-input")
    # o mock é compartilhado entre os testes: parte de uma conversa conhecida
    page.locator("#sb-convs-list .conv", has_text="Plano da fase 0").click()
    expect(page.locator('#sb-convs-list .conv[aria-current="true"]')).to_contain_text(
        "Plano da fase 0"
    )
    caixa.fill("rascunho da primeira")
    page.locator("#sb-convs-list .conv", has_text="Dúvida de UML").click()
    expect(page.locator('#sb-convs-list .conv[aria-current="true"]')).to_contain_text(
        "Dúvida de UML"
    )
    expect(caixa).to_have_value("")
    caixa.fill("rascunho da segunda")
    page.locator("#sb-convs-list .conv", has_text="Plano da fase 0").click()
    expect(caixa).to_have_value("rascunho da primeira")
    page.locator("#sb-convs-list .conv", has_text="Dúvida de UML").click()
    expect(caixa).to_have_value("rascunho da segunda")


def test_aprovacao_pendente_avisa_fora_do_chat_e_a_paleta_leva_ate_ela(abrir):
    page = abrir("#/chat")
    enviar(page, "apague os arquivos antigos")
    esperar_fim(page)
    page.keyboard.press("Alt+3")
    expect(page.locator("html")).to_have_attribute("data-view", "memoria")
    expect(page.locator("#badge-chat")).to_have_class(re.compile("show"))
    page.keyboard.press("Control+k")
    page.keyboard.type("pendente")
    expect(page.locator('#palette-list [role="option"]').first).to_contain_text(
        "Revisar ação pendente"
    )
    page.keyboard.press("Enter")
    expect(page.locator("html")).to_have_attribute("data-view", "chat")
    expect(page.locator(".approval").first).to_be_focused()


# ── robustez (achados da revisão de código) ─────────────────────────────────────
def test_duas_chamadas_da_mesma_ferramenta_viram_dois_chips(abrir):
    page = abrir("#/chat")
    enviar(page, "chame a ferramenta duas vezes")
    esperar_fim(page)
    chips = page.locator(".tool-chip")
    expect(chips).to_have_count(2)
    expect(chips.nth(0)).to_have_attribute("data-estado", "negado")
    expect(chips.nth(1)).to_have_attribute("data-estado", "ok")


def test_limpar_durante_a_resposta_pede_para_esperar_e_o_stream_orfao_nao_escreve(abrir):
    page = abrir("#/chat")
    enviar(page, "responda bem lento por favor")
    page.fill("#composer-input", "/limpar")
    page.keyboard.press("Enter")
    expect(page.locator(".toast").last).to_contain_text("Espere a resposta terminar")
    expect(page.locator("#chat-col .msg-user")).to_have_count(1)  # a conversa ficou intacta
    # limpar por código no meio do stream: cancela de verdade, nada reaparece depois
    page.evaluate("Orion.chat.limpar()")
    page.wait_for_timeout(3800)  # o mock só começa a falar após 2,5 s
    expect(page.locator(".msg-orion")).to_have_count(0)
    expect(page.locator("#btn-send")).to_have_attribute("data-mode", "send")


def test_busca_ignora_texto_oculto_e_acompanha_a_conversa(abrir):
    page = abrir("#/chat")
    enviar(page, "oi")
    esperar_fim(page)
    page.keyboard.press("Control+f")
    page.keyboard.type("ajudar")  # só existe no estado vazio, escondido
    expect(page.locator("#find-count")).to_have_text("Nada encontrado")
    page.fill("#find-input", "pronto")
    expect(page.locator("#find-count")).to_have_text("1 de 1")
    page.evaluate("Orion.chat.limpar()")  # a busca aberta refaz sozinha
    expect(page.locator("#find-count")).to_have_text("Nada encontrado")


def test_buscar_a_partir_do_inicio_foca_a_busca_e_nao_o_campo(abrir):
    page = abrir()
    page.fill("#home-input", "/buscar qualquer")
    page.keyboard.press("Enter")
    expect(page.locator("html")).to_have_attribute("data-view", "chat")
    expect(page.locator("#find-input")).to_be_focused()
    page.wait_for_timeout(300)
    expect(page.locator("#find-input")).to_be_focused()


def test_arrastar_texto_nao_e_bloqueado_mas_arquivo_sim(abrir):
    page = abrir("#/chat")
    resultado = page.evaluate(
        """() => {
        const disparar = dt => { const e = new DragEvent('dragover', { bubbles: true, cancelable: true, dataTransfer: dt });
            document.body.dispatchEvent(e); return e.defaultPrevented; };
        const texto = new DataTransfer(); texto.setData('text/plain', 'um trecho');
        const arquivo = new DataTransfer(); arquivo.items.add(new File(['x'], 'a.png', { type: 'image/png' }));
        return [disparar(texto), disparar(arquivo)];
    }"""
    )
    assert resultado == [False, True]


def test_anexo_enviado_libera_a_miniatura_e_trocar_de_conversa_zera_o_tentar_de_novo(
    abrir, tmp_path
):
    page = abrir(
        "#/chat",
        init="window.__revogadas = 0; const r = URL.revokeObjectURL; URL.revokeObjectURL = u => { window.__revogadas++; return r(u); };",
    )
    # o mock é compartilhado entre testes: começa de uma conversa que não é a de destino
    page.locator("#sb-convs-list .conv", has_text="Plano da fase 0").click()
    expect(page.locator('#sb-convs-list .conv[aria-current="true"]')).to_contain_text(
        "Plano da fase 0"
    )
    img = tmp_path / "foto.png"
    img.write_bytes(
        bytes.fromhex(
            "89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c4890000000d49444154789c6360000002000001e221bc330000000049454e44ae426082"
        )
    )
    page.set_input_files("#file-input", str(img))
    expect(page.locator(".attach-chip")).to_have_attribute("data-estado", "ok")
    page.fill("#composer-input", "o que é isso?")
    page.keyboard.press("Enter")
    esperar_fim(page)
    assert page.evaluate("window.__revogadas") >= 1, "a miniatura ficou na memória depois do envio"
    assert page.evaluate("Orion.composer.temPedido()") is True
    page.locator("#sb-convs-list .conv", has_text="Dúvida de UML").click()
    expect(page.locator('#sb-convs-list .conv[aria-current="true"]')).to_contain_text(
        "Dúvida de UML"
    )
    assert page.evaluate("Orion.composer.temPedido()") is False


def test_desktop_parar_cancela_no_launcher_e_o_proximo_envio_espera_o_idle(abrir):
    page = abrir(init=SHIM_DESKTOP)
    page.wait_for_function("window.__hub && window.__hub.readyState === 1")
    enviar(page, "primeira pergunta")
    page.evaluate(
        "window.__hub.emit({state: 'processing'}); window.__hub.emit({ai_chunk: 'Começo da resposta '})"
    )
    expect(page.locator("#btn-send")).to_have_attribute("data-mode", "stop")
    page.keyboard.press("Escape")
    assert ["cancel_command"] in page.evaluate("window.__chamadas")
    expect(page.locator("#btn-send")).to_have_attribute("data-mode", "send")
    # sobra do pedido cancelado não entra em lugar nenhum
    page.evaluate("window.__hub.emit({ai_chunk: 'sobra que não deve aparecer'})")
    expect(page.locator("#chat-col")).not_to_contain_text("sobra que não deve aparecer")
    # enquanto o launcher não confirma o idle, um novo envio espera
    page.fill("#composer-input", "segunda pergunta")
    page.keyboard.press("Enter")
    expect(page.locator(".toast").last).to_contain_text("Espere a resposta terminar")
    assert len([c for c in page.evaluate("window.__chamadas") if c[0] == "process_command"]) == 1
    page.evaluate("window.__hub.emit({state: 'idle'})")
    page.keyboard.press("Enter")
    page.wait_for_function("window.__chamadas.filter(c => c[0] === 'process_command').length === 2")


def test_modo_foco_no_desktop_mantem_os_botoes_da_janela(abrir):
    page = abrir("#/chat", init=SHIM_DESKTOP)
    page.keyboard.press("Control+.")
    expect(page.locator("html")).to_have_attribute("data-foco", "true")
    expect(page.locator("#window-controls")).to_be_visible()
    page.click("#btn-min")
    assert ["minimize"] in page.evaluate("window.__chamadas")


# ── segurança no navegador (ORION_REGRAS 10–12) ─────────────────────────────────
def test_markdown_malicioso_nao_executa_nem_vaza(abrir):
    page = abrir("#/chat")
    saidas: list[str] = []
    page.on("request", lambda r: saidas.append(r.url))
    enviar(page, "me mostre um teste de xss")
    esperar_fim(page)
    prose = ultima_resposta(page).locator(".prose")
    assert page.evaluate("window.__pwned") is None, "script do texto executou!"
    assert page.locator("#chat-col script").count() == 0
    assert page.locator("#chat-col img[onerror], #chat-col img[src*='evil']").count() == 0
    assert page.locator("#chat-col a[href^='javascript']").count() == 0
    assert not [u for u in saidas if "evil.example" in u], (
        "o navegador buscou imagem externa (exfiltração)"
    )
    expect(prose.locator(".msg-img-bloqueada")).to_have_count(1)
    expect(prose).to_contain_text("<script>")  # aparece como TEXTO
    link = prose.locator("a.md-link", has_text="link legítimo")
    expect(link).to_have_attribute("href", "https://exemplo.com/pagina")
    expect(link).to_have_attribute("target", "_blank")


# ── desktop (pywebview + hub) ───────────────────────────────────────────────────
SHIM_DESKTOP = """
window.__chamadas = [];
window.pywebview = { api: {
  process_command: (t, m) => { window.__chamadas.push(['process_command', t, m]); return true; },
  get_config: async () => ({ token: '' }),
  open_external: u => { window.__chamadas.push(['open_external', u]); return true; },
  cancel_command: () => window.__chamadas.push(['cancel_command']),
  minimize_app: () => window.__chamadas.push(['minimize']),
  toggle_maximize: () => window.__chamadas.push(['maximize']),
  close_app: () => window.__chamadas.push(['close']),
}};
class HubFalso {
  constructor() { window.__hub = this; setTimeout(() => { this.readyState = 1; this.onopen && this.onopen(); }, 20); }
  close() { this.onclose && this.onclose(); }
  emit(m) { this.onmessage && this.onmessage({ data: JSON.stringify(m) }); }
}
window.WebSocket = HubFalso;
"""


def test_desktop_usa_o_hub_e_a_ponte_do_pywebview(abrir):
    page = abrir(init=SHIM_DESKTOP)
    expect(page.locator("html")).to_have_class(re.compile("shell-desktop"))
    expect(page.locator("#window-controls")).to_be_visible()
    page.wait_for_function("window.__hub && window.__hub.readyState === 1")
    enviar(page, "olá pelo hub")
    chamadas = page.evaluate("window.__chamadas")
    assert chamadas == [["process_command", "olá pelo hub", "auto"]]
    expect(page.locator(".msg-user .bubble")).to_have_count(1)  # eco do hub não duplica
    page.evaluate("window.__hub.emit({user_text: 'olá pelo hub'})")
    expect(page.locator(".msg-user .bubble")).to_have_count(1)
    page.evaluate("""() => { const h = window.__hub;
        h.emit({state: 'processing', intensity: 0.3}); h.emit({tier: 'Groq'});
        h.emit({ai_chunk: 'Resposta '}); h.emit({ai_chunk: 'via **hub**.'});
        h.emit({tool: {name: 'buscar_memoria', decision: 'allow'}});
        h.emit({state: 'idle', intensity: 0}); }""")
    expect(ultima_resposta(page).locator(".prose")).to_have_text("Resposta via hub.")
    expect(ultima_resposta(page).locator(".msg-tier")).to_have_text("Groq")
    expect(ultima_resposta(page).locator(".tool-chip")).to_have_attribute("data-estado", "ok")
    # fala vinda de fora (mic_engine) aparece como se fosse digitada
    page.evaluate("window.__hub.emit({user_text: 'oi do microfone'})")
    expect(page.locator(".msg-user .bubble").last).to_have_text("oi do microfone")
    # link abre fora do app, pela ponte
    page.evaluate(
        "window.__hub.emit({ai_chunk: ' [site](https://exemplo.com/x)'}); window.__hub.emit({state:'idle'})"
    )
    page.locator("a.md-link", has_text="site").click()
    assert ["open_external", "https://exemplo.com/x"] in page.evaluate("window.__chamadas")
    # controles da janela
    page.click("#btn-min")
    page.click("#btn-max")
    assert ["minimize"] in page.evaluate("window.__chamadas") and ["maximize"] in page.evaluate(
        "window.__chamadas"
    )


# ── configurações ───────────────────────────────────────────────────────────────
def test_tema_e_preferencias_persistem_ao_recarregar(abrir):
    page = abrir("#/config")
    page.locator('.theme-opt:has(input[value="grafite"])').click()
    expect(page.locator("html")).to_have_attribute("data-theme", "grafite")
    page.locator('.segmented:has(input[value="compacta"]) label', has_text="Compacta").click()
    page.fill("#cfg-scale", "1.2")
    page.locator("#cfg-scale").dispatch_event("input")
    page.reload()
    page.wait_for_selector("html[data-pronto='true']")
    expect(page.locator("html")).to_have_attribute("data-theme", "grafite")
    expect(page.locator("html")).to_have_attribute("data-density", "compacta")
    assert page.evaluate("document.documentElement.style.getPropertyValue('--ui-scale')") == "1.2"


def test_conexao_testar_e_token_do_orion_app(abrir, mock_token_url):
    page = abrir("#/chat", url=mock_token_url, http_ok=True)
    enviar(page, "olá")
    erro = page.locator(".msg-error")
    expect(erro).to_contain_text("Acesso negado")
    erro.get_by_role("button", name="Configurar token").click()
    expect(page.locator("html")).to_have_attribute("data-view", "config")
    page.fill("#cfg-token", TOKEN)
    page.locator("#cfg-token").dispatch_event("change")
    page.click("#cfg-test")
    expect(page.locator("#cfg-test-out")).to_contain_text("token válido")
    page.click('.sb-item[data-view="chat"]')
    enviar(page, "oi")
    esperar_fim(page)
    expect(ultima_resposta(page).locator(".prose")).to_contain_text("Entendido")


def _tela(page):
    return page.get_by_role("dialog", name="Entrar no Orion")


def _entrar(page, senha=SENHA, usuario="admin"):
    tela = _tela(page)
    tela.get_by_label("Usuário").fill(usuario)
    tela.get_by_label("Senha").fill(senha)
    page.keyboard.press("Enter")


def test_tela_de_entrada_cobre_o_app_inteiro_e_nada_dele_aparece_atras(abrir, mock_login_url):
    page = abrir("#/chat", url=mock_login_url, http_ok=True)
    expect(_tela(page)).to_be_visible()
    info = page.evaluate(
        """() => {
        const t = document.querySelector('#login');
        const cor = getComputedStyle(t).backgroundColor;
        const pontos = [[2, 2], [innerWidth - 3, 2], [2, innerHeight - 3], [innerWidth - 3, innerHeight - 3],
                        [innerWidth / 2, 20], [innerWidth / 2, innerHeight / 2], [60, innerHeight / 2]];
        return { cor, cobre: pontos.map(([x, y]) => !!document.elementFromPoint(x, y)?.closest('#login')),
                 appInerte: document.querySelector('#app').inert,
                 caixa: t.getBoundingClientRect().toJSON() };
    }"""
    )
    assert re.fullmatch(r"rgb\(\d+, \d+, \d+\)", info["cor"]), info[
        "cor"
    ]  # sólida: sem transparência
    assert all(info["cobre"]) and info["appInerte"] is True
    assert info["caixa"]["width"] >= 1440 and info["caixa"]["height"] >= 900
    # só entrada: usuário, senha e o botão; sem nenhum controle do app focável por Tab
    page.keyboard.press("Escape")
    expect(_tela(page)).to_be_visible()  # Esc não fecha
    focaveis = page.evaluate(
        "() => [...document.querySelectorAll('#login input, #login button')].map(e => e.id)"
    )
    assert focaveis == ["login-usuario", "login-senha", "login-entrar"]


def test_login_pede_usuario_e_senha_recusa_errados_e_libera_o_chat(abrir, mock_login_url):
    page = abrir("#/chat", url=mock_login_url, http_ok=True)
    tela = _tela(page)
    expect(tela).to_be_visible()
    expect(tela.get_by_label("Usuário")).to_be_focused()
    expect(page.locator(".msg-error")).to_have_count(0)

    page.keyboard.press("Enter")  # vazio: pede o usuário antes de qualquer chamada
    expect(tela).to_contain_text("Digite o usuário.")
    _entrar(page, "senha-errada")
    expect(tela).to_contain_text("Usuário ou senha incorretos.")
    expect(tela.get_by_label("Senha")).to_have_value("")  # a errada não fica no campo
    expect(tela.get_by_label("Senha")).to_be_focused()
    _entrar(page, SENHA, usuario="intruso")  # usuário errado falha igual
    expect(tela).to_contain_text("Usuário ou senha incorretos.")

    _entrar(page)
    expect(tela).to_have_count(0)
    expect(page.locator("#login")).to_be_hidden()
    enviar(page, "oi")
    esperar_fim(page)
    expect(ultima_resposta(page).locator(".prose")).to_contain_text("Entendido")


def test_login_bloqueado_mostra_o_aviso_de_espera(abrir, mock_login_url):
    page = abrir("#/chat", url=mock_login_url, http_ok=True)
    _entrar(page, "bloqueada")
    expect(_tela(page)).to_contain_text("Muitas tentativas")


def test_sessao_vencida_volta_a_tela_de_entrada_ao_enviar_mensagem(abrir, mock_login_url):
    page = abrir("#/chat", url=mock_login_url, http_ok=True)
    _entrar(page)
    expect(page.locator("#login")).to_be_hidden()
    page.context.clear_cookies()  # a sessão some (venceu, ou foi revogada)
    enviar(page, "oi")
    expect(_tela(page)).to_be_visible()
    expect(page.locator(".msg-error")).to_contain_text("Acesso negado")
    _entrar(page)
    expect(page.locator("#login")).to_be_hidden()


def test_sair_pelas_configuracoes_leva_a_tela_de_entrada(abrir, mock_login_url):
    page = abrir("#/config", url=mock_login_url, http_ok=True)
    _entrar(page)
    botao = page.locator("#cfg-sessao-btn")
    expect(botao).to_have_text("Sair")
    expect(page.locator("#cfg-sessao-desc")).to_contain_text("logado")
    botao.click()
    expect(_tela(page)).to_be_visible()  # sem sessão não há o que mostrar
    _entrar(page)
    expect(page.locator("#login")).to_be_hidden()
    expect(botao).to_have_text("Sair")


def test_sem_login_no_cerebro_nao_aparece_tela_de_entrada_nem_a_linha_de_sessao(abrir):
    page = abrir("#/config")  # mock sem login: o legado nem tem /auth/status
    page.wait_for_timeout(300)
    expect(_tela(page)).to_have_count(0)
    expect(page.locator("#cfg-sessao")).to_be_hidden()


def test_endereco_invalido_do_cerebro_e_recusado(abrir):
    page = abrir("#/config")
    page.fill("#cfg-url", "isso não é url")
    page.locator("#cfg-url").dispatch_event("change")
    expect(page.locator("#cfg-url")).to_have_attribute("aria-invalid", "true")
    expect(page.locator("#cfg-test-out")).to_contain_text("endereço completo")


def test_atividade_mostra_metricas_servicos_e_cascata(abrir):
    page = abrir("#/config")
    page.locator('.settings-nav a[data-sec="cfg-atividade"]').click()
    expect(page.locator("#a-meters .meter")).to_have_count(4)
    expect(page.locator("#a-tiers .tier-row")).to_have_count(3, timeout=8000)
    expect(page.locator("#a-servicos")).to_contain_text("Qdrant")
    expect(page.locator("#a-vetores")).not_to_have_text("—")


# ── memória e integrações ───────────────────────────────────────────────────────
def test_memoria_busca_abre_detalhe_com_vizinhos_e_esc_fecha(abrir):
    page = abrir("#/memoria")
    expect(page.locator("#mem-stats")).to_contain_text("nós", timeout=15000)
    page.fill("#mem-search", "backup")
    resultado = page.locator(".mem-result").first
    expect(resultado).to_be_visible()
    resultado.click()
    expect(page.locator("#mem-detail")).to_have_attribute("data-open", "true")
    expect(page.locator("#mem-detail-tipo")).to_have_text("Tópico")
    expect(
        page.locator("#mem-detail .mem-links button").first
    ).to_be_visible()  # vizinhos vêm do grafo inteiro
    page.keyboard.press("Escape")
    expect(page.locator("#mem-detail")).to_have_attribute("data-open", "false")
    page.click('#mem-filters [data-filtro="topico"]')
    expect(page.locator('#mem-filters [data-filtro="topico"]')).to_have_attribute(
        "aria-pressed", "false"
    )


def test_integracoes_listam_estado_real_e_acao_de_tts(abrir):
    page = abrir("#/integracoes")
    card = page.locator('.integ-card[data-id="tts"]')
    expect(card.locator(".badge")).to_have_text("Ativa", timeout=5000)
    card.get_by_role("button", name="Desligar").click()
    expect(card.get_by_role("button", name="Ligar")).to_be_visible()
    expect(page.locator('.integ-card[data-id="telegram"] .integ-status')).to_contain_text(
        "Bot rodando"
    )


def test_painel_mostra_alertas_modelos_clis_aprovacoes_e_politica(abrir):
    page = abrir("#/painel")
    corpo = page.locator("#painel-corpo")
    expect(page.locator("#painel-corpo .banner").first).to_be_visible(timeout=8000)
    # alertas: quarentena (grave) vem antes dos avisos; texto, não só cor
    expect(corpo.locator(".painel-alertas")).to_contain_text("Modelo “reserva”: em quarentena")
    expect(corpo.locator(".painel-alertas")).to_contain_text(
        "A CLI “gemini” esgotou o limite de hoje."
    )
    expect(corpo.locator(".painel-alertas")).to_contain_text("Servidor MCP “web”: falhou")
    primeiro = corpo.locator(".painel-alertas .banner").first
    assert "banner-danger" in (primeiro.get_attribute("class") or "")
    # modelos
    ok = corpo.locator('[data-endpoint="omniroute"]')
    expect(ok).to_contain_text("Funcionando")
    expect(ok).to_contain_text("último erro: HTTP 502")
    expect(ok.locator(".painel-provedores")).to_have_text("Serviu: gemini ×30 · groq ×8")
    reserva = corpo.locator('[data-endpoint="reserva"]')
    expect(reserva).to_contain_text("Em quarentena · volta em 2 min")
    expect(reserva).to_contain_text("3× cota")
    expect(reserva).to_contain_text("llama-3.3-70b · pesado")
    expect(ok).not_to_contain_text("padrão")  # a camada padrão não aparece
    expect(corpo.locator("[data-roteamento]")).to_have_text(
        "Roteamento por tipo de tarefa: 12 rápidas · 3 pesadas · 1 com imagem"
    )
    # CLIs: uso, esgotada e não instalada
    expect(corpo.locator('[data-cli="claude"] .meter-val')).to_have_text("4/20")
    expect(corpo.locator('[data-cli="claude"]')).to_have_attribute("data-sev", "normal")
    expect(corpo.locator('[data-cli="gemini"]')).to_have_attribute("data-sev", "critico")
    expect(corpo.locator('[data-cli="gemini"] .painel-meter-nota')).to_have_text("esgotada hoje")
    expect(corpo.locator('[data-cli="codex"]')).to_have_attribute("data-sev", "nd")
    # política
    expect(corpo.locator('[data-id="decisoes"]')).to_contain_text(
        "12 decisões: 9 liberadas, 2 pediram aval, 1 negada."
    )
    expect(corpo.locator('[data-id="decisoes"] .painel-recentes li')).to_have_count(3)
    expect(corpo.locator('[data-id="sistema"]')).to_contain_text("MCP · google")
    expect(corpo.locator('[data-id="aprovacoes"] h3')).to_have_text("Aprovações")


def test_painel_aprovacao_pendente_aparece_e_leva_ao_chat(abrir):
    page = abrir("#/chat")
    enviar(page, "apague os arquivos antigos")  # o mock responde com um pedido de aval
    expect(page.locator(".approval").first).to_be_visible(timeout=15000)
    page.keyboard.press("Alt+6")
    expect(page.locator("html")).to_have_attribute("data-view", "painel")
    cartao = page.locator('#painel-corpo [data-id="aprovacoes"]')
    expect(cartao).to_contain_text("executar_comando", timeout=8000)
    expect(page.locator("#painel-corpo .painel-alertas")).to_contain_text(
        re.compile(r"esperam? o seu aval")
    )  # o mock é da sessão inteira: pode haver mais de uma pendente
    cartao.get_by_role("button", name="Abrir o chat para decidir").click()
    expect(page.locator("html")).to_have_attribute("data-view", "chat")


def test_painel_tudo_em_ordem_e_falha_do_cerebro(abrir):
    import json as _json

    calmo = {
        "uptime_s": 30, "modelos": {"configurado": True, "endpoints": []}, "clis": [],
        "aprovacoes": {"pendentes": 0, "itens": []},
        "decisoes": {"janela_h": 24, "total": 0, "por_acao": {"allow": 0, "confirm": 0, "deny": 0},
                     "mais_usadas": [], "recentes": []},
        "avisos": {"pendentes": 0}, "jobs": {"ativo": False, "ultima_rodada": None, "erros": []},
        "memoria": {"ok": True, "vetores": False}, "canais": {"telegram": False},
        "ferramentas": 0, "mcp": {},
    }  # fmt: skip
    page = abrir(http_ok=True)  # o 500 de propósito aparece no console do navegador
    estado = {"falha": False}

    def rota(route):
        if estado["falha"]:
            route.fulfill(status=500, body="erro interno")
        else:
            route.fulfill(content_type="application/json", body=_json.dumps(calmo))

    page.route("**/painel", rota)
    page.keyboard.press("Alt+6")
    expect(page.locator("#painel-corpo .banner-info")).to_contain_text(
        "Tudo em ordem", timeout=8000
    )
    expect(page.locator('[data-id="decisoes"]')).to_contain_text(
        "Nenhuma decisão nas últimas 24 h."
    )
    expect(page.locator('[data-id="sistema"]')).to_contain_text("nenhum servidor")
    # o cérebro falha na atualização: avisa que os números podem estar velhos, sem apagar a tela
    estado["falha"] = True
    page.click("#painel-refresh")
    expect(page.locator("#painel-corpo .painel-alertas")).to_contain_text(
        "A última atualização falhou", timeout=8000
    )
    expect(page.locator('[data-id="sistema"]')).to_be_visible()


def test_painel_pela_paleta_e_por_comando_de_barra(abrir):
    page = abrir("#/chat")
    page.keyboard.press("Control+k")
    page.fill("#palette-input", "painel")
    page.keyboard.press("Enter")
    expect(page.locator("html")).to_have_attribute("data-view", "painel")
    page.keyboard.press("Alt+2")
    # `enviar` escolhe o campo pela tela atual: sem esperar a troca de tela, ele escreve no campo da home
    expect(page.locator("html")).to_have_attribute("data-view", "chat")
    enviar(page, "/painel")
    expect(page.locator("html")).to_have_attribute("data-view", "painel")


def test_cerebro_offline_degrada_sem_quebrar(abrir):
    # a interface vem do mock, mas o endereço do cérebro configurado aponta para uma porta morta
    page = abrir(
        init="localStorage.setItem('orion_base_url', JSON.stringify('http://127.0.0.1:9'))",
        http_ok=True,
    )
    expect(page.locator("#conn-text")).to_have_text("Sem conexão", timeout=8000)
    expect(page.locator("#sb-convs-list")).to_contain_text("offline")
    enviar(page, "alguém aí?")
    expect(page.locator(".msg-error")).to_contain_text("A conexão com o cérebro caiu")
    page.click('.sb-item[data-view="memoria"]')
    expect(page.locator("#mem-banner")).to_contain_text("demonstração", timeout=15000)


# ── movimento e janela estreita ─────────────────────────────────────────────────
def test_movimento_reduzido_pula_o_boot_e_as_transicoes(abrir):
    page = abrir(reduced=True, boot=True)
    assert page.locator("#boot").count() == 0
    enviar(page, "oi")
    expect(page.locator(".msg-user").first).to_have_attribute("data-visible", "true")


def test_boot_aparece_uma_vez_por_sessao(abrir):
    page = abrir(boot=True)
    expect(page.locator("#boot")).to_have_count(0, timeout=8000)
    page.reload()
    page.wait_for_selector("html[data-pronto='true']")
    assert page.locator("#boot").count() == 0


# ── acessibilidade (axe-core) ───────────────────────────────────────────────────
pytestmark_axe = pytest.mark.skipif(
    not AXE.exists(), reason="rode `npm ci` em tests/front_e2e para instalar o axe-core"
)


def _violacoes(page):
    # o diálogo aparece com um fade (opacity do scrim): medido no meio dele, o contraste dá falso positivo
    page.wait_for_function(
        """() => [...document.querySelectorAll('.dialog-scrim[data-open="true"]')]
            .every(s => getComputedStyle(s).opacity === '1')"""
    )
    res = page.evaluate(
        """() => axe.run(document, { runOnly: { type: 'tag',
        values: ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa', 'best-practice'] } })"""
    )
    return [
        (v["impact"], v["id"], [n["target"] for n in v["nodes"]][:4]) for v in res["violations"]
    ]


@pytestmark_axe
@pytest.mark.parametrize("tema", TEMAS)
@pytest.mark.parametrize("rota", ROTAS)
def test_axe_sem_violacoes_em_todas_as_telas_e_temas(abrir, rota, tema):
    page = abrir(rota, axe=True)
    page.evaluate(f"Orion.prefs.set('theme', '{tema}')")
    page.wait_for_timeout(900 if rota == "#/memoria" else 500)
    assert _violacoes(page) == []


@pytestmark_axe
def test_axe_chat_com_resposta_aprovacao_e_erro(abrir):
    page = abrir("#/chat", axe=True)
    enviar(page, "Explique a diferença entre diagrama de classes e de sequência, com exemplo")
    esperar_fim(page)
    enviar(page, "apague os arquivos antigos")
    esperar_fim(page)
    enviar(page, "isso vai dar falha")
    expect(page.locator(".msg-error")).to_have_count(1)
    assert _violacoes(page) == []


@pytestmark_axe
def test_axe_paleta_e_menu_de_modelo_abertos(abrir):
    page = abrir("#/chat", axe=True)
    page.click("#model-btn")
    expect(page.locator("#model-menu")).to_have_attribute("data-open", "true")
    page.wait_for_timeout(400)  # espera a transição: contraste de meia-opacidade engana o axe
    assert _violacoes(page) == []
    page.keyboard.press("Escape")
    page.keyboard.press("Control+k")
    page.keyboard.type("t")
    expect(page.locator("#palette")).to_have_attribute("data-open", "true")
    page.wait_for_timeout(400)
    assert _violacoes(page) == []


@pytestmark_axe
@pytest.mark.parametrize("tema", TEMAS)
def test_axe_tela_de_entrada_aberta(abrir, mock_login_url, tema):
    page = abrir("#/chat", url=mock_login_url, http_ok=True, axe=True)
    page.evaluate(f"Orion.prefs.set('theme', '{tema}')")
    expect(_tela(page)).to_be_visible()
    page.wait_for_timeout(500)
    assert _violacoes(page) == []
    _entrar(page, "senha-errada")
    expect(_tela(page)).to_contain_text("Usuário ou senha incorretos.")
    assert _violacoes(page) == []


@pytestmark_axe
def test_so_um_h1_e_landmarks_unicos(abrir):
    page = abrir()
    info = json.loads(
        page.evaluate("""() => JSON.stringify({
        h1: document.querySelectorAll('h1').length,
        main: document.querySelectorAll('main').length,
        nav: [...document.querySelectorAll('nav')].map(n => n.getAttribute('aria-label')),
        banner: document.querySelectorAll('header').length })""")
    )
    assert info["h1"] == 1 and info["main"] == 1 and info["banner"] == 1
    assert all(info["nav"]) and len(set(info["nav"])) == len(info["nav"]), (
        "navs precisam de rótulos distintos"
    )


@pytest.mark.parametrize("largura", [700, 860])
@pytest.mark.parametrize("rota", ROTAS)
def test_janela_estreita_recolhe_a_barra_e_nao_rola_na_horizontal(abrir, largura, rota):
    page = abrir(rota, viewport=(largura, 800))
    page.wait_for_timeout(600)
    expect(page.locator("html")).to_have_attribute("data-sb", "collapsed")
    expect(page.locator("#sb-toggle")).to_be_hidden()
    assert page.evaluate(
        "document.documentElement.scrollWidth <= document.documentElement.clientWidth"
    ), f"rolagem horizontal em {rota or 'home'} a {largura}px"


def test_janela_larga_mantem_a_barra_aberta_e_o_atalho_recolhe(abrir):
    page = abrir(viewport=(1280, 800))
    expect(page.locator("html")).to_have_attribute("data-sb", "expanded")
    page.set_viewport_size({"width": 800, "height": 800})
    expect(page.locator("html")).to_have_attribute("data-sb", "collapsed")
    page.set_viewport_size({"width": 1280, "height": 800})
    expect(page.locator("html")).to_have_attribute("data-sb", "expanded")


@pytestmark_axe
def test_senha_de_fabrica_avisa_so_depois_do_login_e_a_troca_some_com_o_aviso(
    abrir, mock_fabrica_url
):
    page = abrir("#/chat", url=mock_fabrica_url, http_ok=True, axe=True)
    expect(_tela(page)).to_be_visible()
    expect(page.locator(".toast")).to_have_count(
        0
    )  # antes do login nada anuncia a senha de fábrica
    _entrar(page)
    aviso = page.locator(".toast", has_text="senha de fábrica")
    expect(aviso).to_be_visible()
    aviso.get_by_role("button", name="Trocar").click()
    expect(page.locator("html")).to_have_attribute("data-view", "config")
    expect(page.locator("#cfg-senha-aviso")).to_contain_text("senha de fábrica")
    page.fill("#cfg-senha-atual", "errada-errada")
    page.fill("#cfg-senha-nova", "curta")
    page.click("#cfg-senha-btn")
    expect(page.locator("#cfg-senha-out")).to_contain_text("senha atual incorreta")
    page.fill("#cfg-senha-atual", SENHA)
    page.click("#cfg-senha-btn")
    expect(page.locator("#cfg-senha-out")).to_contain_text("pelo menos 12 caracteres")
    page.fill("#cfg-senha-nova", "uma-senha-nova-bem-longa-9")
    page.click("#cfg-senha-btn")
    expect(page.locator("#cfg-senha-out")).to_contain_text("Senha trocada.")
    expect(page.locator("#cfg-senha-aviso")).not_to_contain_text("senha de fábrica")
    page.wait_for_timeout(500)
    assert _violacoes(page) == []  # a tela de Configurações com o formulário de trocar senha


# ── acabamento (análise visual de 06/10/2026) ───────────────────────────────────
def test_integracoes_sem_nome_de_arquivo_e_com_dica_no_microfone_parado(abrir):
    page = abrir("#/integracoes")
    mic = page.locator('.integ-card[data-id="mic"] .integ-status')
    expect(mic).to_contain_text("Microfone parado", timeout=5000)
    expect(mic).to_contain_text("computador onde o cérebro roda")
    assert ".py" not in page.locator("#integ-grid").inner_text()
    # a faixa de estado tem a mesma altura em todos os cartões (botão ou não)
    alturas = page.evaluate(
        "[...document.querySelectorAll('.integ-status')].map(e => Math.round(e.getBoundingClientRect().height))"
    )
    assert len(set(alturas)) <= 2 and min(alturas) >= 50, alturas


def test_pilula_de_estado_so_aparece_com_atividade_ou_aprovacao_pendente(abrir):
    page = abrir("#/chat")
    pilula = page.locator("#state-pill")
    # o backend de mentira é compartilhado: aprovação deixada por outro teste também acende a pílula (é o comportamento certo)
    page.wait_for_timeout(1200)
    for negar in page.get_by_role("button", name="Negar").all():
        negar.click()
    expect(pilula).to_be_hidden()
    enviar(page, "apague os arquivos antigos")
    expect(page.locator(".approval").last).to_have_attribute("data-estado", "pendente")
    esperar_fim(page)
    expect(pilula).to_be_visible()
    expect(page.locator("#state-label")).to_have_text("Aguardando aprovação")
    page.locator(".approval").last.get_by_role("button", name="Negar").click()
    expect(pilula).to_be_hidden()


def test_composer_alinha_com_a_coluna_de_mensagens(abrir):
    page = abrir("#/chat", viewport=(1440, 900))
    enviar(page, "me explique algo longo com tabela e código")
    esperar_fim(page)
    caixa = page.evaluate(
        """() => {
            const c = document.querySelector('#composer-box').getBoundingClientRect();
            const m = document.querySelector('.msg-orion').getBoundingClientRect();
            const u = document.querySelector('.msg-user').getBoundingClientRect();
            return { cL: c.left, cR: c.right, mL: m.left, uR: u.right };
        }"""
    )
    assert abs(caixa["cL"] - caixa["mL"]) <= 1.5, caixa
    assert abs(caixa["cR"] - caixa["uR"]) <= 1.5, caixa


def test_paragrafo_depois_de_tabela_codigo_e_citacao_tem_espaco(abrir):
    page = abrir("#/chat")
    enviar(page, "me explique algo longo com tabela e código")
    esperar_fim(page)
    folgas = page.evaluate(
        """() => {
            const pr = [...document.querySelectorAll('.msg-orion .prose')].pop();
            const out = [];
            for (const f of pr.children) {
                const ant = f.previousElementSibling;
                if (ant && f.tagName === 'P') out.push([ant.className || ant.tagName, f.getBoundingClientRect().top - ant.getBoundingClientRect().bottom]);
            }
            return out;
        }"""
    )
    assert folgas, "a resposta de teste não tem parágrafo depois de outro bloco"
    assert all(g >= 6 for _, g in folgas), folgas


def test_grafico_de_latencia_so_aparece_com_medicoes_suficientes(abrir):
    page = abrir("#/config")
    legenda = page.locator("#a-spark-legenda")
    expect(legenda).to_contain_text("medições", timeout=8000)
    texto = legenda.inner_text()
    if "Coletando" in texto:
        expect(page.locator("#a-spark")).to_be_hidden()
    else:
        assert "mín" in texto and "máx" in texto
        expect(page.locator("#a-spark")).to_be_visible()


def test_memoria_resultados_trazem_tipo_e_ligacoes_para_distinguir_nos(abrir):
    page = abrir("#/memoria")
    page.fill("#mem-search", "memória")
    primeiro = page.locator(".mem-result").first
    expect(primeiro).to_be_visible(timeout=8000)
    meta = primeiro.locator("small")
    expect(meta).to_have_text(re.compile(r"(Tópico|Fala do Orion|Fala sua) · "))
    assert "ligaç" in meta.inner_text()


def test_alto_contraste_na_home_mantem_a_constelacao_visivel(abrir):
    page = abrir(init="localStorage.setItem('orion_theme', JSON.stringify('contraste'))")
    assert page.evaluate("document.documentElement.dataset.theme") == "contraste"
    assert (
        float(page.evaluate("getComputedStyle(document.querySelector('#sky-veil')).opacity")) < 0.5
    )
    page.click('.sb-item[data-view="chat"]')
    expect(page.locator("html")).to_have_attribute("data-view", "chat")
    page.wait_for_timeout(600)
    assert (
        float(page.evaluate("getComputedStyle(document.querySelector('#sky-veil')).opacity")) > 0.9
    )


def test_html_nao_usa_estilo_inline_estatico():
    from pathlib import Path

    html = (
        Path(__file__).resolve().parents[2] / "Orion_Core" / "Front_end_Orion" / "index.html"
    ).read_text(encoding="utf-8")
    assert 'style="' not in html, 'use classes (components.css), não style="" no index.html'


def test_editar_mensagem_enviada_poe_o_texto_no_campo_sem_apagar_rascunho(abrir):
    page = abrir("#/chat")
    enviar(page, "me lembra de ligar para o Pedro")
    esperar_fim(page)
    usuario = page.locator(".msg-user").first
    usuario.hover()
    usuario.get_by_role("button", name="Editar e reenviar").click()
    expect(page.locator("#composer-input")).to_have_value("me lembra de ligar para o Pedro")
    expect(page.locator("#composer-input")).to_be_focused()
    # com rascunho diferente no campo, não sobrescreve
    page.fill("#composer-input", "outra coisa")
    usuario.hover()
    usuario.get_by_role("button", name="Editar e reenviar").click()
    expect(page.locator("#composer-input")).to_have_value("outra coisa")
    expect(page.locator(".toast").last).to_contain_text("já tem um rascunho")


# ── conversas: renomear, fixar, apagar (barra lateral, menu ⋯ e paleta) ─────────
def _linha(page, titulo: str):
    return page.locator("#sb-convs-list .conv-row", has_text=titulo)


def _abrir_menu(page, titulo: str):
    linha = _linha(page, titulo)
    linha.hover()
    linha.locator(".conv-more").click()
    expect(page.locator("#conv-menu")).to_have_attribute("data-open", "true")


def test_renomear_pelo_menu_persiste_e_cancelar_nao_muda(abrir, mock_isolado_url):
    page = abrir("#/chat", url=mock_isolado_url)
    _abrir_menu(page, "Backup diário")
    page.get_by_role("menuitem", name="Renomear").click()
    dialogo = page.get_by_role("dialog", name="Renomear conversa")
    expect(page.get_by_label("Título")).to_have_value("Backup diário do vault no iCloud")
    page.keyboard.press("Escape")  # cancelar
    expect(dialogo).to_have_count(0)
    expect(_linha(page, "Backup diário")).to_have_count(1)

    _abrir_menu(page, "Backup diário")
    page.get_by_role("menuitem", name="Renomear").click()
    page.get_by_label("Título").fill("  Backup   do   cofre ")
    page.keyboard.press("Enter")
    expect(_linha(page, "Backup do cofre")).to_have_count(1)
    expect(_linha(page, "Backup diário")).to_have_count(0)
    page.reload()
    page.wait_for_selector("html[data-pronto='true']")
    expect(_linha(page, "Backup do cofre")).to_have_count(1, timeout=5000)


def test_renomear_com_titulo_vazio_nao_confirma(abrir, mock_isolado_url):
    page = abrir("#/chat", url=mock_isolado_url)
    _abrir_menu(page, "Ideias para a voz")
    page.get_by_role("menuitem", name="Renomear").click()
    page.get_by_label("Título").fill("   ")
    page.keyboard.press("Enter")
    expect(page.get_by_role("dialog", name="Renomear conversa")).to_be_visible()
    expect(_linha(page, "Ideias para a voz")).to_have_count(1)


def test_fixar_sobe_para_o_grupo_fixadas_e_desafixar_volta(abrir, mock_isolado_url):
    page = abrir("#/chat", url=mock_isolado_url)
    _abrir_menu(page, "Resumo do PDF")
    page.get_by_role("menuitem", name="Fixar no topo").click()
    expect(page.locator("#sb-convs-list .conv-group").first).to_have_text("Fixadas")
    primeira = page.locator("#sb-convs-list .conv-row").first
    expect(primeira).to_contain_text("Resumo do PDF")
    expect(primeira.get_by_role("img", name="Fixada")).to_be_visible()
    _abrir_menu(page, "Resumo do PDF")
    page.get_by_role("menuitem", name="Desafixar").click()
    expect(page.locator("#sb-convs-list .conv-group", has_text="Fixadas")).to_have_count(0)


def test_apagar_pede_confirmacao_e_so_apaga_ao_confirmar(abrir, mock_isolado_url):
    page = abrir("#/chat", url=mock_isolado_url)
    _abrir_menu(page, "Bot do Telegram")
    page.get_by_role("menuitem", name="Apagar").click()
    alerta = page.get_by_role("alertdialog", name="Apagar esta conversa?")
    expect(alerta).to_contain_text("As mensagens ficam guardadas")
    expect(alerta.get_by_role("button", name="Cancelar")).to_be_focused()  # perigo: foco no seguro
    alerta.get_by_role("button", name="Cancelar").click()
    expect(_linha(page, "Bot do Telegram")).to_have_count(1)

    _abrir_menu(page, "Bot do Telegram")
    page.get_by_role("menuitem", name="Apagar").click()
    page.get_by_role("alertdialog").get_by_role("button", name="Apagar").click()
    expect(_linha(page, "Bot do Telegram")).to_have_count(0)
    page.reload()
    page.wait_for_selector("html[data-pronto='true']")
    expect(_linha(page, "Dúvida de UML")).to_have_count(1, timeout=5000)
    expect(_linha(page, "Bot do Telegram")).to_have_count(0)


def test_apagar_a_conversa_ativa_troca_para_a_proxima(abrir, mock_isolado_url):
    page = abrir("#/chat", url=mock_isolado_url)
    atual = page.locator('#sb-convs-list .conv[aria-current="true"]')
    expect(atual).to_contain_text("Plano da fase 0", timeout=5000)
    _abrir_menu(page, "Plano da fase 0")
    page.get_by_role("menuitem", name="Apagar").click()
    page.get_by_role("alertdialog").get_by_role("button", name="Apagar").click()
    expect(_linha(page, "Plano da fase 0")).to_have_count(0)
    expect(atual).to_contain_text("Memória nova em SQLite", timeout=5000)


def test_conversa_importada_somente_leitura_nao_tem_menu(abrir, mock_isolado_url):
    page = abrir("#/chat", url=mock_isolado_url)
    expect(_linha(page, "conversas antigas")).to_have_count(1)
    expect(_linha(page, "conversas antigas").locator(".conv-more")).to_have_count(0)
    expect(_linha(page, "Plano da fase 0").locator(".conv-more")).to_have_count(1)


def test_menu_da_conversa_funciona_so_com_teclado(abrir, mock_isolado_url):
    page = abrir("#/chat", url=mock_isolado_url)
    mais = _linha(page, "Dúvida de UML").locator(".conv-more")
    mais.focus()
    expect(mais).to_have_css("opacity", "1")  # foco dentro da linha mostra o botão
    page.keyboard.press("Enter")
    itens = page.get_by_role("menuitem")
    expect(itens.first).to_be_focused()
    # o menu abre colado no botão ⋯ (e dentro da janela), não solto em outro canto da tela
    pos = page.evaluate(
        """() => {
            const b = document.querySelector('.conv-more[aria-expanded="true"]').getBoundingClientRect();
            const m = document.querySelector('#conv-menu').getBoundingClientRect();
            return { dx: Math.abs(m.left - b.left), dy: m.top - b.bottom, dentro: m.right <= innerWidth && m.bottom <= innerHeight };
        }"""
    )
    assert pos["dx"] <= 190 and 0 <= pos["dy"] <= 12 and pos["dentro"], pos
    page.keyboard.press("ArrowDown")
    expect(itens.nth(1)).to_be_focused()
    page.keyboard.press("ArrowUp")
    page.keyboard.press("ArrowUp")
    expect(itens.last).to_be_focused()  # dá a volta
    page.keyboard.press("Escape")
    expect(page.locator("#conv-menu")).to_have_attribute("data-open", "false")
    expect(mais).to_be_focused()
    expect(mais).to_have_attribute("aria-expanded", "false")


def test_paleta_renomeia_fixa_e_apaga_a_conversa_atual(abrir, mock_isolado_url):
    page = abrir("#/chat", url=mock_isolado_url)
    expect(page.locator('#sb-convs-list .conv[aria-current="true"]')).to_be_visible(timeout=5000)
    page.keyboard.press("Control+k")
    page.fill("#palette-input", "fixar conversa atual")
    page.keyboard.press("Enter")
    expect(page.locator("#sb-convs-list .conv-group").first).to_have_text("Fixadas")
    page.keyboard.press("Control+k")
    page.fill("#palette-input", "renomear conversa atual")
    page.keyboard.press("Enter")
    page.get_by_label("Título").fill("Fase zero revisada")
    page.keyboard.press("Enter")
    expect(_linha(page, "Fase zero revisada")).to_have_count(1)
    page.keyboard.press("Control+k")
    page.fill("#palette-input", "apagar conversa atual")
    page.keyboard.press("Enter")
    page.get_by_role("alertdialog").get_by_role("button", name="Apagar").click()
    expect(_linha(page, "Fase zero revisada")).to_have_count(0)


def test_conversa_apagada_sem_cerebro_que_aceite_mostra_erro_legivel(abrir, mock_isolado_url):
    page = abrir("#/chat", url=mock_isolado_url, http_ok=True)  # o 404 é de propósito
    page.route(
        "**/sessoes/s5",
        lambda r: r.fulfill(status=404, body='{"detail":"conversa não encontrada"}'),
    )
    _abrir_menu(page, "Bot do Telegram")
    page.get_by_role("menuitem", name="Apagar").click()
    page.get_by_role("alertdialog").get_by_role("button", name="Apagar").click()
    expect(page.locator(".toast", has_text="Não consegui apagar")).to_be_visible()
    expect(_linha(page, "Bot do Telegram")).to_have_count(1)


@pytestmark_axe
def test_axe_menu_da_conversa_e_dialogos_abertos(abrir, mock_isolado_url):
    page = abrir("#/chat", axe=True, url=mock_isolado_url)
    _abrir_menu(page, "Dúvida de UML")
    assert _violacoes(page) == []
    page.get_by_role("menuitem", name="Renomear").click()
    assert _violacoes(page) == []
    page.keyboard.press("Escape")
    _abrir_menu(page, "Dúvida de UML")
    page.get_by_role("menuitem", name="Apagar").click()
    assert _violacoes(page) == []


# ── voz por clique (fase 6) ──────────────────────────────────────────────────────
GRAVADOR_FALSO = """
(() => {
  class FakeRecorder {
    static isTypeSupported() { return true; }
    constructor(stream, opcoes) { this.mimeType = opcoes.mimeType; this.state = 'inactive'; }
    start() { this.state = 'recording'; }
    stop() {
      this.state = 'inactive';
      this.ondataavailable?.({ data: new Blob([new Uint8Array(4000)], { type: this.mimeType }) });
      setTimeout(() => this.onstop?.(), 0);
    }
  }
  window.MediaRecorder = FakeRecorder;
  window.__trilhas_paradas = 0;
  const trilha = { stop() { window.__trilhas_paradas++; } };
  navigator.mediaDevices.getUserMedia = async () => ({ getTracks: () => [trilha] });
})();
"""


def test_falar_por_clique_vira_turno_e_mostra_o_que_foi_entendido(abrir):
    page = abrir("#/chat", init=GRAVADOR_FALSO)
    botao = page.locator("#composer-box .voice-ptt-btn")
    expect(botao).to_have_attribute("aria-pressed", "false")
    botao.click()
    expect(botao).to_have_attribute("aria-pressed", "true")
    expect(botao).to_have_attribute("aria-label", "Parar e enviar a fala")
    botao.click()  # segundo clique envia
    expect(page.locator(".msg-user").last).to_contain_text("que horas são")
    expect(ultima_resposta(page)).to_contain_text("São três e meia (4000 bytes).")
    esperar_fim(page)
    expect(botao).to_have_attribute("aria-pressed", "false")
    expect(botao).to_be_enabled()
    assert page.evaluate("window.__trilhas_paradas") == 1, "o microfone ficou aberto"


def test_esc_descarta_a_gravacao_sem_enviar(abrir):
    page = abrir("#/chat", init=GRAVADOR_FALSO)
    botao = page.locator("#composer-box .voice-ptt-btn")
    botao.click()
    expect(botao).to_have_attribute("aria-pressed", "true")
    page.keyboard.press("Escape")
    expect(botao).to_have_attribute("aria-pressed", "false")
    assert page.locator(".msg-user").count() == 0
    assert page.evaluate("window.__trilhas_paradas") == 1


def test_falar_a_partir_da_home_abre_o_chat_e_a_paleta_tem_o_comando(abrir):
    page = abrir("", init=GRAVADOR_FALSO)
    page.locator("#home-form .voice-ptt-btn").click()
    page.locator("#home-form .voice-ptt-btn").click()
    expect(page).to_have_url(re.compile(r"#/chat$"))
    expect(page.locator(".msg-user").last).to_contain_text("que horas são")
    page.keyboard.press("Control+k")
    page.fill("#palette-input", "falar com")
    expect(page.locator("#palette-list")).to_contain_text("Falar com o Orion")


def test_sem_microfone_o_botao_explica_em_vez_de_quebrar(abrir):
    page = abrir(
        "#/chat", init="Object.defineProperty(navigator, 'mediaDevices', { value: undefined });"
    )
    page.locator("#composer-box .voice-ptt-btn").click()
    expect(page.locator(".toast").last).to_contain_text("Microfone indisponível")
