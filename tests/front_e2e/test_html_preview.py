from playwright.sync_api import expect

from orion.artifacts import Artifacts, Payload

from .conftest import AXE
from .test_capabilities import TOKEN_INIT


def test_static_html_opaque_origin_network_scripts_forms_and_bridge_blocked(
    navegador, novo_backend
):
    url, app, _, paths = novo_backend()
    s = app.state.orion
    session = s.memory.new_session("web")
    attack = f"""<h1>Prévia segura</h1>
    <link rel="dns-prefetch" href="{url}/preview-network-canary-dns">
    <meta http-equiv="refresh" content="1;url={url}/preview-network-canary-refresh">
    <style>body {{ background-image: url('{url}/preview-network-canary-css'); }}</style>
    <img src="{url}/preview-network-canary-image" alt="Bloqueado">
    <iframe src="{url}/preview-network-canary-frame" title="Bloqueado"></iframe>
    <form action="{url}/preview-network-canary-form"><button>Enviar formulário</button></form>
    <a href="{url}/preview-network-canary-nav">Abrir endereço</a>
    <script>window.PWNED=true; fetch('{url}/preview-network-canary-fetch');
    parent.document.body.dataset.pwned='yes'; window.pywebview.api.close_app();</script>"""
    Artifacts(s.memory).save(
        Payload(
            title="HTML de ensaio",
            kind="code",
            language="html",
            content=attack,
            session_id=session.id,
        )
    )
    context = navegador.new_context(viewport={"width": 1440, "height": 900})
    context.add_init_script(TOKEN_INIT)
    context.add_init_script(path=str(AXE))
    page = context.new_page()
    console = []
    page.on("console", lambda msg: console.append(msg.text) if msg.type == "error" else None)
    try:
        page.goto(url + "/ui/?semboot#/resultados")
        page.wait_for_selector("html[data-pronto='true']")
        page.get_by_role("button", name="HTML de ensaio", exact=True).click()
        page.get_by_role("button", name="Prévia HTML", exact=True).click()
        frame = page.frame_locator(".artifact-html-preview")
        expect(frame.get_by_role("heading", name="Prévia segura")).to_be_visible()
        child = page.locator(".artifact-html-preview").element_handle().content_frame()
        assert child.evaluate("() => typeof window.PWNED") == "undefined"
        assert child.evaluate("() => typeof window.pywebview") == "undefined"
        assert child.evaluate("() => document.querySelectorAll('script,link,iframe').length") == 0
        assert (
            child.evaluate(
                "() => { try { return parent.document.body.innerHTML; } catch(e) { return e.name; } }"
            )
            == "SecurityError"
        )
        frame.get_by_role("button", name="Enviar formulário").click()
        assert frame.locator("a").get_attribute("href") is None
        frame.get_by_text("Abrir endereço", exact=True).click()
        page.wait_for_timeout(250)
        assert not any("preview-network-canary" in path for path in paths)
        assert not page.evaluate("document.body.dataset.pwned")
        assert page.locator(".artifact-html-preview").get_attribute("sandbox") == ""
        page.get_by_role("button", name="Ver código", exact=True).click()
        expect(page.locator(".artifact-html-preview")).to_have_count(0)
        expect(page.locator(".artifact-code")).to_contain_text("window.PWNED=true")
        page.set_viewport_size({"width": 700, "height": 780})
        assert not page.evaluate("async () => (await axe.run(document)).violations")
        # Block preview in the desktop bridge context, even on Linux.
        page.evaluate("window.pywebview = {api:{}}")
        page.get_by_role("button", name="Fechar prévia", exact=True).click()
        page.get_by_role("button", name="HTML de ensaio", exact=True).click()
        expect(page.get_by_role("button", name="Prévia HTML", exact=True)).to_be_disabled()
        assert not [
            msg
            for msg in console
            if not any(word in msg.lower() for word in ("security policy", "sandbox", "frame"))
        ]
    finally:
        context.close()
