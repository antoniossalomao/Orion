import asyncio
import json
import threading

import pytest
from jsonschema import SchemaError

from orion.tools import Tool, ToolRegistry


async def test_sync_nao_bloqueia_e_async_usa_o_loop_existente():
    loop = asyncio.get_running_loop()
    main_thread = threading.get_ident()
    called = []

    def sync(x):
        called.append(threading.get_ident())
        return {"x": x}

    async def async_fn(x):
        assert asyncio.get_running_loop() is loop
        return {"x": x + 1}

    schema = {"type": "object", "properties": {"x": {"type": "integer"}}, "required": ["x"]}
    sync_tool = Tool("sync", "teste", schema, sync, validar=True)
    async_tool = Tool("async", "teste", schema, async_fn, validar=True)
    assert json.loads(await sync_tool.run_async({"x": 1})) == {"x": 1}
    assert called[0] != main_thread
    assert json.loads(await async_tool.run_async({"x": 1})) == {"x": 2}
    assert json.loads(async_tool.run({"x": 1}))["codigo"] == "async_required"


@pytest.mark.parametrize(
    "args",
    [
        {"itens": [{"modo": "invalid", "url": "https://example.com"}]},
        {"itens": [{"modo": "read", "url": "not a uri"}]},
        {"itens": []},
        {"itens": [{"modo": "read", "url": "https://example.com", "extra": True}]},
        {"itens": [{"modo": "read", "url": "https://example.com"}], "extra": True},
    ],
)
async def test_schema_completo_rejeita_sem_executar(args):
    calls = []
    schema = {
        "type": "object",
        "additionalProperties": False,
        "required": ["itens"],
        "properties": {
            "itens": {"type": "array", "minItems": 1, "items": {"$ref": "#/$defs/item"}}
        },
        "$defs": {
            "item": {
                "type": "object",
                "required": ["modo", "url"],
                "additionalProperties": False,
                "properties": {
                    "modo": {"enum": ["read"]},
                    "url": {"type": "string", "format": "uri"},
                },
            }
        },
    }
    tool = Tool("remote", "teste", schema, lambda **kw: calls.append(kw), validar=True)
    assert json.loads(await tool.run_async(args))["codigo"] == "arguments_invalid"
    assert calls == []


@pytest.mark.parametrize(
    "schema",
    [
        {"type": "unsupported"},
        {"type": "object", "properties": {"x": {"type": "string", "pattern": "["}}},
        {"$schema": "https://example.invalid/schema", "type": "object"},
    ],
)
def test_schema_invalido_recusado_ao_registrar(schema):
    with pytest.raises(SchemaError):
        ToolRegistry([Tool("invalid", "x", schema, lambda: None)])


async def test_ref_remoto_nao_abre_rede_e_erro_execucao_estruturado():
    called = []
    tool = Tool(
        "ref",
        "x",
        {"$ref": "https://example.invalid/schema"},
        lambda **kw: called.append(kw),
        validar=True,
    )
    assert json.loads(await tool.run_async({}))["codigo"] == "schema_invalid"
    assert called == []

    async def broken():
        raise RuntimeError("ensaio")

    result = json.loads(await Tool("broken", "x", {"type": "object"}, broken).run_async({}))
    assert result["codigo"] == "execution_error" and "RuntimeError" in result["erro"]


async def test_cancelamento_async_propagado():
    started = asyncio.Event()
    stopped = asyncio.Event()

    async def slow():
        started.set()
        try:
            await asyncio.Event().wait()
        finally:
            stopped.set()

    task = asyncio.create_task(Tool("slow", "x", {"type": "object"}, slow).run_async({}))
    await started.wait()
    task.cancel()
    with pytest.raises(asyncio.CancelledError):
        await task
    assert stopped.is_set()
