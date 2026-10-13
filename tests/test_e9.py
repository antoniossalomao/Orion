"""E9: automação do dia a dia (lembrete insistente, Downloads, 'posso desligar?', ler depois...)."""

import json

import pytest
from fastapi.testclient import TestClient

from orion.app import create_app
from orion.config import Settings
from orion.jobs import JobRunner
from orion.memory import MemoryStore
from orion.memory.ops import Operations
from tests.fakes import FakeGateway, fala
from tests.test_telegram import (  # noqa: F401 — fixtures e ajudantes do canal de mentira
    USER,
    clique,
    montar,
    policy,
    store,
    tg,
)

TOKEN = "token-de-teste-com-16+"
AUTH = {"Authorization": f"Bearer {TOKEN}"}


@pytest.fixture
def relogio():
    return [1_000_000.0]


@pytest.fixture
def mem(tmp_path, relogio):
    m = MemoryStore(tmp_path / "e9.db", clock=lambda: relogio[0])
    yield m
    m.close()


def _jobs(mem, relogio):
    return JobRunner(mem, Operations(mem), clock=lambda: relogio[0])


# ── E9.3: lembrete insistente ─────────────────────────────────────────────────
def test_lembrete_insistente_reavisa_ate_ficar_pronto(mem, relogio):
    ops = Operations(mem)
    r = ops.add_reminder("Tomar remédio", relogio[0] + 60, insist_min=10)
    jobs = _jobs(mem, relogio)
    assert jobs._lembretes() == 0  # ainda não venceu
    relogio[0] += 61
    assert jobs._lembretes() == 1  # o aviso normal
    relogio[0] += 9 * 60
    assert jobs._lembretes() == 0  # 9 min depois: cedo demais para insistir
    relogio[0] += 2 * 60
    assert jobs._lembretes() == 1  # passou o intervalo: insiste
    assert jobs._lembretes() == 0  # e não repete no mesmo tick
    textos = [n["text"] for n in ops.pending_notifications()]
    assert textos[0].startswith("Lembrete (") and textos[1] == "⏰ Ainda pendente: Tomar remédio"
    relogio[0] += 10 * 60
    assert jobs._lembretes() == 1  # segue insistindo
    ops.complete_reminder(r["id"])
    relogio[0] += 3600
    assert jobs._lembretes() == 0  # feito: para


def test_lembrete_comum_nao_insiste_e_valida_o_intervalo(mem, relogio):
    ops = Operations(mem)
    ops.add_reminder("Uma vez só", relogio[0] - 1)
    jobs = _jobs(mem, relogio)
    assert jobs._lembretes() == 1
    relogio[0] += 86400
    assert jobs._lembretes() == 0
    for ruim in (1, 4, 1441, -5):
        with pytest.raises(ValueError, match="insistir de 5 a 1440"):
            ops.add_reminder("x", relogio[0], insist_min=ruim)


def test_adiar_volta_a_ser_lembrete_novo(mem, relogio):
    ops = Operations(mem)
    r = ops.add_reminder("Ligar", relogio[0] - 1, insist_min=5)
    jobs = _jobs(mem, relogio)
    jobs._lembretes()
    relogio[0] += 10 * 60
    jobs._lembretes()  # insistiu
    ops.snooze_reminder(r["id"], 60)
    novo = ops.list_reminders()[0]
    assert novo["notified"] == 0 and novo["last_nudge"] is None
    assert novo["due_at"] == pytest.approx(relogio[0] + 3600)
    relogio[0] += 3601
    assert jobs._lembretes() == 1  # avisa de novo, como lembrete
    with pytest.raises(ValueError):
        ops.snooze_reminder(r["id"], 0)


def test_ferramenta_gerenciar_lembretes_aceita_insistir_min(mem, relogio):
    from orion.tools.ops_tools import ops_tools

    ferramenta = next(t for t in ops_tools(Operations(mem)) if t.name == "gerenciar_lembretes")
    ok = json.loads(
        ferramenta.run(
            {"acao": "criar", "titulo": "Água", "quando": relogio[0] + 5, "insistir_min": 15}
        )
    )
    assert ok["ok"] and ok["lembrete"]["insist_min"] == 15
    ruim = json.loads(
        ferramenta.run({"acao": "criar", "titulo": "Água", "quando": relogio[0], "insistir_min": 2})
    )
    assert "5 a 1440" in ruim["erro"]


def test_rotas_feito_e_adiar(tmp_path):
    settings = Settings(
        data_dir=tmp_path / "d", admin_token=TOKEN, jobs_enabled=False, _env_file=None
    )
    with TestClient(
        create_app(settings, gateway_factory=lambda _: FakeGateway(fala("oi"))),
        base_url="http://127.0.0.1",
    ) as c:
        ops = c.app.state.orion.ops
        a = ops.add_reminder("A", "2030-01-01T10:00:00", insist_min=10)
        b = ops.add_reminder("B", "2030-01-01T11:00:00")
        assert c.post(f"/lembretes/{a['id']}/feito").status_code == 401
        assert c.post(f"/lembretes/{a['id']}/feito", headers=AUTH).json()["lembrete"]["done"] == 1
        r = c.post(f"/lembretes/{b['id']}/adiar?minutos=30", headers=AUTH).json()["lembrete"]
        assert r["notified"] == 0 and r["due_at"] > b["due_at"] - 1e9
        assert c.post("/lembretes/999/feito", headers=AUTH).status_code == 404
        assert c.post(f"/lembretes/{b['id']}/apagar", headers=AUTH).status_code == 422
        assert c.post(f"/lembretes/{b['id']}/adiar?minutos=0", headers=AUTH).status_code == 422


async def test_telegram_aviso_de_lembrete_traz_botoes_e_eles_funcionam(store, policy, tg):  # noqa: F811
    canal, _, ops = montar(store, policy, tg)
    r = ops.add_reminder("Pagar luz", "2020-01-01T10:00:00", insist_min=30)
    ops.notify("lembrete", "Lembrete: Pagar luz", ref=f"reminder:{r['id']}", urgente=True)
    ops.notify("agendamento", "Outro aviso", ref="schedule:1:2")
    assert await canal.deliver_notifications() == 2
    com_botao, sem_botao = tg.enviadas()
    botoes = com_botao["reply_markup"]["inline_keyboard"][0]
    assert [b["callback_data"] for b in botoes] == [f"lem:{r['id']}:f", f"lem:{r['id']}:a"]
    assert "reply_markup" not in sem_botao
    await canal.handle_update(clique(f"lem:{r['id']}:a"))
    assert ops.list_reminders()[0]["notified"] == 0  # adiado
    await canal.handle_update(clique(f"lem:{r['id']}:f"))
    assert ops.list_reminders() == [] and any("Feito" in c.get("text", "") for _, c in tg.chamadas)
    await canal.handle_update(clique("lem:999:f"))
    assert any("não existe mais" in c.get("text", "") for _, c in tg.chamadas)
    await canal.handle_update(clique("lem:abc:f"))  # formato inválido: cai no "botão inválido"
    assert any("Botão inválido" in c.get("text", "") for _, c in tg.chamadas)
