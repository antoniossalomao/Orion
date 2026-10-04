"""Front do Orion no navegador de verdade: fluxos, teclado, segurança, janela estreita e acessibilidade."""

from __future__ import annotations

import json
import re

import pytest
from playwright.sync_api import expect

from .conftest import AXE, TOKEN

ROTAS = ["", "#/chat", "#/memoria", "#/integracoes", "#/config"]
VIEWS = ["home", "chat", "memoria", "integracoes", "config"]
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
    expect(page.locator("#home-status")).to_contain_text("Automático")
    expect(page.locator("#home-status")).not_to_contain_text("Cérebro conectado")
    assert page.locator("#sky canvas").count() == 1, "a constelação (WebGL) não subiu"


def test_falha_de_sessoes_nao_altera_conexao_nem_dispara_notificacao(abrir):
    page = abrir()
    expect(page.locator("#conn-text")).to_have_text("Conectado")
    resultado = page.evaluate("""async () => {
        const O = Orion;
        const sessoes = O.api.sessoes;
        O.api.sessoes = async () => { throw new Error('sessões indisponíveis'); };
        await O.sidebar.carregar();
        const online = O.sidebar.online();
        await O.sidebar.verificar();
        O.api.sessoes = sessoes;
        return online;
    }""")
    assert resultado is True, "falha ao listar sessões não significa perder conexão"
    expect(page.locator("#conn-text")).to_have_text("Conectado")
    expect(page.locator('#toasts [data-id="conn"]')).to_have_count(0)


def test_reconexao_silenciosa_e_ping_sem_sobreposicao(abrir):
    page = abrir()
    expect(page.locator("#conn-text")).to_have_text("Conectado")
    page.evaluate("""async () => {
        const O = Orion;
        const ping = O.api.ping;
        O.api.ping = async () => ({ ok: false, ms: null });
        await O.sidebar.verificar();
        O.api.ping = ping;
    }""")
    expect(page.locator("#conn-text")).to_have_text("Sem conexão")
    resultado = page.evaluate("""async () => {
        const O = Orion;
        const ping = O.api.ping;
        let chamadas = 0;
        let resolver;
        O.api.ping = () => { chamadas++; return new Promise(r => { resolver = r; }); };
        const primeira = O.sidebar.verificar();
        const segunda = O.sidebar.verificar();
        resolver({ ok: true, ms: 5 });
        await Promise.all([primeira, segunda]);
        O.api.ping = ping;
        return chamadas;
    }""")
    assert resultado == 1, "polling concorrente pode aplicar resultados fora de ordem"
    expect(page.locator("#conn-text")).to_have_text("Conectado")
    expect(page.locator('#toasts [data-id="conn"]')).to_have_count(0)


def test_status_home_preserva_dom_quando_dados_nao_mudam(abrir):
    page = abrir()
    expect(page.locator("#home-status")).to_contain_text("memórias")
    resultado = page.evaluate("""async () => {
        const caixa = document.querySelector('#home-status');
        const original = caixa.firstElementChild;
        const health = Orion.api.health;
        let terminou;
        const pronto = new Promise(r => { terminou = r; });
        Orion.api.health = async () => { const r = await health(); terminou(); return r; };
        Orion.bus.emit('conn', { ok: true, ms: 5 });
        await pronto;
        await new Promise(r => requestAnimationFrame(r));
        Orion.api.health = health;
        return caixa.firstElementChild === original;
    }""")
    assert resultado, "polling não deve recriar os indicadores estáveis da home"


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
    expect(page.locator("html")).to_have_attribute("data-view", "integracoes")


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
        "bot rodando"
    )


def test_cerebro_offline_degrada_sem_quebrar(abrir):
    # a interface vem do mock, mas o endereço do cérebro configurado aponta para uma porta morta
    page = abrir(
        init="localStorage.setItem('orion_base_url', JSON.stringify('http://127.0.0.1:9'))",
        http_ok=True,
    )
    expect(page.locator("#conn-text")).to_have_text("Sem conexão", timeout=8000)
    expect(page.locator("#sb-convs-list")).to_contain_text("Não foi possível carregar as conversas")
    page.click('.sb-item[data-view="chat"]')
    page.fill("#composer-input", "alguém aí?")
    page.keyboard.press("Enter")
    expect(page.locator("#composer-input")).to_have_value("alguém aí?")
    expect(page.locator("#btn-send")).to_be_disabled()
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
    page.wait_for_timeout(500)  # mede contraste depois da animação de entrada
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
