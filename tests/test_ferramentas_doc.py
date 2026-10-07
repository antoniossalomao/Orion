"""A triagem das ferramentas (ORION_FERRAMENTAS.md) não pode divergir do código."""

import re
from collections import Counter

import pytest

from orion.config import PROJECT_ROOT
from orion.delegate import Delegator
from orion.memory import MemoryStore
from orion.memory.ops import Operations
from orion.policy import DEFAULT_TOOLS
from orion.tools import default_registry

DOC = PROJECT_ROOT / "Memorias Do Projeto" / "ORION_FERRAMENTAS.md"
NOVAS = {
    "listar_fatos",
    "esquecer_fato",
    "editar_fato",
    "delegar",
}  # nasceram na reescrita, não vieram do legado
SITUACOES = {"portada", "substituida", "a-portar", "descartar"}
LINHA = re.compile(
    r"^\| `(\w+)` \| `(\w+)\.py` \| (\w+)( \(externo\))? \| ([\w-]+) \| (.+) \| (\d|—) \|$"
)


@pytest.fixture(scope="module")
def linhas():
    achadas = [
        m.groups() for ln in DOC.read_text(encoding="utf-8").splitlines() if (m := LINHA.match(ln))
    ]
    return {g[0]: g for g in achadas}


def test_a_tabela_tem_exatamente_as_55_ferramentas_do_legado(linhas):
    assert set(linhas) == set(DEFAULT_TOOLS) - NOVAS
    assert len(linhas) == 55


def test_risco_e_conteudo_externo_batem_com_a_politica(linhas):
    for nome, (_, _, risco, externo, *_) in linhas.items():
        spec = DEFAULT_TOOLS[nome]
        assert risco == spec.risk.value, nome
        assert bool(externo) is spec.external, nome


def test_modulo_citado_existe_no_legado(linhas):
    for nome, (_, modulo, *_) in linhas.items():
        assert (PROJECT_ROOT / "Orion_Ollama" / "tools" / f"{modulo}.py").is_file(), nome


def test_situacao_portada_so_para_o_que_o_registro_realmente_tem(linhas, tmp_path):
    store = MemoryStore(tmp_path / "t.db")
    try:
        registradas = set(
            default_registry(store, Delegator(store), Operations(store), desktop=True).names()
        )
    finally:
        store.close()
    for nome, (_, _, _, _, situacao, *_) in linhas.items():
        assert situacao in SITUACOES, nome
        assert (nome in registradas) is (situacao == "portada"), nome


def test_resumo_da_tabela_confere_com_as_linhas(linhas):
    cont = Counter(g[4] for g in linhas.values())
    texto = DOC.read_text(encoding="utf-8")
    for situacao in SITUACOES:
        assert re.search(rf"\| {situacao} \| {cont[situacao]} \|", texto), situacao
