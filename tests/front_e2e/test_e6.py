"""E6 no navegador, contra o Orion de verdade: painel lateral, comparar versões, limpar em lote,
"Usou: …" na resposta e a pesquisa noturna pela interface."""

import json
import struct
import zlib

from playwright.sync_api import expect

from orion.agent import AgentEvent
from orion.capture import Capturer
from orion.research import NightResearch, assuntos_salvos
from tests.fakes import FakeGateway, chama, fala, pede

from .test_capabilities import TOKEN_INIT


def _png(cor: int) -> bytes:
    """PNG 2x2 válido, de uma cor só."""

    def bloco(tipo: bytes, dado: bytes) -> bytes:
        return (
            struct.pack(">I", len(dado))
            + tipo
            + dado
            + struct.pack(">I", zlib.crc32(tipo + dado) & 0xFFFFFFFF)
        )

    linha = b"\x00" + bytes([cor, cor, cor]) * 2
    return (
        b"\x89PNG\r\n\x1a\n"
        + bloco(b"IHDR", struct.pack(">IIBBBBB", 2, 2, 8, 2, 0, 0, 0))
        + bloco(b"IDAT", zlib.compress(linha * 2))
        + bloco(b"IEND", b"")
    )


def _gerar(app, tmp_path, nome, conteudo, sessao=None):
    origem = tmp_path / "gerados" / nome
    origem.parent.mkdir(exist_ok=True)
    origem.write_bytes(conteudo.encode() if isinstance(conteudo, str) else conteudo)
    r = json.dumps({"ok": True, "path": str(origem), "tipo": origem.suffix[1:]})
    return app.state.orion.biblioteca.registrar(sessao, "gerar_documento", r)


def _axe_limpo(page):
    page.wait_for_timeout(300)
    assert page.evaluate("async () => (await axe.run(document)).violations.map(v => v.id)") == []


# ── E6.1 e E6.2: painel lateral e comparação ────────────────────────────────
def test_painel_abre_ao_lado_mostra_texto_e_compara_versoes(abrir, novo_backend, tmp_path):
    url, app, _, _ = novo_backend()
    _gerar(app, tmp_path, "ata.md", "# Ata\nlinha 1\nlinha 2\n")
    _gerar(app, tmp_path, "ata.md", "# Ata\nlinha 1\nlinha DOIS\nlinha 3\n")
    page = abrir("#/conhecimento", url=url, init=TOKEN_INIT, axe=True)
    page.get_by_role("button", name="Ver ata.md versão 2").click()
    painel = page.locator("#res-painel")
    expect(painel).to_be_visible()
    expect(painel.get_by_role("heading", name="ata.md")).to_be_visible()
    expect(painel.locator(".res-texto")).to_contain_text("linha DOIS")
    expect(page.locator("html")).to_have_attribute("data-res-aberto", "true")
    painel.get_by_label("Comparar com a versão").select_option(
        label=painel.locator("option").nth(1).inner_text()
    )
    resumo = painel.locator(".res-diff-resumo")
    expect(resumo).to_contain_text("v1 → v2: +2 −1")
    expect(painel.locator('[data-diff="add"]').filter(has_text="linha DOIS")).to_have_count(1)
    expect(painel.locator('[data-diff="del"]').filter(has_text="linha 2")).to_have_count(1)
    _axe_limpo(page)
    painel.get_by_role("button", name="Abrir a versão 1 de ata.md").click()
    expect(painel.locator(".res-texto")).to_contain_text("linha 2")
    page.keyboard.press("Escape")
    expect(painel).to_be_hidden()
    expect(page.locator("html")).to_have_attribute("data-res-aberto", "false")


def test_painel_mostra_imagem_pdf_e_compara_imagens_lado_a_lado(abrir, novo_backend, tmp_path):
    url, app, _, _ = novo_backend()
    _gerar(app, tmp_path, "logo.png", _png(10))
    _gerar(app, tmp_path, "logo.png", _png(200))
    _gerar(app, tmp_path, "contrato.pdf", b"%PDF-1.4\n1 0 obj<<>>endobj\ntrailer<<>>\n%%EOF")
    _gerar(app, tmp_path, "dados.bin", b"\x00\x01")
    page = abrir("#/conhecimento", url=url, init=TOKEN_INIT, http_ok=True)
    painel = page.locator("#res-painel")
    page.get_by_role("button", name="Ver logo.png versão 2").click()
    imagem = painel.locator(".res-previa img")
    expect(imagem).to_have_attribute("src", __import__("re").compile(r"^blob:"))
    painel.get_by_label("Comparar com a versão").select_option(index=1)
    expect(painel.locator('[data-comparacao="imagens"] img')).to_have_count(2)
    page.get_by_role("button", name="Ver contrato.pdf versão 1").click()
    quadro = painel.locator(".res-previa iframe")
    expect(quadro).to_have_attribute("src", __import__("re").compile(r"^blob:"))
    expect(quadro).to_have_attribute("title", "PDF contrato.pdf")
    expect(painel.get_by_label("Comparar com a versão")).to_have_count(0)  # versão única
    assert page.get_by_role("button", name="Ver dados.bin versão 1").count() == 0  # sem prévia


def test_apagar_pelo_painel_fecha_e_atualiza_a_lista(abrir, novo_backend, tmp_path):
    url, app, _, _ = novo_backend()
    _gerar(app, tmp_path, "a.md", "um")
    _gerar(app, tmp_path, "b.md", "dois")
    page = abrir("#/conhecimento", url=url, init=TOKEN_INIT)
    page.get_by_role("button", name="Ver a.md versão 1").click()
    page.locator("#res-painel").get_by_role("button", name="Apagar a.md versão 1 do painel").click()
    page.get_by_role("alertdialog").get_by_role("button", name="Apagar", exact=True).click()
    expect(page.locator("#res-painel")).to_be_hidden()
    expect(page.locator("[data-resultado]")).to_have_count(1)
    assert [a["name"] for a in app.state.orion.memory.list_artifacts()] == ["b.md"]


# ── E6.3: limpar em lote ────────────────────────────────────────────────────
def test_limpar_versoes_antigas_mostra_o_total_antes_de_apagar(abrir, novo_backend, tmp_path):
    url, app, _, _ = novo_backend()
    for i in range(4):
        _gerar(app, tmp_path, "plano.md", f"versao {i}")
    _gerar(app, tmp_path, "outro.md", "so uma")
    page = abrir("#/conhecimento", url=url, init=TOKEN_INIT, axe=True)
    expect(page.locator("[data-resultado]")).to_have_count(5)
    page.get_by_role("button", name="Limpar versões antigas").click()
    pergunta = page.get_by_role("dialog", name="Limpar versões antigas")
    pergunta.get_by_label("Quantas versões manter", exact=False).fill("2")
    pergunta.get_by_role("button", name="Ver o que sai").click()
    alerta = page.get_by_role("alertdialog", name="Apagar 2 versão(ões) antiga(s)?")
    expect(alerta).to_contain_text("Ficam as 2 mais novas de cada arquivo")
    alerta.get_by_role("button", name="Cancelar").click()
    assert len(app.state.orion.memory.list_artifacts()) == 5  # cancelar não apaga
    page.get_by_role("button", name="Limpar versões antigas").click()
    page.get_by_label("Quantas versões manter", exact=False).fill("2")
    page.get_by_role("button", name="Ver o que sai").click()
    page.get_by_role("alertdialog").get_by_role("button", name="Apagar", exact=True).click()
    expect(page.locator("[data-resultado]")).to_have_count(3)
    versoes = sorted(
        a["version"] for a in app.state.orion.memory.list_artifacts() if a["name"] == "plano.md"
    )
    assert versoes == [3, 4]
    _axe_limpo(page)


def test_limpar_sem_nada_a_apagar_avisa(abrir, novo_backend, tmp_path):
    url, app, _, _ = novo_backend()
    _gerar(app, tmp_path, "unico.md", "x")
    page = abrir("#/conhecimento", url=url, init=TOKEN_INIT)
    page.get_by_role("button", name="Limpar versões antigas").click()
    page.get_by_label("Quantas versões manter", exact=False).fill("3")
    page.get_by_role("button", name="Ver o que sai").click()
    expect(page.locator("#toasts")).to_contain_text("Nada a limpar")
    assert len(app.state.orion.memory.list_artifacts()) == 1


# ── E6.4: "Usou: …" ─────────────────────────────────────────────────────────
def test_resposta_mostra_o_que_usou_com_origem(abrir, novo_backend):
    gw = FakeGateway(pede(chama("listar_fatos")), fala("Você não tem fatos ainda."))
    url, _, _, _ = novo_backend(gateway_override=gw)
    page = abrir("#/chat", url=url, init=TOKEN_INIT, axe=True)
    page.fill("#composer-input", "o que você sabe de mim?")
    page.keyboard.press("Enter")
    expect(page.locator(".msg-orion").last).to_contain_text("Você não tem fatos ainda.")
    rodape = page.locator(".usou-rodape").last
    expect(rodape.locator("summary")).to_have_text("Usou: listar fatos")
    rodape.locator("summary").click()
    expect(rodape).to_contain_text("listar fatos · Orion")
    _axe_limpo(page)
    # recarregando, o histórico traz o mesmo rodapé (a proveniência está no banco)
    page.reload()
    page.wait_for_selector("html[data-pronto='true']")
    expect(page.locator(".usou-rodape summary").last).to_have_text("Usou: listar fatos")


def test_historico_mostra_links_das_fontes_e_cartao_do_arquivo_gerado(
    abrir, novo_backend, tmp_path
):
    url, app, _, _ = novo_backend()
    m = app.state.orion.memory
    sessao = m.active_session("web")
    arq = _gerar(app, tmp_path, "resumo.md", "# Resumo\nconteúdo", sessao=sessao.id)
    m.add_message(sessao.id, "user", "pesquise e gere um resumo")
    m.add_message(
        sessao.id,
        "assistant",
        "Pronto, gerei o resumo.",
        provenance={
            "usou": [
                {"nome": "buscar_url", "origem": "mcp:docs", "estado": "completed"},
                {"nome": "gerar_documento", "origem": "nativa", "estado": "completed"},
            ],
            "fontes": ["https://exemplo.com/a", "javascript:alert(1)"],
            "resultados": [{"id": arq["id"], "nome": "resumo.md", "versao": 1}],
        },
    )
    page = abrir("#/chat", url=url, init=TOKEN_INIT, axe=True)
    rodape = page.locator(".usou-rodape").last
    expect(rodape.locator("summary")).to_have_text("Usou: buscar url, gerar documento")
    rodape.locator("summary").click()
    expect(rodape).to_contain_text("buscar url · MCP docs")
    links = rodape.locator("a.usou-link")
    expect(links).to_have_count(1)  # javascript: nunca vira link
    expect(links.first).to_have_attribute("href", "https://exemplo.com/a")
    page.get_by_role("button", name="Abrir o arquivo gerado resumo.md versão 1").click()
    painel = page.locator("#res-painel")
    expect(painel).to_be_visible()
    expect(painel.locator(".res-texto")).to_contain_text("conteúdo")
    _axe_limpo(page)


# ── E6.5: pesquisa noturna pela interface ───────────────────────────────────
def _ligar_pesquisa(app, tmp_path, at=""):
    estado = app.state.orion
    vault = tmp_path / "vault"
    vault.mkdir(exist_ok=True)
    chamadas = []

    async def run_turn(canal, texto):
        chamadas.append(texto)
        yield AgentEvent("text", {"text": "achei novidades"})
        yield AgentEvent("done", {})

    estado.pesquisa = NightResearch(
        memory=estado.memory,
        run_turn=run_turn,
        capturer=Capturer(vault, "00 Inbox"),
        assuntos=lambda: assuntos_salvos(estado.memory, estado.settings.research_topics)[0],
        at=at,
        clock=__import__("time").time,
    )
    return chamadas


def test_pesquisa_edita_assuntos_roda_agora_e_lista_o_relatorio(abrir, novo_backend, tmp_path):
    url, app, _, _ = novo_backend()
    chamadas = _ligar_pesquisa(app, tmp_path)
    page = abrir("#/conhecimento", url=url, init=TOKEN_INIT, axe=True)
    cartao = page.locator('[data-id="pesquisa"]')
    expect(cartao).to_contain_text("Sem hora marcada")
    expect(cartao).to_contain_text("usando os do .env")
    assert cartao.get_by_label("Assunto 1", exact=True).input_value().startswith("MCP")
    for _ in range(3):  # tira os três assuntos do padrão
        cartao.get_by_role("button", name="Remover o assunto 1").click()
    expect(cartao).to_contain_text("Nenhum assunto")
    expect(cartao.get_by_role("button", name="Rodar agora")).to_be_disabled()
    cartao.get_by_role("button", name="Adicionar assunto").click()
    cartao.get_by_label("Assunto 1", exact=True).fill("modelos locais em notebook")
    expect(cartao).to_contain_text("Salve os assuntos para poder rodar agora")
    cartao.get_by_role("button", name="Salvar assuntos").click()
    assert json.loads(app.state.orion.memory.meta_get("research:topics")) == [
        "modelos locais em notebook"
    ]
    expect(cartao).not_to_contain_text("usando os do .env")
    cartao.get_by_role("button", name="Rodar agora").click()
    relatorio = cartao.locator("[data-relatorio]")
    expect(relatorio).to_have_count(1, timeout=15000)
    expect(relatorio).to_contain_text("00 Inbox/")
    assert "modelos locais em notebook" in chamadas[0]
    expect(cartao.get_by_role("button", name="Rodar agora")).to_be_enabled()
    _axe_limpo(page)


def test_pesquisa_indisponivel_explica_o_que_falta(abrir, novo_backend):
    url, _, _, _ = novo_backend()
    page = abrir("#/conhecimento", url=url, init=TOKEN_INIT)
    cartao = page.locator('[data-id="pesquisa"]')
    expect(cartao).to_contain_text("Indisponível neste computador")
    expect(cartao.get_by_role("button", name="Rodar agora")).to_be_disabled()
    cartao.get_by_role("button", name="Remover o assunto 3").click()
    cartao.get_by_role("button", name="Salvar assuntos").click()  # editar funciona sem rodar
    expect(cartao).to_contain_text("2 assunto(s)")
