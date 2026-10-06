"""Ferramentas de operação (lembretes, agendamentos, tarefas, números) sobre `orion.memory.ops`.

Mantêm os nomes e os argumentos do legado (contrato de function-calling em PT); a diferença
é que os ids agora são inteiros e o armazenamento é o SQLite da memória. Todas já têm classe
de risco em `orion.policy.classes` (escrita com log; listagens, leitura).
"""

from __future__ import annotations

import json
from datetime import datetime
from typing import Any

from ..memory.ops import Operations
from .registry import Tool


def _erro(e: Exception) -> dict[str, Any]:
    return {"erro": e.args[0] if e.args else str(e), "ok": False}


def ops_tools(ops: Operations) -> list[Tool]:
    def gerenciar_lembretes(
        acao: str, titulo: str = "", quando: str = "", lembrete_id: int = 0, nota: str = ""
    ) -> dict[str, Any]:
        try:
            if acao == "criar":
                return {"ok": True, "lembrete": ops.add_reminder(titulo, quando, nota)}
            if acao == "listar":
                lista = ops.list_reminders(include_done=True)
                return {"ok": True, "total": len(lista), "lembretes": lista}
            if acao == "pendentes":
                lista = ops.due_reminders()
                return {"ok": True, "total": len(lista), "lembretes": lista}
            if acao == "concluir":
                return {"ok": True, "lembrete": ops.complete_reminder(int(lembrete_id))}
            if acao == "remover":
                return {"ok": ops.remove_reminder(int(lembrete_id)), "removido": lembrete_id}
        except (ValueError, KeyError) as e:
            return _erro(e)
        return {"erro": f"Ação inválida: {acao}. Use: criar, listar, pendentes, concluir, remover."}

    def gerenciar_agendamentos(
        acao: str,
        titulo: str = "",
        tipo: str = "unico",
        quando: str = "",
        horario: str = "",
        intervalo_min: int = 0,
        ferramenta: str = "",
        parametros: str = "{}",
        agendamento_id: int = 0,
    ) -> dict[str, Any]:
        try:
            if acao == "criar":
                try:
                    params = json.loads(parametros or "{}")
                except json.JSONDecodeError:
                    return {"erro": "'parametros' precisa ser um JSON válido", "ok": False}
                if not isinstance(params, dict):
                    return {"erro": "'parametros' precisa ser um objeto JSON", "ok": False}
                s = ops.add_schedule(
                    titulo,
                    tipo,
                    when=quando or None,
                    time_of_day=horario,
                    interval_min=int(intervalo_min),
                    tool=ferramenta,
                    params=params,
                )
                return {"ok": True, "agendamento": s}
            if acao == "listar":
                lista = ops.list_schedules()
                return {"ok": True, "total": len(lista), "agendamentos": lista}
            if acao in ("pausar", "retomar"):
                s = ops.set_schedule_active(int(agendamento_id), acao == "retomar")
                return {"ok": True, "agendamento": s}
            if acao == "remover":
                return {"ok": ops.remove_schedule(int(agendamento_id)), "removido": agendamento_id}
        except (ValueError, KeyError) as e:
            return _erro(e)
        return {"erro": f"Ação inválida: {acao}. Use: criar, listar, pausar, retomar, remover."}

    def gerenciar_tarefas(
        acao: str, titulo: str = "", tarefa_id: int = 0, status: str = "pendente"
    ) -> dict[str, Any]:
        try:
            if acao == "criar":
                return {"ok": True, "tarefa": ops.add_task(titulo)}
            if acao == "listar":
                lista = ops.list_tasks()
                return {"ok": True, "total": len(lista), "tarefas": lista}
            if acao == "atualizar":
                return {"ok": True, "tarefa": ops.set_task_status(int(tarefa_id), status)}
            if acao == "remover":
                return {"ok": ops.remove_task(int(tarefa_id)), "removido": tarefa_id}
        except (ValueError, KeyError) as e:
            return _erro(e)
        return {"erro": f"Ação inválida: {acao}. Use: criar, listar, atualizar, remover."}

    def registrar_numero(
        alvo: str, score: float, motivo: str = "", fonte: str = "orion"
    ) -> dict[str, Any]:
        try:
            return {"ok": True, "numero": ops.add_number(alvo, score, motivo, fonte)}
        except ValueError as e:
            return _erro(e)

    def listar_numeros(somente_pendentes: bool = True, limite_score: float = 0.7) -> dict[str, Any]:
        lista = ops.list_numbers(bool(somente_pendentes), float(limite_score))
        return {"ok": True, "total": len(lista), "numeros": lista}

    def consultar_audit_log(
        limite: int = 50, tool_filtro: str = "", apenas_bloqueados: bool = False
    ) -> dict[str, Any]:
        """Decisões recentes da política (o que rodou, o que pediu aprovação, o que foi negado)."""
        regs = ops.audit_recent(
            limite,
            ferramenta=tool_filtro.strip() or None,
            acao="deny" if apenas_bloqueados else None,
        )
        return {
            "ok": True,
            "total": len(regs),
            "decisoes": [
                {
                    "quando": datetime.fromtimestamp(r["ts"]).strftime("%d/%m %H:%M:%S"),
                    "ferramenta": r["tool"],
                    "decisao": r["action"],
                    "risco": r["risk"],
                    "motivo": r["reason"],
                    "contaminada": bool(r["tainted"]),
                    "args": r["args"],
                }
                for r in regs
            ],
        }

    def notificar_celular(
        mensagem: str, titulo: str = "Orion", urgente: bool = False
    ) -> dict[str, Any]:
        """Põe um aviso na fila; o canal do Telegram entrega no celular (sem ntfy.sh)."""
        cabeca = titulo.strip()[:80] or "Orion"
        texto = f"{'❗ ' if urgente else ''}{cabeca}: {mensagem.strip()[:1000]}"
        return {"ok": True, "aviso_id": ops.notify("agente", texto)}

    obj, texto, inteiro = "object", {"type": "string"}, {"type": "integer"}
    return [
        Tool(
            "gerenciar_lembretes",
            "Cria, lista, conclui ou remove lembretes com data/hora (o aviso chega pelo canal).",
            {
                "type": obj,
                "properties": {
                    "acao": {
                        "type": "string",
                        "enum": ["criar", "listar", "pendentes", "concluir", "remover"],
                    },
                    "titulo": texto,
                    "quando": {"type": "string", "description": "ISO, ex.: 2026-06-25T14:30:00"},
                    "lembrete_id": inteiro,
                    "nota": texto,
                },
                "required": ["acao"],
            },
            gerenciar_lembretes,
        ),
        Tool(
            "gerenciar_agendamentos",
            "Cria/lista/pausa/remove agendamentos (único, diário ou a cada N minutos). O disparo "
            "AVISA o Antônio; a ferramenta informada é só registrada, não roda sozinha.",
            {
                "type": obj,
                "properties": {
                    "acao": {
                        "type": "string",
                        "enum": ["criar", "listar", "pausar", "retomar", "remover"],
                    },
                    "titulo": texto,
                    "tipo": {"type": "string", "enum": ["unico", "diario", "intervalo_min"]},
                    "quando": {"type": "string", "description": "ISO, só para tipo 'unico'"},
                    "horario": {"type": "string", "description": "HH:MM, só para tipo 'diario'"},
                    "intervalo_min": inteiro,
                    "ferramenta": texto,
                    "parametros": {"type": "string", "description": "JSON com os argumentos"},
                    "agendamento_id": inteiro,
                },
                "required": ["acao"],
            },
            gerenciar_agendamentos,
        ),
        Tool(
            "gerenciar_tarefas",
            "Lista de tarefas (to-do) para acompanhar pedidos com várias etapas.",
            {
                "type": obj,
                "properties": {
                    "acao": {"type": "string", "enum": ["criar", "listar", "atualizar", "remover"]},
                    "titulo": texto,
                    "tarefa_id": inteiro,
                    "status": {"type": "string", "enum": ["pendente", "em_andamento", "concluida"]},
                },
                "required": ["acao"],
            },
            gerenciar_tarefas,
        ),
        Tool(
            "registrar_numero",
            "Registra uma observação com relevância de 0 a 1, para avisar depois sem interromper.",
            {
                "type": obj,
                "properties": {
                    "alvo": texto,
                    "score": {"type": "number"},
                    "motivo": texto,
                    "fonte": texto,
                },
                "required": ["alvo", "score"],
            },
            registrar_numero,
        ),
        Tool(
            "listar_numeros",
            "Lista as observações registradas (por padrão só as ainda não avisadas e relevantes).",
            {
                "type": obj,
                "properties": {
                    "somente_pendentes": {"type": "boolean"},
                    "limite_score": {"type": "number"},
                },
            },
            listar_numeros,
        ),
        Tool(
            "consultar_audit_log",
            "Mostra as decisões recentes da política de ferramentas (rodou, pediu aprovação, "
            "negou).",
            {
                "type": obj,
                "properties": {
                    "limite": inteiro,
                    "tool_filtro": texto,
                    "apenas_bloqueados": {"type": "boolean"},
                },
            },
            consultar_audit_log,
        ),
        Tool(
            "notificar_celular",
            "Manda um aviso para o celular do Antônio (entregue pelo Telegram).",
            {
                "type": obj,
                "properties": {"mensagem": texto, "titulo": texto, "urgente": {"type": "boolean"}},
                "required": ["mensagem"],
            },
            notificar_celular,
        ),
    ]
