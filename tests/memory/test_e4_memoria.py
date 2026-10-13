"""E4: isolamento por projeto (regra 52), validade e substituição de fatos (regra 54), reranker."""

import pytest

from orion.memory import MemoryStore
from orion.memory.consolidate import Consolidator
from orion.memory.rerank import OnnxReranker, RerankIndisponivel, rerank
from orion.memory.scope import data_scope
from orion.projects import Projects


class Relogio:
    def __init__(self, t=1_800_000_000.0):
        self.t = t

    def __call__(self):
        return self.t


@pytest.fixture
def relogio():
    return Relogio()


@pytest.fixture
def mem(tmp_path, relogio):
    s = MemoryStore(tmp_path / "e4.db", clock=relogio)
    yield s
    s.close()


# ── E4.1 ────────────────────────────────────────────────────────────────────
def test_search_facts_respeita_o_projeto(mem):
    a = Projects(mem).create("A")["id"]
    pessoal = mem.add_fact("Canário pessoal vermelho", "m")
    with data_scope(a, include_personal=False):
        doa = mem.add_fact("Canário do projeto A azul", "m")
        assert [f.id for f in mem.search_facts("Canário")] == [doa.id]
    assert [f.id for f in mem.search_facts("Canário")] == [pessoal.id]


def test_move_fact_troca_o_escopo_e_desfaz_ligacoes(mem):
    a = Projects(mem).create("A")["id"]
    velho = mem.add_fact("Antônio mora em Marília", "m")
    novo = mem.add_fact("Antônio mora em Bauru", "m")
    mem.supersede_fact(velho.id, novo.id)
    movido = mem.move_fact(novo.id, a)
    assert movido.project_id == a
    assert mem.get_fact(velho.id).superseded_by is None  # não aponta para fato de outro escopo
    with data_scope(a, include_personal=False):
        assert [f.id for f in mem.facts()] == [novo.id]
    assert [f.id for f in mem.facts()] == [velho.id]
    with pytest.raises(KeyError):
        mem.move_fact(999, None)


class Modelo:
    def __init__(self, fatos):
        self.fatos, self.pedidos = fatos, 0

    async def complete(self, messages):
        self.pedidos += 1
        import json

        return json.dumps({"fatos": self.fatos[self.pedidos - 1]})


async def test_consolidacao_grava_no_projeto_de_origem(mem):
    a = Projects(mem).create("A")["id"]
    sp, sa = mem.new_session("web"), mem.new_session("web", project_id=a)
    for i in range(5):
        mem.add_message(sp.id, "user", f"fala pessoal {i}")
        mem.add_message(sa.id, "user", f"fala do projeto {i}")
    modelo = Modelo([["O Antônio gosta de café forte"], ["O projeto A usa a biblioteca Zeta"]])
    r = await Consolidator(mem, modelo).run()
    assert r.ran and r.ok and r.facts_added == 2
    assert [f.text for f in mem.facts()] == ["O Antônio gosta de café forte"]
    with data_scope(a, include_personal=False):
        assert [f.text for f in mem.facts()] == ["O projeto A usa a biblioteca Zeta"]
    again = await Consolidator(mem, Modelo([[], []])).run()
    assert not again.ran  # as marcas de cada escopo avançaram


# ── E4.5 ────────────────────────────────────────────────────────────────────
def test_fato_vencido_ou_substituido_pesa_menos_e_leva_a_marca(mem, relogio):
    novo = mem.add_fact("O notebook do Antônio é um MacBook", "m")
    velho = mem.add_fact("O notebook do Antônio é um Lenovo", "m")
    outro = mem.add_fact("O notebook de reserva é um Dell", "m")
    assert mem.search("notebook", k=5, kinds=("fact",))[0].id in {novo.id, velho.id, outro.id}
    mem.supersede_fact(velho.id, novo.id)
    mem.set_fact_validity(outro.id, relogio.t + 100)
    antes = {h.id: h for h in mem.search("notebook", k=5, kinds=("fact",))}
    assert not antes[outro.id].text.startswith("(antigo)")  # ainda vale
    assert antes[velho.id].text.startswith("(antigo) ")
    relogio.t += 200  # o prazo do Dell passou
    depois = mem.search("notebook", k=5, kinds=("fact",))
    assert depois[0].id == novo.id
    marcados = {h.id for h in depois if h.text.startswith("(antigo) ")}
    assert marcados == {velho.id, outro.id}
    assert mem.get_fact(outro.id).old and mem.get_fact(velho.id).superseded_by == novo.id
    assert next(f.id for f in mem.search_facts("notebook")) == novo.id  # vigente primeiro


def test_supersede_nao_aceita_o_proprio_fato_nem_outro_escopo(mem):
    a = Projects(mem).create("A")["id"]
    f = mem.add_fact("um", "m")
    with pytest.raises(ValueError):
        mem.supersede_fact(f.id, f.id)
    with data_scope(a, include_personal=False):
        g = mem.add_fact("dois", "m")
    with pytest.raises(KeyError):
        mem.supersede_fact(f.id, g.id)


def test_conflitos_sao_achados_e_decididos_so_pelo_antonio(mem, relogio):
    velho = mem.add_fact("Antônio mora em Marília desde 2020", "m")
    relogio.t += 10
    novo = mem.add_fact("Antônio mora em Bauru desde 2026", "m")
    mem.add_fact("O backup roda todo dia de madrugada", "m")
    pares = mem.conflicting_facts()
    assert [(a.id, b.id) for a, b in pares] == [(velho.id, novo.id)]
    assert mem.record_conflict(velho.id, novo.id) is True
    assert mem.record_conflict(velho.id, novo.id) is False  # nunca avisa duas vezes
    [(a, b)] = mem.pending_conflicts()
    assert (a.id, b.id) == (velho.id, novo.id)
    mem.resolve_conflict(velho.id, novo.id, "novo")
    assert mem.get_fact(velho.id).superseded_by == novo.id and mem.pending_conflicts() == []
    with pytest.raises(KeyError):
        mem.resolve_conflict(velho.id, novo.id, "ambos")  # já decidido
    assert mem.conflicting_facts() == []  # o substituído sai da comparação


def test_os_dois_valem_encerra_o_aviso_sem_marcar_nada(mem, relogio):
    a = mem.add_fact("Antônio trabalha com Python no estágio", "m")
    relogio.t += 5
    b = mem.add_fact("Antônio trabalha com Java no estágio", "m")
    assert mem.record_conflict(a.id, b.id)
    mem.resolve_conflict(a.id, b.id, "ambos")
    assert mem.pending_conflicts() == []
    assert mem.get_fact(a.id).superseded_by is None and not mem.get_fact(a.id).old


# ── E4.6 ────────────────────────────────────────────────────────────────────
class Notas:
    def __init__(self, notas):
        self.notas, self.chamadas = notas, 0

    def score(self, pergunta, trechos):
        self.chamadas += 1
        return [self.notas.get(t, 0.0) for t in trechos]


def test_reranker_reordena_e_o_desligado_nao_muda_nada(mem):
    mem.add_fact("café com leite pela manhã", "m")
    mem.add_fact("café forte depois do almoço", "m")
    sem = [h.text for h in mem.search("café", k=5, kinds=("fact",))]
    mem.reranker = Notas({"café forte depois do almoço": 0.9, "café com leite pela manhã": 0.1})
    com = [h.text for h in mem.search("café", k=5, kinds=("fact",))]
    assert com == ["café forte depois do almoço", "café com leite pela manhã"]
    assert set(sem) == set(com)


def test_reranker_que_quebra_nao_derruba_a_busca(mem):
    mem.add_fact("café com leite", "m")
    mem.add_fact("café forte", "m")

    class Quebrado:
        def score(self, pergunta, trechos):
            raise RuntimeError("onnx caiu")

    base = [h.id for h in mem.search("café", k=5, kinds=("fact",))]
    mem.reranker = Quebrado()
    assert [h.id for h in mem.search("café", k=5, kinds=("fact",))] == base


def test_rerank_ordena_estavel():
    assert rerank(Notas({"b": 0.9, "a": 0.9, "c": 0.1}), "q", ["a", "b", "c"]) == ["a", "b", "c"]
    assert rerank(Notas({"c": 0.8}), "q", ["a", "b", "c"]) == ["c", "a", "b"]


def test_onnx_sem_arquivos_ou_sem_biblioteca_avisa(tmp_path):
    with pytest.raises(RerankIndisponivel):
        OnnxReranker(tmp_path)
    (tmp_path / "model.onnx").write_bytes(b"x")
    (tmp_path / "tokenizer.json").write_text("{}")

    class Tok:
        def encode_batch(self, pares):
            class E:
                ids, attention_mask, type_ids = [1, 2], [1, 1], [0, 0]

            return [E() for _ in pares]

    class Sessao:
        def get_inputs(self):
            class Entrada:
                name = "input_ids"

            class Mascara:
                name = "attention_mask"

            return [Entrada(), Mascara()]

        def run(self, _, entradas):
            assert set(entradas) == {"input_ids", "attention_mask"}
            return [[[4.0], [-4.0]]]

    r = OnnxReranker(tmp_path, carregar=lambda p: (Sessao(), Tok()))
    notas = r.score("q", ["bom", "ruim"])
    assert notas[0] > 0.9 > 0.1 > notas[1]
    assert r.score("q", []) == []
