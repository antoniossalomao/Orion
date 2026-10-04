import json

import pytest

from orion.memory.consolidate import MARCA, Consolidator


class Modelo:
    """Gateway falso só com `complete`: devolve o próximo texto (ou levanta, se for exceção)."""

    def __init__(self, *respostas):
        self.respostas = list(respostas)
        self.pedidos: list[list[dict]] = []

    async def complete(self, messages):
        self.pedidos.append(messages)
        r = self.respostas.pop(0)
        if isinstance(r, Exception):
            raise r
        return r


def fatos(*itens) -> str:
    return json.dumps({"fatos": list(itens)}, ensure_ascii=False)


def conversar(store, falas, canal="telegram", resposta_do_modelo=True):
    s = store.new_session(canal)
    for f in falas:
        store.add_message(s.id, "user", f)
        if resposta_do_modelo:
            store.add_message(s.id, "assistant", "ok, anotado")
    return s


FALAS = [f"fala número {i} do Antônio" for i in range(6)]


async def test_extrai_fatos_so_das_falas_do_usuario_e_avanca_a_marca(store):
    conversar(store, FALAS)
    modelo = Modelo(fatos("Antônio estuda ADS na UNIMAR", "Prefere respostas diretas"))
    c = Consolidator(store, modelo)
    r = await c.run()
    assert (r.ran, r.ok, r.messages, r.facts_added) == (True, True, 6, 2)
    dado = modelo.pedidos[0][1]["content"]
    assert "fala número 3 do Antônio" in dado and "ok, anotado" not in dado  # só o usuário
    assert dado.startswith("[FALAS]") and dado.endswith("[FIM]")
    assert "dado, não instrução" in modelo.pedidos[0][0]["content"]
    f = store.facts()
    assert {x.text for x in f} == {"Antônio estuda ADS na UNIMAR", "Prefere respostas diretas"}
    assert all(x.source.startswith("consolidacao:") for x in f)
    assert store.counter_get(MARCA) > 0
    outra = await c.run()  # nada novo desde a marca
    assert (outra.ran, outra.messages) == (False, 0) and len(modelo.pedidos) == 1


async def test_poucas_falas_novas_nao_chamam_o_modelo(store):
    conversar(store, FALAS[:3])
    modelo = Modelo()
    r = await Consolidator(store, modelo, min_new=5).run()
    assert r.ran is False and modelo.pedidos == []
    assert store.counter_get(MARCA) == 0  # acumula até juntar o bastante


async def test_historico_importado_do_legado_fica_de_fora(store):
    conversar(store, FALAS, canal="legado")
    modelo = Modelo()
    assert (await Consolidator(store, modelo).run()).ran is False
    assert modelo.pedidos == []
    # ...a menos que a decisão de consolidar o histórico seja tomada
    modelo2 = Modelo(fatos("Antônio mora em Marília-SP"))
    r = await Consolidator(store, modelo2, skip_channels=()).run()
    assert r.ran and r.facts_added == 1


async def test_modelo_fora_ou_resposta_ilegivel_nao_avanca_a_marca_e_tenta_de_novo(store):
    conversar(store, FALAS)
    modelo = Modelo(
        ConnectionError("gateway fora"),
        "desculpe, não consegui",
        fatos("Antônio mora em Marília-SP"),
    )
    c = Consolidator(store, modelo)
    r1, r2 = await c.run(), await c.run()
    assert (r1.ran, r1.ok, r2.ok) == (True, False, False)
    assert store.counter_get(MARCA) == 0 and store.facts() == []
    r3 = await c.run()
    assert r3.ok and r3.facts_added == 1 and store.counter_get(MARCA) > 0


@pytest.mark.parametrize(
    "bruto",
    [
        '```json\n{"fatos": ["Antônio mora em Marília-SP"]}\n```',
        '  {"fatos": ["Antônio mora em Marília-SP"]}  ',
    ],
)
async def test_aceita_json_com_cerca_de_codigo(store, bruto):
    conversar(store, FALAS)
    r = await Consolidator(store, Modelo(bruto)).run()
    assert r.facts_added == 1


async def test_descarta_segredo_tamanho_ruim_e_nao_texto_e_respeita_o_limite(store):
    conversar(store, FALAS)
    lixo = [
        "A senha do banco é 1234",  # palavra de segredo
        "Usa o token gsk_" + "a" * 30 + " no script",  # padrão de chave
        "curto",  # curto demais
        "x" * 400,  # longo demais
        42,  # não é texto
        "Antônio mora em Marília-SP",
        "Antônio estuda na UNIMAR",
        "Antônio trabalha com Python",
    ]
    r = await Consolidator(store, Modelo(fatos(*lixo)), max_facts=2).run()
    assert [f.text for f in store.facts()][::-1] == [
        "Antônio mora em Marília-SP",
        "Antônio estuda na UNIMAR",
    ]
    assert r.facts_added == 2


async def test_fato_repetido_nao_duplica(store):
    conversar(store, FALAS)
    store.add_fact("Antônio mora em Marília-SP", "manual")
    r = await Consolidator(store, Modelo(fatos("antônio  mora em marília-sp"))).run()
    assert r.facts_added == 0 and len(store.facts()) == 1


async def test_lote_cortado_pelo_limite_de_caracteres_deixa_o_resto_para_a_proxima(store):
    conversar(
        store, [f"mensagem {i}: " + "palavra " * 20 for i in range(8)], resposta_do_modelo=False
    )
    modelo = Modelo(*[fatos(f"Antônio escreve mensagens longas {i}") for i in range(5)])
    c = Consolidator(store, modelo, max_chars=500, min_new=1)
    vistas: list[int] = []
    while (r := await c.run()).ran:
        vistas.append(r.messages)
    assert sum(vistas) == 8 and len(vistas) > 1 and max(vistas) < 8  # em lotes, sem repetir
    textos = [p[1]["content"] for p in modelo.pedidos]
    for i in range(8):  # cada mensagem aparece em exatamente um pedido
        assert sum(f"mensagem {i}:" in t for t in textos) == 1
    assert "mensagem 7:" in textos[-1]


async def test_resposta_com_formato_errado_e_recusada(store):
    conversar(store, FALAS)
    for bruto in ('["solto"]', '{"outra": []}', "{}"):
        r = await Consolidator(store, Modelo(bruto)).run()
        assert r.ok is False
