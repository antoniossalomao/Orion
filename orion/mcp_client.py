"""Cliente MCP (fase 4): usa servidores MCP prontos (filesystem, fetch, git, navegador...) como
ferramentas do Orion, sempre atrás da política de risco.

Configuração em `mcp.json` (veja `mcp.example.json`):

    {"servers": {"arquivos": {
        "command": "npx", "args": ["-y", "@modelcontextprotocol/server-filesystem", "/home/x/Docs"],
        "default_risk": "exec",
        "tools": {"read_text_file": {"risk": "read", "read_path_arg": "path"},
                  "write_file":     {"risk": "write", "path_arg": "path"}}}}}

Regras (ORION_REGRAS.md, regra 24):
- A classe de risco **vem da configuração, nunca do servidor**. Ferramenta que o servidor anuncia
  e o `mcp.json` não classifica usa `default_risk` (padrão `exec`: pede confirmação sempre).
- `allow` limita o que é exposto; o resto do que o servidor anuncia some para o modelo.
- `external: true` marca o servidor inteiro como fonte de conteúdo não confiável (web, e-mail):
  o resultado chega marcado como dado e a sessão passa a confirmar escrita e execução.
- O servidor sobe só com o ambiente mínimo do SDK (PATH, HOME...) mais o `env` declarado: nada
  de `ORION_*` nem das chaves do Orion. Valores `${NOME}` vêm do ambiente/cofre do SO, não do JSON.
- A descrição e o esquema que o servidor anuncia são texto não confiável: a descrição é
  truncada e prefixada com o nome do servidor.

Os servidores falam async (anyio); as ferramentas do Orion são síncronas e rodam em thread. Por
isso o gerente guarda um laço de eventos próprio numa thread de fundo, onde cada servidor vive
numa tarefa; `call` envia a chamada para esse laço e espera o resultado.
"""

from __future__ import annotations

import asyncio
import hashlib
import json
import logging
import re
import threading
from collections.abc import Callable, Mapping
from concurrent.futures import Future
from contextlib import AbstractAsyncContextManager
from pathlib import Path
from typing import Any, Protocol
from urllib.parse import urlsplit

from pydantic import (
    BaseModel,
    ConfigDict,
    Field,
    ValidationError,
    field_validator,
    model_validator,
)

from .policy.classes import Risk, ToolSpec
from .secrets import get_secret
from .tools.registry import Tool

log = logging.getLogger("orion.mcp")

_NOME_SERVIDOR = re.compile(r"^[a-z][a-z0-9]{0,23}$")  # sem "_": o separador é "__"
_INVALIDO = re.compile(r"[^A-Za-z0-9_-]")
MAX_NOME = 64  # limite de nome de função na API compatível com a da OpenAI
MAX_DESCRICAO = 600
MAX_PAGINAS = 20


class McpConfigError(ValueError):
    """`mcp.json` inválido."""


class ToolRule(BaseModel):
    """Classe de risco de uma ferramenta do servidor (declarada por você, não pelo servidor)."""

    model_config = ConfigDict(extra="forbid")
    risk: Risk | None = None  # ausente: usa o `default_risk` do servidor
    path_arg: str | None = None  # argumento com caminho que a ferramenta ESCREVE
    read_path_arg: str | None = None  # argumento com caminho que a ferramenta LÊ
    cmd_arg: str | None = None  # argumento com comando de shell
    external: bool | None = None  # ausente: herda `external` do servidor
    # o modelo escolhe o destino na rede (buscar uma URL, navegar): depois de ler conteúdo externo,
    # a sessão confirma antes de cada uso (canal de exfiltração por GET)
    egress: bool = False


class ServerConfig(BaseModel):
    """Um servidor MCP: ou um comando local (stdio) ou um endereço (Streamable HTTP)."""

    model_config = ConfigDict(extra="forbid")
    command: str | None = None
    url: str | None = None  # servidor remoto (Streamable HTTP): http(s)://..., sem usuário na URL
    headers: dict[str, str] = Field(default_factory=dict)  # só com `url`; `${NOME}` vem do cofre
    args: list[str] = Field(default_factory=list)
    env: dict[str, str] = Field(default_factory=dict)
    cwd: str | None = None
    enabled: bool = True
    external: bool = False
    default_risk: Risk = Risk.EXEC
    allow: list[str] | None = None  # só estas ferramentas são expostas
    tools: dict[str, ToolRule] = Field(default_factory=dict)
    timeout_s: float = Field(default=60.0, gt=0, le=600)

    @model_validator(mode="after")
    def _um_transporte(self) -> ServerConfig:
        if bool(self.command) == bool(self.url):
            raise ValueError(
                "use `command` (servidor local) OU `url` (servidor remoto), não os dois"
            )
        if self.url:
            partes = urlsplit(self.url)
            if partes.scheme not in ("http", "https") or not partes.hostname or partes.username:
                raise ValueError("`url` deve ser http(s) e sem usuário/senha na própria URL")
            if self.args or self.cwd or self.env:
                raise ValueError("`args`, `cwd` e `env` só valem com `command`; use `headers`")
        elif self.headers:
            raise ValueError("`headers` só vale com `url`")
        return self

    @field_validator("default_risk")
    @classmethod
    def _padrao_nunca_mais_frouxo_que_escrita(cls, v: Risk) -> Risk:
        if v is Risk.READ:
            raise ValueError(
                "default_risk 'read' liberaria toda ferramenta nova do servidor sem pergunta; "
                "classifique cada leitura em `tools` (use 'write', 'exec' ou 'destructive')"
            )
        return v


class McpConfig(BaseModel):
    model_config = ConfigDict(extra="forbid")
    servers: dict[str, ServerConfig] = Field(default_factory=dict)

    @field_validator("servers")
    @classmethod
    def _nomes(cls, v: dict[str, ServerConfig]) -> dict[str, ServerConfig]:
        for nome in v:
            if not _NOME_SERVIDOR.match(nome):
                raise ValueError(
                    f"nome de servidor inválido: {nome!r} (minúsculas e dígitos, até 24, "
                    "começando por letra)"
                )
        return v


def load_config(path: Path | str) -> McpConfig:
    """Lê e valida o `mcp.json`; arquivo ausente = nenhum servidor."""
    caminho = Path(path)
    if not caminho.exists():
        return McpConfig()
    try:
        bruto = json.loads(caminho.read_text(encoding="utf-8"))
        return McpConfig.model_validate(_sem_comentarios(bruto))
    except (OSError, ValueError, ValidationError) as e:
        raise McpConfigError(f"{caminho}: {e}") from e


def _sem_comentarios(bruto: Any) -> Any:
    """JSON não tem comentário: chaves que começam com `_` (`_nota`) são ignoradas."""
    if isinstance(bruto, dict):
        return {k: _sem_comentarios(v) for k, v in bruto.items() if not str(k).startswith("_")}
    return bruto


def tool_name(servidor: str, ferramenta: str) -> str:
    """`servidor__ferramenta`, só com caracteres aceitos pelos provedores e até 64."""
    limpo = _INVALIDO.sub("_", ferramenta)
    nome = f"{servidor}__{limpo}"
    if len(nome) <= MAX_NOME:
        return nome
    sufixo = hashlib.sha256(ferramenta.encode()).hexdigest()[:8]
    return f"{nome[: MAX_NOME - 9]}_{sufixo}"


def resolve_env(
    env: Mapping[str, str], lookup: Callable[[str], str | None] = get_secret
) -> dict[str, str]:
    """`${NOME}` (no valor todo ou dentro dele, ex.: `Bearer ${TOKEN}`) vira o segredo `NOME`
    (ambiente ou cofre); falta = erro claro."""
    saida: dict[str, str] = {}
    for chave, valor in env.items():

        def trocar(m: re.Match[str], chave: str = chave) -> str:
            segredo = lookup(m.group(1))
            if not segredo:
                raise McpConfigError(f"variável {m.group(1)} (env.{chave}) não está definida")
            return segredo

        saida[chave] = re.sub(r"\$\{([A-Za-z_][A-Za-z0-9_]*)\}", trocar, valor)
    return saida


def spec_for(servidor: str, cfg: ServerConfig, ferramenta: str) -> ToolSpec:
    regra = cfg.tools.get(ferramenta, ToolRule())
    return ToolSpec(
        tool_name(servidor, ferramenta),
        regra.risk or cfg.default_risk,
        cmd_arg=regra.cmd_arg,
        path_arg=regra.path_arg,
        read_path_arg=regra.read_path_arg,
        external=cfg.external if regra.external is None else regra.external,
        egress=regra.egress,
    )


# ── conexão ───────────────────────────────────────────────────────────────────────
class McpClient(Protocol):
    async def list_tools(self, *, cursor: str | None = None) -> Any: ...
    async def call_tool(
        self,
        name: str,
        arguments: dict[str, Any] | None = None,
        read_timeout_seconds: float | None = None,
    ) -> Any: ...


ClientFactory = Callable[
    [str, ServerConfig, dict[str, str]], AbstractAsyncContextManager[McpClient]
]


def sdk_client_factory(
    _nome: str, cfg: ServerConfig, env: dict[str, str]
) -> AbstractAsyncContextManager[McpClient]:
    """Cliente do SDK oficial: stdin/stdout (`command`) ou Streamable HTTP (`url`)."""
    from mcp import Client, StdioServerParameters

    if cfg.url:
        import httpx2
        from mcp.client.streamable_http import streamable_http_client

        # cabeçalhos próprios do servidor MCP (nunca o token do Orion); sem seguir para outro host
        http = httpx2.AsyncClient(headers=env, timeout=httpx2.Timeout(30.0, read=300.0))
        return Client(streamable_http_client(cfg.url, http_client=http))  # type: ignore[return-value]
    assert cfg.command is not None

    args = [str(Path(a).expanduser()) if a.startswith("~") else a for a in cfg.args]
    return Client(  # type: ignore[return-value]  # o Client é um contexto async que devolve a si
        StdioServerParameters(command=cfg.command, args=args, env=env, cwd=cfg.cwd)
    )


def _texto(resultado: Any) -> tuple[str, bool]:
    """(texto, erro) de um `CallToolResult`: junta os blocos de texto; imagem vira aviso."""
    partes: list[str] = []
    for bloco in getattr(resultado, "content", None) or []:
        texto = getattr(bloco, "text", None)
        partes.append(texto if isinstance(texto, str) else f"[conteúdo {bloco.type} omitido]")
    texto = "\n".join(partes)
    estruturado = getattr(resultado, "structured_content", None)
    if not texto and estruturado is not None:
        texto = json.dumps(estruturado, ensure_ascii=False, default=str)
    return texto, bool(getattr(resultado, "is_error", False))


class McpManager:
    """Sobe os servidores do `mcp.json`, lista as ferramentas e as entrega como `Tool`."""

    def __init__(
        self,
        config: McpConfig,
        *,
        factory: ClientFactory = sdk_client_factory,
        connect_timeout_s: float = 45.0,
        secret_lookup: Callable[[str], str | None] = get_secret,
    ) -> None:
        self._cfg = config
        self._factory = factory
        self._connect_timeout = connect_timeout_s
        self._lookup = secret_lookup
        self._loop: asyncio.AbstractEventLoop | None = None
        self._thread: threading.Thread | None = None
        self._stop: asyncio.Event | None = None
        self._clients: dict[str, McpClient] = {}
        self._tarefas: list[asyncio.Task[None]] = []
        self.tools: list[Tool] = []
        self.specs: dict[str, ToolSpec] = {}
        self.status: dict[str, str] = {}  # servidor -> "ok (N ferramentas)" ou o motivo da falha
        self._remotas: dict[str, tuple[str, str]] = {}  # nome exposto -> (servidor, nome original)

    # ── ciclo de vida ─────────────────────────────────────────────────────────────
    def start(self) -> list[Tool]:
        """Conecta (em paralelo) e devolve as ferramentas. Servidor que falha é pulado."""
        ativos = {n: c for n, c in self._cfg.servers.items() if c.enabled}
        if not ativos:
            return []
        self._loop = asyncio.new_event_loop()
        pronto = threading.Event()

        def laco() -> None:
            asyncio.set_event_loop(self._loop)
            self._stop = asyncio.Event()
            pronto.set()
            self._loop.run_until_complete(self._aguardar_parada())  # type: ignore[union-attr]

        self._thread = threading.Thread(target=laco, name="orion-mcp", daemon=True)
        self._thread.start()
        pronto.wait()
        futuros = {
            nome: asyncio.run_coroutine_threadsafe(self._conectar(nome, cfg), self._loop)
            for nome, cfg in ativos.items()
        }
        for nome, fut in futuros.items():
            try:
                tools = fut.result(timeout=self._connect_timeout + 5)
            except Exception as e:  # noqa: BLE001 — um servidor ruim não derruba o Orion
                self.status[nome] = f"falhou: {type(e).__name__}: {e}"
                log.error("servidor MCP '%s' não subiu: %s", nome, e)
                fut.cancel()
                continue
            self.status[nome] = f"ok ({len(tools)} ferramentas)"
            for t in tools:
                self.tools.append(t)
        return list(self.tools)

    def stop(self) -> None:
        loop, thread = self._loop, self._thread
        if loop is None or thread is None:
            return
        if self._stop is not None:
            loop.call_soon_threadsafe(self._stop.set)
        thread.join(timeout=10)
        self._loop = self._thread = None

    async def _aguardar_parada(self) -> None:
        assert self._stop is not None
        await self._stop.wait()
        for t in self._tarefas:
            t.cancel()
        await asyncio.gather(*self._tarefas, return_exceptions=True)

    async def _conectar(self, nome: str, cfg: ServerConfig) -> list[Tool]:
        # com `url`, o dicionário resolvido são os cabeçalhos HTTP (segredos `${NOME}` do cofre)
        env = resolve_env(cfg.headers if cfg.url else cfg.env, self._lookup)
        pronto: asyncio.Future[list[Tool]] = asyncio.get_running_loop().create_future()

        async def viver() -> None:
            try:
                async with self._factory(nome, cfg, env) as cliente:
                    anunciadas = await self._listar(cliente)
                    self._clients[nome] = cliente
                    pronto.set_result(self._expor(nome, cfg, anunciadas))
                    assert self._stop is not None
                    await self._stop.wait()
            except asyncio.CancelledError:
                raise
            except Exception as e:  # noqa: BLE001
                if not pronto.done():
                    pronto.set_exception(e)
                else:
                    log.error("servidor MCP '%s' caiu: %s", nome, e)
            finally:
                self._clients.pop(nome, None)

        self._tarefas.append(asyncio.create_task(viver(), name=f"mcp-{nome}"))
        return await asyncio.wait_for(pronto, timeout=self._connect_timeout)

    @staticmethod
    async def _listar(cliente: McpClient) -> list[Any]:
        achadas: list[Any] = []
        cursor: str | None = None
        for _ in range(MAX_PAGINAS):
            pagina = await cliente.list_tools(cursor=cursor)
            achadas.extend(pagina.tools)
            cursor = getattr(pagina, "next_cursor", None)
            if not cursor:
                break
        return achadas

    def _expor(self, servidor: str, cfg: ServerConfig, anunciadas: list[Any]) -> list[Tool]:
        tools: list[Tool] = []
        for a in anunciadas:
            if cfg.allow is not None and a.name not in cfg.allow:
                continue
            spec = spec_for(servidor, cfg, a.name)
            if spec.name in self._remotas:  # dois nomes diferentes viraram o mesmo: o 2º sai
                log.warning(
                    "MCP '%s': ferramenta '%s' repete o nome %s", servidor, a.name, spec.name
                )
                continue
            self._remotas[spec.name] = (servidor, a.name)
            self.specs[spec.name] = spec
            esquema = a.input_schema if isinstance(a.input_schema, dict) else {}
            if esquema.get("type") != "object":
                esquema = {"type": "object", "properties": {}}
            descricao = f"[MCP {servidor}] {(a.description or a.name)[:MAX_DESCRICAO]}"
            tools.append(
                Tool(
                    spec.name,
                    descricao,
                    esquema,
                    self._chamador(spec.name, cfg.timeout_s),
                    validar=True,  # o esquema do servidor vale de verdade (C08)
                )
            )
        return tools

    def _chamador(self, nome: str, timeout_s: float) -> Callable[..., dict[str, Any]]:
        def chamar(**args: Any) -> dict[str, Any]:
            return self.call(nome, args, timeout_s)

        return chamar

    # ── chamada ───────────────────────────────────────────────────────────────────
    def call(self, nome: str, args: dict[str, Any], timeout_s: float = 60.0) -> dict[str, Any]:
        servidor, original = self._remotas[nome]
        cliente, loop = self._clients.get(servidor), self._loop
        if cliente is None or loop is None:
            return {"erro": f"o servidor MCP '{servidor}' não está conectado"}
        futuro: Future[Any] = asyncio.run_coroutine_threadsafe(
            cliente.call_tool(original, args, read_timeout_seconds=timeout_s), loop
        )
        try:
            resultado = futuro.result(timeout=timeout_s + 5)
        except TimeoutError:
            futuro.cancel()
            return {"erro": f"tempo esgotado ({timeout_s:.0f}s) no servidor MCP '{servidor}'"}
        except Exception as e:  # noqa: BLE001 — falha do servidor vira resultado, não derruba o turno
            if "timed out" in str(e).lower():  # o SDK levanta MCPError, não TimeoutError
                return {"erro": f"tempo esgotado ({timeout_s:.0f}s) no servidor MCP '{servidor}'"}
            return {"erro": f"servidor MCP '{servidor}': {type(e).__name__}: {e}"}
        texto, erro = _texto(resultado)
        return {"erro": texto or "erro sem mensagem"} if erro else {"texto": texto}


def manager_from_file(path: Path | str, **kw: Any) -> McpManager | None:
    """Gerente para o `mcp.json` (None se não há arquivo ou servidor habilitado)."""
    cfg = load_config(path)
    if not any(c.enabled for c in cfg.servers.values()):
        return None
    return McpManager(cfg, **kw)


def describe(manager: McpManager) -> list[str]:
    """Linhas legíveis: servidor, status e cada ferramenta com sua classe de risco."""
    linhas = [f"{n}: {s}" for n, s in manager.status.items()]
    for t in manager.tools:
        sp = manager.specs[t.name]
        marcas = f"{sp.risk.value}{' · externo' if sp.external else ''}"
        extra = "".join(
            f" · {k}={v}"
            for k, v in (
                ("lê", sp.read_path_arg),
                ("escreve", sp.path_arg),
                ("comando", sp.cmd_arg),
            )
            if v
        )
        linhas.append(f"  {t.name}  [{marcas}{extra}]")
    return linhas
