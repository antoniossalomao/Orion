import sqlite3
from concurrent.futures import ThreadPoolExecutor
from contextlib import closing

from orion.memory import MemoryStore
from orion.memory.schema import DDL_V1, DDL_V2, SCHEMA_VERSION


def test_resposta_atrasada_e_relogio_igual_nao_trocam_selecao(tmp_path):
    path = tmp_path / "sessions.db"
    with closing(MemoryStore(path, clock=lambda: 100)) as store:
        a, b = store.new_session("web"), store.new_session("web")
        store.activate_session("web", a.id)
        store.add_message(b.id, "assistant", "Resposta atrasada de B")
        assert store.active_session("web").id == a.id
    with closing(MemoryStore(path, clock=lambda: 1)) as store:
        assert store.active_session("web").id == a.id
        store.activate_session("web", b.id)
        store.add_message(a.id, "assistant", "Resposta atrasada de A")
        assert store.active_session("web").id == b.id


def test_primeiro_acesso_concorrente_cria_uma_sessao_por_canal(store):
    with ThreadPoolExecutor(max_workers=8) as pool:
        ids = list(pool.map(lambda _: store.active_session("web").id, range(20)))
    assert len(set(ids)) == 1
    assert len(store.list_sessions("web")) == 1


def test_arquivar_ativa_nao_reabre_conversa_anterior(store):
    antiga = store.new_session("web")
    atual = store.new_session("web")
    store.archive_session(atual.id)
    assert store.selected_session("web") is None
    assert store.active_session("web").id not in {antiga.id, atual.id}


def test_banco_v2_migra_selecao_sem_perder_mensagens(tmp_path):
    path = tmp_path / "v2.db"
    with sqlite3.connect(path) as c:
        c.executescript(DDL_V1 + DDL_V2)
        c.execute("INSERT INTO meta VALUES ('schema_version','2')")
        for sid, canal, timestamp, archived in [
            ("old", "web", 1, 0),
            ("new", "web", 2, 0),
            ("archive", "web", 9, 1),
            ("tg", "telegram", 3, 0),
        ]:
            c.execute(
                "INSERT INTO sessions VALUES (?,?,?,?,?,?)",
                (sid, canal, sid, timestamp, timestamp, archived),
            )
        c.execute(
            "INSERT INTO messages(session_id, role, text, created_at)"
            " VALUES ('new','user','Mensagem v2',2)"
        )
    with closing(MemoryStore(path)) as store:
        assert store.query("SELECT value FROM meta WHERE key='schema_version'")[0][0] == str(
            SCHEMA_VERSION
        )
        web, telegram = store.selected_session("web"), store.selected_session("telegram")
        assert web is not None and web.id == "new"
        assert telegram is not None and telegram.id == "tg"
        assert store.history("new")[0].text == "Mensagem v2"
        store.activate_session("web", "old")
    with closing(MemoryStore(path)) as store:
        assert store.active_session("web").id == "old"
        assert store.history("new")[0].text == "Mensagem v2"
