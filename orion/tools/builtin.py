"""Ferramentas nativas da fase 2: memória e delegação.

As demais (arquivos, e-mail, navegador...) entram na fase 4, por servidores MCP
e pelo `orion-desktop`, sempre com a classe de risco em `orion.policy.classes`.
"""

from __future__ import annotations

from typing import Any

from ..delegate import Delegator
from ..memory import MemoryStore
from ..memory.ops import Operations
from .desktop import desktop_tools
from .ops_tools import ops_tools
from .registry import Tool, ToolRegistry


def memory_tools(store: MemoryStore) -> list[Tool]:
    def buscar_memoria(consulta: str, limite: int = 5) -> dict[str, Any]:
        hits = store.search(consulta, k=max(1, min(int(limite), 15)))
        return {
            "resultados": [
                {"tipo": h.kind, "id": h.id, "texto": h.text[:1200], "fonte": h.source}
                for h in hits
            ]
        }

    def salvar_memoria(texto: str, fonte: str = "conversa") -> dict[str, Any]:
        f = store.add_fact(texto, fonte)
        return {"ok": True, "id": f.id, "texto": f.text}

    def listar_fatos() -> dict[str, Any]:
        return {"fatos": [{"id": f.id, "texto": f.text, "fonte": f.source} for f in store.facts()]}

    def esquecer_fato(id: int) -> dict[str, Any]:
        return {"ok": store.forget_fact(int(id))}

    obj = "object"
    return [
        Tool(
            "buscar_memoria",
            "Busca na memória pessoal do Antônio (fatos, notas do vault, conversas).",
            {
                "type": obj,
                "properties": {"consulta": {"type": "string"}, "limite": {"type": "integer"}},
                "required": ["consulta"],
            },
            buscar_memoria,
        ),
        Tool(
            "salvar_memoria",
            "Guarda um fato durável sobre o Antônio (preferência, dado, decisão), com a fonte.",
            {
                "type": obj,
                "properties": {"texto": {"type": "string"}, "fonte": {"type": "string"}},
                "required": ["texto"],
            },
            salvar_memoria,
        ),
        Tool(
            "listar_fatos",
            "Lista tudo o que o Orion sabe sobre o Antônio.",
            {"type": obj, "properties": {}},
            listar_fatos,
        ),
        Tool(
            "esquecer_fato",
            "Apaga de verdade um fato da memória (pede confirmação do Antônio).",
            {"type": obj, "properties": {"id": {"type": "integer"}}, "required": ["id"]},
            esquecer_fato,
        ),
    ]


def delegate_tool(delegator: Delegator) -> Tool:
    def delegar(tarefa: str, pasta: str, agente: str | None = None) -> dict[str, Any]:
        return delegator.delegate(tarefa, pasta, agente)

    return Tool(
        "delegar",
        "Delega uma tarefa pesada (código, pesquisa longa) a uma CLI oficial "
        "(claude, codex, gemini) numa pasta de trabalho. Pede confirmação do Antônio.",
        {
            "type": "object",
            "properties": {
                "tarefa": {"type": "string"},
                "pasta": {"type": "string", "description": "pasta de trabalho existente"},
                "agente": {"type": "string", "enum": ["claude", "codex", "gemini"]},
            },
            "required": ["tarefa", "pasta"],
        },
        delegar,
    )


def default_registry(
    store: MemoryStore,
    delegator: Delegator | None = None,
    ops: Operations | None = None,
    *,
    desktop: bool = False,
) -> ToolRegistry:
    reg = ToolRegistry(memory_tools(store))
    if desktop:  # opt-in (ORION_DESKTOP_TOOLS): age no computador, sempre sob a política
        for t in desktop_tools():
            reg.register(t)
    if ops is not None:
        for t in ops_tools(ops):
            reg.register(t)
    if delegator is not None:
        reg.register(delegate_tool(delegator))
    return reg
