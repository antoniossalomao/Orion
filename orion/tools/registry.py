"""Registro de ferramentas: nome (PT, contrato de function-calling), esquema e função."""

from __future__ import annotations

import asyncio
import inspect
import json
from collections.abc import Callable
from copy import deepcopy
from dataclasses import dataclass
from functools import cached_property
from typing import Any, cast

from jsonschema import Draft202012Validator, FormatChecker, SchemaError, ValidationError
from jsonschema.validators import validator_for
from referencing import Registry


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
                "parameters": deepcopy(self.validator.schema),
            },
        }

    @cached_property
    def validator(self):
        schema = deepcopy(self.parameters)
        cls = validator_for(schema, default=Draft202012Validator)
        declared = schema.get("$schema")
        known = cls.META_SCHEMA.get("$id") or cls.META_SCHEMA.get("id")
        if declared and str(declared).rstrip("#") != str(known).rstrip("#"):
            raise SchemaError("versão JSON Schema não suportada")
        check_schema = cast(Callable[..., None], cls.check_schema)
        check_schema(schema, format_checker=FormatChecker())
        # Registry vazio não busca $ref remoto: schema não abre rede implicitamente.
        return cls(schema, format_checker=FormatChecker(), registry=Registry())

    @staticmethod
    def _error(message: str, code: str) -> str:
        return json.dumps({"ok": False, "erro": message, "codigo": code}, ensure_ascii=False)

    def _invalid(self, args: dict[str, Any]) -> str | None:
        try:
            self.validator.validate(args)
        except SchemaError:
            return self._error("schema da ferramenta inválido", "schema_invalid")
        except ValidationError as e:
            missing = (
                [k for k in self.parameters.get("required", []) if k not in args]
                if isinstance(args, dict)
                else []
            )
            if e.validator == "required" and missing:
                message = "argumentos obrigatórios ausentes: " + ", ".join(missing)
            else:
                path = "$" + "".join(f"[{key}]" for key in e.absolute_path)
                message = f"argumentos inválidos em {path}: restrição {e.validator}"
            return self._error(message, "arguments_invalid")
        except Exception:  # noqa: BLE001 — referência remota/irresolúvel não pode executar
            return self._error("referência de schema indisponível", "schema_invalid")
        return None

    @staticmethod
    def _result(value: Any) -> str:
        return json.dumps(value, ensure_ascii=False, default=str)

    def run(self, args: dict[str, Any]) -> str:
        """Caminho síncrono compatível. Funções async exigem run_async, sem loop novo."""
        if invalid := self._invalid(args):
            return invalid
        if inspect.iscoroutinefunction(self.fn):
            return self._error("ferramenta assíncrona: use run_async", "async_required")
        try:
            value = self.fn(**args)
            if inspect.isawaitable(value):
                if inspect.iscoroutine(value):
                    value.close()
                return self._error("ferramenta assíncrona: use run_async", "async_required")
            return self._result(value)
        except TypeError as e:
            return self._error(f"argumentos inválidos: {e}", "arguments_invalid")
        except Exception as e:  # noqa: BLE001 — erro da tool não derruba o turno
            return self._error(f"{type(e).__name__}: {e}", "execution_error")

    async def run_async(self, args: dict[str, Any]) -> str:
        """Mesmo contrato JSON; síncronas saem do loop e awaitables usam o loop existente."""
        if invalid := self._invalid(args):
            return invalid
        try:
            value = (
                self.fn(**args)
                if inspect.iscoroutinefunction(self.fn)
                else await asyncio.to_thread(self.fn, **args)
            )
            if inspect.isawaitable(value):
                value = await value
            return self._result(value)
        except TypeError as e:
            return self._error(f"argumentos inválidos: {e}", "arguments_invalid")
        except Exception as e:  # noqa: BLE001 — CancelledError (BaseException) propaga
            return self._error(f"{type(e).__name__}: {e}", "execution_error")


class ToolRegistry:
    def __init__(self, tools: list[Tool] | None = None) -> None:
        self._tools: dict[str, Tool] = {}
        for t in tools or []:
            self.register(t)

    def register(self, tool: Tool) -> None:
        if tool.name in self._tools:
            raise ValueError(f"ferramenta duplicada: {tool.name}")
        _ = tool.validator
        self._tools[tool.name] = tool

    def get(self, name: str) -> Tool | None:
        return self._tools.get(name)

    def names(self) -> list[str]:
        return list(self._tools)

    def schemas(self) -> list[dict[str, Any]]:
        return [t.schema() for t in self._tools.values()]
