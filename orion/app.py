"""App FastAPI do Orion: lifespan, estado injetado, /health, /chat (SSE) e aprovações."""

from __future__ import annotations

import asyncio
import contextlib
import hmac
import json
import logging
import re
import shutil
import tempfile
import threading
import time
import uuid
import webbrowser
from collections.abc import AsyncIterator, Callable
from contextlib import AbstractAsyncContextManager, asynccontextmanager
from dataclasses import dataclass, field
from datetime import UTC, datetime
from pathlib import Path
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
from fastapi import Path as PathParam
from fastapi.responses import FileResponse, JSONResponse, StreamingResponse
from fastapi.staticfiles import StaticFiles
from pydantic import BaseModel, ConfigDict, Field
from starlette.exceptions import HTTPException as StarletteHTTPException
from starlette.middleware.trustedhost import TrustedHostMiddleware

from . import __version__, saidas
from .agenda import agenda_do_briefing
from .agent import Agent, AgentEvent
from .artifacts import ArtifactError
from .artifacts import router as artifact_router
from .auth import AuthError, AuthService, LockedOut, NotConfigured, WeakPassword
from .briefing import build_briefing
from .capabilities import Capabilities, describe
from .captura_rapida import router as captura_router
from .capture import Capturer
from .channels import TelegramChannel
from .config import PROJECT_ROOT, Settings
from .conversor import ConversaoError, converter_e_guardar
from .costs import Custos, cotas_de
from .delegate import Delegator
from .desligar import posso_desligar
from .desligar import texto as texto_desligar
from .downloads import Downloads
from .extensions.catalog import Catalog
from .extensions.catalogo import Catalogos, urls_validas
from .extensions.context import ContextReader, ContextRequest
from .extensions.host import MCPError, MCPHost
from .extensions.manager import PluginManager
from .extensions.plugins import PluginError
from .extensions.routes import router as extension_router
from .extensions.skill_runtime import SkillReference, SkillRuntime
from .extensions.skills import SkillError
from .fact_routes import router as fact_router
from .gateway import CAMADA_LOCAL, ChatGateway, Endpoint
from .grafo_routes import router as grafo_router
from .historicos import Historicos
from .historicos import ferramentas as historicos_tools
from .jobs import JobRunner
from .ler_depois import LerDepois, LerDepoisError
from .log import request_id
from .mcp_client import SPEC_RECURSO, McpConfigError, McpManager, manager_from_file
from .mcp_indice import SPEC as INDICE_SPEC
from .mcp_indice import Indice
from .memory import MemoryStore
from .memory.compact import Compactor
from .memory.consolidate import Consolidator
from .memory.embedders import GeminiEmbedder
from .memory.ops import Operations
from .memory.sleep import SleepCycle
from .memory.store import Message, Session
from .modos import ModoError, Modos
from .painel import Painel, texto_do_painel
from .palmas import ClapDetector, acao_das_palmas
from .plugins import PluginStore
from .policy import ApprovalStore, PathGuard, PolicyEngine, redact
from .policy.paths import default_safe_roots
from .ponte.hub import PonteHub
from .ponte.rotas import ESCOPO as PONTE_ESCOPO
from .ponte.rotas import rota_permitida as rota_da_ponte_permitida
from .ponte.rotas import router as ponte_router
from .project_routes import router as project_router
from .projects import ProjectError
from .propostas import PropostaError, Propostas
from .provedores import chave_env
from .research import Research
from .resultados import PREVIA_IMAGEM, PREVIA_PDF, PREVIA_TEXTO, Library, nome_seguro
from .rotinas import Executor as ExecutorRotinas
from .rotinas import RotinaStore
from .screen_memory import (
    ScreenMemory,
    app_em_foco,
    capturador_de_tela,
    comando_tela,
    ocr_tesseract,
    parse_lista,
    tela_bloqueada,
    titulo_da_janela,
)
from .secrets import get_secret
from .skills import TOOL_SPEC as SKILL_SPEC
from .skills import SkillCatalog, skill_tool
from .tarefas_bg import TarefaBgError, TarefasBg
from .tools import default_registry
from .tools.processes import ProcessManager
from .tools.registry import ToolRegistry
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
MAX_ENVIO_TELEGRAM = 50 * 1024 * 1024  # limite de envio de arquivo por bot
MAX_TEXTO_DOCUMENTO = 2_000_000  # caracteres indexados por documento enviado


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
    skills: SkillRuntime | None = None
    catalog: Catalog | None = None
    mcp_host: MCPHost | None = None
    extensions: PluginManager | None = None
    accounts: Any = None
    calendar: Any = None
    events: Any = None
    file_plans: Any = None
    export_credentials: Any = None
    telegram: TelegramChannel | None = None  # None sem token, sem usuários ou sem gateway
    mcp: McpManager | None = None  # None sem mcp.json, sem servidor habilitado ou sem gateway
    painel: Painel | None = None
    transcriber: Transcriber | None = None  # voz (A) e `transcrever_audio`
    speaker: Speaker | None = None  # voz (A): fala da resposta; None sem voz ou sem fala
    live: Callable[[], AbstractAsyncContextManager[Any]] | None = None  # voz ao vivo (B)
    voz: VozStats = field(default_factory=VozStats)
    # palavra de ativação (REGRAS.md: palavra de ativação só abre o microfone; detecção local)
    escuta: WakeListener | None = None
    # memória da tela (REGRAS.md: memória da tela: só texto, local e com opt-in)
    tela: ScreenMemory | None = None
    biblioteca: Library | None = None  # resultados gerados (C34)
    # pânico e não perturbe (REGRAS.md: pânico é corte; não perturbe só segura)
    modos: Modos | None = None
    custos: Custos | None = None  # cota gratuita (REGRAS.md: custo zero (sem API paga))
    # organizador de Downloads (REGRAS.md: automação sua vale pelo arquivo concedido)
    downloads: Downloads | None = None
    # área de transferência e comandos (REGRAS.md: históricos locais: opt-in, só texto, só neste
    # computador)
    historicos: Historicos | None = None
    ler_depois: LerDepois | None = None  # links para ler depois (E9.8)
    # rotinas suas e concedidas (REGRAS.md: automação sua vale pelo arquivo concedido)
    rotinas: RotinaStore | None = None
    executor_rotinas: ExecutorRotinas | None = None
    # TarefasBg (REGRAS.md: tarefa em segundo plano só lê e propõe); None sem agente
    tarefas_bg: Any = None
    # NightResearch (REGRAS.md: pesquisa noturna só lê, sem aprovação); None sem agente,
    # ORION_WEB_TOOLS ou vault
    pesquisa: Any = None
    tarefas_pesquisa: Any = None  # a tarefa do `rodar agora` (guarda a referência)
    # ponte de desktop (REGRAS.md: ponte de desktop só abre e cola o que você aprovou)
    ponte: PonteHub = field(default_factory=PonteHub)


def build_policy(
    settings: Settings, approvals: ApprovalStore | None = None, ops: Operations | None = None
) -> PolicyEngine:
    """Com `ops`, cada decisão também vai para a tabela `audit` (`consultar_audit_log`); se a
    gravação falhar, a política nega o que não for leitura (fail-closed, REGRAS.md: falha de
    auditoria nega a ação)."""
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
        if state.auth.device_scope(enviado) == PONTE_ESCOPO:
            # escopo estreito: `require_auth` confere o caminho (REGRAS.md: ponte de desktop só abre
            # e cola o que você aprovou)
            return "ponte"
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
    if quem == "ponte" and not (
        rota_da_ponte_permitida(request.method, request.url.path)
        and not (
            state.modos is not None and state.modos.panico() and request.url.path != "/modo/panico"
        )
    ):
        raise HTTPException(403, "o token da ponte não alcança esta rota")
    if quem == "sessao" and request.method not in _METODOS_SEGUROS and not _mesma_origem(request):
        raise HTTPException(403, "origem não permitida")
    return quem


Admin = Depends(require_auth)


def _alguma_cli() -> bool:
    return any(shutil.which(c) for c in ("claude", "codex", "gemini"))


_ESPERA = {"": 60.0, "rapido": 60.0, "pesado": 120.0, "visao": 90.0}


def endpoints_dos_provedores(settings: Settings, camadas: set[str] | None = None) -> list[Endpoint]:
    """Um endpoint por modelo configurado em `ORION_PROVEDORES`, na ordem da lista (prioridade).
    `camadas` filtra (ex.: {"visao"}); provedor sem chave fica de fora, com aviso no log."""
    endpoints: list[Endpoint] = []
    for p in settings.provedores:
        chave = get_secret(chave_env(p.id))
        if not chave:
            log.warning("provedor %s sem chave (%s): fica de fora", p.id, chave_env(p.id))
            continue
        for camada, modelo in p.modelos():
            if camadas is not None and camada not in camadas:
                continue
            nome = f"{p.id}:{camada}" if camada else p.id
            endpoints.append(
                Endpoint(
                    nome,
                    p.base_url(),
                    modelo,
                    chave,
                    timeout_s=_ESPERA[camada],
                    tier=camada,
                    provedor=p.id,
                )
            )
    return endpoints


def gateway_from_settings(settings: Settings) -> ChatGateway | None:
    endpoints = endpoints_dos_provedores(settings)
    if settings.gateway_url and settings.gateway_model:
        chave = settings.gateway_api_key or get_secret("ORION_GATEWAY_API_KEY")
        url = settings.gateway_url
        endpoints.append(Endpoint("gateway", url, settings.gateway_model, chave))
        # camadas do roteamento: só entram as que têm modelo próprio; o "gateway" fica de reserva
        for camada, modelo, espera in (
            ("rapido", settings.gateway_model_fast, 60.0),
            ("pesado", settings.gateway_model_heavy, 120.0),
            ("visao", settings.vision_model, 90.0),
        ):
            if modelo and modelo != settings.gateway_model:
                endpoints.append(
                    Endpoint(camada, url, modelo, chave, timeout_s=espera, tier=camada)
                )
    if settings.local_model:
        # REGRAS.md: o modelo deste computador entra por último e nunca recebe ferramentas
        endpoints.append(
            Endpoint(
                "local",
                settings.local_url,
                settings.local_model,
                None,
                timeout_s=180.0,
                tier=CAMADA_LOCAL,
                tools=False,
            )
        )
    return ChatGateway(endpoints) if endpoints else None


def compactor_from_settings(settings: Settings, memory: MemoryStore) -> Compactor | None:
    """Auto-compact (REGRAS.md: resumo da conversa nunca substitui o registro): só liga com o modelo
    local, num gateway PRÓPRIO que não tem outro
    endpoint, para o texto da conversa nunca sair do computador para ser resumido."""
    if settings.compact_at <= 0 or not settings.local_model:
        return None
    local = Endpoint(
        "local",
        settings.local_url,
        settings.local_model,
        None,
        timeout_s=300.0,
        tier=CAMADA_LOCAL,
        tools=False,
    )
    return Compactor(
        memory,
        ChatGateway([local]),
        at=settings.compact_at,
        model=f"local:{settings.local_model}",
    )


def roteamento_ligado(settings: Settings) -> bool:
    """O roteamento só liga se alguma camada tem modelo próprio (senão não há o que escolher)."""
    return bool(
        settings.gateway_model_fast
        or settings.gateway_model_heavy
        or settings.vision_model
        or any(p.rapido or p.pesado or p.visao for p in settings.provedores)
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
        log.error(
            "telegram desligado: configure os modelos (ORION_PROVEDORES ou ORION_GATEWAY_URL/MODEL)"
        )
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
    if not settings.vision_tools:
        return None
    # provedores: o modelo de visão de cada um (ou o padrão, se não houver) — ordem da lista
    endpoints = endpoints_dos_provedores(settings, {"visao"})
    com_visao = {e.provedor for e in endpoints}
    endpoints += [
        e for e in endpoints_dos_provedores(settings, {""}) if e.provedor not in com_visao
    ]
    if settings.gateway_url and settings.gateway_model:
        chave = settings.gateway_api_key or get_secret("ORION_GATEWAY_API_KEY")
        modelo = settings.vision_model or settings.gateway_model
        endpoints.append(Endpoint("gateway", settings.gateway_url, modelo, chave, timeout_s=90.0))
    return Vision(endpoints) if endpoints else None


def mcp_from_settings(settings: Settings) -> McpManager | None:
    """Servidores MCP do `mcp.json`; configuração inválida desliga o MCP, não o Orion."""
    if not settings.mcp_enabled:
        return None
    try:
        extras = (
            PluginStore(settings.effective_plugins_dir).servidores_mcp()
            if settings.plugins_enabled
            else {}
        )
        return manager_from_file(settings.effective_mcp_config, extras)
    except McpConfigError as e:
        log.error("MCP desligado: %s", e)
        return None


def _estado_da_tela(tela: ScreenMemory | None, memory: MemoryStore) -> dict[str, Any]:
    """Só contagens: o texto guardado nunca aparece no painel nem na API de estado."""
    if tela is None:
        return {"ligada": False}
    return {
        "ligada": True,
        "pausada": tela.pausada,
        "registros": memory.screen_count(),
        "hoje": memory.screen_count(time.time() - 86400),
        **tela.stats,
    }


def _privacidade(memory: MemoryStore, dias: int, agora: float) -> dict[str, Any]:
    """Por dia e provedor: envios, bytes e tipos de conteúdo; e o que saiu hoje, mais novo antes.
    Só tamanhos e tipos (REGRAS.md: toda saída de dado fica registrada, sem conteúdo); o que roda
    neste computador (`saidas.LOCAIS`) fica de fora."""
    hoje = datetime.fromtimestamp(agora).replace(hour=0, minute=0, second=0, microsecond=0)
    inicio = hoje.timestamp() - (dias - 1) * 86400
    linhas = [
        r
        for r in memory.external_calls(inicio, limite=50_000)
        if r["provider"] not in saidas.LOCAIS and not r["provider"].startswith("local")
    ]
    por_dia: dict[str, dict[str, dict[str, Any]]] = {}
    for r in linhas:
        dia = datetime.fromtimestamp(r["ts"]).strftime("%Y-%m-%d")
        p = por_dia.setdefault(dia, {}).setdefault(
            r["provider"], {"envios": 0, "bytes": 0, "tipos": {}}
        )
        p["envios"] += 1
        p["bytes"] += int(r["bytes_out"])
        tipo = r["content_kind"] or "texto"
        p["tipos"][tipo] = p["tipos"].get(tipo, 0) + 1
    dias_lista = [
        (hoje.timestamp() - i * 86400) for i in range(dias - 1, -1, -1)
    ]  # do mais velho para hoje
    serie = []
    for ts in dias_lista:
        dia = datetime.fromtimestamp(ts).strftime("%Y-%m-%d")
        provs = por_dia.get(dia, {})
        serie.append(
            {
                "dia": dia,
                "envios": sum(p["envios"] for p in provs.values()),
                "bytes": sum(p["bytes"] for p in provs.values()),
                "provedores": provs,
            }
        )
    de_hoje = [
        {
            "hora": datetime.fromtimestamp(r["ts"]).strftime("%H:%M:%S"),
            "provedor": r["provider"],
            "tipo": r["kind"],
            "conteudo": r["content_kind"] or "texto",
            "bytes": int(r["bytes_out"]),
            "ok": bool(r["ok"]),
        }
        for r in linhas
        if r["ts"] >= hoje.timestamp()
    ][:200]
    provedores = sorted({r["provider"] for r in linhas})
    return {"dias": dias, "serie": serie, "provedores": provedores, "hoje": de_hoje}


def screen_from_settings(settings: Settings, memory: MemoryStore) -> ScreenMemory | None:
    """Só com ORION_SCREEN_MEMORY=true. Sem tesseract a rodada avisa e não grava nada."""
    if not settings.screen_memory:
        return None
    langs = settings.screen_ocr_langs
    return ScreenMemory(
        memory,
        capture=capturador_de_tela(),
        ocr=lambda img: ocr_tesseract(img, langs=langs),
        title=titulo_da_janela,
        interval_s=settings.screen_interval_s,
        retention_days=settings.screen_retention_days,
        exclude=parse_lista(settings.screen_exclude),
        allow_unknown_title=settings.screen_allow_unknown_title,
        app=app_em_foco,
        exclude_apps=parse_lista(settings.screen_exclude_apps),
        locked=tela_bloqueada,
    )


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
    on_palmas: Callable[[], bool | None] | None = None,
) -> tuple[WakeListener | None, str]:
    """A escuta (palavra de ativação e/ou duas palmas), ou (None, motivo) se não pode subir. O
    motivo vai ao painel: a escuta nunca deixa de subir em silêncio."""
    if not (settings.wake_enabled or settings.clap_enabled):
        return None, ""
    try:
        detector = (
            criar_detector(
                settings.wake_engine,
                settings.wake_model,
                settings.wake_words,
                settings.wake_threshold,
            )
            if settings.wake_enabled
            else None
        )
        source = SoundDeviceSource(settings.wake_device)
    except ValueError as e:
        return None, str(e)
    except ImportError as e:
        return None, f"falta instalar {e.name or 'uma dependência'} (ver ORION_LEGADO.md §4.4)"
    except Exception as e:  # noqa: BLE001 — modelo corrompido, PortAudio ausente: motivo curto
        return None, f"não consegui preparar a escuta ({type(e).__name__})"
    cfg = WakeConfig(
        silencio_s=settings.wake_silence_s,
        max_s=settings.wake_max_s,
        max_por_hora=settings.wake_max_per_hour,
    )
    bipe = bipe_com_sounddevice() if settings.wake_beep else None
    palmas = ClapDetector(settings.clap_ratio) if settings.clap_enabled and on_palmas else None
    return (
        WakeListener(
            source,
            detector,
            on_fala,
            config=cfg,
            bipe=bipe,
            evento=evento,
            palmas=palmas,
            on_palmas=on_palmas,
            max_palmas_por_hora=settings.clap_max_per_hour,
        ),
        "",
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


def _iniciar_escuta(
    settings: Settings,
    ops: Operations,
    agent: Agent | None,
    transcriber: Transcriber | None,
    speaker: Speaker | None,
    stats: VozStats,
    fabrica: Callable[..., tuple[WakeListener | None, str]],
    *,
    ponte: PonteHub | None = None,
    modos: Modos | None = None,
    rotinas: ExecutorRotinas | None = None,
) -> tuple[tuple[WakeListener | None], str]:
    """Sobe a escuta (palavra de ativação e/ou duas palmas) numa thread. Devolve `((escuta,),
    motivo)`: o motivo explica por que não subiu (aparece no painel). A fala gravada segue o mesmo
    caminho do botão de microfone (`turno_de_voz`), com o canal "web": memória, política e audit
    valem igual. Palmas só abrem o Orion (REGRAS.md: palmas só abrem; nunca aprovam nem executam);
    só `abrir_e_ouvir` usa a voz."""
    if not (settings.wake_enabled or settings.clap_enabled):
        return (None,), ""
    precisa_voz = settings.wake_enabled or (
        settings.clap_enabled and settings.clap_action == "abrir_e_ouvir"
    )
    if precisa_voz:
        if not settings.voice_enabled:
            return (None,), "a escuta com voz usa a voz: ligue ORION_VOICE_ENABLED"
        if agent is None or transcriber is None:
            return (
                None,
            ), "precisa do gateway e da chave de transcrição (ORION_TRANSCRIBE_API_KEY)"
    loop = asyncio.get_running_loop()

    async def turno(wav: bytes) -> None:
        if agent is None or transcriber is None:
            return
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
                "action": "deny" if tipo in ("recusada", "palmas_recusadas") else "allow",
                "risk": "read",
                "reason": motivo,
            }
        )

    def bloqueio() -> str:
        if modos is None:
            return ""
        if modos.panico():
            return "pânico"
        return "não perturbe" if modos.nao_perturbe() else ""

    def abrir_navegador() -> object:
        return webbrowser.open(f"http://127.0.0.1:{settings.port}/ui/#/chat")

    on_palmas = (
        acao_das_palmas(
            settings.clap_action,
            abrir_ponte=lambda: bool(ponte and ponte.enviar({"cmd": "abrir", "rota": "#/chat"})),
            abrir_navegador=abrir_navegador,
            bloqueado=bloqueio,
            rotina=(
                (
                    lambda nome: asyncio.run_coroutine_threadsafe(
                        rotinas.executar(nome, "palmas"), loop
                    )
                )
                if rotinas is not None
                else None
            ),
        )
        if settings.clap_enabled
        else None
    )
    if on_palmas is not None:
        escuta, motivo = fabrica(settings, on_fala, evento, on_palmas=on_palmas)
    else:
        escuta, motivo = fabrica(settings, on_fala, evento)
    if escuta is None:
        if motivo:
            log.warning("escuta não subiu: %s", motivo)
        return (None,), motivo
    threading.Thread(target=escuta.run, name="orion-wake", daemon=True).start()
    return (escuta,), ""


def _sem_mcp(nome: str, args: dict[str, Any]) -> dict[str, Any]:
    return {"erro": "sem servidor MCP"}


class EscutaControle(BaseModel):
    ativa: bool


class PanicoControle(BaseModel):
    ativo: bool
    # só para sair (REGRAS.md: pânico é corte; não perturbe só segura)
    senha: str = Field(default="", max_length=1024)


class NaoPerturbeControle(BaseModel):
    ate: str | None = Field(default=None, pattern=r"^([01]\d|2[0-3]):[0-5]\d$")  # None: desliga


class Mensagem(BaseModel):
    texto: str = Field(min_length=1, max_length=8000)
    canal: str = Field(default="web", pattern=_CANAL)
    contexto: list[ContextRequest] = Field(default_factory=list, max_length=8)
    skills: list[str] = Field(default_factory=list, max_length=3)
    referencias: list[SkillReference] = Field(default_factory=list, max_length=8)
    # skill do catálogo simples (`orion/skills.py`) carregada à força no começo do turno (E7.6)
    skill: str | None = Field(default=None, pattern=r"^[a-z0-9][a-z0-9-]{0,63}$")


class PedidoEditado(BaseModel):
    texto: str = Field(min_length=1, max_length=8000)
    canal: str = Field(default="web", pattern=_CANAL)


class SessaoNova(BaseModel):
    project_id: str | None = Field(default=None, pattern=r"^[a-f0-9]{32}$")
    canal: str = Field(default="web", pattern=_CANAL)
    titulo: str | None = Field(default=None, max_length=120)


class SessaoEditar(BaseModel):
    canal: str = Field(default="web", pattern=_CANAL)
    titulo: str | None = Field(default=None, min_length=1, max_length=120)
    favorita: bool | None = None
    arquivada: bool | None = None


class SessaoAtivar(BaseModel):
    sessao_id: str = Field(pattern=r"^[a-f0-9]{32}$")
    canal: str = Field(default="web", pattern=_CANAL)


class Retomada(BaseModel):
    canal: str = Field(default="web", pattern=_CANAL)


class LimparResultados(BaseModel):
    model_config = ConfigDict(extra="forbid")
    manter: int = Field(ge=1, le=1000)
    nome: str | None = Field(default=None, min_length=1, max_length=120)
    simular: bool = False


def _converter_do_telegram(
    settings: Settings, biblioteca: Library, nome: str, dados: bytes, formato: str
) -> tuple[str, bytes]:
    """E9.9: o arquivo mandado pelo Telegram vai para uma pasta temporária, é convertido no
    computador e a cópia fica na biblioteca de resultados; só o convertido volta."""
    base = settings.data_dir / "conversoes"
    base.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="tg-", dir=base) as tmp:
        origem = Path(tmp) / nome_seguro(nome)
        origem.write_bytes(dados)
        r = converter_e_guardar(origem, formato, Path(tmp) / "saida", biblioteca)
        arquivo = r["arquivo"]
        if arquivo.stat().st_size > MAX_ENVIO_TELEGRAM:
            raise ConversaoError("o resultado passa de 50 MB (limite do Telegram)")
        return arquivo.name, arquivo.read_bytes()


def _conferir_desligar(settings: Settings, tarefas_bg: Any, downloads: Any) -> dict[str, Any]:
    """E9.5: lê o que ficaria pela metade (repositórios, tarefa, backup, downloads)."""
    pasta = (
        Path(downloads.estado()["pasta"]) if downloads is not None else Path.home() / "Downloads"
    )
    return posso_desligar(
        repos=list(settings.repos),
        backup_dir=settings.effective_backup_dir,
        downloads_dir=pasta,
        tarefas=tarefas_bg.listar() if tarefas_bg is not None else [],
    )


class LinkLerDepois(BaseModel):
    model_config = ConfigDict(extra="forbid")
    url: str = Field(min_length=8, max_length=2000)


class AreaTexto(BaseModel):
    model_config = ConfigDict(extra="forbid")
    texto: str = Field(min_length=1, max_length=20_000)


class DesfazerDownloads(BaseModel):
    model_config = ConfigDict(extra="forbid")
    escopo: str = Field(default="ultimo", pattern="^(ultimo|hoje)$")


class FraseRotina(BaseModel):
    model_config = ConfigDict(extra="forbid")
    texto: str = Field(min_length=1, max_length=200)


class TarefaBgPedido(BaseModel):
    model_config = ConfigDict(extra="forbid")
    objetivo: str = Field(min_length=1, max_length=500)
    max_passos: int = Field(default=10, ge=1, le=30)
    max_min: int = Field(default=10, ge=1, le=30)


class PropostaDecisao(BaseModel):
    model_config = ConfigDict(extra="forbid")
    acao: str = Field(pattern="^(aceitar|descartar)$")


class PromptMcp(BaseModel):
    model_config = ConfigDict(extra="forbid")
    argumentos: dict[str, str] = Field(default_factory=dict, max_length=20)


class AssuntosPesquisa(BaseModel):
    model_config = ConfigDict(extra="forbid")
    assuntos: list[str] = Field(max_length=20)


async def _turno_de_rotina(state: AppState, nome: str) -> AsyncIterator[AgentEvent]:
    """A frase de uma rotina concedida roda a rotina em vez de ir ao modelo (REGRAS.md: automação
    sua vale pelo arquivo concedido)."""
    assert state.executor_rotinas is not None
    r = await state.executor_rotinas.executar(nome, "frase")
    icone = {"concluida": "✅", "aguardando_aprovacao": "⏸", "recusada": "⛔"}.get(r.status, "⚠️")
    texto = f"{icone} Rotina {nome}: {r.status.replace('_', ' ')}"
    yield AgentEvent("text", {"text": texto + (f" — {r.motivo}" if r.motivo else "")})
    a = state.policy.approvals.get(r.aprovacao_id) if r.aprovacao_id else None
    if a is not None:
        yield AgentEvent(
            "approval",
            {
                "id": a.id,
                "tool": a.tool,
                "reason": a.reason,
                "args": redact(a.args, limite=2000),
                "args_truncated": False,
            },
        )
    yield AgentEvent("done", {"provenance": {"rotina": nome, "passos": r.passos}})


def _iso(ts: float) -> str:
    return datetime.fromtimestamp(ts, UTC).isoformat()


def _pesquisa_noturna(settings: Settings, agent: Any, memory: MemoryStore) -> Any:
    """Só existe com agente, ferramentas de web e vault: tudo escolhido por você. Sem hora
    (`ORION_RESEARCH_AT`) ela não roda sozinha, só no `rodar agora` da interface. Os assuntos
    são os editados na interface (E6.5) ou, na falta deles, `ORION_RESEARCH_TOPICS`."""
    from .research import NightResearch, assuntos_salvos

    if agent is None or not settings.web_tools or settings.vault_dir is None:
        if settings.research_at:
            log.warning("pesquisa noturna pedida, mas faltam gateway, ORION_WEB_TOOLS ou o vault")
        return None
    return NightResearch(
        memory=memory,
        run_turn=lambda canal, texto: agent.run(canal, texto, read_only=True),
        capturer=Capturer(settings.vault_dir, settings.capture_folder),
        assuntos=lambda: assuntos_salvos(memory, settings.research_topics)[0],
        at=settings.research_at,
        clock=time.time,
    )


def sse(ev: AgentEvent) -> str:
    """Mesmo formato do /chat do legado (`text`, `tier`, `[DONE]`) + `tool`/`approval`/`error`."""
    d = ev.data
    if ev.kind == "done":
        payload = json.dumps(
            {
                "provenance": d.get("provenance", {}),
                **({"message_id": d["message_id"]} if "message_id" in d else {}),
                **({"session_id": d["session_id"]} if "session_id" in d else {}),
            },
            ensure_ascii=False,
        )
        return f"data: {payload}\n\ndata: [DONE]\n\n"
    corpo = {
        "text": {"text": d.get("text")},
        "tier": {"tier": f"{d.get('endpoint')}/{d.get('model')}"},
        "tool": {"tool": d},
        "activity": {"tool": d},
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
        "palmas": {
            "pedida": settings.clap_enabled,
            "ouvindo": bool(escuta and escuta.stats.ouvindo and settings.clap_enabled),
            "acao": settings.clap_action,
            "acionadas": escuta.stats.palmas if escuta else 0,
            "recusadas": escuta.stats.palmas_recusadas if escuta else 0,
            "picos_ultima_hora": escuta.picos_de_palmas() if escuta else [],
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


async def _vigiar_modos(modos: Modos, escuta: WakeListener | None, a_cada_s: float = 2.0) -> None:
    """Aplica o pânico à palavra de ativação mesmo quando quem ligou foi a CLI (o estado mora no
    banco). Só retoma a escuta que o próprio pânico pausou (REGRAS.md: a sua pausa continua sua)."""
    pausada_pelo_panico = False
    while True:
        try:
            panico = await asyncio.to_thread(modos.panico)
            if escuta is not None:
                if panico and not escuta.stats.pausada:
                    escuta.pausar()
                    pausada_pelo_panico = True
                elif not panico and pausada_pelo_panico:
                    escuta.retomar()
                    pausada_pelo_panico = False
        except asyncio.CancelledError:
            raise
        except Exception:
            log.exception("vigia dos modos falhou")
        await asyncio.sleep(a_cada_s)


def _senha_de_novo(state: AppState, senha: str, cliente: str) -> bool:
    """Sair do pânico pelo painel pede a senha de novo (REGRAS.md: pânico é corte; não perturbe só
    segura), com a mesma trava do login.
    Sem senha definida (só token de máquina), vale o próprio token."""
    if not state.auth.has_password():
        esperado = state.settings.admin_token
        return bool(esperado) and hmac.compare_digest(senha.encode(), esperado.encode())
    if state.auth.throttle.espera(cliente):
        return False
    if len(senha) <= 1024 and state.auth.check_password(senha):
        state.auth.throttle.acertou(cliente)
        return True
    state.auth.throttle.falhou(cliente)
    return False


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
    screen_factory: Callable[[Settings, MemoryStore], ScreenMemory | None] | None = None,
) -> FastAPI:
    settings = settings or Settings()

    @asynccontextmanager
    async def lifespan(app: FastAPI) -> AsyncIterator[None]:
        settings.data_dir.mkdir(parents=True, exist_ok=True)
        memory = (memory_factory or memory_from_settings)(settings)
        if settings.rerank:  # E4.6: pronto e desligado; sem o modelo, segue sem reranker
            from .memory.rerank import reranker_da_pasta

            memory.reranker = reranker_da_pasta(
                settings.rerank_model_dir or settings.data_dir / "modelos" / "rerank"
            )
        # registro de saída (REGRAS.md: toda saída de dado fica registrada, sem conteúdo)
        saidas.definir_destino(memory.add_external_call)
        from .documents import recuperar_interrompidos

        recuperar_interrompidos(memory)  # indexação que a queda do Orion deixou pela metade
        ops = Operations(memory)
        modos = Modos(memory, dnd_at=settings.dnd_at, audit=ops.audit_add)
        ops.segurar = modos.nao_perturbe  # não perturbe segura os avisos não urgentes
        custos = Custos(memory, cotas_de(settings), ops=ops)
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
                "(REGRAS.md: acesso de fora só com login)"
            )
        gateway = (gateway_factory or gateway_from_settings)(settings)
        if gateway is not None:  # orçamento diário por provedor (`limite_dia`): fim da fila
            gateway.orcamento = lambda ep: custos.no_orcamento(ep.provedor)
        transcriber = (transcriber_factory or transcriber_from_settings)(settings)
        tela = (screen_factory or screen_from_settings)(settings, memory)
        historicos = Historicos(memory)
        biblioteca = Library(memory, settings.data_dir / "resultados")
        agent = None
        indice = None
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
            indice = Indice(mcp_tools, settings.mcp_index_at)  # só as de servidor MCP (E7.4)
            if indice.ativo:
                policy.register_tool(INDICE_SPEC)
                mcp_tools = [*mcp_tools, indice.tool()]
            if mcp is not None and mcp.recursos_ativos():
                policy.register_tool(SPEC_RECURSO)
                mcp_tools = [*mcp_tools, mcp.tool_recurso()]
            if settings.desktop_tools:
                processos = ProcessManager(settings.data_dir / "processos")
            skills = (
                SkillCatalog(
                    settings.effective_skills_dir,
                    PluginStore(settings.effective_plugins_dir).pastas_de_skills()
                    if settings.plugins_enabled
                    else [],
                )
                if settings.skills_enabled
                else None
            )
            if skills is not None and skills.skills:
                policy.register_tool(SKILL_SPEC)
                mcp_tools = [*mcp_tools, skill_tool(skills)]
            mcp_tools = [
                *mcp_tools,
                *historicos_tools(
                    historicos,
                    policy.tools,
                    area=settings.clipboard_days > 0,
                    comandos=settings.command_history,
                ),
            ]
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
                library=biblioteca,
                modos=modos,
                compactor=compactor_from_settings(settings, memory),
                servidor_mcp=getattr(mcp, "servidor_de", None),
                mcp_indice=indice,
            )
        jobs, tarefa_jobs, agenda = None, None, None
        pesquisa = _pesquisa_noturna(settings, agent, memory)
        downloads = Downloads(
            settings.data_dir / "regras_downloads.yaml", memory, policy.path_guard
        )
        ler_depois = LerDepois(
            memory,
            vault_dir=settings.vault_dir,
            web=settings.web_tools,
            gateway=gateway,
            modos=modos,
        )
        rotinas = RotinaStore(settings.data_dir / "rotinas", memory)
        executor_rotinas = ExecutorRotinas(
            store=rotinas,
            tools=agent.tools if agent is not None else ToolRegistry([]),
            policy=policy,
            memory=memory,
            ops=ops,
            briefing=lambda: build_briefing(ops, time.time(), agenda=agenda),
            modos=modos,
        )
        tarefas_bg = (
            TarefasBg(
                memory=memory,
                ops=ops,
                run_turn=lambda canal, texto: agent.run(canal, texto, read_only=True),
                custos=custos,
                modos=modos,
            )
            if agent is not None
            else None
        )
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
            sono = (
                SleepCycle(
                    memory,
                    ops,
                    gateway if gateway is not None and hasattr(gateway, "complete") else None,
                    at=settings.sleep_at,
                )
                if settings.sleep_at
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
                briefing_at=settings.briefing_at,
                agenda=agenda,
                research=pesquisa,
                sleep=sono,
                screen=tela,
                downloads=downloads,
                historicos=historicos,
                clipboard_days=settings.clipboard_days,
                command_history=settings.command_history,
                weekly_ai=(
                    gateway
                    if settings.weekly_ai and gateway is not None and hasattr(gateway, "complete")
                    else None
                ),
                custos=custos,
                modos=modos,
                gateway_down_min=settings.gateway_down_min,
            )
            tarefa_jobs = asyncio.create_task(jobs.run_forever(settings.jobs_tick_s))
        telegram = (telegram_factory or telegram_from_settings)(
            settings, agent, memory, policy, ops
        )
        tarefa_telegram = asyncio.create_task(telegram.run()) if telegram is not None else None
        if agent is not None:
            Research(settings.research).attach(agent.tools, policy)
        from .accounts import Accounts

        accounts = Accounts(memory)
        mcp_host = MCPHost(settings.mcp_connections, oauth_factory=accounts.provider)
        accounts.host = mcp_host
        await mcp_host.start()
        catalog = Catalog(mcp_host, agent.tools if agent else ToolRegistry(), policy)
        if catalog is not None and agent is not None:
            agent.refresh_tools = catalog.refresh
            try:
                await catalog.refresh()
            except Exception:  # noqa: BLE001 — catálogo externo não derruba chat nativo
                log.warning("mcp_catalog_unavailable")
        from .calendar import Calendar

        calendar = Calendar(memory, mcp_host, catalog.registry, policy)
        from .calendar_events import Events

        events = Events(calendar)
        from .file_plans import FilePlans

        file_plans = FilePlans(events)
        from .development import Development
        from .export_credentials import ExportCredentials

        Development(memory, catalog.registry, policy)
        skill_runtime = await asyncio.to_thread(SkillRuntime, settings.skill_sources)
        if agent is not None:
            skill_runtime.attach_tools(agent.tools, policy)
        extensions = PluginManager(
            settings.data_dir / "extensions", skill_runtime, mcp_host, catalog, policy
        )
        try:
            urls_catalogo = urls_validas(settings.plugin_catalogs)
        except ValueError as e:
            log.warning("ORION_PLUGIN_CATALOGS ignorado: %s", e)
            urls_catalogo = []
        extensions.catalogos = Catalogos(settings.effective_catalog_dir, urls_catalogo)
        voz_stats = VozStats()
        ponte_hub = PonteHub()
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
            ponte=ponte_hub,
            modos=modos,
            rotinas=executor_rotinas,
        )
        tarefa_modos = asyncio.create_task(_vigiar_modos(modos, escuta[0]))
        painel = Painel(
            started_at=time.time(),
            memory=memory,
            ops=ops,
            policy=policy,
            modos=modos,
            custos=custos,
            agent=agent,
            jobs=jobs,
            mcp=mcp,
            delegator=delegador,
            telegram_ativo=lambda: telegram is not None,
            tela=lambda: _estado_da_tela(tela, memory),
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
            telegram.modos = modos
            telegram.tela = lambda acao: comando_tela(tela, memory, acao)
            telegram.agenda = agenda
            telegram.rotinas = rotinas
            telegram.ler_depois = ler_depois
            if settings.telegram_voice_reply:
                telegram.speaker = EdgeSpeaker(settings.voice_tts_voice)
            telegram.converter = lambda nome, dados, formato: _converter_do_telegram(
                settings, biblioteca, nome, dados, formato
            )
            telegram.desligar = lambda: texto_desligar(
                _conferir_desligar(settings, tarefas_bg, downloads)
            )
            telegram.executar_rotina = executor_rotinas.executar
        app.state.orion = AppState(
            settings,
            memory,
            policy,
            painel.started_at,
            ops,
            auth,
            agent,
            jobs,
            skills=skill_runtime,
            catalog=catalog,
            mcp_host=mcp_host,
            extensions=extensions,
            accounts=accounts,
            calendar=calendar,
            events=events,
            file_plans=file_plans,
            export_credentials=ExportCredentials(memory),
            telegram=telegram,
            mcp=mcp,
            painel=painel,
            transcriber=transcriber,
            speaker=speaker,
            live=live,
            voz=voz_stats,
            escuta=escuta[0],
            tela=tela,
            biblioteca=biblioteca,
            modos=modos,
            custos=custos,
            pesquisa=pesquisa,
            tarefas_bg=tarefas_bg,
            downloads=downloads,
            historicos=historicos,
            ler_depois=ler_depois,
            rotinas=rotinas,
            executor_rotinas=executor_rotinas,
            ponte=ponte_hub,
        )
        try:
            async with mcp_export.server.session_manager.run():
                yield
        finally:
            if escuta[0] is not None:
                escuta[0].parar()  # a thread é daemon; o microfone fecha no `finally` do laço
            for tarefa in (tarefa_jobs, tarefa_telegram, tarefa_modos):
                if tarefa is not None:
                    tarefa.cancel()
                    with contextlib.suppress(asyncio.CancelledError):
                        await tarefa
            await ler_depois.encerrar()
            if tarefas_bg is not None:
                await tarefas_bg.encerrar()
            if telegram is not None:
                await telegram.aclose()
            await accounts.close()
            await extensions.close()
            await skill_runtime.close()
            await mcp_host.close()
            if mcp is not None:
                await asyncio.to_thread(mcp.stop)
            if gateway is not None and hasattr(gateway, "aclose"):
                await gateway.aclose()
            if transcriber is not None:
                await transcriber.aclose()
            saidas.definir_destino(None)
            auth.close()
            memory.close()

    # sem /docs, /redoc nem /openapi.json: o mapa da API não é público (REGRAS.md: acesso de fora só
    # com login)
    app = FastAPI(
        title="Orion",
        version=__version__,
        lifespan=lifespan,
        docs_url=None,
        redoc_url=None,
        openapi_url=None,
    )
    from .mcp_export import Export, ExportAuth

    mcp_export = Export(lambda: app.state.orion)
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
            admin_configured=bool(state.settings.admin_token) or state.auth.has_password(),
            voice=state.live is not None,
            tts=state.speaker is not None,
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
            "pending_notifications": len(state.ops.pending_notifications(limit=1000, todos=True)),
        }

    def project_archived(session: Session | None) -> bool:
        return bool(
            session
            and session.project_id
            and app.state.orion.memory.query(
                "SELECT archived FROM projects WHERE id=?", (session.project_id,)
            )[0][0]
        )

    def sessao_json(session: Session, ativa: str | None) -> dict[str, Any]:
        return {
            "sessao_id": session.id,
            "titulo": session.title or "Conversa sem título",
            "canal": session.channel,
            "project_id": session.project_id,
            "criada": datetime.fromtimestamp(session.created_at, UTC).isoformat(),
            "ultima_atividade": datetime.fromtimestamp(session.last_active_at, UTC).isoformat(),
            "ativa": session.id == ativa,
            "favorita": session.favorite,
            "arquivada": session.archived,
            "importada": session.read_only,
            "somente_leitura": session.archived or session.read_only or project_archived(session),
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
            raise HTTPException(
                503, "modelos não configurados (ORION_PROVEDORES ou ORION_GATEWAY_URL/MODEL)"
            )
        return state.agent

    @app.get("/sessoes", dependencies=[Admin])
    def sessoes_listar(
        state: State,
        canal: Annotated[str, Query(pattern=_CANAL)] = "web",
        limite: Annotated[int, Query(ge=1, le=100)] = 50,
        arquivadas: bool | None = None,
        projeto: Annotated[str | None, Query(max_length=64)] = None,
    ) -> dict[str, Any]:
        """`arquivadas=true`: só as arquivadas. `projeto=<id>` ou `nenhum` (sem projeto)."""
        selected = state.memory.selected_session(canal)
        ativa = selected.id if selected else None
        sessoes = [
            sessao_json(s, ativa)
            for s in state.memory.list_sessions(
                canal, limite, archived=arquivadas or None, project=projeto or None
            )
        ]
        return {"sessoes": sessoes, "total": len(sessoes), "ativa": ativa}

    @app.get("/sessoes/busca", dependencies=[Admin])
    def buscar_sessoes(
        state: State,
        texto: Annotated[str, Query(min_length=1, max_length=200)],
        canal: Annotated[str, Query(pattern=_CANAL)] = "web",
        limite: Annotated[int, Query(ge=1, le=100)] = 25,
        offset: Annotated[int, Query(ge=0, le=1_000_000)] = 0,
        arquivadas: bool | None = None,
    ) -> dict[str, Any]:
        sessions, total = state.memory.search_sessions(
            canal, texto, limit=limite, offset=offset, archived=arquivadas or None
        )
        selected = state.memory.selected_session(canal)
        items = [
            {**sessao_json(s, selected.id if selected else None), "trecho": snippet}
            for s, snippet in sessions
        ]
        next_offset = offset + len(items)
        return {
            "sessoes": items,
            "total": total,
            "mais": next_offset < total,
            "proximo_offset": next_offset if next_offset < total else None,
        }

    @app.post("/sessoes", dependencies=[Admin])
    def sessao_nova(state: State, corpo: SessaoNova | None = None) -> dict[str, Any]:
        corpo = corpo or SessaoNova()
        try:
            session = state.memory.new_session(
                corpo.canal, (corpo.titulo or "").strip() or None, project_id=corpo.project_id
            )
        except ValueError:
            raise HTTPException(422, "project_unavailable") from None
        return {"ok": True, **sessao_json(session, session.id), "mensagens": []}

    @app.post("/sessoes/ativar", dependencies=[Admin])
    def sessao_ativar(corpo: SessaoAtivar, state: State) -> dict[str, Any]:
        try:
            session = state.memory.activate_session(corpo.canal, corpo.sessao_id)
        except KeyError:
            # Mesmo resultado para inexistente e sessão de outro canal: não revela existência.
            raise HTTPException(404, "sessão inexistente neste canal") from None
        except ValueError as e:
            raise HTTPException(409, str(e)) from None
        # Snapshot compatível com o adaptador e com o limite de contexto após limpeza.
        mensagens = [
            {
                "role": m.role,
                "content": m.text,
                "timestamp": datetime.fromtimestamp(m.created_at, UTC).isoformat(),
                "provenance": m.provenance,
            }
            for m in state.memory.context_history(session.id, limit=50)
            if m.role in {"user", "assistant"}
        ]
        return {"ok": True, **sessao_json(session, session.id), "mensagens": mensagens}

    @app.patch("/sessoes/{session_id}", dependencies=[Admin])
    async def sessao_editar(
        session_id: Annotated[str, PathParam(pattern=r"^[a-f0-9]{32}$")],
        corpo: SessaoEditar,
        state: State,
    ) -> dict[str, Any]:
        session = conversation(state, corpo.canal, session_id)
        assert session is not None
        if corpo.arquivada and (
            state.policy.approvals.unresolved(session_id)
            or (state.agent and state.agent.busy(session_id))
        ):
            raise HTTPException(409, "termine a resposta e resolva as aprovações antes de arquivar")
        try:
            updated = state.memory.edit_session(
                corpo.canal,
                session_id,
                title=corpo.titulo,
                favorite=corpo.favorita,
                archived=corpo.arquivada,
            )
        except KeyError:
            raise HTTPException(404, "sessão inexistente neste canal") from None
        except ValueError as e:
            raise HTTPException(409, str(e)) from None
        selected = state.memory.selected_session(corpo.canal)
        return {"ok": True, **sessao_json(updated, selected.id if selected else None)}

    @app.delete("/sessoes/{session_id}", dependencies=[Admin])
    def sessao_apagar(
        session_id: Annotated[str, PathParam(pattern=r"^[a-f0-9]{32}$")],
        state: State,
        canal: Annotated[str, Query(pattern=_CANAL)] = "web",
    ) -> dict[str, Any]:
        """Só conversa arquivada e sem nada apontando para ela (ramos, artefatos, planos)."""
        if state.policy.approvals.unresolved(session_id) or (
            state.agent and state.agent.busy(session_id)
        ):
            raise HTTPException(409, "termine a resposta e resolva as aprovações antes de apagar")
        try:
            state.memory.delete_session(canal, session_id)
        except KeyError:
            raise HTTPException(404, "sessão inexistente neste canal") from None
        except ValueError as e:
            raise HTTPException(409, str(e)) from None
        return {"ok": True}

    @app.get("/sessoes/{session_id}/resumo", dependencies=[Admin])
    def sessao_resumo(
        session_id: Annotated[str, PathParam(pattern=r"^[a-f0-9]{32}$")], state: State
    ) -> dict[str, Any]:
        """O resumo gerado do começo da conversa (auto-compact, REGRAS.md: resumo da conversa nunca
        substitui o registro), se existe. É um texto
        gerado por um modelo local e pode ter erro; o registro da conversa não muda."""
        if state.memory.get_session(session_id) is None:
            raise HTTPException(404, "sessão inexistente")
        r = state.memory.get_summary(session_id)
        if r is None:
            return {"resumo": None}
        return {
            "resumo": {
                "texto": r["text"],
                "ate_mensagem": r["upto_message_id"],
                "modelo": r["model"],
                "criado": r["created_at"],
                "gerado": True,
            }
        }

    @app.delete("/sessoes/{session_id}/resumo", dependencies=[Admin])
    def sessao_resumo_descartar(
        session_id: Annotated[str, PathParam(pattern=r"^[a-f0-9]{32}$")], state: State
    ) -> dict[str, bool]:
        """Descarta o resumo (o modelo volta a ver só a janela de contexto até gerar outro)."""
        if state.memory.get_session(session_id) is None:
            raise HTTPException(404, "sessão inexistente")
        return {"ok": state.memory.drop_summary(session_id)}

    def conversation(state: AppState, canal: str, sessao: str | None) -> Session | None:
        session = (
            state.memory.get_session(sessao) if sessao else state.memory.selected_session(canal)
        )
        if sessao and (session is None or session.channel != canal):
            raise HTTPException(404, "sessão inexistente neste canal")
        return session

    def mensagem_json(m: Message, state: AppState | None = None) -> dict[str, Any]:
        d: dict[str, Any] = {
            "id": m.id,
            "role": m.role,
            "content": m.text,
            "timestamp": datetime.fromtimestamp(m.created_at, UTC).isoformat(),
            "provenance": m.provenance,
        }
        if state is not None and m.role == "user":
            versoes = state.memory.message_versions(m.session_id, m.version_of or m.id)
            if len(versoes) > 1:  # E3.5: o pedido foi editado; as setas ‹ n/m › trocam a exibição
                d["versoes"] = [
                    {"id": v.id, "texto": v.text, "atual": v.id == m.id} for v in versoes
                ]
        return d

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
        session = conversation(state, canal, sessao)
        messages, total, cursor = (
            state.memory.history_page(session.id, limit=limite, before=antes, complete=completo)
            if session
            else ([], 0, None)
        )
        return {
            "sessao": session.id if session else None,
            "total": total,
            "mensagens": [mensagem_json(m, state) for m in messages],
            "mais": cursor is not None,
            "proximo_antes": cursor,
            "somente_leitura": bool(
                session and (session.archived or session.read_only or project_archived(session))
            ),
        }

    @app.post("/historico/{msg_id}/editar", dependencies=[Admin])
    async def editar_pedido(msg_id: int, corpo: PedidoEditado, state: State) -> StreamingResponse:
        """Reescreve um pedido como nova versão e refaz o turno (E3.5). O pedido antigo e tudo que
        veio depois ficam guardados, fora do contexto. Bloqueado com resposta em andamento ou
        aprovação pendente (a mesma regra de "limpar")."""
        agente = _agente(state)
        session = state.memory.active_session(corpo.canal)
        if session.archived or session.read_only or project_archived(session):
            raise HTTPException(409, "conversa somente leitura")
        if state.policy.approvals.unresolved(session.id) or agente.busy(session.id):
            raise HTTPException(409, "termine a resposta e resolva as aprovações antes de editar")
        achou = state.memory.query(
            "SELECT 1 FROM messages WHERE id=? AND session_id=? AND role='user' AND superseded=0",
            (msg_id, session.id),
        )
        if not achou:
            raise HTTPException(404, "pedido não encontrado nesta conversa")
        return StreamingResponse(
            _stream(
                agente.run(
                    corpo.canal,
                    corpo.texto,
                    expected_session=session.id,
                    edit_message=msg_id,
                )
            ),
            media_type="text/event-stream",
        )

    @app.delete("/historico", dependencies=[Admin])
    async def limpar_historico(
        state: State,
        canal: str = "web",
        sessao: Annotated[str | None, Query(max_length=64)] = None,
    ) -> dict[str, Any]:
        session = conversation(state, canal, sessao)
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
        session = conversation(state, canal, sessao)
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

    @app.get("/skills", dependencies=[Admin])
    def listar_skills(
        state: State, canal: Annotated[str, Query(pattern=_CANAL)] = "web"
    ) -> list[dict]:
        session = state.memory.selected_session(canal)
        scope = f"project:{session.project_id}" if session and session.project_id else "personal"
        return [s for s in state.skills.summaries() if s["scope"] == scope] if state.skills else []

    @app.post("/chat", dependencies=[Admin])
    async def chat(corpo: Mensagem, state: State) -> StreamingResponse:
        agente = _agente(state)
        if state.rotinas is not None and state.executor_rotinas is not None:
            rotina = state.rotinas.por_frase(corpo.texto)  # frase EXATA de uma rotina concedida
            if rotina is not None:
                return StreamingResponse(
                    _stream(_turno_de_rotina(state, rotina.nome)), media_type="text/event-stream"
                )
        session = state.memory.active_session(corpo.canal)
        scope = f"project:{session.project_id}" if session.project_id else "personal"
        selection = None
        if state.skills is not None:
            try:
                selection = await asyncio.to_thread(
                    state.skills.select, corpo.texto, corpo.skills, corpo.referencias, context=scope
                )
            except SkillError as error:
                raise HTTPException(422, str(error)) from error
        external = []
        if corpo.contexto:
            if state.mcp_host is None:
                raise HTTPException(503, "mcp_unavailable")
            try:
                external = await ContextReader(state.mcp_host).selected(corpo.contexto, scope)
            except MCPError as error:
                raise HTTPException(422, error.code) from error
        return StreamingResponse(
            _stream(
                agente.run(
                    corpo.canal,
                    corpo.texto,
                    external=external,
                    selection=selection,
                    expected_session=session.id,
                    skill_forcada=corpo.skill,
                )
            ),
            media_type="text/event-stream",
        )

    @app.get("/skills/catalogo", dependencies=[Admin])
    def catalogo_de_skills(state: State) -> dict[str, Any]:
        """As skills da sua pasta `skills/` (nome e descrição): para `/skill` e a paleta."""
        skills = state.agent._skills if state.agent is not None else None
        itens = [
            {"nome": s.nome, "descricao": s.descricao}
            for s in (skills.skills.values() if skills else [])
        ]
        return {"total": len(itens), "skills": itens}

    # ── biblioteca de resultados (C34/C35): o que o Orion gerou, com origem e versões ──
    def _biblioteca(state: AppState) -> Library:
        assert state.biblioteca is not None
        return state.biblioteca

    @app.get("/resultados", dependencies=[Admin])
    def listar_resultados(state: State, projeto: str | None = None) -> dict[str, Any]:
        itens = [
            {
                "id": a["id"],
                "nome": a["name"],
                "tipo": a["kind"],
                "versao": a["version"],
                "anterior": a["parent_id"],
                "bytes": a["bytes"],
                "ferramenta": a["tool"],
                "conversa": a["sessao_titulo"],
                "sessao_id": a["session_id"],
                "projeto_id": a["project_id"],
                "criado": _iso(a["created_at"]),
                "previa": (
                    "texto"
                    if Path(a["name"]).suffix.lower() in PREVIA_TEXTO
                    else "imagem"
                    if Path(a["name"]).suffix.lower() in PREVIA_IMAGEM
                    else "pdf"
                    if Path(a["name"]).suffix.lower() == PREVIA_PDF
                    else None
                ),
            }
            for a in state.memory.list_artifacts(projeto)
        ]
        return {"total": len(itens), "resultados": itens}

    @app.get("/resultados/{rid}/texto", dependencies=[Admin])
    def previa_de_texto(rid: int, state: State) -> dict[str, Any]:
        caminho = _biblioteca(state).caminho(rid)
        if caminho is None:
            raise HTTPException(404, "resultado não encontrado")
        if caminho.suffix.lower() not in PREVIA_TEXTO:
            raise HTTPException(415, "sem prévia de texto para este tipo (baixe o arquivo)")
        texto = caminho.read_text(encoding="utf-8", errors="replace")
        return {"texto": texto[:20_000], "truncado": len(texto) > 20_000}

    @app.get("/resultados/{rid}/arquivo", dependencies=[Admin])
    def arquivo_do_resultado(rid: int, state: State, previa: bool = False) -> FileResponse:
        """Download (sempre como anexo, nunca renderizado); com `previa=true`, só imagem raster e
        PDF são entregues inline. HTML/SVG gerados pelo modelo nunca executam no navegador."""
        biblioteca = _biblioteca(state)
        caminho = biblioteca.caminho(rid)
        a = state.memory.get_artifact(rid)
        if caminho is None or a is None:
            raise HTTPException(404, "resultado não encontrado")
        cabecalhos = {"X-Content-Type-Options": "nosniff", "Content-Security-Policy": "sandbox"}
        sufixo = Path(a["name"]).suffix.lower()
        mime = PREVIA_IMAGEM.get(sufixo)
        if previa and mime:
            return FileResponse(caminho, media_type=mime, headers=cabecalhos)
        if previa and sufixo == PREVIA_PDF:
            # o visualizador de PDF do navegador não abre sob `sandbox`: aqui vale a CSP vazia
            return FileResponse(
                caminho,
                media_type="application/pdf",
                filename=a["name"],
                content_disposition_type="inline",
                headers={
                    "X-Content-Type-Options": "nosniff",
                    "Content-Security-Policy": "default-src 'none'",
                },
            )
        return FileResponse(
            caminho,
            media_type="application/octet-stream",
            filename=a["name"],
            headers=cabecalhos,
        )

    @app.get("/resultados/{rid}/diff", dependencies=[Admin])
    def diff_do_resultado(rid: int, com: int, state: State) -> dict[str, Any]:
        """Compara a versão `rid` (nova) com `com` (base): linhas de `difflib.unified_diff`."""
        try:
            achado = _biblioteca(state).diff(rid, com)
        except ValueError as e:
            raise HTTPException(415, str(e)) from None
        if achado is None:
            raise HTTPException(404, "resultado não encontrado")
        return achado

    @app.post("/resultados/limpar", dependencies=[Admin])
    def limpar_resultados(corpo: LimparResultados, state: State) -> dict[str, Any]:
        """Apaga as versões além das `manter` mais novas. `simular` só conta (o front mostra o
        total e pede confirmação antes de apagar de verdade)."""
        r = _biblioteca(state).limpar(corpo.manter, corpo.nome, simular=corpo.simular)
        if not corpo.simular and r["apagados"]:
            audit_log.info(
                "resultados_limpos",
                extra={"audit": {"manter": corpo.manter, "apagados": r["apagados"]}},
            )
        return r

    @app.delete("/resultados/{rid}", dependencies=[Admin])
    def apagar_resultado(rid: int, state: State) -> dict[str, bool]:
        if not _biblioteca(state).apagar(rid):
            raise HTTPException(404, "resultado não encontrado")
        audit_log.info("resultado_apagado", extra={"audit": {"resultado": rid}})
        return {"ok": True}

    # ── diagnóstico do cliente MCP (E7.3): o que o mcp.json subiu, e o que não ──
    @app.get("/mcp/servidores", dependencies=[Admin])
    def servidores_mcp(state: State) -> dict[str, Any]:
        if state.mcp is None:
            return {"ativo": False, "servidores": []}
        return {"ativo": True, "servidores": state.mcp.diagnostico()}

    @app.post("/mcp/servidores/{nome}/testar", dependencies=[Admin])
    def testar_servidor_mcp(nome: str, state: State) -> dict[str, Any]:
        if state.mcp is None:
            raise HTTPException(409, "nenhum servidor MCP configurado em mcp.json")
        resultado = state.mcp.testar(nome)
        audit_log.info(
            "mcp_teste", extra={"audit": {"servidor": nome, "ok": bool(resultado.get("ok"))}}
        )
        if resultado.get("erro") == "servidor desconhecido":
            raise HTTPException(404, "servidor desconhecido")
        return resultado

    # ── prompts de servidores MCP (E7.5, REGRAS.md: recurso e prompt de MCP são dado, não comando):
    # só você invoca, o modelo não os vê ──
    @app.get("/mcp/prompts", dependencies=[Admin])
    def prompts_mcp(state: State) -> dict[str, Any]:
        itens = state.mcp.listar_prompts() if state.mcp is not None else []
        return {"total": len(itens), "prompts": itens}

    @app.post("/mcp/prompts/{servidor}/{nome}", dependencies=[Admin])
    def obter_prompt_mcp(
        servidor: str, nome: str, corpo: PromptMcp, state: State
    ) -> dict[str, Any]:
        """Devolve o texto do prompt para o campo de mensagem; nada é enviado ao modelo aqui."""
        if state.mcp is None:
            raise HTTPException(409, "nenhum servidor MCP configurado em mcp.json")
        r = state.mcp.obter_prompt(servidor, nome, corpo.argumentos)
        audit_log.info("mcp_prompt", extra={"audit": {"servidor": servidor, "prompt": nome}})
        if "erro" in r:
            raise HTTPException(502, r["erro"])
        return {**r, "origem": f"mcp:{servidor}"}

    # ── pesquisa noturna pela interface (E6.5, REGRAS.md: pesquisa noturna só lê, sem aprovação) ──
    def _assuntos_da_pesquisa(state: AppState) -> dict[str, Any]:
        from .research import MAX_ASSUNTO, MAX_ASSUNTOS, assuntos_salvos

        assuntos, origem = assuntos_salvos(state.memory, state.settings.research_topics)
        return {
            "assuntos": assuntos,
            "origem": origem,
            "max": MAX_ASSUNTOS,
            "max_tamanho": MAX_ASSUNTO,
            "hora": state.settings.research_at or None,
            "disponivel": state.pesquisa is not None,
            "rodando": bool(state.pesquisa is not None and state.pesquisa.rodando),
        }

    @app.get("/pesquisa/assuntos", dependencies=[Admin])
    def ver_assuntos_da_pesquisa(state: State) -> dict[str, Any]:
        return _assuntos_da_pesquisa(state)

    @app.put("/pesquisa/assuntos", dependencies=[Admin])
    def trocar_assuntos_da_pesquisa(corpo: AssuntosPesquisa, state: State) -> dict[str, Any]:
        """Quem escolhe os assuntos é você (nunca o modelo): até 5, 200 caracteres cada."""
        from .research import salvar_assuntos

        try:
            salvar_assuntos(state.memory, corpo.assuntos)
        except ValueError as e:
            raise HTTPException(422, str(e)) from None
        audit_log.info("pesquisa_assuntos", extra={"audit": {"total": len(corpo.assuntos)}})
        return _assuntos_da_pesquisa(state)

    @app.get("/pesquisa/relatorios", dependencies=[Admin])
    def relatorios_da_pesquisa(state: State) -> dict[str, Any]:
        itens = state.pesquisa.relatorios() if state.pesquisa is not None else []
        return {"total": len(itens), "relatorios": itens}

    @app.post("/pesquisa/rodar", status_code=202, dependencies=[Admin])
    async def rodar_pesquisa_agora(state: State) -> dict[str, Any]:
        """Dispara a pesquisa em segundo plano (o relatório cai no 00 Inbox). Respeita o pânico
        e a cota gratuita (REGRAS.md: custo zero (sem API paga)): o job opcional não passa de 90% do
        dia."""
        pesquisa = state.pesquisa
        if pesquisa is None:
            raise HTTPException(
                409, "pesquisa indisponível: falta ORION_WEB_TOOLS, gateway ou vault"
            )
        if state.modos is not None and state.modos.panico():
            raise HTTPException(409, "modo pânico: a pesquisa está cortada")
        if pesquisa.rodando:
            raise HTTPException(409, "a pesquisa já está rodando")
        if not pesquisa.assuntos:
            raise HTTPException(422, "nenhum assunto definido")
        if state.custos is not None:
            for provedor in ("gateway", "gemini"):
                if not state.custos.pode_usar(provedor, opcional=True, quem="a pesquisa noturna"):
                    raise HTTPException(429, "cota gratuita do dia quase no fim: tente amanhã")
        audit_log.info("pesquisa_manual", extra={"audit": {"assuntos": len(pesquisa.assuntos)}})

        async def rodar() -> None:
            try:
                await pesquisa.run(marcar=False)
            except Exception:  # um relatório que falha não derruba o servidor
                log.exception("pesquisa manual falhou")

        state.tarefas_pesquisa = asyncio.create_task(rodar())
        return {"iniciada": True}

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

    from .activity_routes import router as activity_router

    app.include_router(activity_router(require_auth))

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

    @app.get("/tela", dependencies=[Admin])
    def tela_estado(state: State) -> dict[str, Any]:
        """Estado da memória da tela (REGRAS.md: memória da tela: só texto, local e com opt-in):
        ligada, pausada e contagens; nunca o texto."""
        return _estado_da_tela(state.tela, state.memory)

    @app.get("/tela/busca", dependencies=[Admin])
    def tela_busca(
        state: State,
        q: str = Query(min_length=2, max_length=200),
        dias: int = Query(7, ge=1, le=90),
        limite: int = Query(10, ge=1, le=20),
    ) -> dict[str, Any]:
        """Busca no texto que esteve na tela (E5.1): quando, em qual janela e o trecho. O mesmo
        texto que o modelo lê por `buscar_tela`, só que a pedido do próprio Antônio."""
        achados = state.memory.search_screen(q, limit=limite, dias=dias)
        return {
            "resultados": [
                {
                    "id": a["id"],
                    "quando": a["ts"],
                    "janela": a["titulo"],
                    "app": a["app"],
                    "trecho": a["trecho"],
                }
                for a in achados
            ]
        }

    @app.delete("/tela", dependencies=[Admin])
    def tela_limpar(state: State) -> dict[str, Any]:
        """Apaga TODO o texto de tela guardado (a captura em si segue como estava)."""
        apagados = state.memory.clear_screen()
        audit_log.info("tela_limpa", extra={"audit": {"registros": apagados}})
        return {"ok": True, "apagados": apagados}

    @app.post("/tela/pausa", dependencies=[Admin])
    def tela_pausar(corpo: EscutaControle, state: State) -> dict[str, Any]:
        """`ativa=false` pausa a captura; `ativa=true` retoma. 409 se a memória da tela está
        desligada."""
        if state.tela is None:
            raise HTTPException(status_code=409, detail="a memória da tela está desligada")
        state.tela.pausar(not corpo.ativa)
        return {"pausada": state.tela.pausada}

    # ── modos (REGRAS.md: pânico é corte; não perturbe só segura) e privacidade (REGRAS.md: toda
    # saída de dado fica registrada, sem conteúdo) ─────────────────────────
    @app.get("/modo", dependencies=[Admin])
    def modo_estado(state: State) -> dict[str, Any]:
        assert state.modos is not None
        return state.modos.estado()

    @app.post("/modo/panico", dependencies=[Admin])
    def modo_panico(corpo: PanicoControle, request: Request, state: State) -> dict[str, Any]:
        """Entrar é um clique; sair pede a senha de novo (o pânico nunca volta sozinho)."""
        assert state.modos is not None
        if corpo.ativo:
            state.modos.entrar_panico("web")
            if state.escuta is not None:
                state.escuta.pausar()  # já, sem esperar a vigia
            return state.modos.estado()
        cliente = request.client.host if request.client else "?"
        if not _senha_de_novo(state, corpo.senha, cliente):
            raise HTTPException(403, "para sair do modo pânico, confirme a senha")
        try:
            state.modos.sair_panico("web")
        except ModoError as e:
            raise HTTPException(503, str(e)) from None
        return state.modos.estado()

    @app.post("/modo/nao-perturbe", dependencies=[Admin])
    def modo_nao_perturbe(corpo: NaoPerturbeControle, state: State) -> dict[str, Any]:
        """`ate: "07:00"` liga até a próxima vez que o relógio marcar; `null` desliga o manual
        (o horário fixo de `ORION_DND_AT` continua valendo)."""
        assert state.modos is not None
        if corpo.ate is None:
            state.modos.desligar_nao_perturbe()
        else:
            state.modos.ligar_nao_perturbe(corpo.ate)
        return state.modos.estado()

    @app.get("/privacidade", dependencies=[Admin])
    def privacidade(state: State, dias: Annotated[int, Query(ge=1, le=90)] = 7) -> dict[str, Any]:
        """O que saiu do computador (REGRAS.md: toda saída de dado fica registrada, sem conteúdo):
        por dia e provedor, envios, bytes e tipo de
        conteúdo; e a lista de hoje. Nunca o conteúdo. O modelo local não conta como saída."""
        return _privacidade(state.memory, dias, time.time())

    @app.get("/atividade", dependencies=[Admin])
    def atividade(state: State, limite: Annotated[int, Query(ge=1, le=100)] = 30) -> dict[str, Any]:
        """A caixa de atividade: avisos (lembretes, briefing, relatórios, ciclo de sono...),
        aprovações esperando e erros dos jobs, num lugar só. Só leitura."""
        avisos = [
            {
                "id": n["id"],
                "tipo": n["kind"],
                "texto": str(n["text"])[:1500],
                "criado": _iso(n["created_at"]),
                "entregue": n["delivered_at"] is not None,
            }
            for n in state.ops.recent_notifications(limite)
        ]
        pendentes = state.policy.approvals.pending()
        return {
            "avisos": avisos,
            "nao_lidos": sum(not a["entregue"] for a in avisos),
            "aprovacoes": len(pendentes),
            "erros_dos_jobs": list(getattr(state.jobs, "ultimos_erros", []))[:5],
        }

    @app.get("/notifications", dependencies=[Admin])
    def avisos(state: State, project_id: str | None = None) -> list[dict[str, Any]]:
        """Avisos ainda não entregues (lembrete vencido, agendamento disparado). O canal
        entrega e confirma em `/notifications/{id}/ack`."""
        from .memory.scope import data_scope

        with data_scope(project_id, include_personal=False):
            return state.ops.pending_notifications()

    @app.post("/notifications/{notification_id}/ack", dependencies=[Admin])
    def confirmar_aviso(
        notification_id: int, state: State, project_id: str | None = None
    ) -> dict[str, bool]:
        from .memory.scope import data_scope

        with data_scope(project_id, include_personal=False):
            acknowledged = state.ops.ack_notification(notification_id)
        if not acknowledged:
            raise HTTPException(404, "aviso inexistente ou já confirmado")
        return {"ok": True}

    # ── organizador de Downloads (E9.4, REGRAS.md: automação sua vale pelo arquivo concedido) ──
    def _historicos(state: AppState, qual: str) -> Historicos:
        """404 quando o histórico está desligado (REGRAS.md: opt-in); 409 no pânico."""
        ligado = (
            state.settings.clipboard_days > 0 if qual == "area" else state.settings.command_history
        )
        if state.historicos is None or not ligado:
            raise HTTPException(
                404, "histórico desligado (ORION_CLIPBOARD_DAYS / ORION_COMMAND_HISTORY)"
            )
        if state.modos is not None and state.modos.panico():
            raise HTTPException(409, "modo pânico")
        return state.historicos

    @app.get("/transcricao/propostas", dependencies=[Admin])
    def propostas_da_transcricao(state: State) -> dict[str, Any]:
        return {"itens": Propostas(state.memory, state.ops).listar()}

    @app.post("/transcricao/propostas/{pid}/{acao}", dependencies=[Admin])
    def decidir_proposta(
        pid: int, acao: Annotated[str, PathParam(pattern="^(aprovar|descartar)$")], state: State
    ) -> dict[str, Any]:
        try:
            return Propostas(state.memory, state.ops).decidir(pid, acao)
        except PropostaError as e:
            raise HTTPException(e.status, str(e)) from None
        except ValueError as e:  # data do lembrete inválida etc.
            raise HTTPException(422, str(e)) from None

    @app.post("/ler-depois", dependencies=[Admin])
    async def ler_depois_adicionar(corpo: LinkLerDepois, state: State) -> dict[str, Any]:
        assert state.ler_depois is not None
        try:
            item = state.ler_depois.adicionar(corpo.url)
        except LerDepoisError as e:
            raise HTTPException(422, str(e)) from None
        if item["status"] == "pendente":
            state.ler_depois.agendar(item["id"])
        return item

    @app.get("/ler-depois", dependencies=[Admin])
    def ler_depois_listar(
        state: State, status: str | None = Query(None, max_length=20)
    ) -> dict[str, Any]:
        assert state.ler_depois is not None
        return {"itens": state.ler_depois.listar(status)}

    @app.delete("/ler-depois/{rid}", dependencies=[Admin])
    def ler_depois_descartar(rid: int, state: State) -> dict[str, Any]:
        assert state.ler_depois is not None
        try:
            return state.ler_depois.descartar(rid)
        except LerDepoisError as e:
            raise HTTPException(404, str(e)) from None

    @app.post("/area-transferencia", dependencies=[Admin])
    def guardar_area(corpo: AreaTexto, state: State) -> dict[str, str]:
        return {"resultado": _historicos(state, "area").guardar_area(corpo.texto)}

    @app.get("/area-transferencia", dependencies=[Admin])
    def ver_area(
        state: State, q: str = Query("", max_length=200), limite: int = Query(30, ge=1, le=100)
    ) -> dict[str, Any]:
        h = _historicos(state, "area")
        itens = (
            h.buscar_area(q, limite, state.settings.clipboard_days)
            if q.strip()
            else h.recentes_area(limite)
        )
        return {"dias": state.settings.clipboard_days, "itens": itens}

    @app.delete("/area-transferencia", dependencies=[Admin])
    def limpar_area(state: State) -> dict[str, int]:
        return {"apagados": _historicos(state, "area").limpar_area()}

    @app.get("/comandos", dependencies=[Admin])
    def ver_comandos(
        state: State,
        q: str = Query("", max_length=200),
        limite: int = Query(20, ge=1, le=50),
        dias: int = Query(90, ge=1, le=365),
    ) -> dict[str, Any]:
        return {"itens": _historicos(state, "comandos").buscar_comandos(q, limite, dias)}

    @app.post("/comandos/importar", dependencies=[Admin])
    def importar_comandos(state: State) -> dict[str, int]:
        return {"novos": _historicos(state, "comandos").importar()}

    @app.delete("/comandos", dependencies=[Admin])
    def limpar_comandos(state: State) -> dict[str, int]:
        return {"apagados": _historicos(state, "comandos").limpar_comandos()}

    @app.get("/posso-desligar", dependencies=[Admin])
    def posso_desligar_rota(state: State) -> dict[str, Any]:
        return _conferir_desligar(state.settings, state.tarefas_bg, state.downloads)

    def _downloads(state: AppState) -> Downloads:
        assert state.downloads is not None
        return state.downloads

    @app.get("/downloads", dependencies=[Admin])
    def ver_downloads(state: State) -> dict[str, Any]:
        d = _downloads(state)
        return {**d.estado(), "historico": d.historico(20)}

    @app.post("/downloads/conceder", dependencies=[Admin])
    def conceder_downloads(state: State) -> dict[str, Any]:
        try:
            r = _downloads(state).conceder()
        except ValueError as e:
            raise HTTPException(422, str(e)) from None
        audit_log.info("downloads_concedido")
        return r

    @app.post("/downloads/revogar", dependencies=[Admin])
    def revogar_downloads(state: State) -> dict[str, bool]:
        return {"ok": _downloads(state).revogar()}

    @app.post("/downloads/rodar", dependencies=[Admin])
    def rodar_downloads(state: State, simular: bool = False) -> dict[str, Any]:
        if state.modos is not None and state.modos.panico():
            raise HTTPException(409, "modo pânico: o organizador está cortado")
        r = _downloads(state).rodar(simular=simular)
        return {
            "status": r.status,
            "movidos": r.movidos,
            "ficaram": r.ficaram,
            "recusados": r.recusados,
            "simulado": simular,
        }

    @app.post("/downloads/desfazer", dependencies=[Admin])
    def desfazer_downloads(corpo: DesfazerDownloads, state: State) -> dict[str, Any]:
        return _downloads(state).desfazer(corpo.escopo)

    # ── rotinas (E9.1, REGRAS.md: automação sua vale pelo arquivo concedido): arquivos seus,
    # concedidos pelo hash ──
    def _rotinas(state: AppState) -> RotinaStore:
        assert state.rotinas is not None
        return state.rotinas

    def _rotina_json(c: Any) -> dict[str, Any]:
        r = c.rotina
        return {
            "nome": c.nome,
            "estado": c.estado,
            "erro": c.erro,
            "descricao": r.descricao if r else "",
            "gatilhos": [g.model_dump(exclude_none=True) for g in r.gatilhos] if r else [],
            "passos": len(r.passos) if r else 0,
            "liberar": r.liberar if r else [],
        }

    @app.get("/rotinas", dependencies=[Admin])
    def listar_rotinas(state: State) -> dict[str, Any]:
        itens = [_rotina_json(c) for c in _rotinas(state).listar()]
        return {"total": len(itens), "rotinas": itens}

    @app.post("/rotinas/por-frase", dependencies=[Admin])
    async def rotina_por_frase(corpo: FraseRotina, state: State) -> dict[str, Any]:
        """Roda a rotina concedida cuja frase bate EXATAMENTE (sem acento nem pontuação)."""
        c = _rotinas(state).por_frase(corpo.texto)
        if c is None or state.executor_rotinas is None:
            raise HTTPException(404, "nenhuma rotina concedida com essa frase")
        return (await state.executor_rotinas.executar(c.nome, "frase")).publico()

    @app.post("/rotinas/{nome}/conceder", dependencies=[Admin])
    def conceder_rotina(nome: str, state: State) -> dict[str, Any]:
        try:
            c = _rotinas(state).conceder(nome)
        except KeyError:
            raise HTTPException(404, "rotina não encontrada") from None
        except ValueError as e:
            raise HTTPException(422, str(e)) from None
        audit_log.info("rotina_concedida", extra={"audit": {"rotina": nome}})
        return _rotina_json(c)

    @app.post("/rotinas/{nome}/revogar", dependencies=[Admin])
    def revogar_rotina(nome: str, state: State) -> dict[str, bool]:
        return {"ok": _rotinas(state).revogar(nome)}

    @app.post("/rotinas/{nome}/executar", dependencies=[Admin])
    async def executar_rotina(
        nome: str,
        state: State,
        origem: Annotated[str, Query(pattern="^(manual|tecla|frase|telegram|palmas)$")] = "manual",
    ) -> dict[str, Any]:
        if state.executor_rotinas is None or _rotinas(state).obter(nome) is None:
            raise HTTPException(404, "rotina não encontrada")
        r = await state.executor_rotinas.executar(nome, origem)
        return r.publico()

    # ── tarefa em segundo plano (E9.2, REGRAS.md: tarefa em segundo plano só lê e propõe): só lê; o
    # que faria vira proposta ──
    def _tarefas(state: AppState) -> TarefasBg:
        if state.tarefas_bg is None:
            raise HTTPException(503, "tarefa em segundo plano indisponível: falta o gateway")
        return state.tarefas_bg

    def _bg(fn: Callable[[], Any]) -> Any:
        try:
            return fn()
        except TarefaBgError as e:
            raise HTTPException(e.status, str(e)) from None

    @app.post("/tarefas-bg", status_code=202, dependencies=[Admin])
    async def criar_tarefa_bg(corpo: TarefaBgPedido, state: State) -> dict[str, Any]:
        t = _tarefas(state)
        r = _bg(lambda: t.criar(corpo.objetivo, corpo.max_passos, corpo.max_min))
        audit_log.info("tarefa_bg", extra={"audit": {"id": r["id"], "passos": corpo.max_passos}})
        return r

    @app.get("/tarefas-bg", dependencies=[Admin])
    def listar_tarefas_bg(state: State) -> dict[str, Any]:
        itens = _tarefas(state).listar()
        return {"total": len(itens), "tarefas": itens}

    @app.get("/tarefas-bg/{tid}", dependencies=[Admin])
    def ver_tarefa_bg(tid: int, state: State) -> dict[str, Any]:
        return _bg(lambda: _tarefas(state).obter(tid))

    @app.post("/tarefas-bg/{tid}/cancelar", dependencies=[Admin])
    def cancelar_tarefa_bg(tid: int, state: State) -> dict[str, Any]:
        return _bg(lambda: _tarefas(state).cancelar(tid))

    @app.post("/tarefas-bg/{tid}/propostas/{indice}", dependencies=[Admin])
    def decidir_proposta_bg(
        tid: int, indice: int, corpo: PropostaDecisao, state: State
    ) -> dict[str, Any]:
        return _bg(lambda: _tarefas(state).decidir(tid, indice, corpo.acao))

    @app.post("/lembretes/{lembrete_id}/{acao}", dependencies=[Admin])
    def agir_no_lembrete(
        lembrete_id: int,
        acao: Annotated[str, PathParam(pattern="^(feito|adiar)$")],
        state: State,
        project_id: str | None = None,
        minutos: Annotated[int, Query(ge=1, le=1440)] = 60,
    ) -> dict[str, Any]:
        """ "Feito" conclui (e para de insistir); "adiar" volta a ser um lembrete novo (E9.3)."""
        from .memory.scope import data_scope

        try:
            with data_scope(project_id, include_personal=False):
                r = (
                    state.ops.complete_reminder(lembrete_id)
                    if acao == "feito"
                    else state.ops.snooze_reminder(lembrete_id, minutos)
                )
        except KeyError:
            raise HTTPException(404, "lembrete não encontrado") from None
        return {"ok": True, "lembrete": r}

    @app.exception_handler(MCPError)
    async def mcp_error(request: Request, error: MCPError):
        return JSONResponse(status_code=422, content={"detail": error.code})

    @app.exception_handler(PluginError)
    async def plugin_error(request: Request, error: PluginError):
        return JSONResponse(status_code=422, content={"detail": str(error)})

    @app.exception_handler(ProjectError)
    async def project_error(request: Request, error: ProjectError):
        return JSONResponse(
            status_code=404 if str(error).endswith("not_found") else 409,
            content={"detail": str(error)},
        )

    @app.exception_handler(ArtifactError)
    async def artifact_error(request: Request, error: ArtifactError):
        return JSONResponse(
            status_code=404 if str(error).endswith("not_found") else 409,
            content={"detail": str(error)},
        )

    from .accounts import router as account_router
    from .branches import router as branch_router
    from .calendar import router as calendar_router
    from .calendar_events import router as event_router
    from .documents import router as document_router
    from .export_credentials import router as export_client_router
    from .file_plans import router as file_plan_router

    app.include_router(ponte_router(require_auth))
    app.include_router(captura_router(require_auth))
    app.include_router(export_client_router(require_auth))
    app.mount("/mcp-export", ExportAuth(mcp_export.app, lambda: app.state.orion))

    app.include_router(file_plan_router(require_auth))

    app.include_router(event_router(require_auth))
    app.include_router(calendar_router(require_auth))
    app.include_router(account_router(require_auth))
    app.include_router(branch_router(require_auth))
    app.include_router(document_router(require_auth))
    app.include_router(fact_router(require_auth))
    app.include_router(grafo_router(require_auth))
    app.include_router(artifact_router(require_auth))
    app.include_router(project_router(require_auth))
    app.include_router(extension_router(require_auth))

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
