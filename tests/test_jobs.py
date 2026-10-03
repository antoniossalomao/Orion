import asyncio
import json
from datetime import datetime

import pytest

from orion.jobs import JobRunner
from orion.memory import MemoryStore
from orion.memory.consolidate import Consolidator
from orion.memory.ops import Operations
from tests.memory.conftest import FakeEmbedder

AGORA = datetime(2026, 10, 3, 12, 0).timestamp()


class Relogio:
    def __init__(self, t=AGORA):
        self.t = t

    def __call__(self):
        return self.t


@pytest.fixture
def relogio():
    return Relogio()


@pytest.fixture
def store(tmp_path, relogio):
    s = MemoryStore(tmp_path / "j.db", clock=relogio)
    yield s
    s.close()


@pytest.fixture
def embedder():
    return FakeEmbedder()


@pytest.fixture
def ops(store):
    return Operations(store)


def runner(store, ops, relogio, **kw):
    return JobRunner(store, ops, clock=relogio, **kw)


async def test_lembrete_vencido_vira_um_aviso_uma_vez_so(store, ops, relogio):
    ops.add_reminder("Pagar boleto", "2026-10-03T11:00:00", "banco")
    ops.add_reminder("Dentista", "2026-10-05T09:00:00")
    j = runner(store, ops, relogio)
    rel = await j.tick()
    assert rel.lembretes == 1 and rel.erros == []
    avisos = ops.pending_notifications()
    assert [a["kind"] for a in avisos] == ["lembrete"]
    assert avisos[0]["text"] == "Lembrete (03/10 11:00): Pagar boleto — banco"
    assert avisos[0]["ref"].startswith("reminder:")
    assert (await j.tick()).lembretes == 0 and len(ops.pending_notifications()) == 1
    relogio.t = datetime(2026, 10, 5, 9, 1).timestamp()
    assert (await j.tick()).lembretes == 1


async def test_agendamento_avisa_recalcula_e_nao_executa_a_ferramenta(store, ops, relogio):
    s = ops.add_schedule("Briefing", "diario", time_of_day="14:00", tool="executar_comando")
    j = runner(store, ops, relogio)
    assert (await j.tick()).agendamentos == 0
    relogio.t = datetime(2026, 10, 3, 14, 1).timestamp()
    assert (await j.tick()).agendamentos == 1
    aviso = ops.pending_notifications()[0]
    assert aviso["kind"] == "agendamento" and aviso["ref"] == f"schedule:{s['id']}"
    assert "não é executada sozinha" in aviso["text"]  # só registrada: nunca roda sem aprovação
    seguinte = ops.list_schedules()[0]
    assert seguinte["next_run"] == datetime(2026, 10, 4, 14).timestamp()
    assert (await j.tick()).agendamentos == 0


async def test_um_passo_que_falha_nao_derruba_os_outros(store, ops, relogio, monkeypatch):
    ops.add_reminder("Pagar boleto", "2026-10-03T11:00:00")

    def quebra():
        raise ConnectionError("API de embedding fora do ar")

    monkeypatch.setattr(store, "embed_pending", quebra)
    rel = await runner(store, ops, relogio).tick()
    assert rel.lembretes == 1  # o aviso saiu mesmo com o passo de embeddings quebrado
    assert len(rel.erros) == 1 and "embeddings" in rel.erros[0]


async def test_embeddings_pendentes_sao_refeitos_com_pausa_entre_rodadas(
    tmp_path, relogio, embedder
):
    store = MemoryStore(tmp_path / "e.db", embedder=embedder, clock=relogio)
    try:
        embedder.quebrado = True
        store.add_fact("Antônio mora em Marília", "t")  # embedding adiado
        embedder.quebrado = False
        j = runner(store, Operations(store), relogio, embed_every_s=300)
        assert (await j.tick()).embeddings == 1
        store._conn.execute("UPDATE facts SET embedded=0")
        assert (await j.tick()).embeddings is not None and embedder.chamadas >= 1
        n = embedder.chamadas
        await j.tick()
        assert embedder.chamadas == n  # dentro da janela de 5 min: não repete
        relogio.t += 301
        store._conn.execute("UPDATE facts SET embedded=0")
        await j.tick()
        assert embedder.chamadas > n
    finally:
        store.close()


async def test_backup_diario_com_poda_e_sem_repetir_no_dia(store, ops, relogio, tmp_path):
    ops.add_task("coisa a guardar")
    destino = tmp_path / "nuvem" / "backups"
    j = runner(store, ops, relogio, backup_dir=destino, backup_keep=2, backup_every_s=3600)
    rel = await j.tick()
    assert rel.backup and (destino / rel.backup.split("/")[-1].split("\\")[-1]).exists()
    assert MemoryStore.verify_backup(rel.backup)["tasks"] == 1  # o backup restaura e tem os dados
    relogio.t += 3601
    assert (await j.tick()).backup is None  # já existe o de hoje
    for _ in range(3):
        relogio.t += 86_400
        await j.tick()
    assert len(list(destino.glob("orion-*.db"))) == 2  # poda


async def test_sem_pasta_de_backup_nao_faz_backup(store, ops, relogio):
    assert (await runner(store, ops, relogio).tick()).backup is None


async def test_vault_e_indexado_e_reindexado_so_quando_devido(store, ops, relogio, tmp_path):
    vault = tmp_path / "vault"
    vault.mkdir()
    (vault / "Orion.md").write_text("# Orion\nAssistente pessoal do Antônio", encoding="utf-8")
    j = runner(store, ops, relogio, vault_dir=vault, vault_every_s=3600)
    assert (await j.tick()).vault["new"] == 1
    assert store.search("assistente pessoal", kinds=["chunk"])
    (vault / "Nova.md").write_text("nota nova", encoding="utf-8")
    assert (await j.tick()).vault is None  # ainda dentro da hora
    relogio.t += 3601
    assert (await j.tick()).vault["new"] == 1


class Modelo:
    def __init__(self, *respostas):
        self.respostas = list(respostas)

    async def complete(self, messages):
        r = self.respostas.pop(0)
        if isinstance(r, Exception):
            raise r
        return r


async def test_consolidacao_roda_na_cadencia_e_falha_tenta_de_novo_mais_cedo(store, ops, relogio):
    s = store.new_session("telegram")
    for i in range(6):
        store.add_message(s.id, "user", f"fala {i}")
    modelo = Modelo(ConnectionError("fora"), json.dumps({"fatos": ["Antônio mora em Marília-SP"]}))
    j = runner(
        store, ops, relogio, consolidator=Consolidator(store, modelo), consolidate_every_s=1000
    )
    rel = await j.tick()
    assert rel.consolidacao == {"ran": True, "falas": 6, "fatos": 0}  # falhou: nada gravado
    assert store.facts() == []
    relogio.t += 150  # falha: nova tentativa em 20% do intervalo (200s), não no ciclo inteiro
    assert (await j.tick()).consolidacao is None  # cedo demais (150s de 200s)
    relogio.t += 100
    rel = await j.tick()
    assert rel.consolidacao["fatos"] == 1 and len(store.facts()) == 1


async def test_laco_roda_cancela_limpo_e_sobrevive_a_erro(store, ops, relogio, monkeypatch):
    chamadas = []

    async def tick():
        chamadas.append(1)
        if len(chamadas) == 1:
            raise RuntimeError("falha de rodada")

    j = runner(store, ops, relogio)
    monkeypatch.setattr(j, "tick", tick)
    tarefa = asyncio.create_task(j.run_forever(0.01))
    await asyncio.sleep(0.15)
    assert len(chamadas) >= 2  # a rodada que falhou não matou o laço
    tarefa.cancel()
    with pytest.raises(asyncio.CancelledError):
        await tarefa
