"""E8 no navegador, contra o Orion de verdade: catálogo, importação do Claude Code e a prova do
Orion Pesquisa (instalar pelo catálogo, ativar, atualizar, voltar)."""

from playwright.sync_api import expect

from orion.extensions.catalogo import escrever_indice
from tests.extensions.test_e8 import _pesquisa_zip, claude_zip

from .test_capabilities import TOKEN_INIT


def _axe_limpo(page):
    page.wait_for_timeout(300)
    assert page.evaluate("async () => (await axe.run(document)).violations.map(v => v.id)") == []


def _dados_com_catalogo(tmp_path, *versoes):
    pasta = tmp_path / "dados"
    escrever_indice(
        pasta / "catalogo", [(f"orion-pesquisa-{v}.zip", _pesquisa_zip(v)) for v in versoes]
    )
    return pasta


def test_catalogo_instala_inativo_e_so_avisa_da_versao_nova(abrir, novo_backend, tmp_path):
    dados = _dados_com_catalogo(tmp_path, "1.0.0")
    url, app, _, _ = novo_backend(data_dir=dados)
    page = abrir("#/integracoes", url=url, init=TOKEN_INIT, axe=True)
    page.get_by_role("tab", name="Catálogo").click()
    card = page.locator('[data-catalogo="orion-pesquisa"]')
    expect(card).to_contain_text("Catálogo local")
    expect(card).to_contain_text("v1.0.0")
    card.get_by_role("button", name="Instalar").click()
    expect(card).to_contain_text("Instalado")
    assert [(r["id"], r["state"]) for r in app.state.orion.extensions.store.list()] == [
        ("orion-pesquisa", "disabled")
    ]
    assert app.state.orion.skills.enabled == set()  # instalar não ativa nada
    _axe_limpo(page)
    # a versão nova entra no catálogo: aparece como aviso e só instala se você mandar
    escrever_indice(dados / "catalogo", [("orion-pesquisa-1.1.0.zip", _pesquisa_zip("1.1.0"))])
    page.get_by_role("tab", name="Plugins").click()
    page.get_by_role("tab", name="Catálogo").click()
    nova = page.locator('[data-catalogo="orion-pesquisa"][data-versao="1.1.0"]')
    expect(nova).to_contain_text("Atualização")
    expect(nova).to_contain_text("instalado: v1.0.0")
    nova.get_by_role("button", name="Atualizar para v1.1.0").click()
    page.get_by_role("alertdialog").get_by_role("button", name="Baixar e instalar").click()
    page.get_by_role("tab", name="Plugins").click()
    versoes = {r["version"] for r in app.state.orion.extensions.store.versions("orion-pesquisa")}
    assert versoes == {"1.0.0", "1.1.0"}
    _axe_limpo(page)


def test_catalogo_vazio_explica_onde_colocar(abrir, novo_backend):
    url, _, _, _ = novo_backend()
    page = abrir("#/integracoes", url=url, init=TOKEN_INIT)
    page.get_by_role("tab", name="Catálogo").click()
    expect(page.get_by_text("Nenhum pacote no catálogo")).to_be_visible()
    expect(page.get_by_text("ORION_PLUGIN_CATALOGS")).to_be_visible()


def test_importar_do_claude_code_mostra_o_que_fica_de_fora_antes_de_instalar(
    abrir, novo_backend, tmp_path
):
    url, app, _, _ = novo_backend()
    zip_path = tmp_path / "plugin-claude.zip"
    zip_path.write_bytes(claude_zip())
    page = abrir("#/integracoes", url=url, init=TOKEN_INIT, axe=True)
    with page.expect_file_chooser() as escolha:
        page.get_by_role("button", name="Importar do Claude Code ou Codex").click()
    escolha.value.set_files(str(zip_path))
    dialogo = page.get_by_role("dialog", name="Importar do Claude Code ou Codex")
    expect(dialogo).to_contain_text("Formato Claude Code · Meu Plugin! v2.1.0")
    expect(dialogo).to_contain_text("Skills que viram parte do plugin: revisar")
    ignorados = dialogo.get_by_role("list", name="O que não foi importado")
    expect(ignorados).to_contain_text("servidores MCP (docs, github)")
    expect(ignorados).to_contain_text("hooks/")
    assert app.state.orion.extensions.store.list() == []  # a prévia não instalou
    _axe_limpo(page)
    dialogo.get_by_role("button", name="Instalar sem ativar").click()
    card = page.locator('[data-plugin="meu-plugin"]')
    expect(card).to_be_visible()
    expect(card).to_contain_text("Pacote importado")
    assert [r["state"] for r in app.state.orion.extensions.store.list()] == ["disabled"]


def test_importar_pacote_que_nao_serve_explica_o_motivo(abrir, novo_backend, tmp_path):
    url, _, _, _ = novo_backend()
    zip_path = tmp_path / "vazio.zip"
    from tests.extensions.test_e8 import zip_de

    zip_path.write_bytes(zip_de({"README.md": "nada"}))
    page = abrir("#/integracoes", url=url, init=TOKEN_INIT, http_ok=True)
    with page.expect_file_chooser() as escolha:
        page.get_by_role("button", name="Importar do Claude Code ou Codex").click()
    escolha.value.set_files(str(zip_path))
    expect(page.locator("#toasts")).to_contain_text("Não consegui ler o pacote")
