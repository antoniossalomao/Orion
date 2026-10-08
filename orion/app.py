"""App FastAPI do Orion: lifespan, estado injetado, /health, /chat (SSE) e aprovações."""

from __future__ import annotations

import asyncio
import contextlib
import hmac
import json
import logging
import re
import shutil
import threading
import time
import uuid
from collections.abc import AsyncIterator, Callable
from contextlib import AbstractAsyncContextManager, asynccontextmanager
from dataclasses import dataclass, field
from datetime import UTC, datetime
from typing import Annotated, Any
from urllib.parse import urlparse

from fastapi import (
    Depends,
    FastAPI,
    Header,
    HTTPException,
    Query,
    Request,
    Response,
    WebSocket,
    WebSocketDisconnect,
)
from fastapi.responses import FileResponse, StreamingResponse
from fastapi.staticfiles import StaticFiles
from pydantic import BaseModel, Field
from starlette.exceptions import HTTPException as StarletteHTTPException
from starlette.middleware.trustedhost import TrustedHostMiddleware

from . import __version__
from .agenda import agenda_do_briefing
from .agent import Agent, AgentEvent
from .auth import AuthError, AuthService, LockedOut, NotConfigured, WeakPassword
from .capabilities import Capabilities, describe
from .capture import Capturer
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
from .memory.store import Message, Session
from .painel import Painel, texto_do_painel
from .policy import ApprovalStore, PathGuard, PolicyEngine, redact
from .policy.paths import default_safe_roots
from .secrets import get_secret
from .skills import TOOL_SPEC as SKILL_SPEC
from .skills import SkillCatalog, skill_tool
from .tools import default_registry
from .tools.processes import ProcessManager
from .transcribe import Transcriber
from .vision import Vision
from .voice import (
    MAX_AUDIO,
    EdgeSpeaker,
    Speaker,
    VozStats,
    conexao_gemini,
    ponte_ao_vivo,
    turno_de_voz,
)
from .wake import (
    SoundDeviceSource,
    WakeConfig,
    WakeListener,
    bipe_com_sounddevice,
    criar_detector,
    tocar_mp3,
)

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
    painel: Painel | None = None
    transcriber: Transcriber | None = None  # voz (A) e `transcrever_audio`
    speaker: Speaker | None = None  # voz (A): fala da resposta; None sem voz ou sem fala
    live: Callable[[], AbstractAsyncContextManager[Any]] | None = None  # voz ao vivo (B)
    voz: VozStats = field(default_factory=VozStats)
    escuta: WakeListener | None = None  # palavra de ativação (regra 38)


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


_NOME_IMAGEM = re.compile(r"^img-\d{9,12}-[0-9a-f]{8}\.(png|jpg|webp)$")
_TIPO_IMAGEM = {"png": "image/png", "jpg": "image/jpeg", "webp": "image/webp"}
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
    url = settings.gateway_url
    endpoints = [Endpoint("gateway", url, settings.gateway_model, chave)]
    # camadas do roteamento: só entram as que têm modelo próprio; o "gateway" fica de reserva
    for camada, modelo, espera in (
        ("rapido", settings.gateway_model_fast, 60.0),
        ("pesado", settings.gateway_model_heavy, 120.0),
        ("visao", settings.vision_model, 90.0),
    ):
        if modelo and modelo != settings.gateway_model:
            endpoints.append(Endpoint(camada, url, modelo, chave, timeout_s=espera, tier=camada))
    return ChatGateway(endpoints)


def roteamento_ligado(settings: Settings) -> bool:
    """O roteamento só liga se alguma camada tem modelo próprio (senão não há o que escolher)."""
    return bool(
        settings.gateway_model_fast or settings.gateway_model_heavy or settings.vision_model
    )


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
        transcriber=transcriber_from_settings(settings),
        capture=(
            Capturer(settings.vault_dir, settings.capture_folder) if settings.vault_dir else None
        ),
        token=token,
        allowed_users=settings.telegram_allowed_users,
        agent=agent,
        memory=memory,
        approvals=policy.approvals,
        ops=ops,
    )


def vision_from_settings(settings: Settings) -> Vision | None:
    """Visão só com `ORION_VISION_TOOLS=true` e gateway configurado (o mesmo endpoint e chave)."""
    if not (settings.vision_tools and settings.gateway_url and settings.gateway_model):
        return None
    chave = settings.gateway_api_key or get_secret("ORION_GATEWAY_API_KEY")
    modelo = settings.vision_model or settings.gateway_model
    return Vision([Endpoint("gateway", settings.gateway_url, modelo, chave, timeout_s=90.0)])


def mcp_from_settings(settings: Settings) -> McpManager | None:
    """Servidores MCP do `mcp.json`; configuração inválida desliga o MCP, não o Orion."""
    if not settings.mcp_enabled:
        return None
    try:
        return manager_from_file(settings.effective_mcp_config)
    except McpConfigError as e:
        log.error("MCP desligado: %s", e)
        return None


def transcriber_from_settings(settings: Settings) -> Transcriber | None:
    """Sem chave de transcrição não há voz no Telegram nem `transcrever_audio`."""
    chave = settings.transcribe_api_key or get_secret("ORION_TRANSCRIBE_API_KEY")
    if not chave:
        return None
    return Transcriber(chave, base_url=settings.transcribe_url, model=settings.transcribe_model)


def speaker_from_settings(settings: Settings) -> Speaker | None:
    if not (settings.voice_enabled and settings.voice_speak):
        return None
    return EdgeSpeaker(settings.voice_tts_voice)


def live_from_settings(settings: Settings) -> Callable[[], AbstractAsyncContextManager[Any]] | None:
    """Sem opt-in ou sem chave não há voz ao vivo (o áudio do microfone vai para o Google)."""
    chave = (
        settings.voice_live_api_key
        or settings.search_api_key
        or settings.embed_api_key
        or get_secret("ORION_VOICE_LIVE_API_KEY")
        or get_secret("ORION_SEARCH_API_KEY")
        or get_secret("ORION_EMBED_API_KEY")
    )
    if not (settings.voice_live_enabled and chave):
        return None
    return lambda: conexao_gemini(chave, settings.voice_live_model, settings.voice_live_voice)


def wake_from_settings(
    settings: Settings,
    on_fala: Callable[[bytes], None],
    evento: Callable[[str, str], None],
) -> tuple[WakeListener | None, str]:
    """A escuta da palavra de ativação, ou (None, motivo) se não pode subir. O motivo vai ao
    painel: a escuta nunca deixa de subir em silêncio."""
    if not settings.wake_enabled:
        return None, ""
    try:
        detector = criar_detector(
            settings.wake_engine, settings.wake_model, settings.wake_words, settings.wake_threshold
        )
        source = SoundDeviceSource(settings.wake_device)
    except ValueError as e:
        return None, str(e)
    except ImportError as e:
        return None, f"falta instalar {e.name or 'uma dependência'} (ver ORION_OPERACAO §4.4)"
    except Exception as e:  # noqa: BLE001 — modelo corrompido, PortAudio ausente: motivo curto
        return None, f"não consegui preparar a escuta ({type(e).__name__})"
    cfg = WakeConfig(
        silencio_s=settings.wake_silence_s,
        max_s=settings.wake_max_s,
        max_por_hora=settings.wake_max_per_hour,
    )
    bipe = bipe_com_sounddevice() if settings.wake_beep else None
    return WakeListener(source, detector, on_fala, config=cfg, bipe=bipe, evento=evento), ""


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


def _iniciar_escuta(
    settings: Settings,
    ops: Operations,
    agent: Agent | None,
    transcriber: Transcriber | None,
    speaker: Speaker | None,
    stats: VozStats,
    fabrica: Callable[..., tuple[WakeListener | None, str]],
) -> tuple[tuple[WakeListener | None], str]:
    """Sobe a escuta da palavra de ativação numa thread. Devolve `((escuta,), motivo)`: o motivo
    explica por que não subiu (aparece no painel). A fala gravada segue o mesmo caminho do botão
    de microfone (`turno_de_voz`), com o canal "web": memória, política e audit valem igual."""
    if not settings.wake_enabled:
        return (None,), ""
    if not settings.voice_enabled:
        return (None,), "a palavra de ativação usa a voz: ligue ORION_VOICE_ENABLED"
    if agent is None or transcriber is None:
        return (None,), "precisa do gateway e da chave de transcrição (ORION_TRANSCRIBE_API_KEY)"
    loop = asyncio.get_running_loop()

    async def turno(wav: bytes) -> None:
        async for m in turno_de_voz(
            agent=agent, transcriber=transcriber, speaker=speaker, audio=wav
        ):
            if isinstance(m, bytes):
                await asyncio.to_thread(tocar_mp3, m)
            elif m["type"] == "heard":
                stats.turnos += 1
            elif m["type"] == "error":
                stats.erro(str(m.get("msg", "")))

    def on_fala(wav: bytes) -> None:
        asyncio.run_coroutine_threadsafe(turno(wav), loop).result(timeout=300)

    def evento(tipo: str, motivo: str) -> None:
        ops.audit_add(
            {
                "tool": f"voz_{tipo}",
                "action": "deny" if tipo == "recusada" else "allow",
                "risk": "read",
                "reason": motivo,
            }
        )

    escuta, motivo = fabrica(settings, on_fala, evento)
    if escuta is None:
        if motivo:
            log.warning("palavra de ativação não subiu: %s", motivo)
        return (None,), motivo
    threading.Thread(target=escuta.run, name="orion-wake", daemon=True).start()
    return (escuta,), ""


def _sem_mcp(nome: str, args: dict[str, Any]) -> dict[str, Any]:
    return {"erro": "sem servidor MCP"}


class EscutaControle(BaseModel):
    ativa: bool


class Mensagem(BaseModel):
    texto: str = Field(min_length=1, max_length=8000)
    canal: str = Field(default="web", pattern=_CANAL)


class Retomada(BaseModel):
    canal: str = Field(default="web", pattern=_CANAL)


class Ativacao(BaseModel):
    sessao_id: str = Field(min_length=1, max_length=64)
    canal: str = Field(default="web", pattern=_CANAL)


class AjusteDeFato(BaseModel):
    texto: str = Field(min_length=1, max_length=2000)


class AjusteDeConversa(BaseModel):
    titulo: str | None = Field(default=None, min_length=1, max_length=120)
    favorita: bool | None = None
    arquivada: bool | None = None


def _canal_valido(canal: str) -> None:
    if not re.fullmatch(_CANAL, canal):
        raise HTTPException(422, "canal inválido")


def _iso(ts: float) -> str:
    return datetime.fromtimestamp(ts, UTC).isoformat()


def _item_da_conversa(memory: MemoryStore, s: Any, ativa_id: str | None) -> dict[str, Any]:
    """Mesma forma que o legado usa (`sessao_id`, `titulo`, `criada`, `ativa`, `favorita`)."""
    titulo = s.title or (memory.first_user_text(s.id) or "").strip()
    return {
        "sessao_id": s.id,
        "titulo": " ".join(titulo.split())[:80] or "Nova conversa",
        "criada": _iso(s.created_at),
        "ultima_atividade": _iso(s.last_active_at),
        "ativa": s.id == ativa_id,
        "favorita": s.pinned,
        "somente_leitura": s.archived,
        "arquivada": s.shelved,
    }


def _pesquisa_noturna(settings: Settings, agent: Any, memory: MemoryStore) -> Any:
    """Só liga com hora, assuntos, agente, ferramentas de web e vault: tudo escolhido por você."""
    from .research import NightResearch, parse_assuntos

    assuntos = parse_assuntos(settings.research_topics)
    if not (settings.research_at and assuntos):
        return None
    if agent is None or not settings.web_tools or settings.vault_dir is None:
        log.warning("pesquisa noturna pedida, mas faltam gateway, ORION_WEB_TOOLS ou o vault")
        return None
    return NightResearch(
        memory=memory,
        run_turn=lambda canal, texto: agent.run(canal, texto, read_only=True),
        capturer=Capturer(settings.vault_dir, settings.capture_folder),
        assuntos=assuntos,
        at=settings.research_at,
        clock=time.time,
    )


def _item_do_fato(f: Any) -> dict[str, Any]:
    return {
        "id": f.id,
        "texto": f.text,
        "fonte": f.source,
        "criado": _iso(f.created_at),
        "atualizado": _iso(f.updated_at),
    }


def _mensagens(memory: MemoryStore, sid: str, limite: int = 80) -> list[dict[str, Any]]:
    return [
        {"role": m.role, "content": m.text, "timestamp": _iso(m.created_at)}
        for m in memory.context_history(sid, limite)
        if m.role in ("user", "assistant")
    ]


def sse(ev: AgentEvent) -> str:
    """Mesmo formato do /chat do legado (`text`, `tier`, `[DONE]`) + `tool`/`approval`/`error`."""
    corpo = ev.corpo()
    if corpo is None:  # "done"
        return "data: [DONE]\n\n"
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


AUTH_WS_S = 5.0  # quem não traz cookie nem cabeçalho tem este tempo para mandar o token


async def _ws_abrir(ws: WebSocket, state: AppState) -> bool:
    """Aceita e autentica um WebSocket de voz. Navegador: cookie da sessão (SameSite=Strict) e
    `Origin` igual ao `Host` (o WebSocket não tem CORS: sem isso, outra página abriria o
    microfone do Orion). Cliente de máquina: `Authorization` ou a primeira mensagem
    `{"cmd": "auth", "token": ...}` (o token nunca vai na URL). Fecha e devolve False se negar."""
    origem = ws.headers.get("origin")
    if origem and urlparse(origem).netloc != ws.headers.get("host", ""):
        await ws.close(code=1008)
        return False
    await ws.accept()

    async def negar(msg: str, codigo: int = 4401) -> bool:
        await ws.send_text(json.dumps({"type": "error", "msg": msg}))
        await ws.close(code=codigo)
        return False

    if not state.settings.admin_token and not state.auth.has_password():
        return await negar("autenticação não configurada", 4503)
    esperado = state.settings.admin_token

    def token_vale(enviado: str) -> bool:
        return bool(esperado) and hmac.compare_digest(enviado.encode(), esperado.encode())

    cabecalho = ws.headers.get("authorization")
    if cabecalho is not None:
        if token_vale(cabecalho.removeprefix("Bearer ").strip()):
            return True
        return await negar("login necessário")
    if state.auth.validate(ws.cookies.get(COOKIE_SESSAO)):
        return True
    try:
        async with asyncio.timeout(AUTH_WS_S):
            msg = json.loads(await ws.receive_text())
    except (TimeoutError, ValueError, WebSocketDisconnect, RuntimeError):
        return await negar("login necessário")
    if isinstance(msg, dict) and msg.get("cmd") == "auth" and token_vale(str(msg.get("token", ""))):
        return True
    return await negar("login necessário")


class Login(BaseModel):
    usuario: str | None = Field(default=None, max_length=64)  # ausente: o usuário único
    senha: str = Field(min_length=1, max_length=256)


class TrocaDeSenha(BaseModel):
    senha_atual: str = Field(default="", max_length=256)
    nova: str = Field(min_length=1, max_length=256)


class Decisao(BaseModel):
    approved: bool
    actor: str = "antonio"
    channel: str = "web"


def _painel_da_voz(
    settings: Settings,
    stats: VozStats,
    clique_pronto: bool,
    ao_vivo_pronto: bool,
    escuta: WakeListener | None = None,
    escuta_motivo: str = "",
) -> dict[str, Any]:
    return {
        "escuta": {
            "pedida": settings.wake_enabled,
            "ouvindo": escuta.stats.ouvindo if escuta else False,
            "pausada": escuta.stats.pausada if escuta else False,
            "ativacoes": escuta.stats.ativacoes if escuta else 0,
            "ultima": escuta.stats.ultima if escuta else None,
            "ultimo_erro": (escuta.stats.ultimo_erro if escuta else None) or escuta_motivo or None,
        },
        "clique": {
            "ligada": settings.voice_enabled and clique_pronto,
            "fala": settings.voice_enabled and settings.voice_speak,
            "turnos": stats.turnos,
        },
        "ao_vivo": {
            "ligada": settings.voice_live_enabled and ao_vivo_pronto,
            "sessoes": stats.sessoes_ao_vivo,
            "ativas": stats.ao_vivo_ativas,
            "minutos": round(stats.segundos_ao_vivo / 60, 1),
        },
        "falhas": stats.falhas,
        "ultimo_erro": stats.ultimo_erro,
    }


def create_app(
    settings: Settings | None = None,
    memory_factory: Callable[[Settings], MemoryStore] | None = None,
    gateway_factory: Callable[[Settings], ChatGateway | None] | None = None,
    telegram_factory: Callable[..., TelegramChannel | None] | None = None,
    mcp_factory: Callable[[Settings], McpManager | None] | None = None,
    transcriber_factory: Callable[[Settings], Transcriber | None] | None = None,
    speaker_factory: Callable[[Settings], Speaker | None] | None = None,
    live_factory: (
        Callable[[Settings], Callable[[], AbstractAsyncContextManager[Any]] | None] | None
    ) = None,
    wake_factory: (
        Callable[
            [Settings, Callable[[bytes], None], Callable[[str, str], None]],
            tuple[WakeListener | None, str],
        ]
        | None
    ) = None,
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
        if settings.seed_default_password and auth.seed_default():
            log.warning(
                "SENHA DE FÁBRICA ativa (usuário '%s'): troque em Configurações",
                settings.auth_user,
            )
        if settings.hosts_de_fora and auth.uses_default_password():
            auth.close()
            memory.close()
            raise RuntimeError(
                f"host {', '.join(settings.hosts_de_fora)} em ORION_ALLOWED_HOSTS com a senha de "
                "fábrica ainda ativa: troque a senha antes de expor o Orion na rede "
                "(`orion set-password` ou Configurações › Conexão)"
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
        transcriber = (transcriber_factory or transcriber_from_settings)(settings)
        agent = None
        mcp = None
        delegador: Delegator | None = None
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
            skills = (
                SkillCatalog(settings.effective_skills_dir) if settings.skills_enabled else None
            )
            if skills is not None and skills.skills:
                policy.register_tool(SKILL_SPEC)
                mcp_tools = [*mcp_tools, skill_tool(skills)]
            agent = Agent(
                gateway=gateway,
                tools=default_registry(
                    memory,
                    delegador,
                    ops,
                    desktop=settings.desktop_tools,
                    web=settings.web_tools,
                    transcriber=transcriber,
                    vision=vision_from_settings(settings),
                    captures_dir=settings.data_dir / "capturas",
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
                        "brave_key": lambda: (
                            settings.brave_api_key or get_secret("ORION_BRAVE_API_KEY")
                        ),
                        "image_key": lambda: (
                            settings.image_api_key
                            or settings.search_api_key
                            or settings.embed_api_key
                            or get_secret("ORION_IMAGE_API_KEY")
                            or get_secret("ORION_SEARCH_API_KEY")
                            or get_secret("ORION_EMBED_API_KEY")
                        ),
                        "image_model": settings.image_model,
                        "image_dir": settings.data_dir / "imagens",
                    },
                    extra=mcp_tools,
                ),
                policy=policy,
                memory=memory,
                ops=ops,
                routing=roteamento_ligado(settings),
                skills=skills,
            )
        jobs, tarefa_jobs, agenda = None, None, None
        if settings.jobs_enabled:
            consolidador = (
                Consolidator(memory, gateway)
                if gateway is not None and settings.consolidate and hasattr(gateway, "complete")
                else None
            )
            agenda = agenda_do_briefing(
                tool=settings.briefing_calendar_tool,
                email=settings.briefing_calendar_email,
                specs=mcp.specs if mcp is not None else {},
                call=mcp.call if mcp is not None else _sem_mcp,
                audit=ops.audit_add,
            )
            pesquisa = _pesquisa_noturna(settings, agent, memory)
            jobs = JobRunner(
                memory,
                ops,
                backup_dir=settings.effective_backup_dir,
                backup_keep=settings.backup_keep,
                vault_dir=settings.vault_dir,
                consolidator=consolidador,
                audit_days=settings.audit_retention_days,
                processes=processos,
                briefing_at=settings.briefing_at,
                agenda=agenda,
                research=pesquisa,
            )
            tarefa_jobs = asyncio.create_task(jobs.run_forever(settings.jobs_tick_s))
        telegram = (telegram_factory or telegram_from_settings)(
            settings, agent, memory, policy, ops
        )
        tarefa_telegram = asyncio.create_task(telegram.run()) if telegram is not None else None
        voz_stats = VozStats()
        speaker = (speaker_factory or speaker_from_settings)(settings)
        live = (live_factory or live_from_settings)(settings)
        escuta, escuta_motivo = _iniciar_escuta(
            settings,
            ops,
            agent,
            transcriber,
            speaker,
            voz_stats,
            wake_factory or wake_from_settings,
        )
        painel = Painel(
            started_at=time.time(),
            memory=memory,
            ops=ops,
            policy=policy,
            agent=agent,
            jobs=jobs,
            mcp=mcp,
            delegator=delegador,
            telegram_ativo=lambda: telegram is not None,
            voz=lambda: _painel_da_voz(
                settings,
                voz_stats,
                agent is not None and transcriber is not None,
                live is not None,
                escuta[0],
                escuta_motivo,
            ),
        )
        if telegram is not None:
            telegram.painel = lambda: texto_do_painel(painel.montar())
            telegram.agenda = agenda
        app.state.orion = AppState(
            settings, memory, policy, painel.started_at, ops, auth, agent, jobs, telegram, mcp,
            painel,
            transcriber=transcriber,
            speaker=speaker,
            live=live,
            voz=voz_stats,
            escuta=escuta[0],
        )  # fmt: skip
        try:
            yield
        finally:
            if escuta[0] is not None:
                escuta[0].parar()  # a thread é daemon; o microfone fecha no `finally` do laço
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
            if transcriber is not None:
                await transcriber.aclose()
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

    @app.get("/capabilities", response_model=Capabilities)
    def capabilities(state: State) -> Capabilities:
        return describe(
            agent_ready=state.agent is not None,
            admin_configured=bool(state.settings.admin_token),
        )

    @app.get("/capabilities/details", dependencies=[Admin])
    def capability_details(state: State) -> dict[str, Any]:
        """Informações da instalação nunca entram na descoberta anônima."""
        return {
            "gateway_model": state.settings.gateway_model if state.agent is not None else None,
            "components": {
                "memory": state.memory.ping(),
                "vectors": state.memory.vectors_available,
                "jobs": state.jobs is not None,
                "telegram": state.telegram is not None,
                "desktop_tools": state.agent is not None and state.settings.desktop_tools,
            },
            "tools": state.agent.tools.names() if state.agent is not None else [],
        }

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
                "voice": {
                    "click": state.settings.voice_enabled
                    and state.agent is not None
                    and state.transcriber is not None,
                    "live": state.settings.voice_live_enabled and state.live is not None,
                },
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
        """Público: o front decide entre mostrar o login ou o app. Se a senha ainda é a de fábrica,
        isso só aparece **depois** do login (senão a rota anunciaria o alvo a quem passa)."""
        autenticado = quem_e(request, state, authorization) is not None
        saida = {
            "configured": state.auth.has_password(),
            "authenticated": autenticado,
            "token_auth": bool(state.settings.admin_token),
        }
        if autenticado:
            saida["default_password"] = state.auth.uses_default_password()
        return saida

    @app.post("/auth/login")
    def login(corpo: Login, request: Request, response: Response, state: State) -> dict[str, bool]:
        if not _mesma_origem(request):
            raise HTTPException(403, "origem não permitida")
        cliente = request.client.host if request.client else "?"
        try:
            token = state.auth.login(corpo.senha, cliente, corpo.usuario)
        except LockedOut as e:
            raise HTTPException(429, str(e), headers={"Retry-After": str(e.retry_after)}) from None
        except NotConfigured as e:
            raise HTTPException(503, str(e)) from None
        except AuthError:
            audit_log.warning("login_falhou", extra={"audit": {"cliente": cliente}})
            raise HTTPException(401, "usuário ou senha incorretos") from None
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

    # ── conversas (barra lateral do front) ────────────────────────────────
    # "Apagar" esconde a conversa (as mensagens ficam no banco e o que o Orion já consolidou
    # delas continua na memória). Só mexe nas conversas que a barra do canal mostra: as do
    # Telegram e de outros canais não passam por aqui.
    @app.get("/sessoes", dependencies=[Admin])
    def listar_conversas(
        state: State, canal: str = "web", arquivadas: bool = False
    ) -> dict[str, Any]:
        _canal_valido(canal)
        ativa = state.memory.active_session(canal)
        itens = [
            _item_da_conversa(state.memory, s, ativa.id)
            for s in state.memory.list_sessions_ui(canal, shelved=arquivadas)
        ]
        return {"total": len(itens), "sessoes": itens, "ativa": ativa.id}

    @app.get("/sessoes/busca", dependencies=[Admin])
    def buscar_conversas(
        state: State,
        q: Annotated[str, Query(min_length=1, max_length=200)],
        canal: str = "web",
        limite: Annotated[int, Query(ge=1, le=50)] = 20,
        deslocamento: Annotated[int, Query(ge=0)] = 0,
    ) -> dict[str, Any]:
        """Acha conversas pelo título ou por uma palavra do corpo das mensagens."""
        _canal_valido(canal)
        achados = state.memory.search_sessions(canal, q, limite, deslocamento)
        return {
            "total": len(achados),
            "resultados": [
                {**_item_da_conversa(state.memory, a["sessao"], None), "trecho": a["trecho"]}
                for a in achados
            ],
        }

    @app.post("/sessoes", dependencies=[Admin])
    def nova_conversa(state: State, canal: str = "web") -> dict[str, Any]:
        _canal_valido(canal)
        atual = state.memory.active_session(canal)
        # já está numa conversa vazia: reaproveita em vez de empilhar conversas em branco
        if state.memory.history(atual.id, 1):
            atual = state.memory.new_session(canal)
        return {"ok": True, "sessao_id": atual.id}

    @app.post("/sessoes/ativar", dependencies=[Admin])
    def ativar_conversa(corpo: Ativacao, state: State) -> dict[str, Any]:
        s = state.memory.session_visible(corpo.sessao_id, corpo.canal)
        if s is None:
            raise HTTPException(404, "conversa não encontrada")
        if s.archived:
            raise HTTPException(409, "conversa arquivada: só leitura (use o histórico)")
        state.memory.activate_session(s.id)
        return {"ok": True, "sessao_id": s.id, "mensagens": _mensagens(state.memory, s.id)}

    def conversa(state: AppState, canal: str, sessao: str | None) -> Session | None:
        """A conversa pedida (visível neste canal) ou, sem `sessao`, a selecionada do canal."""
        _canal_valido(canal)
        if not sessao:
            return state.memory.selected_session(canal)
        session = state.memory.session_visible(sessao, canal)
        if session is None:
            raise HTTPException(404, "conversa não encontrada")
        return session

    def mensagem_json(m: Message) -> dict[str, Any]:
        return {
            "id": m.id,
            "role": m.role,
            "content": m.text,
            "timestamp": _iso(m.created_at),
            "provenance": m.provenance,
        }

    @app.get("/historico", dependencies=[Admin])
    def historico(
        state: State,
        canal: str = "web",
        sessao: Annotated[str | None, Query(max_length=64)] = None,
        limite: Annotated[int, Query(ge=1, le=100)] = 50,
        antes: Annotated[int | None, Query(ge=1)] = None,
        completo: bool = False,
    ) -> dict[str, Any]:
        """Página por ID (do fim para o começo). Por padrão respeita o limite de contexto de
        "limpar"; `completo=true` mostra tudo (a limpeza nunca apaga mensagens)."""
        session = conversa(state, canal, sessao)
        messages, total, cursor = (
            state.memory.history_page(session.id, limit=limite, before=antes, complete=completo)
            if session
            else ([], 0, None)
        )
        return {
            "sessao": session.id if session else None,
            "total": total,
            "mensagens": [mensagem_json(m) for m in messages],
            "mais": cursor is not None,
            "proximo_antes": cursor,
            "somente_leitura": bool(session and session.archived),
        }

    @app.delete("/historico", dependencies=[Admin])
    async def limpar_historico(
        state: State,
        canal: str = "web",
        sessao: Annotated[str | None, Query(max_length=64)] = None,
    ) -> dict[str, Any]:
        session = conversa(state, canal, sessao)
        if session:
            if session.archived:
                raise HTTPException(409, "conversa arquivada: só leitura")
            try:
                if state.agent:
                    await state.agent.clear_history(session.id)
                else:
                    if state.policy.approvals.unresolved(session.id):
                        raise ValueError("resolva as aprovações desta conversa antes de limpar")
                    state.memory.clear_context(session.id)
            except ValueError as e:
                raise HTTPException(409, str(e)) from None
        return {
            "ok": True,
            "sessao": session.id if session else None,
            "mensagem": "Contexto limpo; registro de mensagens e memória preservados.",
        }

    @app.get("/exportar", dependencies=[Admin])
    def exportar(
        state: State,
        canal: str = "web",
        sessao: Annotated[str | None, Query(max_length=64)] = None,
        completo: bool = False,
    ) -> dict[str, Any]:
        session = conversa(state, canal, sessao)
        lines = ["# Conversa com o Orion", ""]
        total = 0
        if session:
            # Lock compartilhado do store: a exportação lê um snapshot consistente.
            with state.memory.transaction():
                before = None
                pages = []
                while True:
                    messages, total, before = state.memory.history_page(
                        session.id, limit=100, before=before, complete=completo
                    )
                    pages.append(messages)
                    if before is None:
                        break
                for page in reversed(pages):
                    for m in page:
                        autor = "Antônio" if m.role == "user" else "Orion"
                        lines.extend([f"## {autor} — {_iso(m.created_at)}", "", m.text, ""])
                        if m.provenance:
                            # JSON em bloco indentado: até texto com crases mantém o bloco literal.
                            lines.extend(
                                ["Proveniência:", ""]
                                + [
                                    "    " + linha
                                    for linha in json.dumps(
                                        m.provenance, ensure_ascii=False, indent=2
                                    ).splitlines()
                                ]
                                + [""]
                            )
        return {
            "markdown": "\n".join(lines),
            "total_msgs": total,
            "sessao": session.id if session else None,
        }

    @app.patch("/sessoes/{sessao_id}", dependencies=[Admin])
    def ajustar_conversa(
        sessao_id: str, corpo: AjusteDeConversa, state: State, canal: str = "web"
    ) -> dict[str, Any]:
        _canal_valido(canal)
        if corpo.titulo is None and corpo.favorita is None and corpo.arquivada is None:
            raise HTTPException(422, "nada para mudar: mande `titulo`, `favorita` e/ou `arquivada`")
        if (
            corpo.arquivada is not None
            and state.memory.session_visible(sessao_id, canal) is not None
        ):
            if not state.memory.shelve_session(sessao_id, corpo.arquivada):
                raise HTTPException(409, "conversa importada é só leitura: não dá para arquivar")
        s = state.memory.session_visible(sessao_id, canal)
        if s is None:
            raise HTTPException(404, "conversa não encontrada")
        if corpo.titulo is not None and not state.memory.rename_session(s.id, corpo.titulo):
            raise HTTPException(422, "título vazio")
        if corpo.favorita is not None:
            state.memory.pin_session(s.id, corpo.favorita)
        novo = state.memory.get_session(s.id)
        assert novo is not None
        return {"ok": True, "sessao": _item_da_conversa(state.memory, novo, None)}

    @app.delete("/sessoes/{sessao_id}", dependencies=[Admin])
    def apagar_conversa(sessao_id: str, state: State, canal: str = "web") -> dict[str, bool]:
        _canal_valido(canal)
        s = state.memory.session_visible(sessao_id, canal)
        if s is None:
            raise HTTPException(404, "conversa não encontrada")
        state.memory.delete_session(s.id)
        audit_log.info(
            "conversa_apagada",
            extra={"audit": {"sessao": s.id, "canal": canal, "titulo": (s.title or "")[:60]}},
        )
        return {"ok": True}

    # ── memória: o que o Orion sabe sobre o Antônio (C36) ─────────────────
    @app.get("/memoria/fatos", dependencies=[Admin])
    def listar_fatos(
        state: State,
        q: Annotated[str | None, Query(max_length=200)] = None,
        limite: Annotated[int, Query(ge=1, le=200)] = 100,
    ) -> dict[str, Any]:
        achados = state.memory.search_facts(q, limite) if q else state.memory.facts()[:limite]
        return {"total": len(achados), "fatos": [_item_do_fato(f) for f in achados]}

    @app.patch("/memoria/fatos/{fato_id}", dependencies=[Admin])
    def corrigir_fato(fato_id: int, corpo: AjusteDeFato, state: State) -> dict[str, Any]:
        try:
            novo = state.memory.update_fact(fato_id, corpo.texto, source="manual")
        except KeyError:
            raise HTTPException(404, "fato não encontrado") from None
        except ValueError:
            raise HTTPException(422, "fato vazio") from None
        audit_log.info("fato_corrigido", extra={"audit": {"fato": fato_id}})
        return {"ok": True, "fato": _item_do_fato(novo)}

    @app.delete("/memoria/fatos/{fato_id}", dependencies=[Admin])
    def esquecer_fato(fato_id: int, state: State) -> dict[str, bool]:
        if not state.memory.forget_fact(fato_id):
            raise HTTPException(404, "fato não encontrado")
        audit_log.info("fato_esquecido", extra={"audit": {"fato": fato_id}})
        return {"ok": True}

    @app.post("/approvals/{approval_id}/resume", dependencies=[Admin])
    async def retomar(
        approval_id: str, state: State, corpo: Retomada | None = None
    ) -> StreamingResponse:
        """Depois de aprovada, executa a ação e deixa o modelo relatar o resultado. O corpo é
        opcional (o front não manda nenhum): sem ele vale o canal `web`."""
        agente = _agente(state)
        canal = corpo.canal if corpo else "web"
        return StreamingResponse(
            _stream(agente.resume(canal, approval_id)), media_type="text/event-stream"
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

    @app.get("/imagens/{nome}", dependencies=[Admin])
    def imagem(nome: str, state: State) -> FileResponse:
        """Imagem gerada por `gerar_imagem`. Só com login; o nome é conferido (nada de `..`,
        subpasta nem extensão estranha) e o tipo vem da extensão, nunca do conteúdo."""
        casou = _NOME_IMAGEM.match(nome)
        arquivo = state.settings.data_dir / "imagens" / nome
        if not casou or not arquivo.is_file():
            raise HTTPException(404, "imagem inexistente")
        return FileResponse(
            arquivo,
            media_type=_TIPO_IMAGEM[casou.group(1)],
            headers={
                "Cache-Control": "private, max-age=3600",
                "X-Content-Type-Options": "nosniff",
                "Content-Security-Policy": "default-src 'none'; sandbox",
            },
        )

    @app.get("/painel", dependencies=[Admin])
    def painel_unico(state: State) -> dict[str, Any]:
        """O estado do Orion numa resposta só (modelos, CLIs, aprovações, política, jobs...)."""
        assert state.painel is not None
        return state.painel.montar()

    @app.post("/voz/escuta", dependencies=[Admin])
    def escuta_pausar_ou_retomar(corpo: EscutaControle, state: State) -> dict[str, Any]:
        """Pausa ou retoma a escuta da palavra de ativação (o microfone segue aberto, mas nenhum
        quadro chega ao detector). Só há o que controlar se ela subiu."""
        if state.escuta is None:
            raise HTTPException(
                status_code=409, detail="a escuta da palavra de ativação está desligada"
            )
        if corpo.ativa:
            state.escuta.retomar()
        else:
            state.escuta.pausar()
        return {"pausada": state.escuta.stats.pausada}

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

    # ── voz (fase 6) ──────────────────────────────────────────────────────
    async def _erro_e_fecha(ws: WebSocket, msg: str) -> None:
        await ws.send_text(json.dumps({"type": "error", "msg": msg}))
        await ws.close(code=1000)

    async def _um_turno_de_voz(ws: WebSocket, state: AppState, audio: bytes) -> None:
        assert state.agent is not None and state.transcriber is not None
        try:
            async for m in turno_de_voz(
                agent=state.agent,
                transcriber=state.transcriber,
                speaker=state.speaker,
                audio=audio,
            ):
                if isinstance(m, bytes):
                    await ws.send_bytes(m)
                    continue
                if m["type"] == "heard":
                    state.voz.turnos += 1
                elif m["type"] == "error":
                    state.voz.erro(str(m.get("msg", "")))
                await ws.send_text(json.dumps(m, ensure_ascii=False))
        except WebSocketDisconnect:
            raise  # o navegador saiu: quem chamou encerra
        except Exception:
            log.exception("turno de voz falhou")
            state.voz.erro("falha interna no turno")
            with contextlib.suppress(RuntimeError, WebSocketDisconnect):  # navegador já saiu
                await ws.send_text(json.dumps({"type": "error", "msg": "falha interna no turno"}))
                await ws.send_text(json.dumps({"type": "done"}))

    @app.websocket("/ws/voz")
    async def ws_voz(ws: WebSocket) -> None:
        """A: uma fala por mensagem binária (webm/ogg/mp4/wav); a resposta volta como eventos
        JSON do chat e, no fim, o áudio da fala. `{"cmd": "cancel"}` interrompe o turno."""
        state: AppState = ws.app.state.orion
        if not await _ws_abrir(ws, state):
            return
        if not state.settings.voice_enabled:
            return await _erro_e_fecha(ws, "voz desligada (ORION_VOICE_ENABLED)")
        if state.agent is None:
            return await _erro_e_fecha(ws, "gateway de modelos não configurado")
        if state.transcriber is None:
            return await _erro_e_fecha(ws, "sem chave de transcrição (ORION_TRANSCRIBE_API_KEY)")
        turno: asyncio.Task[None] | None = None
        try:
            while True:
                msg = await ws.receive()
                if msg["type"] == "websocket.disconnect":
                    break
                dados = msg.get("bytes")
                if dados:
                    if turno is not None and not turno.done():
                        await ws.send_text(
                            json.dumps({"type": "error", "msg": "ainda estou no turno anterior"})
                        )
                    elif len(dados) > MAX_AUDIO:
                        await ws.send_text(
                            json.dumps({"type": "error", "msg": "fala longa demais"})
                        )
                    else:
                        turno = asyncio.create_task(_um_turno_de_voz(ws, state, dados))
                elif msg.get("text") and turno is not None and not turno.done():
                    try:
                        cmd = json.loads(msg["text"]).get("cmd")
                    except (ValueError, AttributeError):
                        continue
                    if cmd == "cancel":
                        turno.cancel()
                        await ws.send_text(json.dumps({"type": "done"}))
        except (WebSocketDisconnect, RuntimeError):
            pass
        finally:
            if turno is not None and not turno.done():
                turno.cancel()
            if turno is not None:
                await asyncio.gather(turno, return_exceptions=True)

    @app.websocket("/ws/voice")
    async def ws_voice(ws: WebSocket) -> None:
        """B: voz ao vivo (Gemini Live). PCM 16 kHz do microfone → PCM 24 kHz e transcrição.
        Só conversa: o modelo não tem ferramenta nem memória. A sessão vai para o audit."""
        state: AppState = ws.app.state.orion
        if not await _ws_abrir(ws, state):
            return
        if not state.settings.voice_live_enabled:
            return await _erro_e_fecha(ws, "voz ao vivo desligada (ORION_VOICE_LIVE_ENABLED)")
        if state.live is None:
            return await _erro_e_fecha(ws, "sem chave do Gemini (ORION_VOICE_LIVE_API_KEY)")
        _audit_voz(state, "início: o áudio do microfone vai para o Gemini; sem ferramentas")
        inicio = time.monotonic()
        state.voz.sessoes_ao_vivo += 1
        state.voz.ao_vivo_ativas += 1
        motivo = "erro"
        try:
            motivo = await ponte_ao_vivo(
                ws, state.live(), max_s=state.settings.voice_live_max_min * 60.0
            )
        finally:
            state.voz.ao_vivo_ativas -= 1
            state.voz.segundos_ao_vivo += time.monotonic() - inicio
            if motivo != "navegador":
                state.voz.erro(f"voz ao vivo encerrada ({motivo})")
        _audit_voz(state, f"fim ({motivo})")
        with contextlib.suppress(RuntimeError, WebSocketDisconnect):
            await ws.close()

    def _audit_voz(state: AppState, motivo: str) -> None:
        try:
            state.ops.audit_add(
                {"tool": "voz_ao_vivo", "action": "allow", "risk": "read", "reason": motivo}
            )
        except Exception:
            log.exception("audit da voz ao vivo falhou")

    if settings.serve_ui and FRONT_DIR.is_dir():
        # por último: as rotas da API têm prioridade sobre o mount
        app.mount("/ui", FrontStatic(directory=FRONT_DIR, html=True), name="ui")

    return app
