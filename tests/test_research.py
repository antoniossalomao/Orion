"""D2: pesquisa noturna só leitura, um relatório por dia na caixa de entrada do vault."""

from datetime import datetime

import pytest

from orion.agent import AgentEvent
from orion.capture import Capturer
from orion.memory import MemoryStore
from orion.research import MAX_ASSUNTOS, NightResearch, parse_assuntos


def _ts(h, m=0, dia=8):
    return datetime(2026, 10, dia, h, m).timestamp()


@pytest.fixture
def store(tmp_path):
    s = MemoryStore(tmp_path / "a.db")
    yield s
    s.close()


def _pesquisa(store, tmp_path, agora, assuntos=("zkML", "MCP"), roda=None):
    chamadas = []

    async def run_turn(canal, texto):
        chamadas.append((canal, texto))
        if roda:
            raise roda
        yield AgentEvent("text", {"text": f"resumo de {len(chamadas)}"})
        yield AgentEvent("done", {})

    vault = tmp_path / "vault"
    vault.mkdir(exist_ok=True)
    n = NightResearch(
        memory=store,
        run_turn=run_turn,
        capturer=Capturer(vault, "00 Inbox", clock=lambda: agora[0]),
        assuntos=list(assuntos),
        at="03:00",
        clock=lambda: agora[0],
    )
    return n, chamadas, vault


def test_parse_assuntos_limita_quantidade_e_tamanho():
    assert parse_assuntos(" zkML ; ; MCP  e  Pix ;") == ["zkML", "MCP e Pix"]
    assert len(parse_assuntos(";".join(f"a{i}" for i in range(20)))) == MAX_ASSUNTOS
    assert len(parse_assuntos("x" * 900)[0]) == 200


async def test_roda_na_janela_uma_vez_por_dia_e_grava_um_relatorio(store, tmp_path):
    agora = [_ts(2, 59)]
    n, chamadas, vault = _pesquisa(store, tmp_path, agora)
    assert not n.devida()  # antes da hora
    agora[0] = _ts(3, 5)
    assert n.devida()
    rel = await n.run()
    assert rel and rel.startswith("00 Inbox/")
    assert [c[0] for c in chamadas] == ["pesquisa", "pesquisa"]
    assert "zkML" in chamadas[0][1] and "só ferramentas de leitura" in chamadas[0][1]
    texto = (vault / rel).read_text(encoding="utf-8")
    assert "fonte: orion" in texto and "tags: [captura, pesquisa]" in texto
    assert "## zkML" in texto and "## MCP" in texto
    assert "confira antes de confiar" in texto
    assert not n.devida()  # já fez hoje
    agora[0] = _ts(3, 5, dia=9)
    assert n.devida()  # no dia seguinte volta
    agora[0] = _ts(10, 0, dia=9)
    assert not n.devida()  # perdeu a janela de 6 h


async def test_cada_assunto_tem_conversa_propria_para_o_taint_nao_vazar(store, tmp_path):
    agora = [_ts(3, 5)]
    n, _, _ = _pesquisa(store, tmp_path, agora)
    await n.run()
    sessoes = [s for s in store.list_sessions("pesquisa")]
    assert len(sessoes) == 2 and {s.title for s in sessoes} == {"Pesquisa: zkML", "Pesquisa: MCP"}


async def test_falha_em_um_assunto_nao_derruba_o_relatorio(store, tmp_path):
    agora = [_ts(3, 5)]
    n, _, vault = _pesquisa(store, tmp_path, agora, roda=RuntimeError("boom"))
    rel = await n.run()
    assert rel and "falhou: RuntimeError" in (vault / rel).read_text(encoding="utf-8")


async def test_sem_vault_valido_nao_levanta(store, tmp_path):
    agora = [_ts(3, 5)]
    n, _, vault = _pesquisa(store, tmp_path, agora)
    vault.rmdir()
    assert await n.run() is None


def test_hora_invalida_e_recusada(store, tmp_path):
    with pytest.raises(ValueError):
        NightResearch(
            memory=store,
            run_turn=None,
            capturer=None,
            assuntos=["a"],
            at="25:00",
            clock=lambda: 0,  # type: ignore[arg-type]
        )
