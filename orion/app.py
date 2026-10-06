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
from urllib.parse import urlparse

from fastapi import Depends, FastAPI, Header, HTTPException, Request, Response
from fastapi.responses import StreamingResponse
from fastapi.staticfiles import StaticFiles
from pydantic import BaseModel, Field
from starlette.exceptions import HTTPException as StarletteHTTPException
from starlette.middleware.trustedhost import TrustedHostMiddleware

from . import __version__
from .agent import Agent, AgentEvent
from .auth import AuthError, AuthService, LockedOut, NotConfigured, WeakPassword
from .channels import TelegramChannel
from .config import PROJECT_ROOT, Settings
from .delegate import Delegator
from .gateway import ChatGateway, Endpoint
from .jobs import JobRunner
from .log import request_id
from .mcp_client import McpConfigError, McpManager, manager_from_file
from .memory import MemoryStore
from .memory.consolidate import Consolidator
from .memory.embedders import GeminiEmbedder
from .memory.ops import Operations
from .policy import ApprovalStore, PathGuard, PolicyEngine, redact
from .policy.paths import default_safe_roots
from .secrets import get_secret
from .tools import default_registry
from .tools.processes import ProcessManager
from .transcribe import Transcriber

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
    auth: AuthService
    agent: Agent | None = None  # None enquanto o gateway não está configurado
    jobs: JobRunner | None = None  # None com ORION_JOBS_ENABLED=false
    telegram: TelegramChannel | None = None  # None sem token, sem usuários ou sem gateway
    mcp: McpManager | None = None  # None sem mcp.json, sem servidor habilitado ou sem gateway


def build_policy(
    settings: Settings, approvals: ApprovalStore | None = None, ops: Operations | None = None
) -> PolicyEngine:
    """Com `ops`, cada decisão também vai para a tabela `audit` (`consultar_audit_log`); se a
    gravação falhar, a política nega o que não for leitura (fail-closed, regra 8)."""
    guard = PathGuard(
        protected_roots=(PROJECT_ROOT,),
        safe_roots=(*default_safe_roots(), *settings.extra_safe_roots),
    )

    def registrar(evento: dict[str, Any]) -> None:
        audit_log.info("tool_decision", extra={"audit": evento})
        if ops is not None:
            ops.audit_add(evento)

    return PolicyEngine(
        path_guard=guard,
        approvals=approvals or ApprovalStore(ttl_s=settings.approval_ttl_s),
        audit=registrar,
    )


def get_state(request: Request) -> AppState:
    return request.app.state.orion


State = Annotated[AppState, Depends(get_state)]


COOKIE_SESSAO = "orion_session"
_METODOS_SEGUROS = {"GET", "HEAD", "OPTIONS"}


def _mesma_origem(request: Request) -> bool:
    """Defesa extra contra CSRF para quem entra por cookie (o principal é SameSite=Strict):
    um POST vindo de outro site traz `Origin` diferente do `Host` e é recusado."""
    origem = request.headers.get("origin")
    if origem:
        return urlparse(origem).netloc == request.headers.get("host", "")
    return request.headers.get("sec-fetch-site") in (None, "same-origin", "none")


def quem_e(request: Request, state: AppState, authorization: str | None) -> str | None:
    """ "token" (credencial de máquina), "sessao" (login com senha) ou None. Cabeçalho
    `Authorization` presente e errado **não** cai para o cookie: é erro, não alternativa."""
    if authorization is not None:
        esperado = state.settings.admin_token
        enviado = authorization.removeprefix("Bearer ").strip()
        if esperado and hmac.compare_digest(enviado.encode(), esperado.encode()):
            return "token"
        return None
    if state.auth.validate(request.cookies.get(COOKIE_SESSAO)):
        return "sessao"
    return None


def require_auth(
    request: Request, state: State, authorization: Annotated[str | None, Header()] = None
) -> str:
    """Toda rota da API passa por aqui: token de admin (`Authorization: Bearer`) ou sessão do
    login (cookie httpOnly, SameSite=Strict). Sem nenhum dos dois, a API fica desligada."""
    if not state.settings.admin_token and not state.auth.has_password():
        raise HTTPException(
            503, "autenticação não configurada (ORION_ADMIN_TOKEN ou `orion set-password`)"
        )
    quem = quem_e(request, state, authorization)
    if quem is None:
        raise HTTPException(401, "login necessário")
    if quem == "sessao" and request.method not in _METODOS_SEGUROS and not _mesma_origem(request):
        raise HTTPException(403, "origem não permitida")
    return quem


Admin = Depends(require_auth)


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
    chave_voz = settings.transcribe_api_key or get_secret("ORION_TRANSCRIBE_API_KEY")
    return TelegramChannel(
        transcriber=Transcriber(
            chave_voz, base_url=settings.transcribe_url, model=settings.transcribe_model
        )
        if chave_voz
        else None,
        token=token,
        allowed_users=settings.telegram_allowed_users,
        agent=agent,
        memory=memory,
        approvals=policy.approvals,
        ops=ops,
    )


def mcp_from_settings(settings: Settings) -> McpManager | None:
    """Servidores MCP do `mcp.json`; configuração inválida desliga o MCP, não o Orion."""
    if not settings.mcp_enabled:
        return None
    try:
        return manager_from_file(settings.effective_mcp_config)
    except McpConfigError as e:
        log.error("MCP desligado: %s", e)
        return None


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


class Login(BaseModel):
    senha: str = Field(min_length=1, max_length=256)


class TrocaDeSenha(BaseModel):
    senha_atual: str = Field(default="", max_length=256)
    nova: str = Field(min_length=1, max_length=256)


class Decisao(BaseModel):
    approved: bool
    actor: str = "antonio"
    channel: str = "web"


def create_app(
    settings: Settings | None = None,
    memory_factory: Callable[[Settings], MemoryStore] | None = None,
    gateway_factory: Callable[[Settings], ChatGateway | None] | None = None,
    telegram_factory: Callable[..., TelegramChannel | None] | None = None,
    mcp_factory: Callable[[Settings], McpManager | None] | None = None,
) -> FastAPI:
    settings = settings or Settings()

    @asynccontextmanager
    async def lifespan(app: FastAPI) -> AsyncIterator[None]:
        settings.data_dir.mkdir(parents=True, exist_ok=True)
        memory = (memory_factory or memory_from_settings)(settings)
        ops = Operations(memory)
        policy = build_policy(settings, ops=ops)
        auth = AuthService(
            settings.auth_db_path, user=settings.auth_user, ttl_s=settings.session_ttl_h * 3600
        )
        if settings.hosts_de_fora and not (settings.admin_token or auth.has_password()):
            auth.close()
            memory.close()
            raise RuntimeError(
                f"host {', '.join(settings.hosts_de_fora)} em ORION_ALLOWED_HOSTS exige login: "
                "defina a senha (`orion set-password`) ou ORION_ADMIN_TOKEN "
                "(regra 17 do ORION_REGRAS.md: acesso de fora só com login)"
            )
        gateway = (gateway_factory or gateway_from_settings)(settings)
        agent = None
        mcp = None
        processos: ProcessManager | None = None
        if gateway is not None:
            # `delegar` só aparece para o modelo se alguma CLI oficial estiver instalada.
            delegador = Delegator(memory) if _alguma_cli() else None
            mcp = (mcp_factory or mcp_from_settings)(settings)
            mcp_tools = await asyncio.to_thread(mcp.start) if mcp is not None else []
            if mcp is not None:
                for spec in mcp.specs.values():
                    policy.register_tool(spec)
            if settings.desktop_tools:
                processos = ProcessManager(settings.data_dir / "processos")
            agent = Agent(
                gateway=gateway,
                tools=default_registry(
                    memory,
                    delegador,
                    ops,
                    desktop=settings.desktop_tools,
                    web=settings.web_tools,
                    processes=processos,
                    web_options={
                        "search_key": lambda: (
                            settings.search_api_key
                            or settings.embed_api_key
                            or get_secret("ORION_SEARCH_API_KEY")
                            or get_secret("ORION_EMBED_API_KEY")
                        ),
                        "search_model": settings.search_model,
                        "cidade_padrao": settings.weather_city,
                    },
                    extra=mcp_tools,
                ),
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
                audit_days=settings.audit_retention_days,
                processes=processos,
            )
            tarefa_jobs = asyncio.create_task(jobs.run_forever(settings.jobs_tick_s))
        telegram = (telegram_factory or telegram_from_settings)(
            settings, agent, memory, policy, ops
        )
        tarefa_telegram = asyncio.create_task(telegram.run()) if telegram is not None else None
        app.state.orion = AppState(
            settings, memory, policy, time.time(), ops, auth, agent, jobs, telegram, mcp
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
            if mcp is not None:
                await asyncio.to_thread(mcp.stop)
            if gateway is not None and hasattr(gateway, "aclose"):
                await gateway.aclose()
            auth.close()
            memory.close()

    # sem /docs, /redoc nem /openapi.json: o mapa da API não é público (regra 17)
    app = FastAPI(
        title="Orion",
        version=__version__,
        lifespan=lifespan,
        docs_url=None,
        redoc_url=None,
        openapi_url=None,
    )
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
    def health(
        request: Request, state: State, authorization: Annotated[str | None, Header()] = None
    ) -> dict[str, Any]:
        """Sem login só o mínimo (serve de sonda); os detalhes pedem sessão ou token."""
        try:
            memoria = "ok" if state.memory.ping() else "erro"
        except Exception:  # noqa: BLE001 — /health nunca pode levantar
            memoria = "erro"
        basico = {"status": "ok" if memoria == "ok" else "degradado", "version": __version__}
        if quem_e(request, state, authorization) is None:
            return basico
        return {
            **basico,
            "uptime_s": round(time.time() - state.started_at, 1),
            "components": {
                "memory": memoria,
                "vectors": state.memory.vectors_available,
                "gateway": state.agent is not None,
                "jobs": state.jobs is not None,
                "telegram": state.telegram is not None,
                "mcp": state.mcp.status if state.mcp is not None else {},
            },
            "pending_approvals": len(state.policy.approvals.pending()),
            "pending_notifications": len(state.ops.pending_notifications(limit=1000)),
        }

    # ── login ─────────────────────────────────────────────────────────────
    def _cookie_seguro(request: Request, state: AppState) -> bool:
        return state.settings.cookie_secure or request.url.scheme == "https"

    def _abrir_sessao(response: Response, request: Request, state: AppState, token: str) -> None:
        response.set_cookie(
            COOKIE_SESSAO,
            token,
            max_age=state.settings.session_ttl_h * 3600,
            httponly=True,
            samesite="strict",
            secure=_cookie_seguro(request, state),
            path="/",
        )

    @app.get("/auth/status")
    def auth_status(
        request: Request, state: State, authorization: Annotated[str | None, Header()] = None
    ) -> dict[str, bool]:
        """Público: o front decide entre mostrar o login ou o app."""
        return {
            "configured": state.auth.has_password(),
            "authenticated": quem_e(request, state, authorization) is not None,
            "token_auth": bool(state.settings.admin_token),
        }

    @app.post("/auth/login")
    def login(corpo: Login, request: Request, response: Response, state: State) -> dict[str, bool]:
        if not _mesma_origem(request):
            raise HTTPException(403, "origem não permitida")
        cliente = request.client.host if request.client else "?"
        try:
            token = state.auth.login(corpo.senha, cliente)
        except LockedOut as e:
            raise HTTPException(429, str(e), headers={"Retry-After": str(e.retry_after)}) from None
        except NotConfigured as e:
            raise HTTPException(503, str(e)) from None
        except AuthError:
            audit_log.warning("login_falhou", extra={"audit": {"cliente": cliente}})
            raise HTTPException(401, "senha incorreta") from None
        _abrir_sessao(response, request, state, token)
        return {"ok": True}

    @app.post("/auth/logout")
    def logout(request: Request, response: Response, state: State) -> dict[str, bool]:
        state.auth.logout(request.cookies.get(COOKIE_SESSAO))
        response.delete_cookie(COOKIE_SESSAO, path="/")
        return {"ok": True}

    @app.post("/auth/password")
    def trocar_senha(
        corpo: TrocaDeSenha,
        request: Request,
        response: Response,
        state: State,
        quem: Annotated[str, Depends(require_auth)],
    ) -> dict[str, bool]:
        """Troca (ou define a primeira) senha. Revoga todas as sessões e abre uma nova para
        quem trocou. Com senha já existente, pede a atual mesmo com o token de admin."""
        if state.auth.has_password() and not state.auth.check_password(corpo.senha_atual):
            raise HTTPException(401, "senha atual incorreta")
        try:
            state.auth.set_password(corpo.nova)
        except WeakPassword as e:
            raise HTTPException(422, str(e)) from None
        if quem == "sessao":
            _abrir_sessao(response, request, state, state.auth.login(corpo.nova, "troca-de-senha"))
        return {"ok": True}

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
                "args_truncated": redact(a.args, limite=2000) != redact(a.args, limite=10**9),
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
