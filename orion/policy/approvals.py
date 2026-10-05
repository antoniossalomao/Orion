"""Aprovações fora de banda (regra 2).

A confirmação NÃO vem de uma frase no chat: o motor pede, e só um canal
autenticado (botão do Telegram, tela web com login) chama `decide`. A
aprovação fica ligada a (sessão, ferramenta, hash dos argumentos), vale por um
tempo e é consumida uma única vez. O caminho das ferramentas nunca recebe
`decide`, então o LLM não consegue se auto-aprovar.

`pending()` também serve de fila ("inbox"): ações propostas enquanto o Antônio
está offline ficam esperando a decisão dele.
"""

from __future__ import annotations

import hashlib
import json
import secrets
import threading
import time
from collections.abc import Callable
from dataclasses import dataclass, field, replace
from enum import StrEnum
from typing import Any


class Status(StrEnum):
    PENDING = "pending"
    APPROVED = "approved"
    DENIED = "denied"
    CONSUMED = "consumed"
    EXPIRED = "expired"


def hash_call(tool: str, args: dict[str, Any]) -> str:
    """Hash canônico (sha256 completo) de (ferramenta, argumentos)."""
    payload = json.dumps(
        {"tool": tool, "args": args}, sort_keys=True, ensure_ascii=False, default=str
    )
    return hashlib.sha256(payload.encode()).hexdigest()


@dataclass(frozen=True)
class Approval:
    id: str
    session_id: str
    tool: str
    args_hash: str
    reason: str
    created_at: float
    expires_at: float
    args: dict[str, Any] = field(default_factory=dict)  # o que o Antônio está aprovando
    status: Status = Status.PENDING
    decided_by: str | None = None
    channel: str | None = None


class ApprovalStore:
    def __init__(self, ttl_s: int = 600, clock: Callable[[], float] = time.time) -> None:
        self._ttl = ttl_s
        self._clock = clock
        self._items: dict[str, Approval] = {}
        self._lock = threading.Lock()

    def _expire(self, a: Approval) -> Approval:
        if a.status in (Status.PENDING, Status.APPROVED) and self._clock() >= a.expires_at:
            a = replace(a, status=Status.EXPIRED)
            self._items[a.id] = a
        return a

    def request(self, session_id: str, tool: str, args: dict[str, Any], reason: str) -> Approval:
        """Cria (ou devolve a pendente idêntica) uma solicitação de aprovação."""
        h = hash_call(tool, args)
        with self._lock:
            for a in list(self._items.values()):
                a = self._expire(a)
                if (a.session_id, a.args_hash, a.status) == (session_id, h, Status.PENDING):
                    return a
            agora = self._clock()
            novo = Approval(
                id=secrets.token_urlsafe(8),
                session_id=session_id,
                tool=tool,
                args_hash=h,
                reason=reason,
                created_at=agora,
                expires_at=agora + self._ttl,
                args=json.loads(json.dumps(args, default=str)),
            )
            self._items[novo.id] = novo
            return novo

    def decide(self, approval_id: str, approved: bool, *, channel: str, actor: str) -> Approval:
        """Registra a decisão. Chamado só por canais autenticados."""
        with self._lock:
            a = self._items.get(approval_id)
            if a is None:
                raise KeyError(approval_id)
            a = self._expire(a)
            if a.status is not Status.PENDING:
                raise ValueError(f"aprovação {approval_id} não está pendente ({a.status.value})")
            a = replace(
                a,
                status=Status.APPROVED if approved else Status.DENIED,
                decided_by=actor,
                channel=channel,
            )
            self._items[a.id] = a
            return a

    def consume(self, session_id: str, tool: str, args: dict[str, Any]) -> bool:
        """True uma única vez se há aprovação válida para exatamente esta chamada."""
        h = hash_call(tool, args)
        with self._lock:
            for a in list(self._items.values()):
                a = self._expire(a)
                if (a.session_id, a.args_hash, a.status) == (session_id, h, Status.APPROVED):
                    self._items[a.id] = replace(a, status=Status.CONSUMED)
                    return True
        return False

    def pending(self, session_id: str | None = None) -> list[Approval]:
        with self._lock:
            vivos = [self._expire(a) for a in list(self._items.values())]
        return sorted(
            (
                a
                for a in vivos
                if a.status is Status.PENDING and (session_id is None or a.session_id == session_id)
            ),
            key=lambda a: a.created_at,
        )

    def unresolved(self, session_id: str) -> bool:
        """Pendentes ou aprovadas ainda não consumidas não podem desaparecer do contexto."""
        with self._lock:
            return any(
                self._expire(a).status in (Status.PENDING, Status.APPROVED)
                for a in list(self._items.values())
                if a.session_id == session_id
            )

    def invalidate_tools(self, names: set[str]) -> None:
        with self._lock:
            for a in list(self._items.values()):
                if a.tool in names and a.status in (Status.PENDING, Status.APPROVED):
                    self._items[a.id] = replace(
                        a, status=Status.DENIED, reason="origem ou revisão revogada"
                    )

    def get(self, approval_id: str) -> Approval | None:
        with self._lock:
            a = self._items.get(approval_id)
            return self._expire(a) if a else None
