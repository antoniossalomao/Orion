"""Registro de ferramentas: nome (PT, contrato de function-calling), esquema e função."""

from __future__ import annotations

import json
from collections.abc import Callable
from dataclasses import dataclass
from typing import Any


@dataclass(frozen=True)
class Tool:
    name: str
    description: str
    parameters: dict[str, Any]  # JSON Schema
    fn: Callable[..., Any]

    def schema(self) -> dict[str, Any]:
        return {
            "type": "function",
            "function": {
                "name": self.name,
                "description": self.description,
                "parameters": self.parameters,
            },
        }

    def run(self, args: dict[str, Any]) -> str:
        """Executa e devolve JSON (o que o modelo lê). Erro vira {"erro": ...}."""
        faltando = [k for k in self.parameters.get("required", []) if k not in args]
        if faltando:
            return json.dumps(
                {"erro": f"argumentos obrigatórios ausentes: {', '.join(faltando)}"},
                ensure_ascii=False,
            )
        try:
            return json.dumps(self.fn(**args), ensure_ascii=False, default=str)
        except TypeError as e:
            return json.dumps({"erro": f"argumentos inválidos: {e}"}, ensure_ascii=False)
        except Exception as e:  # noqa: BLE001 — falha de ferramenta vira resultado, não derruba o turno
            return json.dumps({"erro": f"{type(e).__name__}: {e}"}, ensure_ascii=False)


class ToolRegistry:
    def __init__(self, tools: list[Tool] | None = None) -> None:
        self._tools: dict[str, Tool] = {}
        for t in tools or []:
            self.register(t)

    def register(self, tool: Tool) -> None:
        if tool.name in self._tools:
            raise ValueError(f"ferramenta duplicada: {tool.name}")
        self._tools[tool.name] = tool

    def get(self, name: str) -> Tool | None:
        return self._tools.get(name)

    def names(self) -> list[str]:
        return list(self._tools)

    def schemas(self) -> list[dict[str, Any]]:
        return [t.schema() for t in self._tools.values()]
