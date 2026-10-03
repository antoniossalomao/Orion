"""App FastAPI do Orion: lifespan, estado injetado, /health, /chat (SSE) e aprovações."""

from __future__ import annotations

import asyncio
import contextlib
import hmac
import json
import logging
import shutil
import time
import uuid
from collections.abc import AsyncIterator, Callable
from contextlib import asynccontextmanager
from dataclasses import dataclass
from typing import Annotated, Any

from fastapi import Depends, FastAPI, Header, HTTPException, Request
from fastapi.responses import StreamingResponse
from fastapi.staticfiles import StaticFiles
from pydantic import BaseModel, Field
from starlette.exceptions import HTTPException as StarletteHTTPException
from starlette.middleware.trustedhost import TrustedHostMiddleware

from . import __version__
from .agent import Agent, AgentEvent
from .channels import TelegramChannel
from .config import PROJECT_ROOT, Settings
from .delegate import Delegator
from .gateway import ChatGateway, Endpoint
from .jobs import JobRunner
from .log import request_id
from .memory import MemoryStore
from .memory.consolidate import Consolidator
from .memory.embedders import GeminiEmbedder
from .memory.ops import Operations
from .policy import ApprovalStore, PathGuard, PolicyEngine, redact
from .policy.paths import default_safe_roots
from .secrets import get_secret
from .tools import default_registry

FRONT_DIR = PROJECT_ROOT / "Orion_Core" / "Front_end_Orion"

audit_log = logging.getLogger("orion.audit")
log = logging.getLogger("orion.app")


class FrontStatic(StaticFiles):
    """Serve só o que o navegador precisa da pasta do front: nada de código Python."""

    async def get_response(self, path: str, scope):
        if path.endswith((".py", ".pyc")) or "__pycache__" in path:
            raise StarletteHTTPException(404)
        return await super().get_response(path, scope)


@dataclass
class AppState:
    """Tudo que os endpoints usam, criado no lifespan e injetado por `Depends`."""

    settings: Settings
    memory: MemoryStore
    policy: PolicyEngine
    started_at: float
    ops: Operations
    agent: Agent | None = None  # None enquanto o gateway não está configurado
    jobs: JobRunner | None = None  # None com ORION_JOBS_ENABLED=false
    telegram: TelegramChannel | None = None  # None sem token, sem usuários ou sem gateway


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


def _alguma_cli() -> bool:
    return any(shutil.which(c) for c in ("claude", "codex", "gemini"))


def gateway_from_settings(settings: Settings) -> ChatGateway | None:
    if not (settings.gateway_url and settings.gateway_model):
        return None
    chave = settings.gateway_api_key or get_secret("ORION_GATEWAY_API_KEY")
    return ChatGateway([Endpoint("gateway", settings.gateway_url, settings.gateway_model, chave)])


def telegram_from_settings(
    settings: Settings,
    agent: Agent | None,
    memory: MemoryStore,
    policy: PolicyEngine,
    ops: Operations,
) -> TelegramChannel | None:
    """O canal só sobe com token, lista de usuários (default-deny) e gateway de modelos."""
    token = settings.telegram_token or get_secret("ORION_TELEGRAM_TOKEN")
    if not token:
        return None
    if not settings.telegram_allowed_users:
        log.error("telegram desligado: defina ORION_TELEGRAM_ALLOWED_USERS (default-deny)")
        return None
    if agent is None:
        log.error("telegram desligado: configure o gateway de modelos (ORION_GATEWAY_URL/MODEL)")
        return None
    return TelegramChannel(
        token=token,
        allowed_users=settings.telegram_allowed_users,
        agent=agent,
        memory=memory,
        approvals=policy.approvals,
        ops=ops,
    )


def embedder_from_settings(settings: Settings) -> GeminiEmbedder | None:
    """Sem chave de API, sem embeddings: a busca segue só por palavra-chave."""
    chave = settings.embed_api_key or get_secret("ORION_EMBED_API_KEY")
    if not chave:
        return None
    return GeminiEmbedder(chave, model=settings.embed_model, dim=settings.embed_dim)


def memory_from_settings(settings: Settings) -> MemoryStore:
    embedder = embedder_from_settings(settings)
    try:
        return MemoryStore(settings.db_path, embedder=embedder)
    except RuntimeError as e:
        if embedder is None:
            raise
        # trocou o modelo/dimensão sem `reset_vectors()`: sobe sem vetores em vez de não subir
        log.error("embeddings desligados: %s", e)
        return MemoryStore(settings.db_path)


_CANAL = r"^[a-z0-9_-]{1,32}$"


class Mensagem(BaseModel):
    texto: str = Field(min_length=1, max_length=8000)
    canal: str = Field(default="web", pattern=_CANAL)


class Retomada(BaseModel):
    canal: str = Field(default="web", pattern=_CANAL)


def sse(ev: AgentEvent) -> str:
    """Mesmo formato do /chat do legado (`text`, `tier`, `[DONE]`) + `tool`/`approval`/`error`."""
    d = ev.data
    if ev.kind == "done":
        return "data: [DONE]\n\n"
    corpo = {
        "text": {"text": d.get("text")},
        "tier": {"tier": f"{d.get('endpoint')}/{d.get('model')}"},
        "tool": {"tool": d},
        "approval": {"approval": d},
        "error": {"error": d.get("message")},
    }[ev.kind]
    return f"data: {json.dumps(corpo, ensure_ascii=False)}\n\n"


async def _stream(eventos: AsyncIterator[AgentEvent]) -> AsyncIterator[str]:
    terminou = False
    try:
        async for ev in eventos:
            terminou = terminou or ev.kind == "done"
            yield sse(ev)
    except Exception:
        log.exception("turno do agente falhou")
        yield sse(AgentEvent("error", {"message": "falha interna no turno"}))
    if not terminou:
        yield "data: [DONE]\n\n"


class Decisao(BaseModel):
    approved: bool
    actor: str = "antonio"
    channel: str = "web"


def create_app(
    settings: Settings | None = None,
    memory_factory: Callable[[Settings], MemoryStore] | None = None,
    gateway_factory: Callable[[Settings], ChatGateway | None] | None = None,
    telegram_factory: Callable[..., TelegramChannel | None] | None = None,
) -> FastAPI:
    settings = settings or Settings()

    @asynccontextmanager
    async def lifespan(app: FastAPI) -> AsyncIterator[None]:
        settings.data_dir.mkdir(parents=True, exist_ok=True)
        memory = (memory_factory or memory_from_settings)(settings)
        ops = Operations(memory)
        policy = build_policy(settings)
        gateway = (gateway_factory or gateway_from_settings)(settings)
        agent = None
        if gateway is not None:
            # `delegar` só aparece para o modelo se alguma CLI oficial estiver instalada.
            delegador = Delegator(memory) if _alguma_cli() else None
            agent = Agent(
                gateway=gateway,
                tools=default_registry(memory, delegador, ops, desktop=settings.desktop_tools),
                policy=policy,
                memory=memory,
                ops=ops,
            )
        jobs, tarefa_jobs = None, None
        if settings.jobs_enabled:
            consolidador = (
                Consolidator(memory, gateway)
                if gateway is not None and settings.consolidate and hasattr(gateway, "complete")
                else None
            )
            jobs = JobRunner(
                memory,
                ops,
                backup_dir=settings.effective_backup_dir,
                backup_keep=settings.backup_keep,
                vault_dir=settings.vault_dir,
                consolidator=consolidador,
            )
            tarefa_jobs = asyncio.create_task(jobs.run_forever(settings.jobs_tick_s))
        telegram = (telegram_factory or telegram_from_settings)(
            settings, agent, memory, policy, ops
        )
        tarefa_telegram = asyncio.create_task(telegram.run()) if telegram is not None else None
        app.state.orion = AppState(
            settings, memory, policy, time.time(), ops, agent, jobs, telegram
        )
        try:
            yield
        finally:
            for tarefa in (tarefa_jobs, tarefa_telegram):
                if tarefa is not None:
                    tarefa.cancel()
                    with contextlib.suppress(asyncio.CancelledError):
                        await tarefa
            if telegram is not None:
                await telegram.aclose()
            if gateway is not None and hasattr(gateway, "aclose"):
                await gateway.aclose()
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
            "components": {
                "memory": memoria,
                "vectors": state.memory.vectors_available,
                "gateway": state.agent is not None,
                "jobs": state.jobs is not None,
                "telegram": state.telegram is not None,
            },
            "pending_approvals": len(state.policy.approvals.pending()),
            "pending_notifications": len(state.ops.pending_notifications(limit=1000)),
        }

    def _agente(state: AppState) -> Agent:
        if state.agent is None:
            raise HTTPException(503, "gateway de modelos não configurado (ORION_GATEWAY_URL/MODEL)")
        return state.agent

    @app.post("/chat", dependencies=[Admin])
    async def chat(corpo: Mensagem, state: State) -> StreamingResponse:
        agente = _agente(state)
        return StreamingResponse(
            _stream(agente.run(corpo.canal, corpo.texto)), media_type="text/event-stream"
        )

    @app.post("/approvals/{approval_id}/resume", dependencies=[Admin])
    async def retomar(approval_id: str, corpo: Retomada, state: State) -> StreamingResponse:
        """Depois de aprovada, executa a ação e deixa o modelo relatar o resultado."""
        agente = _agente(state)
        return StreamingResponse(
            _stream(agente.resume(corpo.canal, approval_id)), media_type="text/event-stream"
        )

    @app.get("/approvals", dependencies=[Admin])
    def pendentes(state: State) -> list[dict[str, Any]]:
        return [
            {
                "id": a.id,
                "session_id": a.session_id,
                "tool": a.tool,
                "reason": a.reason,
                # redigido: quem aprova precisa ver O QUÊ (comando, caminho), sem vazar segredo
                "args": redact(a.args, limite=2000),
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

    @app.get("/notifications", dependencies=[Admin])
    def avisos(state: State) -> list[dict[str, Any]]:
        """Avisos ainda não entregues (lembrete vencido, agendamento disparado). O canal
        entrega e confirma em `/notifications/{id}/ack`."""
        return state.ops.pending_notifications()

    @app.post("/notifications/{notification_id}/ack", dependencies=[Admin])
    def confirmar_aviso(notification_id: int, state: State) -> dict[str, bool]:
        if not state.ops.ack_notification(notification_id):
            raise HTTPException(404, "aviso inexistente ou já confirmado")
        return {"ok": True}

    if settings.serve_ui and FRONT_DIR.is_dir():
        # por último: as rotas da API têm prioridade sobre o mount
        app.mount("/ui", FrontStatic(directory=FRONT_DIR, html=True), name="ui")

    return app
