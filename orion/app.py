"""App FastAPI do Orion (fase 1): lifespan, estado injetado, /health e aprovações."""

from __future__ import annotations

import hmac
import logging
import time
import uuid
from collections.abc import AsyncIterator, Callable
from contextlib import asynccontextmanager
from dataclasses import dataclass
from typing import Annotated, Any

from fastapi import Depends, FastAPI, Header, HTTPException, Request
from pydantic import BaseModel
from starlette.middleware.trustedhost import TrustedHostMiddleware

from . import __version__
from .config import PROJECT_ROOT, Settings
from .log import request_id
from .memory import MemoryStore
from .policy import ApprovalStore, PathGuard, PolicyEngine
from .policy.paths import default_safe_roots

audit_log = logging.getLogger("orion.audit")


@dataclass
class AppState:
    """Tudo que os endpoints usam, criado no lifespan e injetado por `Depends`."""

    settings: Settings
    memory: MemoryStore
    policy: PolicyEngine
    started_at: float


def build_policy(settings: Settings, approvals: ApprovalStore | None = None) -> PolicyEngine:
    guard = PathGuard(
        protected_roots=(PROJECT_ROOT,),
        safe_roots=(*default_safe_roots(), *settings.extra_safe_roots),
    )
    return PolicyEngine(
        path_guard=guard,
        approvals=approvals or ApprovalStore(ttl_s=settings.approval_ttl_s),
        audit=lambda evento: audit_log.info("tool_decision", extra={"audit": evento}),
    )


def get_state(request: Request) -> AppState:
    return request.app.state.orion


State = Annotated[AppState, Depends(get_state)]


def require_admin(state: State, authorization: Annotated[str | None, Header()] = None) -> None:
    """Token de admin até o login da fase 5. Não é enviado por navegador de
    outra origem sem preflight (CORS está desligado), então também barra CSRF."""
    esperado = state.settings.admin_token
    if not esperado:
        raise HTTPException(503, "admin_token não configurado (ORION_ADMIN_TOKEN)")
    enviado = (authorization or "").removeprefix("Bearer ").strip()
    if not hmac.compare_digest(enviado.encode(), esperado.encode()):
        raise HTTPException(401, "token inválido")


Admin = Depends(require_admin)


class Decisao(BaseModel):
    approved: bool
    actor: str = "antonio"
    channel: str = "web"


def create_app(
    settings: Settings | None = None,
    memory_factory: Callable[[Settings], MemoryStore] | None = None,
) -> FastAPI:
    settings = settings or Settings()

    @asynccontextmanager
    async def lifespan(app: FastAPI) -> AsyncIterator[None]:
        settings.data_dir.mkdir(parents=True, exist_ok=True)
        memory = (memory_factory or (lambda s: MemoryStore(s.db_path)))(settings)
        app.state.orion = AppState(settings, memory, build_policy(settings), time.time())
        try:
            yield
        finally:
            memory.close()

    app = FastAPI(title="Orion", version=__version__, lifespan=lifespan)
    # Host fora da lista (DNS rebinding a partir de uma página web) é recusado.
    app.add_middleware(TrustedHostMiddleware, allowed_hosts=settings.allowed_hosts)

    @app.middleware("http")
    async def com_request_id(request: Request, call_next):
        rid = request.headers.get("x-request-id") or uuid.uuid4().hex[:12]
        token = request_id.set(rid)
        try:
            resposta = await call_next(request)
        finally:
            request_id.reset(token)
        resposta.headers["x-request-id"] = rid
        return resposta

    @app.get("/health")
    def health(state: State) -> dict[str, Any]:
        try:
            memoria = "ok" if state.memory.ping() else "erro"
        except Exception:  # noqa: BLE001 — /health nunca pode levantar
            memoria = "erro"
        return {
            "status": "ok" if memoria == "ok" else "degradado",
            "version": __version__,
            "uptime_s": round(time.time() - state.started_at, 1),
            "components": {"memory": memoria, "vectors": state.memory.vectors_available},
            "pending_approvals": len(state.policy.approvals.pending()),
        }

    @app.get("/approvals", dependencies=[Admin])
    def pendentes(state: State) -> list[dict[str, Any]]:
        return [
            {
                "id": a.id,
                "session_id": a.session_id,
                "tool": a.tool,
                "reason": a.reason,
                "expires_at": a.expires_at,
            }
            for a in state.policy.approvals.pending()
        ]

    @app.post("/approvals/{approval_id}/decide", dependencies=[Admin])
    def decidir(approval_id: str, corpo: Decisao, state: State) -> dict[str, str]:
        try:
            a = state.policy.approvals.decide(
                approval_id, corpo.approved, channel=corpo.channel, actor=corpo.actor
            )
        except KeyError:
            raise HTTPException(404, "aprovação inexistente") from None
        except ValueError as e:
            raise HTTPException(409, str(e)) from None
        return {"id": a.id, "status": a.status.value}

    return app
