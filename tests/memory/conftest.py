import json
import re
import unicodedata
import zlib
from datetime import datetime

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


# ── export falso do SurrealDB (formato do `backup_memoria` do legado) ──────────
AGORA = datetime(2026, 10, 3, 12, 0).timestamp()
SESSAO_A = "11111111-aaaa-bbbb-cccc-000000000001"
SESSAO_B = "22222222-aaaa-bbbb-cccc-000000000002"


def evento(i, ator, texto, ts, sessao=None, **extra):
    e = {
        "id": f"evento:⟨e{i}⟩",
        "fonte": "chat",
        "ator": ator,
        "texto": texto,
        "timestamp": ts,
        **extra,
    }
    if sessao:
        e["sessao_id"] = sessao
    return e


@pytest.fixture
def export(tmp_path):
    pasta = tmp_path / "backup"
    pasta.mkdir()
    (pasta / "sessao.json").write_text(
        json.dumps(
            [
                {
                    "id": f"sessao:`{SESSAO_A}`",
                    "criada": "2026-06-01T10:00:00",
                    "titulo": "Projeto Orion",
                },
                {"id": f"sessao:⟨{SESSAO_B}⟩", "criada": "2026-06-02T09:00:00", "titulo": None},
                {"titulo": "sem id"},
            ]
        ),
        encoding="utf-8",
    )
    (pasta / "evento.json").write_text(
        json.dumps(
            [
                evento(
                    2,
                    "Orion",
                    "Marília-SP, certo.",
                    "2026-06-01T10:00:05",
                    SESSAO_A,
                    intencao="resposta",
                    fontes_rag=["p1"],
                ),
                evento(
                    1,
                    "Antônio",
                    "Em que cidade eu moro?",
                    "2026-06-01T10:00:00",
                    SESSAO_A,
                    intencao="objetivo",
                ),
                evento(
                    3,
                    "Lyra",
                    "resposta de quando eu tinha outro nome",
                    "2026-06-02T09:00:10",
                    SESSAO_B,
                ),
                evento(4, "Antonio", "mensagem antiga sem sessão", "2026-01-01T08:00:00"),
                evento(5, "sistema", "briefing automático", "2026-06-02T09:00:00", SESSAO_B),
                evento(6, "Antônio", "   ", "2026-06-02T09:00:20", SESSAO_B),  # vazio
                {
                    "fonte": "chat",
                    "ator": "Antônio",
                    "texto": "sem id",
                    "timestamp": "2026-06-02T09:00:30",
                },  # sem id
                evento(
                    7,
                    "Antônio",
                    "sessão que não existe no export",
                    "2026-06-03T09:00:00",
                    "99999999-aaaa",
                ),
            ]
        ),
        encoding="utf-8",
    )
    (pasta / "lembrete.json").write_text(
        json.dumps(
            [
                {
                    "id": "lembrete:a",
                    "titulo": "Pagar boleto",
                    "quando": "2026-06-05T09:00:00",
                    "criado": "2026-06-01T08:00:00",
                    "concluido": False,
                },
                {
                    "id": "lembrete:b",
                    "titulo": "Entregar trabalho",
                    "quando": "2099-01-01T09:00:00",
                    "nota": "UML",
                    "concluido": False,
                },
            ]
        ),
        encoding="utf-8",
    )
    (pasta / "numero.json").write_text("[]", encoding="utf-8")
    return pasta


def escrever(pasta, tabela, registros):
    (pasta / f"{tabela}.json").write_text(json.dumps(registros), encoding="utf-8")


@pytest.fixture
def export_completo(export):
    """O export com as demais tabelas operacionais e as relações do grafo."""
    escrever(
        export,
        "agendamento",
        [
            {  # diário atrasado: o próximo horário é recalculado a partir de agora
                "id": "agendamento:d",
                "titulo": "Checar saúde",
                "tipo": "diario",
                "horario": "08:00",
                "proxima_execucao": "2026-06-01T08:00:00",
                "ferramenta": "checar_saude_sistema",
                "parametros": '{"verbose": true}',
                "ativo": True,
            },
            {  # único que nunca disparou e já passou: entra desativado
                "id": "agendamento:u",
                "titulo": "Aviso velho",
                "tipo": "unico",
                "proxima_execucao": "2026-06-01T08:00:00",
                "ativo": True,
            },
            {  # pausado pelo usuário continua pausado
                "id": "agendamento:p",
                "titulo": "Pausado",
                "tipo": "intervalo_min",
                "intervalo_min": 30,
                "proxima_execucao": "2026-10-03T12:30:00",
                "ativo": False,
                "parametros": "{quebrado",
            },
            {"id": "agendamento:x", "titulo": "Tipo desconhecido", "tipo": "cron"},
        ],
    )
    escrever(
        export,
        "tarefa",
        [
            {"id": "tarefa:a", "titulo": "Estudar UML", "status": "em_andamento"},
            {"id": "tarefa:b", "titulo": "Status esquisito", "status": "talvez"},
            {"id": "tarefa:c", "titulo": ""},
        ],
    )
    escrever(
        export,
        "numero",
        [
            {
                "id": "numero:a",
                "alvo": "fatura",
                "score": 7,
                "motivo": "vence amanhã",
                "notificado": True,
            },
            {"id": "numero:b", "alvo": "log", "score": "alto"},
        ],
    )
    escrever(
        export,
        "prompt",
        [
            {
                "id": "prompt:a",
                "titulo": "Resumo",
                "comando": "resumo",
                "conteudo": "Resuma em 5 linhas",
            }
        ],
    )
    escrever(
        export,
        "precedeu",
        [
            {"id": "precedeu:1", "in": "evento:⟨e1⟩", "out": "evento:⟨e2⟩"},
            {
                "id": "precedeu:2",
                "in": "evento:⟨e6⟩",
                "out": "evento:⟨e1⟩",
            },  # e6 é vazio: não importado
        ],
    )
    escrever(
        export,
        "sobre",
        [
            {"id": "sobre:1", "in": "evento:⟨e1⟩", "out": "topico:⟨marília⟩"},
            {"id": "sobre:2", "in": "evento:⟨e2⟩", "out": "topico:⟨marília⟩"},
            {"id": "sobre:3", "in": "pessoa:⟨x⟩", "out": "topico:⟨marília⟩"},
        ],
    )
    escrever(
        export,
        "conecta",
        [
            {
                "id": "conecta:1",
                "in": "topico:⟨marília⟩",
                "out": "topico:⟨unimar⟩",
                "tipo": "rem",
                "peso": 2,
            }
        ],
    )
    return export
