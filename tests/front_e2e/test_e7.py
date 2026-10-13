"""E7 no navegador, contra o Orion de verdade: servidores MCP em Integrações, `/prompt`, `/skill`
e a paleta, com servidores MCP de verdade (JSON-RPC por stdio)."""

import json
import sys
from pathlib import Path

from playwright.sync_api import expect

from tests.fakes import FakeGateway, fala

from .test_capabilities import TOKEN_INIT

CRU = Path(__file__).resolve().parents[1] / "mcp_cliente" / "servidor_cru.py"


def _dados(tmp_path, servidores, skills=None):
    pasta = tmp_path / "dados"
    pasta.mkdir()
    (pasta / "mcp.json").write_text(json.dumps({"servers": servidores}), encoding="utf-8")
    for nome, corpo in (skills or {}).items():
        (pasta / "skills" / nome).mkdir(parents=True)
        (pasta / "skills" / nome / "SKILL.md").write_text(
            f"---\nname: {nome}\ndescription: Skill {nome}.\n---\n\n{corpo}\n", encoding="utf-8"
        )
    return pasta


def _cru(**env):
    return {
        "command": sys.executable,
        "args": [str(CRU)],
        "env": env,
        "timeout_s": 20,
        "default_risk": "write",
    }


def _axe_limpo(page):
    page.wait_for_timeout(300)
    assert page.evaluate("async () => (await axe.run(document)).violations.map(v => v.id)") == []


def test_integracoes_mostra_servidores_mcp_incompativel_e_testa_o_que_esta_de_pe(
    abrir, novo_backend, tmp_path
):
    dados = _dados(
        tmp_path,
        {"bom": _cru(COM_RECURSOS="1"), "velho": _cru(PROTOCOLO="2024-11-05")},
    )
    url, _, _, _ = novo_backend(data_dir=dados)
    page = abrir("#/integracoes", url=url, init=TOKEN_INIT, axe=True)
    bom = page.locator('[data-servidor="bom"]')
    velho = page.locator('[data-servidor="velho"]')
    expect(bom).to_have_attribute("data-estado", "ok")
    expect(bom).to_contain_text("protocolo 2025-11-25")
    expect(velho).to_have_attribute("data-estado", "incompatível")
    expect(velho).to_contain_text("o servidor velho fala o protocolo 2024-11-05, o Orion fala")
    expect(velho.get_by_text("Última falha")).to_be_visible()
    bom.get_by_text("2 ferramenta(s)").click()
    expect(bom.get_by_role("list", name="Ferramentas de bom")).to_contain_text("bom__eco")
    expect(bom.get_by_role("list", name="Ferramentas de bom")).to_contain_text("write")
    bom.get_by_role("button", name="Testar o servidor bom").click()
    expect(page.locator('[data-teste="bom"]')).to_contain_text("2 ferramenta(s)", timeout=15000)
    _axe_limpo(page)


def test_sem_mcp_json_o_cartao_nao_aparece(abrir, novo_backend):
    url, _, _, _ = novo_backend()
    page = abrir("#/integracoes", url=url, init=TOKEN_INIT)
    expect(page.locator("#mcp-servidores-root")).to_be_hidden()


def test_prompt_de_servidor_vai_para_o_campo_e_nao_e_enviado(abrir, novo_backend, tmp_path):
    dados = _dados(tmp_path, {"cru": _cru(COM_RECURSOS="1")})
    gw = FakeGateway(fala("não devia ser chamado"))
    url, _, _, _ = novo_backend(data_dir=dados, gateway_override=gw)
    page = abrir("#/chat", url=url, init=TOKEN_INIT)
    page.fill("#composer-input", '/prompt cru:revisar texto="olá mundo"')
    page.keyboard.press("Enter")
    expect(page.locator("#composer-input")).to_have_value("Revise: olá mundo")
    expect(page.locator("#toasts")).to_contain_text("Revise antes de enviar")
    page.wait_for_timeout(500)
    assert gw.chamadas == []  # nada foi ao modelo: só você envia
    expect(page.locator(".msg-user")).to_have_count(0)


def test_prompt_pela_paleta_pergunta_o_argumento(abrir, novo_backend, tmp_path):
    dados = _dados(tmp_path, {"cru": _cru(COM_RECURSOS="1")})
    url, _, _, _ = novo_backend(data_dir=dados)
    page = abrir("#/chat", url=url, init=TOKEN_INIT)
    page.wait_for_function("() => Orion.mcpServidores.prompts().length === 1")
    page.keyboard.press("Control+k")
    page.fill("#palette-input", "Prompt MCP")
    page.get_by_role("option", name="Prompt MCP: cru:revisar").click()
    dialogo = page.get_by_role("dialog", name="Prompt cru:revisar")
    dialogo.get_by_label("texto").fill("meu rascunho")
    dialogo.get_by_role("button", name="Trazer").click()
    expect(page.locator("#composer-input")).to_have_value("Revise: meu rascunho")


def test_prompt_sem_servidor_mcp_explica_o_uso(abrir, novo_backend):
    url, _, _, _ = novo_backend()
    page = abrir("#/chat", url=url, init=TOKEN_INIT)
    page.fill("#composer-input", "/prompt sem-formato")
    page.keyboard.press("Enter")
    expect(page.locator("#toasts")).to_contain_text("Use /prompt servidor:nome")


def test_skill_do_catalogo_simples_entra_carregada_no_turno(abrir, novo_backend, tmp_path):
    dados = _dados(tmp_path, {}, skills={"resumir": "Resuma em exatamente 3 linhas."})
    gw = FakeGateway(fala("Resumido em 3 linhas."))
    url, _, _, _ = novo_backend(data_dir=dados, gateway_override=gw)
    page = abrir("#/chat", url=url, init=TOKEN_INIT, axe=True)
    page.fill("#composer-input", "/skill resumir um texto qualquer")
    with page.expect_request(lambda r: r.url == f"{url}/chat") as pedido:
        page.keyboard.press("Enter")
    assert pedido.value.post_data_json == {
        "texto": "um texto qualquer",
        "canal": "web",
        "skill": "resumir",
    }
    expect(page.locator(".msg-orion").last).to_contain_text("Resumido em 3 linhas.")
    tool = next(m for m in gw.chamadas[0] if m["role"] == "tool")
    assert "Resuma em exatamente 3 linhas." in tool["content"]
    expect(page.locator(".usou-rodape summary").last).to_have_text("Usou: carregar skill")
    page.locator(".usou-rodape summary").last.click()
    expect(page.locator(".usou-rodape").last).to_contain_text("skill resumir")
    _axe_limpo(page)


def test_skill_inexistente_preserva_o_rascunho(abrir, novo_backend, tmp_path):
    dados = _dados(tmp_path, {}, skills={"resumir": "x"})
    gw = FakeGateway(fala("não devia ser chamado"))
    url, _, _, _ = novo_backend(data_dir=dados, gateway_override=gw)
    page = abrir("#/chat", url=url, init=TOKEN_INIT)
    page.fill("#composer-input", "/skill fantasma faça algo")
    page.keyboard.press("Enter")
    expect(page.locator("#toasts")).to_contain_text("Não existe a skill “fantasma”")
    expect(page.locator("#composer-input")).to_have_value("/skill fantasma faça algo")
    assert gw.chamadas == []
    page.fill("#composer-input", "/skill resumir")
    page.keyboard.press("Enter")
    expect(page.locator("#toasts")).to_contain_text("escreva o pedido")


def test_usar_skill_pela_paleta_deixa_o_comando_pronto(abrir, novo_backend, tmp_path):
    dados = _dados(tmp_path, {}, skills={"resumir": "Resuma.", "revisar": "Revise."})
    url, _, _, _ = novo_backend(data_dir=dados)
    page = abrir("#/chat", url=url, init=TOKEN_INIT)
    page.keyboard.press("Control+k")
    page.fill("#palette-input", "Usar skill")
    page.get_by_role("option", name="Usar skill…").click()
    dialogo = page.get_by_role("dialog", name="Usar skill")
    dialogo.get_by_label("Skill").select_option("revisar")
    dialogo.get_by_role("button", name="Usar").click()
    expect(page.locator("#composer-input")).to_have_value("/skill revisar ")
