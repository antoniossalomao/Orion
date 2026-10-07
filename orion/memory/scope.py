"""Escopo de dados por operação; asyncio.to_thread conserva ContextVars."""

from collections.abc import Iterator
from contextlib import contextmanager
from contextvars import ContextVar
from dataclasses import dataclass


@dataclass(frozen=True)
class DataScope:
    project_id: str | None = None
    include_personal: bool = True


_PERSONAL = DataScope()
current: ContextVar[DataScope] = ContextVar("orion_data_scope", default=_PERSONAL)


@contextmanager
def data_scope(project_id: str | None = None, *, include_personal: bool = True) -> Iterator[None]:
    token = current.set(DataScope(project_id, include_personal))
    try:
        yield
    finally:
        current.reset(token)


def clause(column: str) -> tuple[str, tuple]:
    scope = current.get()
    return f"({column} IS ? OR (?=1 AND {column} IS NULL))", (
        scope.project_id,
        int(scope.include_personal),
    )
