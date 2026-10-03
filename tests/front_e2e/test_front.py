"""Front do Orion no navegador de verdade: fluxos, teclado, segurança, mobile e acessibilidade."""

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
    expect(page.locator("#sb-convs-list")).to_contain_text("offline")
    enviar(page, "alguém aí?")
    expect(page.locator(".msg-error")).to_contain_text("A conexão com o cérebro caiu")
    page.click('.sb-item[data-view="memoria"]')
    expect(page.locator("#mem-banner")).to_contain_text("demonstração", timeout=15000)


# ── movimento e mobile ──────────────────────────────────────────────────────────
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


@pytest.mark.parametrize("largura", [320, 390])
@pytest.mark.parametrize("rota", ROTAS)
def test_mobile_sem_rolagem_horizontal(abrir, largura, rota):
    page = abrir(rota, viewport=(largura, 800), mobile=True)
    page.wait_for_timeout(700)
    assert page.evaluate(
        "document.documentElement.scrollWidth <= document.documentElement.clientWidth"
    ), f"rolagem horizontal em {rota or 'home'} a {largura}px"
    assert page.evaluate("document.body.scrollWidth <= document.documentElement.clientWidth")


def test_mobile_gaveta_abre_fecha_e_prende_o_foco(abrir):
    page = abrir(viewport=(390, 800), mobile=True)
    assert page.evaluate("document.querySelector('#sidebar').inert") is True, (
        "gaveta fechada tem de ser inert"
    )
    page.click("#menu-btn")
    expect(page.locator("html")).to_have_attribute("data-drawer", "open")
    assert page.evaluate("document.querySelector('#sidebar').inert") is False
    expect(page.locator("#sb-new")).to_be_focused()
    for _ in range(14):
        page.keyboard.press("Tab")
    assert page.evaluate("document.querySelector('#sidebar').contains(document.activeElement)"), (
        "Tab escapou da gaveta"
    )
    page.keyboard.press("Escape")
    expect(page.locator("html")).not_to_have_attribute("data-drawer", "open")
    expect(page.locator("#menu-btn")).to_be_focused()
    page.click("#menu-btn")
    page.click('.sb-item[data-view="config"]')  # navegar fecha a gaveta
    expect(page.locator("html")).to_have_attribute("data-view", "config")
    expect(page.locator("html")).not_to_have_attribute("data-drawer", "open")


def test_mobile_alvos_de_toque_com_44px(abrir):
    page = abrir("#/chat", viewport=(390, 800), mobile=True)
    page.fill("#composer-input", "oi")
    page.click("#btn-send")  # no celular o Enter quebra a linha
    esperar_fim(page)
    pequenos = page.evaluate("""() => {
        const sels = ['#menu-btn', '#palette-btn', '#btn-send', '#btn-attach', '#model-btn', '.msg-actions .icon-btn', '.voice-live-btn'];
        const out = [];
        for (const s of sels) for (const e of document.querySelectorAll(s)) {
            const r = e.getBoundingClientRect();
            if (r.width === 0 || r.height === 0) continue;
            if (Math.min(r.width, r.height) < 43.5) out.push(`${s}: ${Math.round(r.width)}x${Math.round(r.height)}`);
        }
        return out; }""")
    assert not pequenos, f"alvos de toque abaixo de 44px: {pequenos}"


def test_mobile_composer_enter_quebra_linha(abrir):
    page = abrir("#/chat", viewport=(390, 800), mobile=True)
    page.fill("#composer-input", "primeira")
    page.keyboard.press("Enter")
    page.keyboard.type("segunda")
    assert page.locator("#composer-input").input_value() == "primeira\nsegunda"
    assert page.locator(".msg-user").count() == 0


# ── acessibilidade (axe-core) ───────────────────────────────────────────────────
pytestmark_axe = pytest.mark.skipif(
    not AXE.exists(), reason="rode `npm ci` em tests/front_e2e para instalar o axe-core"
)


def _violacoes(page):
    res = page.evaluate("""() => axe.run(document, { runOnly: { type: 'tag',
        values: ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa', 'best-practice'] } })""")
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
def test_axe_mobile_gaveta_aberta(abrir):
    page = abrir(viewport=(390, 800), mobile=True, axe=True)
    page.click("#menu-btn")
    page.wait_for_timeout(400)
    assert _violacoes(page) == []


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
