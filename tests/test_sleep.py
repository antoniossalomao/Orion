"""Ciclo de sono: duplicados avisados (nunca apagados), relações no grafo e padrões validados."""

import json
from datetime import datetime

import pytest

from orion.memory import MemoryStore
from orion.memory.ops import Operations
from orion.memory.sleep import SleepCycle, ler_resposta


class Relogio:
    def __init__(self, t):
        self.t = t

    def __call__(self):
        return self.t


class Modelo:
    def __init__(self, resposta):
        self.resposta, self.pedidos = resposta, []

    async def complete(self, messages):
        self.pedidos.append(messages)
        if isinstance(self.resposta, Exception):
            raise self.resposta
        return self.resposta


@pytest.fixture
def relogio():
    return Relogio(datetime(2026, 10, 8, 3, 10).timestamp())


@pytest.fixture
def store(tmp_path, relogio):
    s = MemoryStore(tmp_path / "s.db", clock=relogio)
    yield s
    s.close()


def _fatos(store, n=7):
    for i in range(n):
        store.add_fact(f"Antônio usa a ferramenta número {i} no projeto Orion", "conversa")


def test_ler_resposta_valida_tamanho_formato_e_segredo():
    bruto = json.dumps(
        {
            "relacoes": [
                ["Antônio", "Estuda em", "Unimar"],
                ["a", "x", "a"],  # origem = destino
                ["Antônio", "usa", "token=abc123"],  # cara de segredo
                ["só", "dois"],
                ["Antônio", "", "Orion"],
            ],
            "padroes": ["Antônio concentra o trabalho no Orion", "curto", "a senha é 1234 pronto"],
        }
    )
    relacoes, padroes = ler_resposta("```json\n" + bruto + "\n```")
    assert relacoes == [("Antônio", "estuda_em", "Unimar")]
    assert padroes == ["Antônio concentra o trabalho no Orion"]
    for ruim in ("nao json", "[]", '{"padroes": []}'):
        with pytest.raises(ValueError):
            ler_resposta(ruim)


def test_devida_so_na_janela_e_uma_vez_por_dia(store, relogio):
    ciclo = SleepCycle(store, Operations(store), None, at="03:00")
    assert ciclo.devida()
    relogio.t = datetime(2026, 10, 8, 2, 59).timestamp()
    assert not ciclo.devida()
    relogio.t = datetime(2026, 10, 8, 10, 0).timestamp()
    assert not ciclo.devida()  # passou da janela de 6 h
    with pytest.raises(ValueError):
        SleepCycle(store, Operations(store), None, at="25:00")


async def test_grava_relacoes_e_padroes_e_avanca_a_marca(store):
    ops = Operations(store)
    _fatos(store)
    modelo = Modelo(
        json.dumps(
            {
                "relacoes": [["Antônio", "usa", "Orion"], ["Orion", "roda_em", "Windows"]],
                "padroes": ["Antônio concentra as ferramentas no projeto Orion"],
            }
        )
    )
    r = await SleepCycle(store, ops, modelo, at="03:00").run()
    assert (r.relacoes, r.padroes, r.ok) == (2, 1, True)
    assert [n["dst"] for n in ops.neighbors("Antônio", "usa")] == ["Orion"]
    assert any(f.source.startswith("sono:destilado:2026-10-08") for f in store.facts())
    assert "[FATOS]" in modelo.pedidos[0][1]["content"]
    # segunda noite sem fatos novos: nem chama o modelo
    modelo2 = Modelo("{}")
    store.counter_set("sono:ultimo", 0)
    r2 = await SleepCycle(store, ops, modelo2, at="03:00").run()
    assert modelo2.pedidos == [] and not r2.ran_model


async def test_fato_destilado_nao_vira_entrada_da_proxima_revisao(store):
    ops = Operations(store)
    _fatos(store)
    modelo = Modelo(json.dumps({"relacoes": [], "padroes": ["Antônio trabalha muito no Orion"]}))
    await SleepCycle(store, ops, modelo, at="03:00").run()
    store.counter_set("sono:fato", 0)
    novos = SleepCycle(store, ops, modelo, at="03:00")._fatos_novos()
    assert all("destilado" not in t and "trabalha muito" not in t for _, t in novos)


async def test_falha_do_modelo_nao_avanca_a_marca_e_nao_levanta(store):
    _fatos(store)
    ciclo = SleepCycle(store, Operations(store), Modelo(RuntimeError("fora")), at="03:00")
    r = await ciclo.run()
    assert r.ran_model and not r.ok and store.counter_get("sono:fato") == 0
    r = await SleepCycle(store, Operations(store), Modelo("lixo"), at="03:00").run()
    assert not r.ok


async def test_duplicados_sao_avisados_uma_vez_e_nada_e_apagado(store):
    ops = Operations(store)
    store.add_fact("Antônio mora em Marília", "conversa")
    store.add_fact("antonio mora em marilia", "vault")
    antes = len(store.facts())
    ciclo = SleepCycle(store, ops, None, at="03:00")  # sem gateway: só o passo 1
    assert (await ciclo.run()).duplicados == 1
    avisos = [n for n in ops.pending_notifications() if n["kind"] == "sono"]
    assert len(avisos) == 1 and "orion esquecer" in avisos[0]["text"]
    store.counter_set("sono:ultimo", 0)
    await ciclo.run()  # o mesmo conjunto: não avisa de novo
    assert len([n for n in ops.pending_notifications() if n["kind"] == "sono"]) == 1
    assert len(store.facts()) == antes
