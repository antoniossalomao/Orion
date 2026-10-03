import json

import pytest

from orion.__main__ import main
from orion.memory.importer import import_surreal_export

SESSAO_A = "11111111-aaaa-bbbb-cccc-000000000001"
SESSAO_B = "22222222-aaaa-bbbb-cccc-000000000002"


def evento(i, ator, texto, ts, sessao=None, **extra):
    e = {
        "id": f"evento:⟨e{i}⟩",
        "fonte": "chat",
        "ator": ator,
        "texto": texto,
        "timestamp": ts,
        **extra,
    }
    if sessao:
        e["sessao_id"] = sessao
    return e


@pytest.fixture
def export(tmp_path):
    pasta = tmp_path / "backup"
    pasta.mkdir()
    (pasta / "sessao.json").write_text(
        json.dumps(
            [
                {
                    "id": f"sessao:`{SESSAO_A}`",
                    "criada": "2026-06-01T10:00:00",
                    "titulo": "Projeto Orion",
                },
                {"id": f"sessao:⟨{SESSAO_B}⟩", "criada": "2026-06-02T09:00:00", "titulo": None},
                {"titulo": "sem id"},
            ]
        ),
        encoding="utf-8",
    )
    (pasta / "evento.json").write_text(
        json.dumps(
            [
                evento(
                    2,
                    "Orion",
                    "Marília-SP, certo.",
                    "2026-06-01T10:00:05",
                    SESSAO_A,
                    intencao="resposta",
                    fontes_rag=["p1"],
                ),
                evento(
                    1,
                    "Antônio",
                    "Em que cidade eu moro?",
                    "2026-06-01T10:00:00",
                    SESSAO_A,
                    intencao="objetivo",
                ),
                evento(
                    3,
                    "Lyra",
                    "resposta de quando eu tinha outro nome",
                    "2026-06-02T09:00:10",
                    SESSAO_B,
                ),
                evento(4, "Antonio", "mensagem antiga sem sessão", "2026-01-01T08:00:00"),
                evento(5, "sistema", "briefing automático", "2026-06-02T09:00:00", SESSAO_B),
                evento(6, "Antônio", "   ", "2026-06-02T09:00:20", SESSAO_B),  # vazio
                {
                    "fonte": "chat",
                    "ator": "Antônio",
                    "texto": "sem id",
                    "timestamp": "2026-06-02T09:00:30",
                },  # sem id
                evento(
                    7,
                    "Antônio",
                    "sessão que não existe no export",
                    "2026-06-03T09:00:00",
                    "99999999-aaaa",
                ),
            ]
        ),
        encoding="utf-8",
    )
    (pasta / "lembrete.json").write_text(
        json.dumps([{"id": "lembrete:a"}, {"id": "lembrete:b"}]), encoding="utf-8"
    )
    (pasta / "numero.json").write_text("[]", encoding="utf-8")
    return pasta


def test_importa_conversas_com_datas_papeis_e_sessoes(store, export):
    rel = import_surreal_export(store, export, assistentes=["Orion", "Lyra"])
    assert (rel.sessoes, rel.mensagens, rel.ja_importadas, rel.invalidas) == (4, 6, 0, 3)
    assert rel.operacionais_ignoradas == {"lembrete": 2}

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
