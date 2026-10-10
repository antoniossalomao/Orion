"""A janela pequena da ponte (E2.3): captura rápida contra o Orion real, com o token da ponte."""

from playwright.sync_api import expect

from tests.fakes import FakeGateway, fala


def _pagina_da_ponte(abrir, url, token, modo="rapido", **kw):
    return abrir(url=url, pagina=f"/ui/ponte.html?modo={modo}#t={token}", axe=True, **kw)


def test_captura_rapida_guarda_tarefa_nota_e_desfaz(abrir, novo_backend, tmp_path):
    vault = tmp_path / "vault"
    vault.mkdir()
    gw = FakeGateway(
        fala('{"tipo": "tarefa", "titulo": "mandar o relatório"}'),
        fala('{"tipo": "nota", "titulo": "ideia", "quando": null}'),
    )
    url, app, _, caminhos = novo_backend(gateway_override=gw, vault_dir=vault)
    s = app.state.orion
    token = s.auth.create_device_token("ponte", "teste")
    page = _pagina_da_ponte(abrir, url, token)
    campo = page.get_by_label("Anotar")
    expect(campo).to_be_focused()
    # o token saiu da barra de endereços (fica só em memória) e nunca foi ao servidor na URL
    assert "t=" not in page.url
    assert not any(token in c for c in caminhos)

    campo.fill("preciso mandar o relatório")
    campo.press("Enter")
    expect(page.locator("#resultado-texto")).to_have_text("Tarefa: mandar o relatório")
    assert [t["title"] for t in s.ops.list_tasks()] == ["mandar o relatório"]
    page.get_by_role("button", name="Desfazer").click()
    expect(page.locator("#resultado-texto")).to_have_text("Desfeito.")
    assert s.ops.list_tasks() == []

    campo.fill("ideia: app que organiza o vault por tema")
    campo.press("Enter")
    expect(page.locator("#resultado-texto")).to_contain_text("Nota:")
    notas = list((vault / "00 Inbox").glob("*.md"))
    assert len(notas) == 1 and "organiza o vault" in notas[0].read_text()
    page.get_by_role("button", name="Desfazer").click()
    expect(page.locator("#resultado-texto")).to_have_text("Desfeito.")
    assert list((vault / "00 Inbox").glob("*.md")) == []
    page.wait_for_timeout(300)
    assert page.evaluate("async () => (await axe.run(document)).violations.map(v => v.id)") == []


def test_captura_sem_modelo_usa_a_regra_e_cria_lembrete(abrir, novo_backend):
    url, app, _, _ = novo_backend(gateway=False)
    s = app.state.orion
    token = s.auth.create_device_token("ponte", "teste")
    page = _pagina_da_ponte(abrir, url, token, http_ok=True)  # o 409 do gasto sem vault é esperado
    campo = page.get_by_label("Anotar")
    campo.fill("lembra de ligar pro dentista amanhã às 15h30")
    campo.press("Enter")
    expect(page.locator("#resultado-texto")).to_contain_text(
        "Lembrete: lembra de ligar pro dentista"
    )
    expect(page.locator("#resultado-texto")).to_contain_text("às 15:30")
    [lembrete] = s.ops.list_reminders()
    assert "dentista" in lembrete["title"]
    campo.fill("gastei R$ 42,50 no almoço")
    campo.press("Enter")
    # gasto só existe depois da E10.3: cai em nota, e sem vault configurado diz por quê
    expect(page.locator("#erro")).to_contain_text("ORION_VAULT_DIR")


def test_sem_token_ou_com_token_errado_a_pagina_nao_guarda(abrir, novo_backend):
    url, app, _, _ = novo_backend()
    sem = abrir(url=url, pagina="/ui/ponte.html?modo=rapido")
    expect(sem.locator("#erro")).to_contain_text("parear")
    errado = abrir(
        url=url,
        pagina="/ui/ponte.html?modo=rapido#t=token-que-nao-existe-123456",
        http_ok=True,
    )
    errado.get_by_label("Anotar").fill("x")
    errado.get_by_label("Anotar").press("Enter")
    expect(errado.locator("#erro")).to_contain_text("parear")
    assert app.state.orion.ops.list_tasks() == []
