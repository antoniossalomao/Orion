import json
import sqlite3

import pytest

from orion.memory import MemoryStore


# ── sessões: um histórico por canal (B2) ────────────────────────────────────
def test_sessao_ativa_por_canal_nao_mistura_historicos(store):
    tg, web = store.active_session("telegram"), store.active_session("web")
    assert tg.id != web.id and store.active_session("telegram").id == tg.id
    store.add_message(tg.id, "user", "oi pelo telegram")
    store.add_message(web.id, "user", "oi pela web")
    assert [m.text for m in store.history(tg.id)] == ["oi pelo telegram"]
    assert [m.text for m in store.history(web.id)] == ["oi pela web"]


def test_historico_ordem_limite_e_proveniencia(store):
    s = store.new_session("web")
    for i in range(5):
        store.add_message(
            s.id,
            "user" if i % 2 == 0 else "assistant",
            f"m{i}",
            provenance={"fontes": [i]} if i == 4 else None,
        )
    h = store.history(s.id, limit=3)
    assert [m.text for m in h] == ["m2", "m3", "m4"]
    assert h[-1].provenance == {"fontes": [4]} and h[0].provenance is None


def test_arquivar_sessao_cria_nova_na_proxima(store):
    a = store.active_session("web")
    store.archive_session(a.id)
    assert store.active_session("web").id != a.id


def test_papel_invalido_e_sessao_inexistente_sao_recusados(store):
    s = store.new_session("web")
    with pytest.raises(sqlite3.IntegrityError):
        store.add_message(s.id, "root", "x")
    with pytest.raises(sqlite3.IntegrityError):
        store.add_message("nao-existe", "user", "x")


# ── fatos ──────────────────────────────────────────────────────────────────
def test_fato_idempotente_edita_e_esquece_de_verdade(store):
    a = store.add_fact("Antônio estuda ADS na UNIMAR", "conversa")
    b = store.add_fact("  antônio   estuda ADS na unimar ", "outra fonte")
    assert a.id == b.id and len(store.facts()) == 1

    store.update_fact(a.id, "Antônio cursa ADS na UNIMAR", source="manual")
    assert store.facts()[0].source == "manual"
    assert store.search("cursa", kinds=["fact"])[0].id == a.id
    assert store.search("estuda", kinds=["fact"]) == []  # índice acompanhou a edição

    assert store.forget_fact(a.id) is True and store.forget_fact(a.id) is False
    assert store.search("UNIMAR") == []
    assert (
        store._conn.execute(
            "SELECT COUNT(*) FROM facts_fts WHERE facts_fts MATCH 'unimar'"
        ).fetchone()[0]
        == 0
    )


def test_fato_vazio_e_edicao_de_inexistente(store):
    with pytest.raises(ValueError):
        store.add_fact("   ", "x")
    with pytest.raises(KeyError):
        store.update_fact(999, "x")


def test_exporta_fatos_para_o_vault(store, tmp_path):
    store.add_fact("Antônio mora em Marília-SP", "conversa")
    arq = store.export_facts(tmp_path / "vault" / "Orion")
    texto = arq.read_text(encoding="utf-8")
    assert texto.startswith("---\ntype: fatos-orion") and "fonte: conversa" in texto
    assert "Marília-SP" in texto and "id 1" in texto


# ── busca por palavra-chave ─────────────────────────────────────────────────
def test_busca_ignora_acento_e_casa_flexoes(store):
    store.add_fact("Antônio trabalha com açúcar e café", "manual")
    for consulta in ["acucar", "ACÚCAR", "cafe", "onde eu trabalho?", "trabalhei"]:
        assert store.search(consulta), consulta


def test_busca_com_sintaxe_estranha_nao_quebra(store):
    store.add_fact("fato qualquer", "manual")
    for sujo in ['"', "a OR", "NEAR(a b)", "col:valor", "*", "'; DROP TABLE facts;--", "", "?!"]:
        assert isinstance(store.search(sujo), list)
    assert len(store.facts()) == 1


def test_busca_cobre_conversas_e_filtra_por_tipo(store):
    s = store.active_session("telegram")
    store.add_message(s.id, "user", "preciso comprar um notebook novo")
    store.add_message(s.id, "tool", "notebook notebook notebook")  # ferramenta não é memória
    hits = store.search("notebook")
    assert [h.kind for h in hits] == ["message"] and hits[0].source == "sessão telegram"
    assert store.search("notebook", kinds=["fact"]) == []


# ── documentos / vault ──────────────────────────────────────────────────────
def test_documento_idempotente_e_reindexa_quando_muda(store):
    assert (
        store.index_document("a.md", "Nota A", "---\ntype: x\n---\nconteúdo sobre redes neurais")
        == "new"
    )
    assert (
        store.index_document("a.md", "Nota A", "---\ntype: x\n---\nconteúdo sobre redes neurais")
        == "same"
    )
    assert store.index_document("a.md", "Nota A", "agora fala de bancos de dados") == "updated"
    assert store.search("redes neurais") == []
    h = store.search("bancos de dados")[0]
    assert h.kind == "chunk" and h.source == "a.md" and h.text.startswith("Nota A")
    assert "type: x" not in h.text


def test_vault_ignora_ocultos_e_remove_o_que_sumiu(store, tmp_path):
    v = tmp_path / "vault"
    (v / ".obsidian").mkdir(parents=True)
    (v / "Projetos").mkdir()
    (v / ".obsidian" / "config.md").write_text("segredo da config", encoding="utf-8")
    (v / "Projetos" / "Orion.md").write_text("# Orion\nassistente pessoal", encoding="utf-8")
    (v / "Ideias.md").write_text("ideia de app", encoding="utf-8")
    assert store.index_vault(v) == {"new": 2, "updated": 0, "same": 0, "removed": 0}
    assert store.search("segredo da config") == []
    (v / "Ideias.md").unlink()
    (v / "Projetos" / "Orion.md").write_text("# Orion\nassistente pessoal v2", encoding="utf-8")
    assert store.index_vault(v) == {"new": 0, "updated": 1, "same": 0, "removed": 1}
    assert store.search("ideia de app") == []


# ── vetores ─────────────────────────────────────────────────────────────────
def test_vetor_acha_sinonimo_que_a_palavra_chave_nao_acha(store, store_vec):
    for s in (store, store_vec):
        s.add_fact("Antônio estuda ADS na UNIMAR", "manual")
        s.add_fact("Antônio mora em Marília-SP", "manual")
    assert store.search("universidade", kinds=["fact"]) == []  # só FTS: nada
    hit = store_vec.search("universidade", kinds=["fact"])[0]
    assert "UNIMAR" in hit.text and hit.via == "vec"
    assert "MORAR" not in hit.text


def test_busca_hibrida_combina_as_duas_vias(store_vec):
    store_vec.add_fact("Antônio estuda ADS na UNIMAR", "manual")
    hit = store_vec.search("estuda UNIMAR", kinds=["fact"])[0]
    assert hit.via == "fts+vec"


def test_embedder_fora_do_ar_nao_derruba_escrita_nem_busca(store_vec, embedder):
    embedder.quebrado = True
    f = store_vec.add_fact("Antônio mora em Marília-SP", "manual")  # não levanta
    assert (
        store_vec._conn.execute("SELECT embedded FROM facts WHERE id=?", (f.id,)).fetchone()[0] == 0
    )
    hits = store_vec.search("Marília")  # cai para palavra-chave
    assert hits and hits[0].via == "fts"
    embedder.quebrado = False
    assert store_vec.embed_pending() == 1
    assert store_vec.search("cidade onde moro", kinds=["fact"])[0].id == f.id


def test_editar_e_esquecer_atualizam_o_vetor(store_vec):
    f = store_vec.add_fact("Antônio mora em Marília", "manual")
    store_vec.update_fact(f.id, "Antônio estuda na faculdade", "manual")
    assert store_vec.search("universidade", kinds=["fact"])[0].id == f.id
    store_vec.forget_fact(f.id)
    assert store_vec.search("universidade", kinds=["fact"]) == []
    assert store_vec._conn.execute("SELECT COUNT(*) FROM vec_facts").fetchone()[0] == 0


def test_troca_de_dimensao_exige_reset(tmp_path, embedder):
    caminho = tmp_path / "x.db"
    s = MemoryStore(caminho, embedder=embedder)
    s.add_fact("Antônio mora em Marília", "manual")
    s.close()

    class Outro:
        dim = 16

        def embed(self, texts):
            return [[1.0] * 16 for _ in texts]

    with pytest.raises(RuntimeError, match="reset_vectors"):
        MemoryStore(caminho, embedder=Outro())
    s = MemoryStore(caminho, embedder=embedder)
    s.reset_vectors()
    assert s.embed_pending() == 1
    s.close()


def test_sem_extensao_vetorial_a_busca_funciona_so_por_palavra(tmp_path, embedder, monkeypatch):
    monkeypatch.setattr(MemoryStore, "_load_vec", staticmethod(lambda conn: False))
    s = MemoryStore(tmp_path / "n.db", embedder=embedder)
    s.add_fact("Antônio mora em Marília", "manual")
    assert not s.vectors_available and s.search("Marília")[0].via == "fts"
    s.close()


# ── backup e restauração ────────────────────────────────────────────────────
def popular(s: MemoryStore):
    s.add_fact("Antônio mora em Marília-SP", "manual")
    s.index_document("n.md", "Nota", "texto sobre o projeto orion")
    sess = s.active_session("web")
    s.add_message(sess.id, "user", "lembra do projeto orion?")


def test_backup_diario_rotaciona_e_nao_duplica_no_mesmo_dia(store, tmp_path):
    relogio = {"t": 1_700_000_000.0}
    store._clock = lambda: relogio["t"]
    popular(store)
    pasta = tmp_path / "bk"
    assert store.daily_backup(pasta, manter=3) is not None
    assert store.daily_backup(pasta, manter=3) is None  # já tem o de hoje
    for _ in range(4):
        relogio["t"] += 86_400
        store.daily_backup(pasta, manter=3)
    assert len(list(pasta.glob("orion-*.db"))) == 3


def test_restore_drill_backup_restaura_e_responde_igual(store, tmp_path):
    popular(store)
    bk = store.backup_to(tmp_path / "bk" / "orion.db")
    assert MemoryStore.verify_backup(bk) == {
        "sessions": 1,
        "messages": 1,
        "facts": 1,
        "documents": 1,
        "chunks": 1,
    }
    novo = MemoryStore.restore(bk, tmp_path / "novo" / "orion.db")
    try:
        assert {h.kind for h in novo.search("projeto orion")} == {"chunk", "message"}
        assert novo.search("Marília")[0].kind == "fact"
        assert novo.history(novo.active_session("web").id)[0].text == "lembra do projeto orion?"
    finally:
        novo.close()


def test_backup_corrompido_ou_estranho_e_recusado(store, tmp_path):
    popular(store)
    bk = store.backup_to(tmp_path / "ok.db")
    ruim = tmp_path / "ruim.db"
    ruim.write_bytes(bk.read_bytes()[:2048])
    with pytest.raises(Exception):  # noqa: B017 — corrompido: qualquer erro do sqlite serve
        MemoryStore.verify_backup(ruim)
    estranho = tmp_path / "estranho.db"
    c = sqlite3.connect(estranho)
    c.execute("CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT)")
    c.execute("INSERT INTO meta VALUES('schema_version','99')")
    c.commit()
    c.close()
    with pytest.raises(ValueError, match="esquema"):
        MemoryStore.verify_backup(estranho)


def test_proveniencia_e_json_valido(store):
    s = store.new_session("web")
    m = store.add_message(s.id, "assistant", "x", provenance={"modelo": "m", "fontes": ["fato:1"]})
    bruto = store._conn.execute("SELECT provenance FROM messages WHERE id=?", (m.id,)).fetchone()[0]
    assert json.loads(bruto)["fontes"] == ["fato:1"]


def test_vault_ignora_arquivo_gigante(store, tmp_path, monkeypatch):
    from orion.memory import store as modulo

    monkeypatch.setattr(modulo, "MAX_NOTA_BYTES", 100)
    v = tmp_path / "vault"
    v.mkdir()
    (v / "pequena.md").write_text("curta", encoding="utf-8")
    (v / "grande.md").write_text("x " * 200, encoding="utf-8")
    assert store.index_vault(v)["new"] == 1
