"""Banco da linhagem antiga da `main` (v3–v11) sobe para o esquema atual sem perder nada."""

import sqlite3

import pytest

from orion.memory import MemoryStore, legacy_main, schema
from orion.memory.scope import data_scope


def _banco_da_main(caminho, versao):
    c = sqlite3.connect(caminho)
    c.executescript(schema.DDL_V1 + schema.DDL_V2)
    c.execute("INSERT INTO meta VALUES ('schema_version', '2')")
    for v in range(2, versao):
        c.executescript(
            f"BEGIN;{legacy_main.MAIN_MIGRATIONS[v]}"
            f"UPDATE meta SET value='{v + 1}' WHERE key='schema_version';COMMIT;"
        )
    return c


def _povoar_v11(c):
    c.execute(
        "INSERT INTO projects(id, name, instructions, archived, created_at, updated_at)"
        " VALUES (7, 'Casa', 'Seja breve', 0, 1, 1)"
    )
    for sid, pinned, deleted, shelved, projeto, archived in (
        ("a" * 32, 1, 0, 0, 7, 0),
        ("b" * 32, 0, 1, 0, None, 0),
        ("c" * 32, 0, 0, 1, None, 0),
        ("d" * 32, 0, 0, 0, None, 1),
    ):
        c.execute(
            "INSERT INTO sessions(id, channel, title, created_at, last_active_at, archived,"
            " pinned, deleted, shelved, project_id) VALUES (?,?,?,?,?,?,?,?,?,?)",
            (sid, "web", f"t-{sid[0]}", 1, 2, archived, pinned, deleted, shelved, projeto),
        )
    c.execute("INSERT INTO active_sessions VALUES ('web', ?)", ("a" * 32,))
    c.execute(
        "INSERT INTO messages(session_id, role, text, created_at) VALUES (?,?,?,?)",
        ("a" * 32, "user", "lembrar do aniversário da Marina", 3),
    )
    c.execute(
        "INSERT INTO facts(text, source, created_at, updated_at) VALUES ('café sem açúcar','m',1,1)"
    )
    c.execute(
        "INSERT INTO documents(source, title, content_hash, indexed_at, project_id)"
        " VALUES ('upload:x.txt', 'x', 'h', 1, 7)"
    )
    c.execute("INSERT INTO chunks(document_id, ord, text) VALUES (1, 0, 'texto do documento')")
    c.execute(
        "INSERT INTO artifacts(session_id, project_id, kind, name, stored, bytes, tool, version,"
        " created_at) VALUES (?,?,?,?,?,?,?,?,?)",
        ("a" * 32, 7, "documento", "plano.md", "1-plano.md", 10, "gerar_documento", 2, 5),
    )
    c.execute(
        "INSERT INTO notifications(kind, ref, text, created_at, urgent) VALUES ('l','r','t',1,1)"
    )
    c.execute(
        "INSERT INTO external_calls(ts, provider, kind, ok, latency_ms) VALUES (1,'groq','chat',1,10)"
    )
    c.execute(
        "INSERT INTO screen_log(ts, title, text) VALUES (strftime('%s','now'), 'janela', 'conteúdo lido')"
    )
    c.execute("INSERT INTO audit(ts, tool, action) VALUES (1, 'buscar_memoria', 'allow')")
    c.execute("INSERT INTO meta VALUES ('counter:uso', '3')")
    c.commit()


def test_banco_v11_da_main_vira_o_esquema_atual_sem_perder_dados(tmp_path):
    arq = tmp_path / "m.db"
    c = _banco_da_main(arq, 11)
    _povoar_v11(c)
    c.close()
    store = MemoryStore(arq)
    assert store.query("SELECT value FROM meta WHERE key='schema_version'")[0][0] == str(
        schema.SCHEMA_VERSION
    )
    projeto = store.query("SELECT * FROM projects")[0]
    assert projeto["name"] == "Casa" and len(projeto["id"]) == 32
    por_id = {s.id: s for s in store.list_sessions("web")}
    assert por_id["a" * 32].favorite and por_id["a" * 32].project_id == projeto["id"]
    # apagada, guardada e importada saem da lista ativa, mas as mensagens ficam
    assert all(por_id[x * 32].archived for x in "bcd")
    ativa = store.selected_session("web")
    assert ativa is not None and ativa.id == "a" * 32
    assert store.history("a" * 32)[0].text.startswith("lembrar")
    with data_scope(projeto["id"]):
        assert store.search("aniversário Marina", kinds=("message",))
        assert store.search("documento", kinds=("chunk",))
    assert store.search("açúcar", kinds=("fact",))
    doc = store.query("SELECT project_id FROM documents")[0]
    assert doc["project_id"] == projeto["id"]
    lib = store.list_artifacts()
    assert lib[0]["project_id"] == projeto["id"] and lib[0]["version"] == 2
    assert store.query("SELECT urgent FROM notifications")[0][0] == 1
    assert store.query("SELECT provider FROM external_calls")[0][0] == "groq"
    assert store.search_screen("conteúdo")
    assert store.query("SELECT count(*) FROM audit")[0][0] == 1
    assert store.meta_get("x") is None and store.counter_get("uso") == 3
    store.close()
    assert (tmp_path / "m.db.main-v11.bak").exists()
    # abrir de novo não converte outra vez
    MemoryStore(arq).close()
    assert not (tmp_path / "m.db.main-v11.bak.main-v17.bak").exists()


@pytest.mark.parametrize("versao", [3, 4, 5, 6, 7, 8, 9, 10])
def test_banco_antigo_da_main_sobe_ate_o_esquema_atual(tmp_path, versao):
    arq = tmp_path / "m.db"
    c = _banco_da_main(arq, versao)
    c.execute(
        "INSERT INTO sessions(id, channel, title, created_at, last_active_at)"
        " VALUES ('s1', 'web', 'antiga', 1, 1)"
    )
    c.execute(
        "INSERT INTO messages(session_id, role, text, created_at) VALUES ('s1','user','oi',1)"
    )
    c.commit()
    c.close()
    store = MemoryStore(arq)
    assert store.history("s1")[0].text == "oi"
    antiga = store.get_session("s1")
    assert antiga is not None and antiga.title == "antiga"
    assert (tmp_path / f"m.db.main-v{versao}.bak").exists()
    store.close()


def test_banco_da_evolucao_nao_e_confundido_com_o_da_main(tmp_path):
    arq = tmp_path / "e.db"
    MemoryStore(arq).close()
    c = sqlite3.connect(arq)
    assert legacy_main.versao_da_main(c) is None  # v17 da evolução tem `audit`, mas não é v3–v11
    c.close()
    assert not list(tmp_path.glob("*.bak"))
