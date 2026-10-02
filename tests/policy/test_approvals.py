import pytest

from orion.policy import ApprovalStore, Status


class Relogio:
    def __init__(self):
        self.t = 1000.0

    def __call__(self):
        return self.t


@pytest.fixture
def relogio():
    return Relogio()


@pytest.fixture
def store(relogio):
    return ApprovalStore(ttl_s=600, clock=relogio)


ARGS = {"cmd": "Remove-Item C:\\x -Recurse"}


def test_fluxo_pedido_decisao_consumo_unico(store):
    a = store.request("s1", "executar_comando", ARGS, "destrutivo")
    assert not store.consume("s1", "executar_comando", ARGS)  # ainda pendente
    store.decide(a.id, True, channel="telegram", actor="antonio")
    assert store.consume("s1", "executar_comando", ARGS) is True
    assert store.consume("s1", "executar_comando", ARGS) is False  # uso único


def test_pedido_idempotente_enquanto_pendente(store):
    a = store.request("s1", "executar_comando", ARGS, "x")
    b = store.request("s1", "executar_comando", ARGS, "x")
    assert a.id == b.id and len(store.pending("s1")) == 1


def test_aprovacao_e_da_chamada_exata_e_da_sessao(store):
    a = store.request("s1", "executar_comando", ARGS, "x")
    store.decide(a.id, True, channel="web", actor="antonio")
    assert not store.consume("s2", "executar_comando", ARGS)  # outra sessão
    assert not store.consume(
        "s1", "executar_comando", {"cmd": "Remove-Item D:\\"}
    )  # outro argumento
    assert not store.consume("s1", "escrever_arquivo", ARGS)  # outra ferramenta
    assert store.consume("s1", "executar_comando", ARGS)


def test_negada_nunca_consome(store):
    a = store.request("s1", "executar_comando", ARGS, "x")
    store.decide(a.id, False, channel="web", actor="antonio")
    assert not store.consume("s1", "executar_comando", ARGS)
    assert store.get(a.id).status is Status.DENIED


def test_expira(store, relogio):
    a = store.request("s1", "executar_comando", ARGS, "x")
    store.decide(a.id, True, channel="web", actor="antonio")
    relogio.t += 601
    assert not store.consume("s1", "executar_comando", ARGS)
    assert store.get(a.id).status is Status.EXPIRED


def test_nao_decide_duas_vezes_nem_expirada(store, relogio):
    a = store.request("s1", "executar_comando", ARGS, "x")
    store.decide(a.id, True, channel="web", actor="antonio")
    with pytest.raises(ValueError):
        store.decide(a.id, False, channel="web", actor="antonio")
    b = store.request("s1", "executar_comando", {"cmd": "outro"}, "x")
    relogio.t += 700
    with pytest.raises(ValueError):
        store.decide(b.id, True, channel="web", actor="antonio")
    with pytest.raises(KeyError):
        store.decide("inexistente", True, channel="web", actor="antonio")


def test_fila_de_pendentes_ordenada(store, relogio):
    store.request("s1", "executar_comando", {"cmd": "a"}, "x")
    relogio.t += 1
    store.request("s2", "executar_comando", {"cmd": "b"}, "x")
    assert [p.session_id for p in store.pending()] == ["s1", "s2"]
    assert [p.session_id for p in store.pending("s2")] == ["s2"]
