"""Operação do dia a dia sobre o mesmo banco da memória: lembretes, agendamentos,
tarefas, "números", prompts salvos, arestas do grafo e fila de avisos.

Substitui as tabelas `lembrete`, `agendamento`, `tarefa`, `numero`, `prompt` e as
relações (`RELATE`) do SurrealDB do legado. Tudo parametrizado; datas em segundos
desde a época (o texto ISO, quando vem de fora, é lido como hora local).
"""

from __future__ import annotations

import json
from datetime import datetime, timedelta
from typing import Any

from .store import MemoryStore

TIPOS_AGENDAMENTO = ("unico", "diario", "intervalo_min")
STATUS_TAREFA = ("pendente", "em_andamento", "concluida")


def parse_when(valor: str | float | int) -> float:
    """ISO (`2026-06-25T14:30:00`) ou epoch → epoch. ISO sem fuso é hora local."""
    if isinstance(valor, int | float):
        return float(valor)
    try:
        return datetime.fromisoformat(str(valor).strip()).timestamp()
    except ValueError:
        raise ValueError(
            f"data/hora inválida: {valor!r} (use ISO, ex.: 2026-06-25T14:30:00)"
        ) from None


def next_run(
    kind: str,
    base: float,
    *,
    when: float | None = None,
    time_of_day: str = "",
    interval_min: int = 0,
) -> float:
    """Próximo disparo de um agendamento, a partir de `base` (mesma regra do legado)."""
    if kind == "unico":
        if when is None:
            raise ValueError("tipo 'unico' exige 'quando' (ISO)")
        return when
    if kind == "diario":
        try:
            h, m = (int(x) for x in time_of_day.split(":"))
            prox = datetime.fromtimestamp(base).replace(hour=h, minute=m, second=0, microsecond=0)
        except ValueError:
            raise ValueError("tipo 'diario' exige 'horario' no formato HH:MM") from None
        if prox.timestamp() <= base:
            prox += timedelta(days=1)
        return prox.timestamp()
    if kind == "intervalo_min":
        if interval_min <= 0:
            raise ValueError("tipo 'intervalo_min' exige 'intervalo_min' > 0")
        return base + interval_min * 60
    raise ValueError(f"tipo inválido: {kind}. Use: {', '.join(TIPOS_AGENDAMENTO)}")


_NOME = {
    "reminders": "lembrete",
    "schedules": "agendamento",
    "tasks": "tarefa",
    "numbers": "número",
    "prompts": "prompt",
}


def _dicts(rows: list[Any]) -> list[dict[str, Any]]:
    return [dict(r) for r in rows]


class Operations:
    def __init__(self, store: MemoryStore) -> None:
        self._s = store

    def _now(self) -> float:
        return self._s.clock()

    # ── lembretes (aviso pontual, com hora) ───────────────────────────────
    def add_reminder(self, title: str, due_at: str | float, note: str = "") -> dict[str, Any]:
        titulo = title.strip()
        if not titulo:
            raise ValueError("informe o título do lembrete")
        with self._s.transaction() as c:
            rid = c.execute(
                "INSERT INTO reminders(title, due_at, note, created_at) VALUES (?,?,?,?)",
                (titulo, parse_when(due_at), note, self._now()),
            ).lastrowid
        return self._one("reminders", int(rid or 0))

    def list_reminders(self, include_done: bool = False) -> list[dict[str, Any]]:
        return _dicts(
            self._s.query(
                "SELECT * FROM reminders WHERE (? OR done=0) ORDER BY due_at, id",
                (1 if include_done else 0,),
            )
        )

    def due_reminders(self, now: float | None = None) -> list[dict[str, Any]]:
        """Vencidos, não concluídos e ainda não avisados."""
        return _dicts(
            self._s.query(
                "SELECT * FROM reminders WHERE done=0 AND notified=0 AND due_at<=? ORDER BY due_at",
                (self._now() if now is None else now,),
            )
        )

    def complete_reminder(self, reminder_id: int) -> dict[str, Any]:
        self._update("reminders", reminder_id, "done=1")
        return self._one("reminders", reminder_id)

    def mark_reminder_notified(self, reminder_id: int) -> None:
        self._update("reminders", reminder_id, "notified=1")

    def remove_reminder(self, reminder_id: int) -> bool:
        return self._delete("reminders", reminder_id)

    # ── agendamentos recorrentes ──────────────────────────────────────────
    def add_schedule(
        self,
        title: str,
        kind: str = "unico",
        *,
        when: str | float | None = None,
        time_of_day: str = "",
        interval_min: int = 0,
        tool: str = "",
        params: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        titulo = title.strip()
        if not titulo:
            raise ValueError("informe o título do agendamento")
        agora = self._now()
        quando = parse_when(when) if when not in (None, "") else None
        prox = next_run(
            kind, agora, when=quando, time_of_day=time_of_day, interval_min=interval_min
        )
        with self._s.transaction() as c:
            sid = c.execute(
                "INSERT INTO schedules(title, kind, time_of_day, interval_min, tool, params,"
                " next_run, created_at) VALUES (?,?,?,?,?,?,?,?)",
                (
                    titulo,
                    kind,
                    time_of_day,
                    interval_min,
                    tool,
                    json.dumps(params or {}, ensure_ascii=False),
                    prox,
                    agora,
                ),
            ).lastrowid
        return self._one("schedules", int(sid or 0))

    def list_schedules(self) -> list[dict[str, Any]]:
        return _dicts(self._s.query("SELECT * FROM schedules ORDER BY next_run IS NULL, next_run"))

    def due_schedules(self, now: float | None = None) -> list[dict[str, Any]]:
        return _dicts(
            self._s.query(
                "SELECT * FROM schedules WHERE active=1 AND next_run IS NOT NULL AND next_run<=?"
                " ORDER BY next_run",
                (self._now() if now is None else now,),
            )
        )

    def mark_schedule_fired(self, schedule_id: int, now: float | None = None) -> dict[str, Any]:
        """Registra o disparo e calcula o próximo (um atraso longo dispara uma vez só)."""
        agora = self._now() if now is None else now
        s = self._one("schedules", schedule_id)
        if s["kind"] == "unico":
            sets, params = "last_run=?, next_run=NULL, active=0", (agora,)
        else:
            prox = next_run(
                s["kind"], agora, time_of_day=s["time_of_day"], interval_min=s["interval_min"]
            )
            sets, params = "last_run=?, next_run=?", (agora, prox)
        self._update("schedules", schedule_id, sets, params)
        return self._one("schedules", schedule_id)

    def set_schedule_active(self, schedule_id: int, active: bool) -> dict[str, Any]:
        """Pausar não perde o horário; retomar recalcula o próximo a partir de agora."""
        s = self._one("schedules", schedule_id)
        if active and s["kind"] != "unico":
            prox = next_run(
                s["kind"], self._now(), time_of_day=s["time_of_day"], interval_min=s["interval_min"]
            )
            self._update("schedules", schedule_id, "active=1, next_run=?", (prox,))
        else:
            self._update("schedules", schedule_id, "active=?", (1 if active else 0,))
        return self._one("schedules", schedule_id)

    def remove_schedule(self, schedule_id: int) -> bool:
        return self._delete("schedules", schedule_id)

    # ── tarefas (to-do) ───────────────────────────────────────────────────
    def add_task(self, title: str) -> dict[str, Any]:
        titulo = title.strip()
        if not titulo:
            raise ValueError("informe o título da tarefa")
        agora = self._now()
        with self._s.transaction() as c:
            tid = c.execute(
                "INSERT INTO tasks(title, created_at, updated_at) VALUES (?,?,?)",
                (titulo, agora, agora),
            ).lastrowid
        return self._one("tasks", int(tid or 0))

    def list_tasks(self, status: str | None = None) -> list[dict[str, Any]]:
        return _dicts(
            self._s.query(
                "SELECT * FROM tasks WHERE (? IS NULL OR status=?) ORDER BY created_at, id",
                (status, status),
            )
        )

    def set_task_status(self, task_id: int, status: str) -> dict[str, Any]:
        if status not in STATUS_TAREFA:
            raise ValueError(f"status inválido. Use: {', '.join(STATUS_TAREFA)}")
        self._update("tasks", task_id, "status=?, updated_at=?", (status, self._now()))
        return self._one("tasks", task_id)

    def remove_task(self, task_id: int) -> bool:
        return self._delete("tasks", task_id)

    def open_goals(self, limit: int = 8) -> list[str]:
        """Objetivos em aberto para o briefing (Goal Drift): tarefas não concluídas e
        lembretes pendentes, os mais antigos primeiro."""
        tarefas = self._s.query(
            "SELECT title, status FROM tasks WHERE status<>'concluida' ORDER BY created_at, id"
        )
        lembretes = self._s.query(
            "SELECT title, due_at FROM reminders WHERE done=0 ORDER BY due_at, id"
        )
        linhas = [f"tarefa ({t['status']}): {t['title']}" for t in tarefas]
        for r in lembretes:
            quando = datetime.fromtimestamp(r["due_at"]).strftime("%d/%m %H:%M")
            linhas.append(f"lembrete para {quando}: {r['title']}")
        return linhas[:limit]

    # ── "números": observações com relevância, para avisar sem interromper ─
    def add_number(
        self, target: str, score: float, reason: str = "", source: str = "orion"
    ) -> dict[str, Any]:
        alvo = target.strip()
        if not alvo:
            raise ValueError("informe o alvo da observação")
        with self._s.transaction() as c:
            nid = c.execute(
                "INSERT INTO numbers(target, score, reason, source, created_at) VALUES (?,?,?,?,?)",
                (alvo, max(0.0, min(1.0, float(score))), reason, source, self._now()),
            ).lastrowid
        return self._one("numbers", int(nid or 0))

    def list_numbers(
        self, only_pending: bool = True, min_score: float = 0.7
    ) -> list[dict[str, Any]]:
        if only_pending:
            return _dicts(
                self._s.query(
                    "SELECT * FROM numbers WHERE notified=0 AND score>=? ORDER BY score DESC, id",
                    (float(min_score),),
                )
            )
        return _dicts(
            self._s.query("SELECT * FROM numbers ORDER BY created_at DESC, id DESC LIMIT 50")
        )

    def mark_number_notified(self, number_id: int) -> None:
        self._update("numbers", number_id, "notified=1")

    # ── prompts salvos ────────────────────────────────────────────────────
    def add_prompt(self, title: str, content: str, command: str = "") -> dict[str, Any]:
        if not title.strip() or not content.strip():
            raise ValueError("informe título e conteúdo do prompt")
        with self._s.transaction() as c:
            pid = c.execute(
                "INSERT INTO prompts(title, command, content, created_at) VALUES (?,?,?,?)",
                (title.strip(), command.strip(), content, self._now()),
            ).lastrowid
        return self._one("prompts", int(pid or 0))

    def list_prompts(self) -> list[dict[str, Any]]:
        return _dicts(self._s.query("SELECT * FROM prompts ORDER BY created_at DESC, id DESC"))

    def remove_prompt(self, prompt_id: int) -> bool:
        return self._delete("prompts", prompt_id)

    # ── grafo ─────────────────────────────────────────────────────────────
    def add_edge(self, src: str, rel: str, dst: str, weight: float = 1.0, kind: str = "") -> bool:
        """Cria a aresta; repetida (mesmo origem, relação e destino) não duplica."""
        with self._s.transaction() as c:
            return (
                c.execute(
                    "INSERT OR IGNORE INTO edges(src, rel, dst, weight, kind, created_at)"
                    " VALUES (?,?,?,?,?,?)",
                    (src, rel, dst, weight, kind, self._now()),
                ).rowcount
                > 0
            )

    def neighbors(self, node: str, rel: str | None = None, limit: int = 50) -> list[dict[str, Any]]:
        """Arestas que saem ou chegam em `node` (direção em `direction`)."""
        rows = self._s.query(
            "SELECT src, rel, dst, weight, kind, 'out' AS direction FROM edges"
            " WHERE src=? AND (? IS NULL OR rel=?)"
            " UNION ALL "
            "SELECT src, rel, dst, weight, kind, 'in' AS direction FROM edges"
            " WHERE dst=? AND (? IS NULL OR rel=?) LIMIT ?",
            (node, rel, rel, node, rel, rel, limit),
        )
        return _dicts(rows)

    def edge_count(self) -> int:
        return int(self._s.query("SELECT COUNT(*) FROM edges")[0][0])

    # ── fila de avisos ────────────────────────────────────────────────────
    def notify(self, kind: str, text: str, ref: str | None = None) -> int:
        with self._s.transaction() as c:
            return int(
                c.execute(
                    "INSERT INTO notifications(kind, ref, text, created_at) VALUES (?,?,?,?)",
                    (kind, ref, text, self._now()),
                ).lastrowid
                or 0
            )

    def pending_notifications(self, limit: int = 50) -> list[dict[str, Any]]:
        return _dicts(
            self._s.query(
                "SELECT * FROM notifications WHERE delivered_at IS NULL ORDER BY id LIMIT ?",
                (limit,),
            )
        )

    def ack_notification(self, notification_id: int) -> bool:
        with self._s.transaction() as c:
            return (
                c.execute(
                    "UPDATE notifications SET delivered_at=? WHERE id=? AND delivered_at IS NULL",
                    (self._now(), notification_id),
                ).rowcount
                > 0
            )

    # ── trilha de auditoria (decisões da política; args já redigidos) ──────
    def audit_add(self, evento: dict[str, Any]) -> int:
        """Grava uma decisão da política. Erro propaga: a política nega o que não for leitura."""
        with self._s.transaction() as c:
            return int(
                c.execute(
                    "INSERT INTO audit(ts, session_id, tool, action, risk, reason, tainted, args)"
                    " VALUES (?,?,?,?,?,?,?,?)",
                    (
                        self._now(),
                        evento.get("session_id"),
                        str(evento.get("tool", "")),
                        str(evento.get("action", "")),
                        evento.get("risk"),
                        str(evento.get("reason") or ""),
                        1 if evento.get("tainted") else 0,
                        json.dumps(evento.get("args", {}), ensure_ascii=False, default=str),
                    ),
                ).lastrowid
                or 0
            )

    def audit_recent(
        self,
        limite: int = 50,
        *,
        ferramenta: str | None = None,
        acao: str | None = None,
        desde: float | None = None,
    ) -> list[dict[str, Any]]:
        """Decisões mais recentes primeiro; `args` volta como objeto."""
        filtros, params = [], []
        for coluna, valor in (("tool", ferramenta), ("action", acao)):
            if valor:
                filtros.append(f"{coluna}=?")
                params.append(valor)
        if desde is not None:
            filtros.append("ts>=?")
            params.append(desde)
        onde = f" WHERE {' AND '.join(filtros)}" if filtros else ""
        rows = self._s.query(
            f"SELECT * FROM audit{onde} ORDER BY id DESC LIMIT ?",
            (*params, max(1, min(int(limite), 500))),
        )
        saida = []
        for r in rows:
            d = dict(r)
            try:
                d["args"] = json.loads(d["args"])
            except ValueError:
                d["args"] = {}
            saida.append(d)
        return saida

    def audit_prune(self, dias: int = 90) -> int:
        """Apaga decisões mais velhas que `dias`; devolve quantas."""
        with self._s.transaction() as c:
            return c.execute(
                "DELETE FROM audit WHERE ts < ?", (self._now() - dias * 86400,)
            ).rowcount

    # ── internos (tabela e colunas são constantes deste módulo, nunca entrada) ──
    def _one(self, tabela: str, rid: int) -> dict[str, Any]:
        rows = self._s.query(f"SELECT * FROM {tabela} WHERE id=?", (rid,))
        if not rows:
            raise KeyError(f"{_NOME.get(tabela, tabela)} {rid} não existe")
        return dict(rows[0])

    def _update(self, tabela: str, rid: int, sets: str, params: tuple[Any, ...] = ()) -> None:
        with self._s.transaction() as c:
            if c.execute(f"UPDATE {tabela} SET {sets} WHERE id=?", (*params, rid)).rowcount == 0:
                raise KeyError(f"{_NOME.get(tabela, tabela)} {rid} não existe")

    def _delete(self, tabela: str, rid: int) -> bool:
        with self._s.transaction() as c:
            return c.execute(f"DELETE FROM {tabela} WHERE id=?", (rid,)).rowcount > 0
