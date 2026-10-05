"""Contrato MCP verificado com o SDK oficial fixado no lockfile (C07)."""

from importlib.metadata import version

SDK_VERSION = "2.3.0"
TESTED_PROTOCOLS = ("2025-11-25", "2026-07-28")


class MCPCompatibilityError(ValueError):
    """Versão fora da combinação comprovada, sem tentar executar ferramentas."""


def ensure_compatible(protocol: str, *, sdk: str | None = None) -> None:
    installed = sdk if sdk is not None else version("mcp")
    if installed != SDK_VERSION:
        raise MCPCompatibilityError(f"SDK MCP incompatível; Orion requer {SDK_VERSION}")
    if protocol not in TESTED_PROTOCOLS:
        raise MCPCompatibilityError(
            "protocolo MCP não validado pelo Orion; versões: " + ", ".join(TESTED_PROTOCOLS)
        )
