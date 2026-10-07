"""Briefing matinal: o que o Antônio tem para hoje, montado só com o que o Orion já guarda.

Texto determinístico, sem modelo: lembretes (atrasados e de hoje), agendamentos de hoje e tarefas
em aberto. Por isso roda sem gateway, não gasta cota e não tem como ser desviado por conteúdo
externo (nada de e-mail ou página entra aqui). A **agenda** do Google entra só se `orion.agenda`
estiver ligada (regra 37): uma consulta de leitura direta ao servidor MCP, sem modelo, com o texto
limpo de endereços e limitado. Sem ela o briefing não finge que consultou.

Quem dispara é o job (`orion.jobs`, `ORION_BRIEFING_AT`), que põe o texto na fila de avisos; o
canal Telegram entrega. O comando `/briefing` do Telegram monta o mesmo texto na hora.
"""

from __future__ import annotations

from collections.abc import Callable
from datetime import datetime, timedelta

from .memory.ops import Operations

_DIAS = [
    "segunda-feira",
    "terça-feira",
    "quarta-feira",
    "quinta-feira",
    "sexta-feira",
    "sábado",
    "domingo",
]
MAX_LINHAS = 8  # por seção; o resto vira "e mais N"


def _secao(titulo: str, linhas: list[str]) -> str:
    if not linhas:
        return ""
    corpo = linhas[:MAX_LINHAS]
    if len(linhas) > MAX_LINHAS:
        corpo.append(f"… e mais {len(linhas) - MAX_LINHAS}")
    return f"{titulo}\n" + "\n".join(f"• {ln}" for ln in corpo)


def build_briefing(
    ops: Operations,
    agora: float,
    nome: str = "Antônio",
    agenda: Callable[[float], str | None] | None = None,
) -> str:
    dt = datetime.fromtimestamp(agora)
    inicio = dt.replace(hour=0, minute=0, second=0, microsecond=0)
    fim = inicio + timedelta(days=1)

    lembretes = [r for r in ops.list_reminders() if r["due_at"] < fim.timestamp()]
    atrasados = [
        f"{datetime.fromtimestamp(r['due_at']):%d/%m %H:%M} — {r['title']}"
        for r in lembretes
        if r["due_at"] < inicio.timestamp()
    ]
    de_hoje = [
        f"{datetime.fromtimestamp(r['due_at']):%H:%M} — {r['title']}"
        for r in lembretes
        if r["due_at"] >= inicio.timestamp()
    ]
    agendados = [
        f"{datetime.fromtimestamp(s['next_run']):%H:%M} — {s['title']}"
        for s in ops.list_schedules()
        if s["active"] and s["next_run"] is not None and agora <= s["next_run"] < fim.timestamp()
    ]
    tarefas = sorted(
        (t for t in ops.list_tasks() if t["status"] != "concluida"),
        key=lambda t: t["status"] != "em_andamento",  # o que está em andamento vem primeiro
    )
    linhas_tarefas = [
        f"{t['title']}" + (" (em andamento)" if t["status"] == "em_andamento" else "")
        for t in tarefas
    ]

    de_agenda = agenda(agora) if agenda is not None else None
    partes = [
        f"☀️ Bom dia, {nome}. Hoje é {_DIAS[dt.weekday()]}, {dt:%d/%m/%Y}.",
        f"📅 Agenda de hoje (Google)\n{de_agenda}" if de_agenda else "",
        _secao("⏰ Atrasados", atrasados),
        _secao("🔔 Lembretes de hoje", de_hoje),
        _secao("🗓️ Agendamentos de hoje", agendados),
        _secao("✅ Tarefas em aberto", linhas_tarefas),
    ]
    corpo = [p for p in partes if p]
    if len(corpo) == 1:
        corpo.append("Nada pendente por aqui.")
    return "\n\n".join(corpo)
