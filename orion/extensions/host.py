"""Conexões MCP locais explícitas, com lifecycle pertencente a uma única tarefa."""

from __future__ import annotations

import asyncio
import logging
import os
from contextlib import asynccontextmanager
from pathlib import Path
from typing import Literal

from mcp import Client, StdioServerParameters
from mcp.client.stdio import stdio_client
from pydantic import BaseModel, ConfigDict, Field, field_validator, model_validator

from .protocol import MCPCompatibilityError, ensure_compatible

log = logging.getLogger("orion.mcp")


class StdioConfig(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True)

    id: str = Field(pattern=r"^[a-z0-9][a-z0-9_-]{0,47}$")
    transport: Literal["stdio"] = "stdio"
    command: str = Field(min_length=1, max_length=4096)
    args: list[str] = Field(default_factory=list, max_length=128)
    cwd: Path | None = None
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


class MCPError(RuntimeError):
    def __init__(self, code: str):
        self.code = code
        super().__init__(code)


class Connection:
    def __init__(self, config: StdioConfig, *, timeout: float = 5):
        self.config = config
        self.timeout = timeout
        self.state = "disabled"
        self.error: str | None = None
        self.protocol: str | None = None
        self.client: Client | None = None
        self._task: asyncio.Task | None = None
        self._stop = asyncio.Event()
        self._ready = asyncio.Event()

    def status(self) -> dict:
        # Nunca serializar configuração/argv/erro bruto: podem conter dados sensíveis.
        return {
            "id": self.config.id,
            "transport": self.config.transport,
            "state": self.state,
            "protocol": self.protocol,
            "error": self.error,
        }

    @asynccontextmanager
    async def _transport(self):
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
        except Exception:  # noqa: BLE001 — servidor não derruba o app nem vaza sua resposta
            self.error = "start_failed"
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
        self.state = "connecting"
        self._task = asyncio.create_task(self._worker(), name=f"mcp:{self.config.id}")
        await self._ready.wait()

    async def close(self) -> None:
        self._stop.set()
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
        # API interna; registro no agente depende de classificação local (C11).
        return await self.connected().call_tool(name, arguments)


class MCPHost:
    def __init__(self, configs: list[StdioConfig], *, timeout: float = 5):
        if len(configs) > 32 or len({c.id for c in configs}) != len(configs):
            raise ValueError("conexões duplicadas ou acima do limite")
        self.connections = {c.id: Connection(c, timeout=timeout) for c in configs}

    async def start(self) -> None:
        await asyncio.gather(*(c.start() for c in self.connections.values()))

    async def close(self) -> None:
        await asyncio.gather(*(c.close() for c in self.connections.values()))

    def statuses(self) -> list[dict]:
        return [c.status() for c in self.connections.values()]
