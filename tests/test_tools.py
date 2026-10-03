import json

import pytest

from orion.delegate import Delegator
from orion.memory import MemoryStore
from orion.policy import DEFAULT_TOOLS, Risk
from orion.tools import Tool, ToolRegistry, default_registry


@pytest.fixture
def store(tmp_path):
    s = MemoryStore(tmp_path / "t.db")
    yield s
    s.close()


def test_toda_ferramenta_registrada_tem_classe_de_risco(store):
    """Sem classe na política a ferramenta seria negada (fail-closed): falha cedo."""
    reg = default_registry(store, Delegator(store))
    assert set(reg.names()) <= set(DEFAULT_TOOLS)
    assert reg.names() == [
        "buscar_memoria", "salvar_memoria", "listar_fatos", "esquecer_fato", "delegar",
    ]  # fmt: skip
    assert DEFAULT_TOOLS["esquecer_fato"].risk is Risk.DESTRUCTIVE
    assert DEFAULT_TOOLS["delegar"].risk is Risk.EXEC


def test_esquemas_no_formato_function_calling(store):
    for esq in default_registry(store, Delegator(store)).schemas():
        assert esq["type"] == "function"
        f = esq["function"]
        assert f["name"] and f["description"] and f["parameters"]["type"] == "object"


def test_ferramentas_de_memoria_funcionam(store):
    reg = default_registry(store)
    salvo = json.loads(
        reg.get("salvar_memoria").run({"texto": "Antônio mora em Marília", "fonte": "teste"})
    )
    assert salvo["ok"] and salvo["id"] == 1
    achou = json.loads(reg.get("buscar_memoria").run({"consulta": "Marília", "limite": 99}))
    assert achou["resultados"][0]["texto"] == "Antônio mora em Marília"
    assert json.loads(reg.get("listar_fatos").run({}))["fatos"][0]["fonte"] == "teste"
    assert json.loads(reg.get("esquecer_fato").run({"id": 1})) == {"ok": True}
    assert json.loads(reg.get("esquecer_fato").run({"id": 1})) == {"ok": False}


def test_registro_recusa_duplicata_e_trata_args_ruins():
    t = Tool("f", "x", {"type": "object", "properties": {}, "required": ["a"]}, lambda a: a)
    reg = ToolRegistry([t])
    with pytest.raises(ValueError, match="duplicada"):
        reg.register(t)
    assert "ausentes: a" in t.run({})
    assert "argumentos inválidos" in Tool("g", "x", {"type": "object"}, lambda: 1).run({"extra": 1})
    assert reg.get("nada") is None
