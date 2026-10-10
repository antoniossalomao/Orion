import copy
import json
from pathlib import Path
from zipfile import ZipFile

from playwright.sync_api import expect

from tests.fakes import FakeGateway, chama, fala, pede

from .test_capabilities import TOKEN_INIT


class ResearchGateway(FakeGateway):
    """Roteia as duas chamadas reais da fixture; modelo/provedor ficam simulados."""

    async def stream(self, messages, tools=None):
        self.chamadas.append(copy.deepcopy(messages))
        self.ferramentas.append(copy.deepcopy(tools))
        responses = [message for message in messages if message["role"] == "tool"]
        if not responses:
            name = next(
                tool["function"]["name"]
                for tool in tools
                if "/pesquisar_fixture]" in tool["function"]["description"]
            )
            script = pede(chama(name, consulta="assunto de ensaio"))
        elif len(responses) == 1:
            name = next(
                tool["function"]["name"]
                for tool in tools
                if "/ler_fixture]" in tool["function"]["description"]
            )
            script = pede(chama(name, url="https://example.com/a"))
        else:
            script = fala(
                "Pesquisa concluída com fontes controladas. [Fonte A](https://example.com/a)."
            )
        for event in script:
            yield event


def zip_fixture(tmp_path, version):
    path = tmp_path / f"pesquisa-{version}.zip"
    manifest = {
        "id": "pesquisa-demo",
        "name": "Pesquisa de ensaio",
        "version": version,
        "description": "Pesquisar fixture e conferir fontes via MCP.",
        "license": "MIT",
        "skills": ["skills/pesquisar"],
        "mcp": [
            {
                "id": "fixture",
                "transport": "stdio",
                "entrypoint": "server.py",
                "tools": {"pesquisar_fixture": "read", "ler_fixture": "read"},
            }
        ],
        "capabilities": ["mcp:fixture:pesquisar_fixture", "mcp:fixture:ler_fixture"],
    }
    with ZipFile(path, "w") as archive:
        archive.writestr("manifest.json", json.dumps(manifest))
        archive.writestr(
            "skills/pesquisar/SKILL.md",
            "---\nname: pesquisar\ndescription: Pesquisar fixture e conferir evidências\nmetadata:\n  version: '"
            + version
            + "'\n---\nPesquise e leia a fonte. Não invente evidência.",
        )
        archive.writestr("server.py", Path("tests/extensions/mcp_research_server.py").read_bytes())
    return path


def import_package(page, path):
    with page.expect_file_chooser() as chooser:
        page.get_by_role("button", name="Importar pacote ZIP").click()
    chooser.value.set_files(str(path))


def test_complete_real_backend_mcp_ui_version_and_restart(abrir, novo_backend, tmp_path):
    gateway = ResearchGateway()
    url, app, _, _ = novo_backend(gateway_override=gateway)
    page = abrir("#/integracoes", url=url, init=TOKEN_INIT)
    expect(page.get_by_role("heading", name="Seu Orion pode ir além")).to_be_visible()
    import_package(page, zip_fixture(tmp_path, "1.0.0"))
    card = page.locator('[data-plugin="pesquisa-demo"]')
    expect(card).to_contain_text("Desativado")
    card.get_by_role("button", name="Ativar", exact=True).click()
    modal = page.get_by_role("dialog")
    modal.get_by_role("checkbox", name="mcp:fixture:pesquisar_fixture").check()
    modal.get_by_role("checkbox", name="mcp:fixture:ler_fixture").check()
    modal.get_by_text("Permissões do servidor fixture", exact=True).click()
    for name in ["pesquisar_fixture", "ler_fixture"]:
        modal.get_by_role("combobox", name=f"Permissão para fixture/{name}").select_option("read")
    modal.get_by_role(
        "checkbox",
        name="Confio no código local deste pacote e autorizo sua execução no computador.",
    ).check()
    modal.get_by_role("button", name="Ativar", exact=True).click()
    expect(card).to_contain_text("Ativo")
    page.get_by_role("tab", name="Skills", exact=True).click()
    page.get_by_role("button", name="Usar no chat").click()
    expect(page.locator("#composer-input")).to_have_value("/pesquisa-demo:pesquisar ")
    page.locator("#composer-input").fill("/pesquisa-demo:pesquisar Pesquise as fontes")
    page.locator("#btn-send").click()
    expect(page.locator(".msg-orion").last).to_contain_text(
        "Pesquisa concluída com fontes controladas"
    )
    expect(page.locator(".msg-orion .history-sources").last).to_contain_text(
        "Pesquisa de ensaio/fixture"
    )
    assert len(gateway.chamadas) == 3
    assert "Evidência controlada" in gateway.chamadas[-1][-1]["content"]
    page.locator('.sb-item[data-view="integracoes"]').click()
    page.get_by_role("tab", name="Plugins", exact=True).click()
    card.get_by_role("button", name="Desativar").click()
    expect(card).to_contain_text("Desativado")
    assert not app.state.orion.mcp_host.connections
    import_package(page, zip_fixture(tmp_path, "2.0.0"))
    # Update fica preparado; selecionar/rollback é revisão pela interface.
    for version in ["2.0.0", "1.0.0"]:
        card.get_by_role("button", name="Versões").click()
        modal = page.get_by_role("dialog")
        modal.get_by_role("combobox", name="Versão do plugin").select_option(label=version)
        modal.get_by_role("button", name="Revisar versão").click()
        page.get_by_role("dialog").get_by_role("button", name="Usar esta versão").click()
        expect(card).to_contain_text(f"v{version}")
        expect(card).to_contain_text("Desativado")
    directory = app.state.orion.settings.data_dir
    app.state.e2e_server.should_exit = True
    app.state.e2e_thread.join(timeout=10)
    assert not app.state.e2e_thread.is_alive()
    new_url, _, _, _ = novo_backend(gateway_override=ResearchGateway(), data_dir=directory)
    page.goto(new_url + "/ui/?semboot#/chat")
    expect(page.locator(".msg-orion").last).to_contain_text(
        "Pesquisa concluída com fontes controladas"
    )
    expect(page.locator(".msg-orion .history-sources").last).to_contain_text(
        "Pesquisa de ensaio/fixture"
    )
