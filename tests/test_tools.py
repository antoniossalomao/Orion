import json

import pytest

from orion.delegate import Delegator
from orion.memory import MemoryStore
from orion.memory.ops import Operations
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
        "buscar_memoria", "salvar_memoria", "listar_fatos", "editar_fato", "esquecer_fato", "delegar",
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


# ── ferramentas de operação (lembretes, agendamentos, tarefas, números) ─────
def _chamar(reg, nome, **args):
    return json.loads(reg.get(nome).run(args))


@pytest.fixture
def reg_ops(store):
    return default_registry(store, None, Operations(store))


def test_ferramentas_de_operacao_tem_classe_de_risco(reg_ops):
    nomes = {
        "gerenciar_lembretes",
        "gerenciar_agendamentos",
        "gerenciar_tarefas",
        "registrar_numero",
        "listar_numeros",
    }
    assert nomes <= set(reg_ops.names()) <= set(DEFAULT_TOOLS)
    assert {n for n in nomes if DEFAULT_TOOLS[n].risk is Risk.WRITE} == nomes - {"listar_numeros"}
    assert DEFAULT_TOOLS["listar_numeros"].risk is Risk.READ
    for esq in reg_ops.schemas():
        assert esq["function"]["parameters"]["type"] == "object"


def test_sem_ops_as_ferramentas_de_operacao_nao_aparecem(store):
    assert "gerenciar_lembretes" not in default_registry(store).names()


def test_lembretes_pela_ferramenta(reg_ops):
    r = _chamar(
        reg_ops,
        "gerenciar_lembretes",
        acao="criar",
        titulo="Pagar boleto",
        quando="2026-06-05T09:00:00",
    )
    assert r["ok"] and r["lembrete"]["title"] == "Pagar boleto"
    lid = r["lembrete"]["id"]
    assert _chamar(reg_ops, "gerenciar_lembretes", acao="listar")["total"] == 1
    assert _chamar(reg_ops, "gerenciar_lembretes", acao="pendentes")["total"] == 1  # já venceu
    assert (
        _chamar(reg_ops, "gerenciar_lembretes", acao="concluir", lembrete_id=lid)["lembrete"][
            "done"
        ]
        == 1
    )
    assert _chamar(reg_ops, "gerenciar_lembretes", acao="remover", lembrete_id=lid)["ok"] is True
    ruim = _chamar(
        reg_ops, "gerenciar_lembretes", acao="criar", titulo="x", quando="semana que vem"
    )
    assert ruim["ok"] is False and "ISO" in ruim["erro"]
    sumiu = _chamar(reg_ops, "gerenciar_lembretes", acao="concluir", lembrete_id=99)
    assert sumiu["ok"] is False and sumiu["erro"] == "lembrete 99 não existe"
    assert _chamar(reg_ops, "gerenciar_lembretes", acao="voar")["codigo"] == "arguments_invalid"


def test_agendamentos_pela_ferramenta_guardam_a_ferramenta_sem_executar(reg_ops):
    r = _chamar(
        reg_ops,
        "gerenciar_agendamentos",
        acao="criar",
        titulo="Clima",
        tipo="diario",
        horario="07:30",
        ferramenta="consultar_clima",
        parametros='{"cidade": "Marília"}',
    )
    assert r["ok"] and r["agendamento"]["tool"] == "consultar_clima"
    sid = r["agendamento"]["id"]
    assert (
        _chamar(reg_ops, "gerenciar_agendamentos", acao="pausar", agendamento_id=sid)[
            "agendamento"
        ]["active"]
        == 0
    )
    assert (
        _chamar(reg_ops, "gerenciar_agendamentos", acao="retomar", agendamento_id=sid)[
            "agendamento"
        ]["active"]
        == 1
    )
    assert _chamar(reg_ops, "gerenciar_agendamentos", acao="listar")["total"] == 1
    assert _chamar(reg_ops, "gerenciar_agendamentos", acao="remover", agendamento_id=sid)["ok"]
    for ruim in (
        {"parametros": "{quebrado"},
        {"parametros": "[1]"},
        {"horario": "8h"},
        {"tipo": "cron"},
    ):
        args = {"acao": "criar", "titulo": "x", "tipo": "diario", "horario": "08:00", **ruim}
        assert _chamar(reg_ops, "gerenciar_agendamentos", **args)["ok"] is False


def test_tarefas_e_numeros_pela_ferramenta(reg_ops):
    t = _chamar(reg_ops, "gerenciar_tarefas", acao="criar", titulo="Estudar UML")["tarefa"]
    feita = _chamar(
        reg_ops, "gerenciar_tarefas", acao="atualizar", tarefa_id=t["id"], status="concluida"
    )
    assert feita["tarefa"]["status"] == "concluida"
    assert (
        _chamar(reg_ops, "gerenciar_tarefas", acao="atualizar", tarefa_id=t["id"], status="feito")[
            "ok"
        ]
        is False
    )
    assert _chamar(reg_ops, "gerenciar_tarefas", acao="listar")["total"] == 1
    assert _chamar(reg_ops, "gerenciar_tarefas", acao="remover", tarefa_id=t["id"])["ok"]

    assert _chamar(reg_ops, "registrar_numero", alvo="fatura", score=0.9, motivo="vence amanhã")[
        "ok"
    ]
    _chamar(reg_ops, "registrar_numero", alvo="log", score=0.1)
    assert [n["target"] for n in _chamar(reg_ops, "listar_numeros")["numeros"]] == ["fatura"]
    assert _chamar(reg_ops, "listar_numeros", somente_pendentes=False)["total"] == 2
    assert _chamar(reg_ops, "registrar_numero", alvo=" ", score=1)["ok"] is False
