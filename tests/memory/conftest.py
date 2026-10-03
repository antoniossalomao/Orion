import re
import unicodedata
import zlib

import pytest

from orion.memory import MemoryStore
from orion.memory.text import _STOPWORDS

# Embedder falso e determinístico: sinônimos viram o mesmo "conceito" — é o que um
# modelo de embedding real faz e a busca por palavra-chave não faz.
CONCEITOS = {
    "estudo": "ESTUDAR",
    "estuda": "ESTUDAR",
    "estudar": "ESTUDAR",
    "faculdade": "ESTUDAR",
    "universidade": "ESTUDAR",
    "unimar": "ESTUDAR",
    "curso": "ESTUDAR",
    "moro": "MORAR",
    "mora": "MORAR",
    "morar": "MORAR",
    "cidade": "MORAR",
    "residencia": "MORAR",
    "carro": "VEICULO",
    "veiculo": "VEICULO",
    "automovel": "VEICULO",
}


def _plano(t: str) -> str:
    return "".join(
        c for c in unicodedata.normalize("NFKD", t.casefold()) if not unicodedata.combining(c)
    )


_PLANAS = {_plano(w) for w in _STOPWORDS}


class FakeEmbedder:
    dim = 128

    def __init__(self):
        self.chamadas = 0
        self.quebrado = False

    def embed(self, texts):
        if self.quebrado:
            raise ConnectionError("API de embedding fora do ar")
        self.chamadas += 1
        saida = []
        for t in texts:
            v = [0.0] * self.dim
            for tok in re.findall(r"\w+", _plano(t)):
                if tok in _PLANAS:  # modelos reais dão peso baixo a palavras funcionais
                    continue
                chave: str = CONCEITOS.get(str(tok), str(tok))
                v[zlib.crc32(chave.encode()) % self.dim] += 1.0
            saida.append(v)
        return saida


@pytest.fixture
def embedder():
    return FakeEmbedder()


@pytest.fixture
def store(tmp_path):
    s = MemoryStore(tmp_path / "orion.db")
    yield s
    s.close()


@pytest.fixture
def store_vec(tmp_path, embedder):
    s = MemoryStore(tmp_path / "orion_vec.db", embedder=embedder)
    yield s
    s.close()
