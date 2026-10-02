import numpy as np
import pytest

from orion.memory.vectors import VectorIndex, normalizar, para_blob


def linhas(*vetores):
    return lambda: [(i + 1, para_blob(v)) for i, v in enumerate(vetores)]


def test_ordena_por_cosseno_e_ignora_o_tamanho_do_vetor():
    idx = VectorIndex()
    carregar = linhas([10, 0, 0], [0.9, 0.1, 0], [0, 1, 0], [0, 0, 5])
    ids = idx.topk("fact", [1, 0.05, 0], 4, carregar)
    assert ids[:2] == [1, 2] and set(ids) == {1, 2, 3, 4}


def test_k_menor_que_o_total_devolve_so_os_melhores():
    idx = VectorIndex()
    carregar = linhas(*[[1, i / 10, 0] for i in range(10)])
    assert idx.topk("chunk", [1, 0, 0], 3, carregar) == [1, 2, 3]


def test_indice_vazio_e_dimensao_diferente():
    idx = VectorIndex()
    assert idx.topk("fact", [1, 0], 5, lambda: []) == []
    assert idx.topk("chunk", [1, 0], 5, linhas([1, 0, 0])) == []  # consulta de 2 dim vs. 3 dim


def test_cache_so_recarrega_quando_invalidado():
    idx = VectorIndex()
    chamadas = []

    def carregar():
        chamadas.append(1)
        return linhas([1, 0])()

    idx.topk("fact", [1, 0], 1, carregar)
    idx.topk("fact", [1, 0], 1, carregar)
    assert len(chamadas) == 1
    idx.invalidar("fact")
    idx.topk("fact", [1, 0], 1, carregar)
    assert len(chamadas) == 2
    idx.invalidar()
    idx.topk("fact", [1, 0], 1, carregar)
    assert len(chamadas) == 3


def test_normalizacao():
    assert np.allclose(np.linalg.norm(normalizar([3, 4])), 1.0)
    assert normalizar([0, 0]).tolist() == [0, 0]
    assert len(para_blob([1, 2, 3])) == 12  # 3 × float32


def test_desempenho_com_dezenas_de_milhares_de_trechos():
    """Memória pessoal grande: 30 mil trechos de 384 dim respondem bem abaixo de 1 s."""
    import time

    rng = np.random.default_rng(0)
    base = rng.standard_normal((30_000, 384)).astype(np.float32)
    base /= np.linalg.norm(base, axis=1, keepdims=True)
    idx = VectorIndex()
    carregar = lambda: [(i, base[i].tobytes()) for i in range(len(base))]  # noqa: E731
    idx.topk("chunk", base[7], 5, carregar)  # carga fria
    t = time.perf_counter()
    ids = idx.topk("chunk", base[7], 5, carregar)
    assert ids[0] == 7
    assert time.perf_counter() - t < 1.0


@pytest.mark.parametrize("k", [1, 2, 50])
def test_k_extremos(k):
    idx = VectorIndex()
    assert len(idx.topk("fact", [1, 0], k, linhas([1, 0], [0, 1], [1, 1]))) == min(k, 3)
