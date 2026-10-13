"""E9 no navegador, contra o Orion de verdade: rotinas, tarefa em segundo plano, Downloads,
"posso desligar?", propostas da transcrição, lembrete insistente e a janela da área de transferência."""

import os
import time

from playwright.sync_api import expect

from orion.memory.ops import Operations
from orion.propostas import Propostas, ler_itens
from tests.fakes import FakeGateway, fala

from .test_capabilities import TOKEN_INIT

ROTINA = """\
descricao: Confere a memória
gatilhos:
  - {tipo: frase, valor: bom dia teste}
passos:
  - ferramenta: buscar_memoria
    argumentos: {consulta: orion}
"""


def _axe_limpo(page):
    page.wait_for_timeout(300)
    assert page.evaluate("async () => (await axe.run(document)).violations.map(v => v.id)") == []


def test_rotina_so_roda_depois_de_liberada_e_cai_se_o_arquivo_muda(abrir, novo_backend, tmp_path):
    dados = tmp_path / "dados"
    (dados / "rotinas").mkdir(parents=True)
    arquivo = dados / "rotinas" / "teste.yaml"
    arquivo.write_text(ROTINA, encoding="utf-8")
    url, app, _, _ = novo_backend(data_dir=dados)
    page = abrir("#/atividade", url=url, init=TOKEN_INIT, axe=True)
    card = page.locator('[data-rotina="teste"]')
    expect(card).to_contain_text("Esperando a sua liberação")
    expect(card).to_contain_text("frase “bom dia teste”")
    expect(card.get_by_role("button", name="Executar a rotina teste")).to_be_disabled()
    card.get_by_role("button", name="Conceder a rotina teste").click()
    expect(card).to_contain_text("Liberada")
    card.get_by_role("button", name="Executar a rotina teste").click()
    expect(card).to_contain_text("Rotina “teste” concluída (1 passo(s))")
    _axe_limpo(page)
    # o arquivo mudou: a liberação cai e a execução fica bloqueada até liberar de novo
    arquivo.write_text(ROTINA + "# editada\n", encoding="utf-8")
    page.get_by_role("button", name="Atualizar").first.click()
    page.reload()
    page.locator('[data-rotina="teste"]').wait_for()
    expect(page.locator('[data-rotina="teste"]')).to_contain_text("O arquivo mudou")
    expect(
        page.locator('[data-rotina="teste"]').get_by_role("button", name="Executar a rotina teste")
    ).to_be_disabled()
    assert app.state.orion.rotinas.obter("teste").estado == "mudou"


def test_tarefa_em_segundo_plano_traz_a_proposta_para_o_campo_sem_enviar(abrir, novo_backend):
    gw = FakeGateway(
        fala("Achei dois repositórios sujos.\nPROPOSTAS:\n- Faça o commit do repositório orion")
    )
    url, app, _, _ = novo_backend(gateway_override=gw)
    page = abrir("#/atividade", url=url, init=TOKEN_INIT, axe=True)
    page.get_by_label("Objetivo (o Orion só lê e propõe)").fill(
        "ver o que mudou nos meus repositórios"
    )
    page.get_by_role("button", name="Começar").click()
    card = page.locator("[data-tarefa]").first
    expect(card).to_have_attribute("data-status", "concluida", timeout=15000)
    expect(card).to_contain_text("Faça o commit do repositório orion")
    _axe_limpo(page)
    card.get_by_role("button", name="Trazer para o campo").click()
    expect(page.locator("#composer-input")).to_have_value("Faça o commit do repositório orion")
    assert len(gw.chamadas) == 1  # a proposta NÃO foi enviada ao modelo
    assert all(
        p["estado"] != "pendente" for p in app.state.orion.tarefas_bg.listar()[0]["proposals"]
    )


def test_downloads_simula_move_e_desfaz(abrir, novo_backend, tmp_path):
    base = tmp_path / "mundo"
    baixados, docs = base / "Downloads", base / "Documentos"
    baixados.mkdir(parents=True)
    docs.mkdir()
    dados = tmp_path / "dados"
    dados.mkdir()
    (dados / "regras_downloads.yaml").write_text(
        f"pasta: {baixados.as_posix()}\nregras:\n  - {{extensoes: [pdf], destino: {docs.as_posix()}/Faturas}}\n",
        encoding="utf-8",
    )
    fatura = baixados / "fatura-1.pdf"
    fatura.write_text("x")
    antigo = time.time() - 3600
    os.utime(fatura, (antigo, antigo))
    url, _, _, _ = novo_backend(data_dir=dados, extra_safe_roots=[base])
    page = abrir("#/atividade", url=url, init=TOKEN_INIT, axe=True)
    secao = page.get_by_role("region", name="Organizador de Downloads")
    expect(secao).to_contain_text("1 regra(s)")
    secao.get_by_role("button", name="Conceder").click()
    expect(secao).to_contain_text("Regras liberadas.")
    secao.get_by_role("button", name="Simular").click()
    expect(secao).to_contain_text("moveria 1 arquivo(s)")
    assert fatura.exists()  # simular não move
    secao.get_by_role("button", name="Organizar agora").click()
    expect(secao).to_contain_text("moveu 1 arquivo(s)")
    assert not fatura.exists() and (docs / "Faturas" / "fatura-1.pdf").exists()
    _axe_limpo(page)
    secao.get_by_role("button", name="Desfazer o último").click()
    expect(secao).to_contain_text("Voltou 1 arquivo(s)")
    assert fatura.exists() and not (docs / "Faturas" / "fatura-1.pdf").exists()


def test_posso_desligar_aponta_o_que_falta(abrir, novo_backend):
    url, _, _, _ = novo_backend()
    page = abrir("#/atividade", url=url, init=TOKEN_INIT, axe=True)
    secao = page.get_by_role("region", name="Posso desligar?")
    secao.get_by_role("button", name="Conferir").click()
    expect(secao.locator("[data-desligar]")).to_contain_text("o backup de hoje ainda não foi feito")
    _axe_limpo(page)


def test_proposta_da_transcricao_so_cria_quando_aprovada(abrir, novo_backend):
    url, app, _, _ = novo_backend()
    s = app.state.orion
    itens = ler_itens(
        '{"itens": [{"tipo": "tarefa", "titulo": "Enviar o orçamento ao cliente"},'
        ' {"tipo": "tarefa", "titulo": "Reservar a sala de reunião"}]}'
    )
    a, b = Propostas(s.memory, s.ops).guardar("reunião de segunda", itens)
    page = abrir("#/atividade", url=url, init=TOKEN_INIT, axe=True)
    um = page.locator(f'[data-proposta="{a}"]')
    expect(um).to_be_visible()
    assert s.ops.list_tasks() == []  # nada criado só por existir a proposta
    _axe_limpo(page)
    um.get_by_role("button", name="Aprovar").click()
    expect(page.locator(f'[data-proposta="{a}"]')).to_have_count(0)
    assert [t["title"] for t in s.ops.list_tasks()] == ["Enviar o orçamento ao cliente"]
    page.locator(f'[data-proposta="{b}"]').get_by_role("button", name="Descartar").click()
    expect(page.locator(f'[data-proposta="{b}"]')).to_have_count(0)
    assert len(s.ops.list_tasks()) == 1  # o descartado nunca virou tarefa


def test_lembrete_insistente_tem_feito_e_adiar_na_atividade(abrir, novo_backend):
    url, app, _, _ = novo_backend()
    ops: Operations = app.state.orion.ops
    r = ops.add_reminder("Ligar para o contador", "2999-01-01T09:00", "", 15)
    ops.notify("lembrete", "⏰ Ainda pendente: Ligar para o contador", ref=f"reminder:{r['id']}:n1")
    page = abrir("#/atividade", url=url, init=TOKEN_INIT, axe=True)
    botoes = page.locator(f'[data-lembrete="{r["id"]}"]')
    expect(botoes.get_by_role("button", name="Adiar 1 h")).to_be_visible()
    _axe_limpo(page)
    botoes.get_by_role("button", name="Feito").click()
    assert all(x["id"] != r["id"] or x["done"] for x in ops.list_reminders(include_done=True))


def test_janela_da_area_de_transferencia_lista_busca_e_avisa_quando_desligada(abrir, novo_backend):
    url, app, _, _ = novo_backend(clipboard_days=7)
    s = app.state.orion
    s.historicos.guardar_area("receita de bolo de cenoura")
    s.historicos.guardar_area("endereço do cliente: rua das flores 10")
    token = s.auth.create_device_token("ponte", "teste")
    page = abrir(url=url, pagina=f"/ui/ponte.html?modo=area#t={token}", axe=True)
    itens = page.locator(".area-item")
    expect(itens).to_have_count(2)
    page.get_by_label("Copiados nos últimos dias").fill("bolo")
    expect(itens).to_have_count(1)
    expect(itens.first).to_contain_text("receita de bolo")
    _axe_limpo(page)


def test_area_de_transferencia_desligada_explica_como_ligar(abrir, novo_backend):
    url, app, _, _ = novo_backend()
    token = app.state.orion.auth.create_device_token("ponte", "teste")
    page = abrir(url=url, pagina=f"/ui/ponte.html?modo=area#t={token}", http_ok=True)
    expect(page.locator("#erro")).to_contain_text("ORION_CLIPBOARD_DAYS")
