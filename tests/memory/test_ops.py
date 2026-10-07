import sqlite3
from datetime import datetime

import pytest

from orion.memory import MemoryStore
from orion.memory.ops import Operations, next_run, parse_when
from orion.memory.schema import DDL_V1, DDL_V2, SCHEMA_VERSION

AGORA = datetime(2026, 10, 3, 12, 0).timestamp()


def em(*args) -> float:
    return datetime(*args).timestamp()


@pytest.fixture
def ops(store):
    store._clock = lambda: AGORA
    return Operations(store)


# ── datas ─────────────────────────────────────────────────────────────────
def test_parse_when_aceita_iso_e_epoch_e_recusa_lixo():
    assert parse_when("2026-06-25T14:30:00") == em(2026, 6, 25, 14, 30)
    assert parse_when(123.5) == 123.5
    with pytest.raises(ValueError, match="ISO"):
        parse_when("amanhã")


def test_proximo_disparo_de_cada_tipo():
    assert next_run("unico", AGORA, when=AGORA + 60) == AGORA + 60
    assert next_run("diario", AGORA, time_of_day="14:30") == em(2026, 10, 3, 14, 30)
    assert next_run("diario", AGORA, time_of_day="08:00") == em(2026, 10, 4, 8)  # já passou hoje
    assert next_run("diario", AGORA, time_of_day="12:00") == em(
        2026, 10, 4, 12
    )  # igual a agora: amanhã
    assert next_run("intervalo_min", AGORA, interval_min=15) == AGORA + 900
    for tipo, kw in [("unico", {}), ("diario", {"time_of_day": "8h"}), ("intervalo_min", {})]:
        with pytest.raises(ValueError):
            next_run(tipo, AGORA, **kw)
    with pytest.raises(ValueError, match="tipo inválido"):
        next_run("cron", AGORA)


# ── lembretes ─────────────────────────────────────────────────────────────
def test_lembrete_vence_avisa_uma_vez_e_conclui(ops):
    r = ops.add_reminder("Pagar boleto", "2026-10-03T11:00:00", "banco")
    futuro = ops.add_reminder("Dentista", "2026-10-05T09:00:00")
    assert r["due_at"] == em(2026, 10, 3, 11) and r["note"] == "banco" and r["done"] == 0
    assert [x["title"] for x in ops.due_reminders()] == ["Pagar boleto"]
    ops.mark_reminder_notified(r["id"])
    assert ops.due_reminders() == []  # avisado uma vez só
    assert [x["title"] for x in ops.due_reminders(em(2026, 10, 6))] == ["Dentista"]
    assert ops.complete_reminder(futuro["id"])["done"] == 1
    assert [x["title"] for x in ops.list_reminders()] == ["Pagar boleto"]
    assert len(ops.list_reminders(include_done=True)) == 2
    assert ops.remove_reminder(r["id"]) and not ops.remove_reminder(r["id"])


def test_lembrete_invalido(ops):
    with pytest.raises(ValueError, match="título"):
        ops.add_reminder("  ", "2026-10-03T11:00:00")
    with pytest.raises(ValueError, match="ISO"):
        ops.add_reminder("x", "semana que vem")
    with pytest.raises(KeyError):
        ops.complete_reminder(999)


# ── agendamentos ──────────────────────────────────────────────────────────
def test_agendamento_diario_dispara_e_recalcula(ops):
    s = ops.add_schedule("Briefing", "diario", time_of_day="14:00", tool="checar_saude_sistema")
    assert s["next_run"] == em(2026, 10, 3, 14) and s["params"] == "{}"
    assert ops.due_schedules() == []
    depois = em(2026, 10, 3, 14, 5)
    assert [x["title"] for x in ops.due_schedules(depois)] == ["Briefing"]
    feito = ops.mark_schedule_fired(s["id"], depois)
    assert feito["last_run"] == depois and feito["next_run"] == em(2026, 10, 4, 14)
    # app ficou três dias desligado: dispara uma vez e segue do horário seguinte (sem repetição)
    muito_depois = em(2026, 10, 7, 20)
    assert len(ops.due_schedules(muito_depois)) == 1
    assert ops.mark_schedule_fired(s["id"], muito_depois)["next_run"] == em(2026, 10, 8, 14)


def test_agendamento_unico_desativa_depois_de_disparar(ops):
    s = ops.add_schedule("Uma vez", "unico", when="2026-10-03T12:30:00")
    ops.mark_schedule_fired(s["id"], em(2026, 10, 3, 12, 31))
    final = ops.list_schedules()[0]
    assert final["active"] == 0 and final["next_run"] is None
    assert ops.due_schedules(em(2030, 1, 1)) == []


def test_agendamento_pausar_e_retomar(ops):
    s = ops.add_schedule("A cada meia hora", "intervalo_min", interval_min=30)
    assert s["next_run"] == AGORA + 1800
    assert ops.set_schedule_active(s["id"], False)["active"] == 0
    assert ops.due_schedules(AGORA + 99_999) == []
    ops._s._clock = lambda: AGORA + 7200  # 2h depois, retoma: não dispara o atraso, recalcula
    retomado = ops.set_schedule_active(s["id"], True)
    assert retomado["active"] == 1 and retomado["next_run"] == AGORA + 7200 + 1800


def test_agendamento_guarda_parametros_como_json_e_valida(ops):
    s = ops.add_schedule(
        "Clima", "diario", time_of_day="07:30", tool="consultar_clima", params={"cidade": "Marília"}
    )
    assert s["params"] == '{"cidade": "Marília"}'
    with pytest.raises(ValueError):
        ops.add_schedule("", "diario", time_of_day="07:30")
    with pytest.raises(ValueError):
        ops.add_schedule("sem horário", "diario")
    assert ops.remove_schedule(s["id"])


# ── tarefas, objetivos, números, prompts ──────────────────────────────────
def test_tarefas_e_objetivos_em_aberto(ops):
    a, b = ops.add_task("Estudar UML"), ops.add_task("Entregar trabalho")
    ops.add_reminder("Pagar boleto", "2026-10-04T09:00:00")
    assert ops.set_task_status(a["id"], "concluida")["status"] == "concluida"
    assert [t["title"] for t in ops.list_tasks("pendente")] == ["Entregar trabalho"]
    assert ops.open_goals() == [
        "tarefa (pendente): Entregar trabalho",
        "lembrete para 04/10 09:00: Pagar boleto",
    ]
    assert len(ops.open_goals(limit=1)) == 1
    with pytest.raises(ValueError, match="status"):
        ops.set_task_status(b["id"], "feito")
    with pytest.raises(ValueError, match="título"):
        ops.add_task(" ")
    assert ops.remove_task(b["id"])


def test_numeros_limitam_score_e_filtram_pendentes(ops):
    alto = ops.add_number("fatura", 5, "vence amanhã")
    ops.add_number("log", 0.2)
    assert alto["score"] == 1.0
    assert [n["target"] for n in ops.list_numbers()] == ["fatura"]
    ops.mark_number_notified(alto["id"])
    assert ops.list_numbers() == []
    assert len(ops.list_numbers(only_pending=False)) == 2
    with pytest.raises(ValueError):
        ops.add_number(" ", 0.5)


def test_prompts_salvos(ops):
    p = ops.add_prompt("Resumo", "Resuma em 5 linhas", command="resumo")
    assert ops.list_prompts()[0]["command"] == "resumo"
    assert ops.remove_prompt(p["id"]) and ops.list_prompts() == []
    with pytest.raises(ValueError):
        ops.add_prompt("sem conteúdo", " ")


# ── grafo e fila de avisos ────────────────────────────────────────────────
def test_grafo_nao_duplica_aresta_e_lista_os_dois_sentidos(ops):
    assert ops.add_edge("message:1", "sobre", "topic:unimar") is True
    assert ops.add_edge("message:1", "sobre", "topic:unimar") is False
    ops.add_edge("topic:unimar", "conecta", "topic:ads", weight=2, kind="rem")
    assert ops.edge_count() == 2
    vizinhos = ops.neighbors("topic:unimar")
    assert {(n["direction"], n["rel"]) for n in vizinhos} == {("in", "sobre"), ("out", "conecta")}
    assert [n["dst"] for n in ops.neighbors("topic:unimar", rel="conecta")] == ["topic:ads"]
    assert ops.neighbors("topic:inexistente") == []


def test_fila_de_avisos_entrega_em_ordem_e_confirma_uma_vez(ops):
    a = ops.notify("lembrete", "Pagar boleto", ref="reminder:1")
    b = ops.notify("agendamento", "Briefing")
    assert [n["id"] for n in ops.pending_notifications()] == [a, b]
    assert ops.ack_notification(a) is True
    assert ops.ack_notification(a) is False
    assert [n["text"] for n in ops.pending_notifications()] == ["Briefing"]


# ── migração do esquema ───────────────────────────────────────────────────
def _banco_v1(caminho):
    c = sqlite3.connect(caminho)
    c.executescript(DDL_V1)
    c.execute("INSERT INTO meta VALUES ('schema_version', '1')")
    c.execute("INSERT INTO sessions VALUES ('s1', 'web', 'antiga', 1.0, 1.0, 0)")
    c.execute(
        "INSERT INTO messages(session_id, role, text, created_at) VALUES ('s1','user','oi',1.0)"
    )
    c.commit()
    c.close()


def test_banco_v1_sobe_para_a_versao_atual_sem_perder_dados(tmp_path):
    caminho = tmp_path / "v1.db"
    _banco_v1(caminho)
    store = MemoryStore(caminho)
    try:
        assert store.query("SELECT value FROM meta WHERE key='schema_version'")[0][0] == str(
            SCHEMA_VERSION
        )
        assert [m.text for m in store.history("s1")] == ["oi"]
        ops = Operations(store)
        ops.add_task("funciona depois da migração")
        assert store.search("oi", kinds=["message"])[0].text == "oi"  # FTS segue de pé
    finally:
        store.close()
    reaberto = MemoryStore(caminho)  # idempotente: abrir de novo não migra de novo
    reaberto.close()


def test_backup_v1_e_aceito_e_banco_de_versao_futura_e_recusado(tmp_path):
    antigo = tmp_path / "antigo.db"
    _banco_v1(antigo)
    contagens = MemoryStore.verify_backup(antigo)
    assert contagens["messages"] == 1 and "reminders" not in contagens  # v1: sem tabelas novas
    restaurado = MemoryStore.restore(antigo, tmp_path / "novo" / "orion.db")
    try:
        assert restaurado.query("SELECT value FROM meta WHERE key='schema_version'")[0][0] == str(
            SCHEMA_VERSION
        )
    finally:
        restaurado.close()

    futuro = tmp_path / "futuro.db"
    _banco_v1(futuro)
    c = sqlite3.connect(futuro)
    c.execute("UPDATE meta SET value='99' WHERE key='schema_version'")
    c.commit()
    c.close()
    with pytest.raises(RuntimeError, match="só entende até"):
        MemoryStore(futuro)


# ── trilha de auditoria (esquema v3) ──────────────────────────────────────────
def test_banco_v2_sobe_para_v3_e_ganha_a_tabela_audit(tmp_path):
    caminho = tmp_path / "v2.db"
    c = sqlite3.connect(caminho)
    c.executescript(DDL_V1 + DDL_V2)
    c.execute("INSERT INTO meta VALUES ('schema_version', '2')")
    c.commit()
    c.close()
    store = MemoryStore(caminho)
    try:
        assert store.query("SELECT value FROM meta WHERE key='schema_version'")[0][0] == str(
            SCHEMA_VERSION
        )
        assert Operations(store).audit_recent() == []
    finally:
        store.close()


def test_audit_grava_filtra_e_poda(ops):
    ops.audit_add({"session_id": "s1", "tool": "executar_comando", "action": "confirm",
                   "risk": "exec", "reason": "fora da lista", "tainted": True,
                   "args": {"cmd": "rm -rf x"}})  # fmt: skip
    ops.audit_add({"session_id": "s1", "tool": "ler_arquivo", "action": "allow", "risk": "read"})
    todos = ops.audit_recent()
    assert [a["tool"] for a in todos] == ["ler_arquivo", "executar_comando"]  # mais novo primeiro
    assert todos[1]["args"] == {"cmd": "rm -rf x"} and todos[1]["tainted"] == 1
    assert [a["tool"] for a in ops.audit_recent(ferramenta="executar_comando")] == [
        "executar_comando"
    ]
    assert len(ops.audit_recent(acao="allow")) == 1
    assert ops.audit_recent(desde=AGORA + 1) == []
    assert ops.audit_prune(dias=1) == 0
    ops._s._clock = lambda: AGORA + 91 * 86400
    assert ops.audit_prune(dias=90) == 2 and ops.audit_recent() == []
