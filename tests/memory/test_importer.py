import json
from datetime import datetime

import pytest

from orion.__main__ import main
from orion.memory.importer import import_surreal_export
from orion.memory.ops import Operations
from tests.memory.conftest import AGORA, SESSAO_A, SESSAO_B, escrever, evento  # noqa: F401


def test_importa_conversas_com_datas_papeis_e_sessoes(store, export):
    rel = import_surreal_export(store, export, assistentes=["Orion", "Lyra"])
    assert (rel.sessoes, rel.mensagens, rel.ja_importadas, rel.invalidas) == (4, 6, 0, 3)
    assert rel.operacionais == {"lembrete": 2}

    legado = {s.title: s for s in store.list_sessions("legado")}
    assert set(legado) == {
        "Projeto Orion",
        "conversa sem título",
        "conversas antigas (pré-sessões)",
        "sessão 99999999",
    }
    msgs = store.history(legado["Projeto Orion"].id)
    assert [(m.role, m.text) for m in msgs] == [
        ("user", "Em que cidade eu moro?"),
        ("assistant", "Marília-SP, certo."),
    ]
    assert msgs[0].created_at < msgs[1].created_at  # ordem cronológica pela data original
    assert msgs[1].provenance == {
        "importado": "surrealdb",
        "fonte": "chat",
        "intencao": "resposta",
        "fontes_rag": ["p1"],
    }

    b = store.history(legado["conversa sem título"].id)
    assert [m.role for m in b] == [
        "system",
        "assistant",
    ]  # "sistema" não vira fala de ninguém; Lyra = assistente
    assert [m.role for m in store.history(legado["conversas antigas (pré-sessões)"].id)] == ["user"]


def test_importado_e_pesquisavel_e_nao_vira_sessao_ativa(store, export):
    import_surreal_export(store, export, assistentes=["Orion"])
    hit = store.search("em que cidade eu moro", kinds=["message"])[0]
    assert hit.source == "sessão legado" and "cidade" in hit.text
    assert store.list_sessions("web") == []
    importadas = {s.id for s in store.list_sessions("legado")}
    assert store.active_session("legado").id not in importadas  # arquivadas: nenhuma é reaberta


def test_nome_antigo_do_assistente_sem_configurar_vira_system(store, export):
    import_surreal_export(store, export, assistentes=["Orion"])
    papeis = {m.text: m.role for s in store.list_sessions("legado") for m in store.history(s.id)}
    assert papeis["resposta de quando eu tinha outro nome"] == "system"


def test_reimportar_nao_duplica(store, export):
    import_surreal_export(store, export, assistentes=["Orion"])
    de_novo = import_surreal_export(store, export, assistentes=["Orion"])
    assert (de_novo.sessoes, de_novo.mensagens, de_novo.ja_importadas) == (0, 0, 6)
    total = store._conn.execute("SELECT COUNT(*) FROM messages").fetchone()[0]
    assert total == 6 and len(store.list_sessions("legado")) == 4


def test_export_incompleto_ou_corrompido(store, tmp_path):
    with pytest.raises(FileNotFoundError, match="backup_memoria"):
        import_surreal_export(store, tmp_path)
    (tmp_path / "evento.json").write_text('{"nao": "lista"}', encoding="utf-8")
    with pytest.raises(ValueError, match="lista"):
        import_surreal_export(store, tmp_path)
    (tmp_path / "evento.json").write_text("{quebrado", encoding="utf-8")
    with pytest.raises(json.JSONDecodeError):
        import_surreal_export(store, tmp_path)


def test_cli_import_surreal(export, tmp_path, monkeypatch, capsys):
    monkeypatch.setenv("ORION_DATA_DIR", str(tmp_path / "dados"))
    monkeypatch.setenv("ORION_LOG_JSON", "false")
    assert main(["import-surreal", str(export), "--assistente", "Lyra"]) == 0
    assert "mensagens novas=6" in capsys.readouterr().out
    assert main(["import-surreal", str(export)]) == 0
    assert "mensagens novas=0" in capsys.readouterr().out


def test_importa_operacao_com_regras_de_seguranca_dos_avisos(store, export_completo):
    store._clock = lambda: AGORA
    rel = import_surreal_export(store, export_completo, assistentes=["Orion"])
    assert rel.operacionais == {
        "lembrete": 2,
        "agendamento": 3,
        "tarefa": 2,
        "numero": 1,
        "prompt": 1,
    }
    # base + agendamento de tipo estranho + tarefa sem título + número com score "alto"
    assert rel.invalidas == 3 + 1 + 1 + 1
    ops = Operations(store)

    por_titulo = {r["title"]: r for r in ops.list_reminders()}
    antigo, futuro = por_titulo["Pagar boleto"], por_titulo["Entregar trabalho"]
    assert antigo["notified"] == 1 and futuro["notified"] == 0  # sem rajada de aviso velho
    assert futuro["note"] == "UML" and antigo["due_at"] == datetime(2026, 6, 5, 9).timestamp()
    assert [r["title"] for r in ops.due_reminders(AGORA)] == []

    ag = {s["title"]: s for s in ops.list_schedules()}
    assert ag["Checar saúde"]["next_run"] == datetime(2026, 10, 4, 8).timestamp()  # recalculado
    assert ag["Checar saúde"]["tool"] == "checar_saude_sistema"
    assert json.loads(ag["Checar saúde"]["params"]) == {"verbose": True}
    assert ag["Aviso velho"]["active"] == 0 and ag["Aviso velho"]["next_run"] is None
    assert (
        ag["Pausado"]["active"] == 0 and ag["Pausado"]["params"] == "{}"
    )  # JSON quebrado não entra
    assert ops.due_schedules(AGORA) == []

    tarefas = {t["title"]: t["status"] for t in ops.list_tasks()}
    assert tarefas == {"Estudar UML": "em_andamento", "Status esquisito": "pendente"}
    numeros = {n["target"]: n for n in ops.list_numbers(only_pending=False)}
    assert set(numeros) == {"fatura"}
    assert (
        numeros["fatura"]["score"] == 1.0 and numeros["fatura"]["notified"] == 1
    )  # 7 → limitado a 1
    assert ops.list_prompts()[0]["content"] == "Resuma em 5 linhas"


def test_importa_grafo_resolvendo_eventos_para_mensagens(store, export_completo):
    rel = import_surreal_export(store, export_completo, assistentes=["Orion"])
    assert (rel.arestas, rel.arestas_repetidas, rel.arestas_sem_no) == (4, 0, 2)
    ops = Operations(store)
    msg = {
        r["external_id"]: r["ref"]
        for r in store.query("SELECT external_id, ref FROM imported WHERE kind='mensagem'")
    }
    m1, m2 = f"message:{msg['e1']}", f"message:{msg['e2']}"
    assert {(n["rel"], n["src"], n["dst"]) for n in ops.neighbors(m1)} == {
        ("precedeu", m1, m2),
        ("sobre", m1, "topic:marília"),
    }
    vizinhos = ops.neighbors("topic:marília")
    assert {(n["direction"], n["rel"]) for n in vizinhos} >= {("in", "sobre"), ("out", "conecta")}
    conecta = next(n for n in vizinhos if n["rel"] == "conecta")
    assert (conecta["dst"], conecta["kind"], conecta["weight"]) == ("topic:unimar", "rem", 2.0)
    assert [n["src"] for n in ops.neighbors("topic:marília", rel="sobre")] == [m1, m2]


def test_reimportar_operacao_e_grafo_nao_duplica(store, export_completo):
    store._clock = lambda: AGORA
    import_surreal_export(store, export_completo, assistentes=["Orion"])
    ops = Operations(store)
    antes = (len(ops.list_reminders()), len(ops.list_schedules()), ops.edge_count())
    de_novo = import_surreal_export(store, export_completo, assistentes=["Orion"])
    assert de_novo.operacionais == {} and de_novo.arestas == 0
    assert de_novo.operacionais_repetidos == {
        "lembrete": 2,
        "agendamento": 3,
        "tarefa": 2,
        "numero": 1,
        "prompt": 1,
    }
    assert de_novo.arestas_repetidas == 4
    assert (len(ops.list_reminders()), len(ops.list_schedules()), ops.edge_count()) == antes


def test_importar_so_operacao_sem_grafo_funciona(store, export):
    # export sem precedeu/sobre/conecta (ex.: base que nunca gerou relações)
    rel = import_surreal_export(store, export, assistentes=["Orion"])
    assert (rel.arestas, rel.arestas_repetidas, rel.arestas_sem_no) == (0, 0, 0)
