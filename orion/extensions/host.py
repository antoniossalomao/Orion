"""Conexões MCP locais explícitas, com lifecycle pertencente a uma única tarefa."""

from __future__ import annotations

import asyncio
import logging
import os
import uuid
from collections.abc import Callable
from contextlib import asynccontextmanager
from pathlib import Path
from typing import Annotated, Literal
from urllib.parse import urlsplit

import httpx2
from mcp import Client, StdioServerParameters
from mcp.client.stdio import stdio_client
from mcp.client.streamable_http import streamable_http_client
from pydantic import BaseModel, ConfigDict, Field, field_validator, model_validator

from ..policy.classes import Risk
from ..secrets import get_secret
from .protocol import MCPCompatibilityError, ensure_compatible

log = logging.getLogger("orion.mcp")


class ProtocolLogFilter(logging.Filter):
    """SDK pode logar payloads e exceções com conteúdo remoto; só conservar categoria."""

    def filter(self, record: logging.LogRecord) -> bool:
        record.msg = "mcp_sdk_event"
        record.args = ()
        record.exc_info = None
        record.exc_text = None
        record.stack_info = None
        return True


def sanitize_protocol_logs() -> None:
    for name, logger in list(logging.Logger.manager.loggerDict.items()):
        if (
            isinstance(logger, logging.Logger)
            and name.startswith(("mcp.client", "mcp.shared", "httpx2", "httpcore2"))
            and not any(isinstance(f, ProtocolLogFilter) for f in logger.filters)
        ):
            logger.addFilter(ProtocolLogFilter())


class StdioConfig(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True)

    id: str = Field(pattern=r"^[a-z0-9][a-z0-9_-]{0,47}$")
    transport: Literal["stdio"] = "stdio"
    command: str = Field(min_length=1, max_length=4096)
    args: list[str] = Field(default_factory=list, max_length=128)
    cwd: Path | None = None
    resources: list[str] = Field(default_factory=list, max_length=128)
    prompts: list[str] = Field(default_factory=list, max_length=128)
    classifications: dict[str, Risk] = Field(default_factory=dict, max_length=256)
    scope: str = Field(default="personal", pattern=r"^(personal|project:[a-f0-9]{32})$")
    enabled: bool = False
    trusted: bool = False

    @field_validator("command")
    @classmethod
    def absolute_command(cls, value: str) -> str:
        if not Path(value).is_absolute() or "\0" in value:
            raise ValueError("executável precisa ser caminho absoluto")
        return value

    @model_validator(mode="after")
    def valid_paths(self) -> StdioConfig:
        if any("\0" in arg or len(arg) > 8192 for arg in self.args):
            raise ValueError("argv inválido")
        if self.cwd is not None and not self.cwd.is_absolute():
            raise ValueError("cwd precisa ser absoluto")
        if self.enabled and not self.trusted:
            raise ValueError("execução local exige revisão e confiança explícitas")
        return self


class HTTPConfig(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True)

    id: str = Field(pattern=r"^[a-z0-9][a-z0-9_-]{0,47}$")
    transport: Literal["http"] = "http"
    url: str = Field(max_length=4096)
    resources: list[str] = Field(default_factory=list, max_length=128)
    prompts: list[str] = Field(default_factory=list, max_length=128)
    classifications: dict[str, Risk] = Field(default_factory=dict, max_length=256)
    scope: str = Field(default="personal", pattern=r"^(personal|project:[a-f0-9]{32})$")
    enabled: bool = False
    authorized: bool = False
    secret_ref: str | None = Field(default=None, pattern=r"^ORION_MCP_[A-Z0-9_]{1,80}$")

    @model_validator(mode="after")
    def valid_endpoint(self) -> HTTPConfig:
        url = urlsplit(self.url)
        if (
            url.scheme not in {"https", "http"}
            or not url.hostname
            or url.username
            or url.password
            or url.query
            or url.fragment
        ):
            raise ValueError("endpoint precisa ser URL HTTP sem credenciais, query ou fragmento")
        if url.scheme == "http" and url.hostname not in {"localhost", "127.0.0.1", "::1"}:
            raise ValueError("HTTP sem TLS permitido apenas no loopback")
        if self.enabled and not self.authorized:
            raise ValueError("conexão exige endereço explicitamente autorizado")
        return self


ConnectionConfig = Annotated[StdioConfig | HTTPConfig, Field(discriminator="transport")]


def error_code(error: BaseException) -> str:
    if isinstance(error, MCPError):
        return error.code
    if isinstance(error, httpx2.HTTPStatusError) and error.response.status_code in {401, 403}:
        return "auth_failed"
    children = getattr(error, "exceptions", ())
    for child in children:
        code = error_code(child)
        if code != "transport_error":
            return code
    return "transport_error"


class MCPError(RuntimeError):
    def __init__(self, code: str, *, possibly_active: bool = False):
        self.code = code
        self.possibly_active = possibly_active
        super().__init__(code)


class Connection:
    def __init__(self, config: StdioConfig | HTTPConfig, *, timeout: float = 5):
        sanitize_protocol_logs()
        self.label: str = config.id
        self.authorized: Callable[[], bool] | None = None
        self.config = config
        self.timeout = timeout
        self.state = "disabled"
        self.error: str | None = None
        self.protocol: str | None = None
        self.client: Client | None = None
        self.generation = ""
        self.last_call: dict | None = None
        self._calls: set[asyncio.Task] = set()
        self._slots = asyncio.Semaphore(4)
        self._http_error: str | None = None
        self._task: asyncio.Task | None = None
        self._stop = asyncio.Event()
        self._ready = asyncio.Event()

    def status(self) -> dict:
        # Nunca serializar configuração/argv/erro bruto: podem conter dados sensíveis.
        return {
            "id": self.config.id,
            "transport": self.config.transport,
            "scope": self.config.scope,
            "state": self.state,
            "protocol": self.protocol,
            "error": self.error,
            "last_call": self.last_call,
        }

    @asynccontextmanager
    async def _transport(self):
        if isinstance(self.config, HTTPConfig):
            headers = {}
            if self.config.secret_ref:
                secret = get_secret(self.config.secret_ref)
                if not secret:
                    raise MCPError("auth_missing")
                headers["Authorization"] = "Bearer " + secret

            async def observe(response: httpx2.Response) -> None:
                if response.status_code in {401, 403}:
                    self._http_error = "auth_failed"

            async with httpx2.AsyncClient(
                headers=headers,
                trust_env=False,
                timeout=httpx2.Timeout(self.timeout),
                event_hooks={"response": [observe]},
            ) as http:
                async with streamable_http_client(self.config.url, http_client=http) as streams:
                    yield streams
            return
        # O SDK só herda variáveis operacionais; nenhuma variável ORION/chave é copiada.
        params = StdioServerParameters(
            command=self.config.command,
            args=self.config.args,
            cwd=self.config.cwd,
            env={"PYTHONUNBUFFERED": "1"},
        )
        # Stderr do servidor pode conter credenciais; stdout pertence ao protocolo.
        with open(os.devnull, "w") as errlog:  # noqa: ASYNC230 — arquivo especial, sem I/O em disco
            async with stdio_client(params, errlog=errlog) as streams:
                yield streams

    async def _worker(self) -> None:
        try:
            # Entrada/saída dos TaskGroups AnyIO ocorrem nesta mesma tarefa.
            async with asyncio.timeout(self.timeout) as deadline:
                async with Client(
                    self._transport(), cache=None, read_timeout_seconds=self.timeout
                ) as client:
                    ensure_compatible(client.session.protocol_version or "")
                    self.client = client
                    self.protocol = client.session.protocol_version
                    self.state = "connected"
                    deadline.reschedule(None)
                    self._ready.set()
                    await self._stop.wait()
        except MCPCompatibilityError:
            self.error = "protocol_incompatible"
        except TimeoutError:
            self.error = "start_timeout"
        except Exception as error:  # noqa: BLE001 — sem erro bruto nem credenciais
            self.error = (
                (self._http_error or error_code(error))
                if isinstance(self.config, HTTPConfig)
                else "start_failed"
            )
        finally:
            self.client = None
            self.state = "failed" if self.error else "disconnected"
            self._ready.set()
            log.info(
                "mcp_state",
                extra={"connection": self.config.id, "state": self.state, "code": self.error},
            )

    async def start(self) -> None:
        if not self.config.enabled or self._task is not None:
            return
        self._stop.clear()
        self._ready.clear()
        self.error = None
        self._http_error = None
        self.generation = uuid.uuid4().hex
        self.state = "connecting"
        self._task = asyncio.create_task(self._worker(), name=f"mcp:{self.config.id}")
        await self._ready.wait()

    async def close(self) -> None:
        self._stop.set()
        active = [task for task in self._calls if task is not asyncio.current_task()]
        for task in active:
            task.cancel()
        if active:
            await asyncio.gather(*active, return_exceptions=True)
        if self._task is not None:
            await self._task
            self._task = None

    def connected(self) -> Client:
        if self.client is None or self.state != "connected":
            raise MCPError("not_connected")
        return self.client

    async def list_tools(self):
        return (await self.connected().list_tools()).tools

    async def call(self, name: str, arguments: dict):
        # Nunca repetir um POST/tool após timeout: o efeito pode já ter ocorrido.
        client = self.connected()
        task = asyncio.current_task()
        if task is not None:
            self._calls.add(task)
        try:
            async with asyncio.timeout(self.timeout), self._slots:
                result = await client.call_tool(name, arguments)
            self.last_call = {"state": "completed", "possibly_active": False}
            return result
        except asyncio.CancelledError:
            self.last_call = {"state": "cancelled", "possibly_active": True}
            raise
        except TimeoutError as error:
            self.last_call = {"state": "timeout", "possibly_active": True}
            raise MCPError("call_timeout", possibly_active=True) from error
        except Exception as error:
            self.last_call = {"state": "failed", "possibly_active": True}
            self.error = self._http_error or "call_failed"
            self._stop.set()
            raise MCPError(self.error, possibly_active=True) from error
        finally:
            if task is not None:
                self._calls.discard(task)


class MCPHost:
    def __init__(self, configs: list[ConnectionConfig], *, timeout: float = 5):
        if len(configs) > 32 or len({c.id for c in configs}) != len(configs):
            raise ValueError("conexões duplicadas ou acima do limite")
        self.connections = {c.id: Connection(c, timeout=timeout) for c in configs}

    async def start(self) -> None:
        await asyncio.gather(*(c.start() for c in self.connections.values()))

    async def close(self) -> None:
        await asyncio.gather(*(c.close() for c in self.connections.values()))

    def statuses(self) -> list[dict]:
        return [c.status() for c in self.connections.values()]
