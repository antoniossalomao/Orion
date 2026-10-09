"""`orion doctor`: offline, sem vazar valor de chave."""

import json

from orion.__main__ import main
from orion.auth import AuthService
from orion.config import Settings
from orion.doctor import checar, relatorio
from orion.memory import MemoryStore


def _s(tmp_path, **kw):
    return Settings(data_dir=tmp_path / "d", _env_file=None, **kw)


def _por_nome(s):
    return {c.nome: c for c in checar(s)}


def test_instalacao_nova_so_tem_avisos(tmp_path):
    itens = _por_nome(_s(tmp_path))
    assert itens["pasta de dados"].nivel == "ok"
    assert itens["login"].nivel == "aviso" and "set-password" in itens["login"].detalhe
    assert itens["banco da memória"].nivel == "aviso"
    texto, codigo = relatorio(list(itens.values()))
    assert codigo == 0 and "0 erro(s)" in texto


def test_banco_integro_e_banco_corrompido(tmp_path):
    s = _s(tmp_path)
    MemoryStore(s.db_path).close()
    assert _por_nome(s)["banco da memória"].nivel == "ok"
    s.db_path.write_bytes(b"isto nao e um banco sqlite" * 50)
    item = _por_nome(s)["banco da memória"]
    assert item.nivel == "erro"
    assert relatorio(list(_por_nome(s).values()))[1] == 1


def test_senha_de_fabrica_e_erro_e_senha_propria_e_ok(tmp_path):
    s = _s(tmp_path)
    a = AuthService(s.auth_db_path, user=s.auth_user)
    a.seed_default()
    a.close()
    assert _por_nome(s)["login"].nivel == "erro"
    a = AuthService(s.auth_db_path, user=s.auth_user)
    a.set_password("uma senha longa e boa 123")
    a.close()
    assert _por_nome(s)["login"].nivel == "ok"


def test_nao_imprime_valor_de_chave(tmp_path):
    s = _s(tmp_path, gateway_url="http://127.0.0.1:20128/v1", gateway_api_key="SEGREDO-123456")
    texto, _ = relatorio(checar(s))
    assert "SEGREDO-123456" not in texto
    assert _por_nome(s)["modelos"].nivel == "ok"


def test_mcp_invalido_e_voz_sem_chave(tmp_path):
    s = _s(tmp_path, voice_enabled=True)
    s.data_dir.mkdir(parents=True)
    (s.data_dir / "mcp.json").write_text("{quebrado")
    itens = _por_nome(s)
    assert itens["MCP"].nivel == "erro"
    assert itens["voz (Groq)"].nivel == "aviso"
    (s.data_dir / "mcp.json").write_text(json.dumps({"servers": {"a": {}, "b": {}}}))
    assert "2 servidor" in _por_nome(s)["MCP"].detalhe


def test_cli_doctor_devolve_codigo(tmp_path, monkeypatch, capsys):
    monkeypatch.setenv("ORION_DATA_DIR", str(tmp_path / "d"))
    monkeypatch.chdir(tmp_path)
    assert main(["doctor"]) == 0
    assert "pasta de dados" in capsys.readouterr().out


def test_aviso_de_opt_in_aponta_a_secao_do_guia(tmp_path, monkeypatch):
    """Memória da tela ligada sem tesseract: o aviso diz onde o ORION_OPERACAO explica (§8)."""
    import shutil

    monkeypatch.setattr(shutil, "which", lambda nome: None)
    item = _por_nome(_s(tmp_path, screen_memory=True))["memória da tela"]
    assert item.nivel == "aviso" and "tesseract" in item.detalhe
    assert "(ver ORION_OPERACAO §8)" in item.detalhe
    pesquisa = _por_nome(_s(tmp_path, research_at="04:00"))["pesquisa noturna"]
    assert "§10" in pesquisa.detalhe
    semanal = _por_nome(_s(tmp_path, weekly_ai=True))["leitura semanal"]
    assert "§11" in semanal.detalhe
