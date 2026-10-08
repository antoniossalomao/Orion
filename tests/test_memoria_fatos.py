"""C36 / `orion esquecer`: listar, corrigir e esquecer fatos pela API e pela CLI."""

import pytest
from fastapi.testclient import TestClient

from orion.__main__ import main
from orion.app import create_app
from orion.config import Settings
from orion.memory import MemoryStore

TOKEN = "token-de-teste-com-16+"
AUTH = {"Authorization": f"Bearer {TOKEN}"}


@pytest.fixture
def c(tmp_path):
    settings = Settings(data_dir=tmp_path / "d", admin_token=TOKEN, _env_file=None)
    with TestClient(
        create_app(settings, gateway_factory=lambda _: None), base_url="http://127.0.0.1"
    ) as cli:
        yield cli


def mem(c):
    return c.app.state.orion.memory


def test_exige_login(c):
    assert c.get("/memoria/fatos").status_code == 401
    assert c.patch("/memoria/fatos/1", json={"texto": "x"}).status_code == 401
    assert c.delete("/memoria/fatos/1").status_code == 401


def test_listar_buscar_corrigir_e_esquecer(c):
    m = mem(c)
    a = m.add_fact("Antônio mora em Marília", "conversa")
    b = m.add_fact("Antônio estuda na Unimar", "vault")
    todos = c.get("/memoria/fatos", headers=AUTH).json()
    assert todos["total"] == 2 and {f["fonte"] for f in todos["fatos"]} == {"conversa", "vault"}
    achou = c.get("/memoria/fatos?q=unimar", headers=AUTH).json()["fatos"]
    assert [f["id"] for f in achou] == [b.id]
    r = c.patch(f"/memoria/fatos/{a.id}", json={"texto": "Antônio mora em Bauru"}, headers=AUTH)
    assert r.status_code == 200 and r.json()["fato"]["fonte"] == "manual"
    assert c.delete(f"/memoria/fatos/{a.id}", headers=AUTH).json() == {"ok": True}
    assert c.delete(f"/memoria/fatos/{a.id}", headers=AUTH).status_code == 404
    assert c.patch("/memoria/fatos/999", json={"texto": "x"}, headers=AUTH).status_code == 404
    assert m.search_facts("Marília") == [] and m.search_facts("Bauru") == []


def test_cli_esquecer_por_trecho_e_por_id(tmp_path, monkeypatch, capsys):
    monkeypatch.setenv("ORION_DATA_DIR", str(tmp_path / "d"))
    monkeypatch.chdir(tmp_path)
    s = Settings(_env_file=None)
    store = MemoryStore(s.db_path)
    f1 = store.add_fact("senha do wifi é azul", "conversa")
    store.add_fact("gosta de café", "conversa")
    store.close()
    assert main(["esquecer", "wifi", "--sim"]) == 0
    assert "NÃO foram tocados" in capsys.readouterr().out
    assert main(["esquecer", "wifi", "--sim"]) == 2  # já foi
    store = MemoryStore(s.db_path)
    assert [f.text for f in store.facts()] == ["gosta de café"]
    ident = store.facts()[0].id
    store.close()
    monkeypatch.setattr("builtins.input", lambda _: "n")
    assert main(["esquecer", str(ident)]) == 1
    monkeypatch.setattr("builtins.input", lambda _: "s")
    assert main(["esquecer", str(ident)]) == 0
    assert f1.id != ident


def test_duplicados_acha_pares_quase_iguais_sem_ligar_para_acento_e_caixa(tmp_path):
    m = MemoryStore(tmp_path / "d.db")
    m.add_fact("Antônio estuda Sistemas de Informação na Unimar", "conversa")
    m.add_fact("antonio estuda sistemas de informacao na unimar em Marília", "vault")
    m.add_fact("Gosta de café sem açúcar", "conversa")
    pares = m.duplicate_facts()
    assert len(pares) == 1 and {pares[0][0].source, pares[0][1].source} == {"conversa", "vault"}
    assert m.duplicate_facts(limiar=0.99) == []
    m.close()


def test_cli_fatos_lista_e_mostra_duplicados(tmp_path, monkeypatch, capsys):
    monkeypatch.setenv("ORION_DATA_DIR", str(tmp_path / "d"))
    monkeypatch.chdir(tmp_path)
    store = MemoryStore(Settings(_env_file=None).db_path)
    store.add_fact("mora em Marília", "conversa")
    store.add_fact("mora em Marilia", "vault")
    store.close()
    assert main(["fatos"]) == 0
    assert "mora em Marília" in capsys.readouterr().out
    assert main(["fatos", "--duplicados"]) == 0
    assert "1 par(es)" in capsys.readouterr().out
