"""Briefing matinal: o texto (só do que o Orion guarda) e o job que o dispara uma vez por dia."""

from datetime import datetime

import pytest

from orion.briefing import MAX_LINHAS, build_briefing
from orion.jobs import JobRunner
from orion.memory import MemoryStore
from orion.memory.ops import Operations

SEGUNDA = datetime(2026, 10, 5, 7, 30)  # segunda-feira
Hora = datetime


def ts(dia, h, m=0):
    return datetime(2026, 10, dia, h, m).timestamp()


class Relogio:
    def __init__(self, t):
        self.t = t

    def __call__(self):
        return self.t


@pytest.fixture
def relogio():
    return Relogio(SEGUNDA.timestamp())


@pytest.fixture
def store(tmp_path, relogio):
    s = MemoryStore(tmp_path / "b.db", clock=relogio)
    yield s
    s.close()


@pytest.fixture
def ops(store):
    return Operations(store)


def test_briefing_reune_atrasados_hoje_agendamentos_e_tarefas(ops, relogio):
    ops.add_reminder("Pagar o boleto", ts(3, 18))  # atrasado (sábado)
    ops.add_reminder("Reunião da faculdade", ts(5, 19, 30))
    ops.add_reminder("Dentista", ts(6, 9))  # amanhã: fora
    ops.add_schedule("Backup do vault", "diario", time_of_day="22:00")
    ops.add_schedule("Pausado", "diario", time_of_day="22:30")
    ops.set_schedule_active(2, False)
    ops.add_schedule("Só amanhã", "unico", when=ts(6, 8))
    ops.add_task("Escrever o relatório")
    t2 = ops.add_task("Revisar PR do Orion")
    ops.set_task_status(t2["id"], "em_andamento")
    t3 = ops.add_task("Já feita")
    ops.set_task_status(t3["id"], "concluida")

    texto = build_briefing(ops, relogio())
    assert texto.startswith("☀️ Bom dia, Antônio. Hoje é segunda-feira, 05/10/2026.")
    assert "⏰ Atrasados\n• 03/10 18:00 — Pagar o boleto" in texto
    assert "🔔 Lembretes de hoje\n• 19:30 — Reunião da faculdade" in texto
    assert "• 22:00 — Backup do vault" in texto
    assert "Pausado" not in texto and "Só amanhã" not in texto and "Dentista" not in texto
    assert texto.index("Revisar PR do Orion (em andamento)") < texto.index("Escrever o relatório")
    assert "Já feita" not in texto


def test_briefing_sem_nada_diz_que_nao_ha_pendencia(ops, relogio):
    assert build_briefing(ops, relogio()).endswith("Nada pendente por aqui.")


def test_briefing_limita_cada_secao(ops, relogio):
    for i in range(MAX_LINHAS + 3):
        ops.add_task(f"tarefa {i}")
    texto = build_briefing(ops, relogio())
    assert "… e mais 3" in texto and texto.count("• tarefa") == MAX_LINHAS


# ── job ───────────────────────────────────────────────────────────────────────
def runner(store, ops, relogio, **kw):
    return JobRunner(store, ops, clock=relogio, **{"briefing_at": "07:30", **kw})


def avisos(ops):
    return [n for n in ops.pending_notifications() if n["kind"] == "briefing"]


async def test_briefing_sai_uma_vez_por_dia_a_partir_da_hora(store, ops, relogio):
    ops.add_task("Estudar UML")
    r = runner(store, ops, relogio)
    relogio.t = ts(5, 7, 29)
    assert (await r.tick()).briefing is False and avisos(ops) == []
    relogio.t = ts(5, 7, 31)
    assert (await r.tick()).briefing is True
    (aviso,) = avisos(ops)
    assert "Estudar UML" in aviso["text"] and aviso["ref"] == "briefing:20261005"
    relogio.t = ts(5, 9, 0)
    assert (await r.tick()).briefing is False and len(avisos(ops)) == 1  # só um por dia
    relogio.t = ts(6, 7, 31)
    assert (await r.tick()).briefing is True and len(avisos(ops)) == 2  # o dia seguinte


async def test_briefing_nao_repete_depois_de_reiniciar(store, ops, relogio):
    relogio.t = ts(5, 8, 0)
    assert (await runner(store, ops, relogio).tick()).briefing is True
    assert (
        await runner(store, ops, relogio).tick()
    ).briefing is False  # outro processo, mesmo banco


async def test_briefing_atrasado_sai_dentro_da_janela_e_nunca_de_noite(store, ops, relogio):
    r = runner(store, ops, relogio)
    relogio.t = ts(5, 13, 0)  # 5h30 depois da hora: ainda vale
    assert (await r.tick()).briefing is True
    r2 = runner(store, ops, relogio)
    relogio.t = ts(6, 23, 0)  # subiu à noite: não manda "bom dia"
    assert (await r2.tick()).briefing is False and len(avisos(ops)) == 1


async def test_sem_hora_configurada_o_briefing_fica_desligado(store, ops, relogio):
    relogio.t = ts(5, 12, 0)
    rel = await JobRunner(store, ops, clock=relogio).tick()
    assert rel.briefing is False and avisos(ops) == []


@pytest.mark.parametrize("ruim", ["7h30", "25:00", "07:60", "abc", "7"])
def test_hora_invalida_e_recusada_na_criacao(store, ops, relogio, ruim):
    with pytest.raises(ValueError, match="HH:MM"):
        runner(store, ops, relogio, briefing_at=ruim)


def semanais(ops):
    return [n for n in ops.pending_notifications() if n["kind"] == "semanal"]


async def test_resumo_semanal_sai_na_segunda_uma_vez_e_conta_a_semana(store, ops, relogio):
    from orion.briefing import build_weekly

    relogio.t = ts(1, 10)  # quinta passada
    s = store.new_session("web")
    store.add_message(s.id, "user", "oi")
    t = ops.add_task("Entregar UML")
    ops.set_task_status(t["id"], "concluida")
    store.add_fact("Antônio usa Obsidian", "conversa")
    relogio.t = ts(5, 7, 31)  # segunda
    texto = build_weekly(store, relogio.t)
    assert "1 vez(es)" in texto and "1 concluída(s)" in texto and "Entregar UML" in texto
    assert "Antônio usa Obsidian" in texto
    r = runner(store, ops, relogio)
    assert (await r.tick()).briefing is True
    assert len(semanais(ops)) == 1 and semanais(ops)[0]["ref"] == "semanal:202641"
    relogio.t = ts(6, 7, 31)  # terça: só o briefing diário
    await r.tick()
    assert len(semanais(ops)) == 1


async def test_resumo_semanal_nao_sai_fora_da_segunda(store, ops, relogio):
    relogio.t = ts(6, 7, 31)  # terça
    await runner(store, ops, relogio).tick()
    assert semanais(ops) == []
