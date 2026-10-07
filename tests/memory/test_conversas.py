"""Conversas na barra lateral: ativar, renomear, fixar e apagar (esquema v4)."""

import sqlite3

from orion.memory import MemoryStore
from orion.memory.schema import DDL_V1, DDL_V2, DDL_V3, SCHEMA_VERSION


def test_ativar_vira_a_ativa_do_canal_e_nao_toca_outro_canal(store):
    a = store.new_session("web")
    b = store.new_session("web")
    tg = store.new_session("telegram")
    assert store.active_session("web").id == b.id
    assert store.activate_session(a.id) is not None
    assert store.active_session("web").id == a.id
    assert store.active_session("telegram").id == tg.id


def test_renomear_normaliza_espaco_e_recusa_titulo_vazio(store):
    s = store.new_session("web")
    assert store.rename_session(s.id, "  Plano   da\nfase 0  ")
    assert store.get_session(s.id).title == "Plano da fase 0"
    assert store.rename_session(s.id, "   ") is False
    assert store.get_session(s.id).title == "Plano da fase 0"
    assert len(_titulo_longo(store)) == 120


def _titulo_longo(store):
    s = store.new_session("web")
    store.rename_session(s.id, "x" * 500)
    return store.get_session(s.id).title


def test_fixadas_vem_primeiro_na_lista_da_barra(store):
    velha, nova = store.new_session("web"), store.new_session("web")
    store.add_message(velha.id, "user", "antiga")
    store.add_message(nova.id, "user", "recente")
    assert [s.id for s in store.list_sessions_ui("web")] == [nova.id, velha.id]
    assert store.pin_session(velha.id, True)
    lista = store.list_sessions_ui("web")
    assert [s.id for s in lista] == [velha.id, nova.id] and lista[0].pinned
    assert store.pin_session(velha.id, False)
    assert [s.id for s in store.list_sessions_ui("web")] == [nova.id, velha.id]


def test_apagar_esconde_mas_guarda_as_mensagens_e_sai_da_ativa(store):
    s = store.new_session("web")
    store.add_message(s.id, "user", "guarde isto")
    assert store.delete_session(s.id) is True
    assert store.delete_session(s.id) is False  # segunda vez: já apagada
    assert s.id not in [x.id for x in store.list_sessions_ui("web")]
    assert s.id not in [x.id for x in store.list_sessions("web")]
    assert store.active_session("web").id != s.id
    assert [m.text for m in store.history(s.id)] == ["guarde isto"]  # só escondeu
    assert store.activate_session(s.id) is None and store.session_visible(s.id, "web") is None
    assert store.rename_session(s.id, "volta") is False and store.pin_session(s.id, True) is False


def test_apagar_tira_a_fixacao(store):
    s = store.new_session("web")
    store.pin_session(s.id, True)
    store.delete_session(s.id)
    assert store.get_session(s.id).pinned is False


def test_barra_mostra_importadas_como_somente_leitura_mas_nao_outros_canais(store):
    sid, _ = store.import_session("ext-1", "legado", "conversa antiga", 1.0)
    tg = store.new_session("telegram")
    web = store.new_session("web")
    ids = [s.id for s in store.list_sessions_ui("web")]
    assert web.id in ids and sid in ids and tg.id not in ids
    antiga = store.session_visible(sid, "web")
    assert antiga is not None and antiga.archived
    assert store.activate_session(sid) is None  # importada: só leitura
    assert store.session_visible(tg.id, "web") is None


def test_primeira_fala_serve_de_titulo(store):
    s = store.new_session("web")
    assert store.first_user_text(s.id) is None
    store.add_message(s.id, "assistant", "olá")
    store.add_message(s.id, "user", "me ajuda com o backup")
    store.add_message(s.id, "user", "outra")
    assert store.first_user_text(s.id) == "me ajuda com o backup"


def test_banco_v3_sobe_para_v4_sem_perder_conversas(tmp_path):
    caminho = tmp_path / "v3.db"
    c = sqlite3.connect(caminho)
    c.executescript(DDL_V1 + DDL_V2 + DDL_V3)
    c.execute("INSERT INTO meta VALUES ('schema_version', '3')")
    c.execute("INSERT INTO sessions VALUES ('s1', 'web', 'antiga', 1.0, 1.0, 0)")
    c.commit()
    c.close()
    store = MemoryStore(caminho)
    try:
        assert store.query("SELECT value FROM meta WHERE key='schema_version'")[0][0] == str(
            SCHEMA_VERSION
        )
        s = store.get_session("s1")
        assert s is not None and s.title == "antiga" and not s.pinned and not s.archived
        assert store.pin_session("s1", True) and store.delete_session("s1")
    finally:
        store.close()
