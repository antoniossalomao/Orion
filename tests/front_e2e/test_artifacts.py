import base64
import io
import tempfile
from pathlib import Path

from PIL import Image
from playwright.sync_api import expect

from orion.artifacts import Artifacts, Payload

from .test_capabilities import TOKEN_INIT


def test_save_response_versions_safe_preview_download_scope_and_narrow(abrir, novo_backend):
    url, app, _, _ = novo_backend()
    page = abrir("#/chat", url=url, init=TOKEN_INIT, axe=True)
    page.locator("#composer-input").fill("Guarde uma análise")
    page.locator("#btn-send").click()
    save = page.get_by_role("button", name="Salvar resultado", exact=True)
    expect(save).to_be_visible()
    save.click()
    dialog = page.get_by_role("dialog")
    dialog.get_by_role("textbox", name="Título do resultado").fill("Análise persistida")
    dialog.get_by_role("button", name="Salvar resultado", exact=True).click()
    expect(page.locator("#artifact-preview")).to_contain_text("Resposta do backend novo.")
    expect(page.locator("#artifact-preview")).to_contain_text("Mensagem")
    artifact_id = page.locator("[data-artifact]").first.get_attribute("data-artifact")
    page.get_by_role("button", name="Criar nova versão", exact=True).click()
    dialog = page.get_by_role("dialog")
    dialog.get_by_role("textbox", name="Conteúdo do resultado").fill(
        "# Revisão\n![Exfil](https://invalid.example/leak)\n<script>window.artifactInjected=true</script>"
    )
    dialog.get_by_role("button", name="Salvar nova versão", exact=True).click()
    expect(page.locator("#artifact-preview")).to_contain_text("imagem externa bloqueada")
    assert not page.locator("#artifact-preview img").count()
    assert not page.evaluate("Boolean(window.artifactInjected)")
    versions = page.get_by_role("combobox", name="Versão do resultado")
    expect(versions).to_have_value("2")
    versions.select_option("1")
    expect(page.locator("#artifact-preview")).to_contain_text("Resposta do backend novo.")
    with page.expect_download() as downloaded:
        page.get_by_role("button", name="Baixar", exact=True).click()
    assert downloaded.value.suggested_filename == "Análise persistida.md"
    page.get_by_role("link", name="Chat", exact=True).click()
    page.get_by_role("link", name="Resultados", exact=True).click()
    expect(page.get_by_role("combobox", name="Versão do resultado")).to_have_value("1")
    page.get_by_role("button", name="Fechar prévia", exact=True).click()
    expect(page.get_by_role("button", name="Análise persistida", exact=True)).to_be_focused()
    page.locator("#artifact-search").fill("Análise")
    expect(page.locator("[data-artifact]")).to_have_count(1)
    page.reload()
    expect(page.get_by_role("button", name="Análise persistida", exact=True)).to_be_visible()
    page.get_by_role("button", name="Análise persistida", exact=True).click()
    expect(page.get_by_role("combobox", name="Versão do resultado")).to_have_value("2")
    page.set_viewport_size({"width": 700, "height": 780})
    page.wait_for_function(
        '() => getComputedStyle(document.querySelector("#view-resultados")).opacity === "1"'
    )
    assert page.evaluate("document.documentElement.scrollWidth <= innerWidth")
    assert not page.evaluate("async () => (await axe.run(document)).violations")
    page.screenshot(
        path=str(Path(tempfile.gettempdir()) / "orion-c35-library-700.png"), full_page=True
    )
    result = Artifacts(app.state.orion.memory).read(artifact_id, None)[1]
    assert result["message_id"] is not None and result["provenance"]


def test_authenticated_local_image_preview_no_remote_image_requests(abrir, novo_backend):
    url, app, _, _ = novo_backend()
    memory = app.state.orion.memory
    session = memory.new_session("web")
    data = io.BytesIO()
    Image.new("RGB", (4, 3), "navy").save(data, format="PNG")
    Artifacts(memory).save(
        Payload(
            title="Imagem local",
            kind="image",
            session_id=session.id,
            content=base64.b64encode(data.getvalue()).decode(),
        )
    )
    page = abrir("#/resultados", url=url, init=TOKEN_INIT)
    page.get_by_role("button", name="Imagem local", exact=True).click()
    image = page.get_by_role("img", name="Imagem local", exact=True)
    expect(image).to_be_visible()
    assert image.get_attribute("src").startswith("blob:")
    page.get_by_role("button", name="Fechar prévia", exact=True).click()
    expect(page.locator("#artifact-preview img")).to_have_count(0)
