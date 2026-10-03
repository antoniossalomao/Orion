import json

import httpx
import pytest

from orion.memory import MemoryStore
from orion.memory.embedders import MAX_LOTE, EmbeddingError, GeminiEmbedder

CHAVE = "AIzaSyFAKEFAKEFAKEFAKEFAKEFAKEFAKE12345"
DIM = 4


def vetor(i: int) -> dict:
    return {"values": [float(i), 1.0, 0.0, 0.5]}


class Servidor:
    """API falsa: devolve um vetor por pedido; `falhas` é uma fila de respostas de erro."""

    def __init__(self, falhas=()):
        self.pedidos: list[httpx.Request] = []
        self.falhas = list(falhas)

    def __call__(self, req: httpx.Request) -> httpx.Response:
        self.pedidos.append(req)
        if self.falhas:
            f = self.falhas.pop(0)
            if isinstance(f, Exception):
                raise f
            return f
        n = len(json.loads(req.content)["requests"])
        return httpx.Response(200, json={"embeddings": [vetor(i) for i in range(n)]})

    def corpo(self, i: int = -1) -> dict:
        return json.loads(self.pedidos[i].content)


def embedder(servidor, **kw):
    pausas: list[float] = []
    e = GeminiEmbedder(
        CHAVE,
        dim=DIM,
        client=httpx.Client(transport=httpx.MockTransport(servidor)),
        sleep=pausas.append,
        **kw,
    )
    e.pausas = pausas  # type: ignore[attr-defined]
    return e


def test_formato_do_pedido_chave_em_cabecalho_e_nunca_na_url():
    srv = Servidor()
    e = embedder(srv)
    assert e.embed(["a", "b"]) == [[0.0, 1.0, 0.0, 0.5], [1.0, 1.0, 0.0, 0.5]]
    req = srv.pedidos[0]
    assert req.url.path.endswith("/models/gemini-embedding-001:batchEmbedContents")
    assert req.headers["x-goog-api-key"] == CHAVE and CHAVE not in str(req.url)
    reqs = srv.corpo()["requests"]
    assert [r["content"]["parts"][0]["text"] for r in reqs] == ["a", "b"]
    assert {r["taskType"] for r in reqs} == {"RETRIEVAL_DOCUMENT"}
    assert {r["outputDimensionality"] for r in reqs} == {DIM}
    assert {r["model"] for r in reqs} == {"models/gemini-embedding-001"}


def test_consulta_usa_o_modo_de_consulta():
    srv = Servidor()
    assert len(embedder(srv).embed_query("onde moro?")) == DIM
    assert srv.corpo()["requests"][0]["taskType"] == "RETRIEVAL_QUERY"


def test_lote_grande_e_dividido_no_limite_da_api():
    srv = Servidor()
    saida = embedder(srv).embed([f"t{i}" for i in range(MAX_LOTE + 5)])
    assert len(saida) == MAX_LOTE + 5 and len(srv.pedidos) == 2
    assert [len(json.loads(p.content)["requests"]) for p in srv.pedidos] == [MAX_LOTE, 5]
    assert embedder(Servidor()).embed([]) == []


def test_503_e_429_tentam_de_novo_com_backoff_e_respeitam_retry_after():
    srv = Servidor([httpx.Response(503), httpx.Response(429, headers={"retry-after": "7"})])
    e = embedder(srv, backoff_s=1.0)
    assert len(e.embed(["x"])) == 1 and len(srv.pedidos) == 3
    assert e.pausas == [1.0, 7.0]  # type: ignore[attr-defined]


def test_falha_de_rede_e_retentada_e_esgota_com_erro_claro():
    srv = Servidor([httpx.ConnectError("sem rede")] * 2)
    assert len(embedder(srv).embed(["x"])) == 1
    sempre = Servidor([httpx.Response(503)] * 10)
    e = embedder(sempre, retries=2)
    with pytest.raises(EmbeddingError, match="3 tentativas"):
        e.embed(["x"])
    assert len(sempre.pedidos) == 3


def test_erro_do_cliente_nao_e_retentado():
    srv = Servidor([httpx.Response(400, text="API key not valid")])
    with pytest.raises(EmbeddingError, match="HTTP 400"):
        embedder(srv).embed(["x"])
    assert len(srv.pedidos) == 1


def test_resposta_fora_do_formato_ou_com_dimensao_errada_e_recusada():
    for corpo in ({"embeddings": [{"values": [1.0, 2.0]}]}, {"nada": 1}, {"embeddings": []}):
        srv = Servidor([httpx.Response(200, json=corpo)])
        with pytest.raises(EmbeddingError):
            embedder(srv).embed(["x"])


def test_chave_vazia_e_recusada():
    with pytest.raises(ValueError, match="chave"):
        GeminiEmbedder("")


def test_store_indexa_e_consulta_pelo_gemini(tmp_path):
    srv = Servidor()
    store = MemoryStore(tmp_path / "g.db", embedder=embedder(srv))
    try:
        store.add_fact("Antônio mora em Marília", "teste")
        assert store.vectors_available
        assert store.embed_pending() == 0  # já embedou ao gravar
        store.search("onde ele mora")
        tipos = [json.loads(p.content)["requests"][0]["taskType"] for p in srv.pedidos]
        assert tipos == ["RETRIEVAL_DOCUMENT", "RETRIEVAL_QUERY"]
    finally:
        store.close()


def test_api_fora_do_ar_nao_derruba_a_escrita_e_a_busca_segue_por_palavra_chave(tmp_path):
    srv = Servidor([httpx.Response(503)] * 50)
    store = MemoryStore(tmp_path / "g.db", embedder=embedder(srv, retries=1))
    try:
        store.add_fact("Antônio mora em Marília", "teste")  # embedding adiado, fato gravado
        assert [h.via for h in store.search("Marília")] == ["fts"]
    finally:
        store.close()
