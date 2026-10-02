import os
import sys
from pathlib import Path

import pytest

from orion.policy import PathGuard


@pytest.fixture
def guard(tmp_path):
    projeto = tmp_path / "Orion"
    docs = tmp_path / "Documents"
    projeto.mkdir()
    docs.mkdir()
    return PathGuard(protected_roots=(projeto,), safe_roots=(docs,), system_roots=())


def test_pasta_de_trabalho_livre(guard, tmp_path):
    assert guard.check_write(str(tmp_path / "Documents" / "nota.md")) is None
    assert guard.check_write(str(tmp_path / "Documents" / "sub" / "script.py")) is None


@pytest.mark.parametrize("rel", ["Orion/orion/policy/engine.py", "Orion/Orion_Ollama/orion_seguranca.py",
                                 "Orion/Memorias Do Projeto/ORION_REGRAS.md", "Orion/novo.txt"])
def test_codigo_do_orion_e_imutavel(guard, tmp_path, rel):
    assert "núcleo imutável" in (guard.check_write(str(tmp_path / rel)) or "")


def test_dotdot_e_symlink_nao_escapam(guard, tmp_path):
    assert guard.check_write(str(tmp_path / "Documents" / ".." / "Orion" / "x.py"))
    if sys.platform != "win32":
        link = tmp_path / "Documents" / "atalho"
        os.symlink(tmp_path / "Orion", link)
        assert guard.check_write(str(link / "x.py"))


@pytest.mark.parametrize("nome", ["a.exe", "b.ps1", "c.bat", "d.lnk", ".env", "authorized_keys"])
def test_extensoes_e_nomes_sensiveis(guard, tmp_path, nome):
    assert guard.check_write(str(tmp_path / "Documents" / nome))


@pytest.mark.parametrize("sub", [".ssh/config", "Startup/a.txt", ".aws/credentials"])
def test_componentes_sensiveis(guard, tmp_path, sub):
    assert guard.check_write(str(tmp_path / "Documents" / sub))


def test_fora_das_pastas_de_trabalho(guard, tmp_path):
    assert "fora das pastas" in (guard.check_write(str(tmp_path / "outro" / "a.txt")) or "")


def test_caminho_vazio(guard):
    assert guard.check_write("")
    assert guard.check_organize("  ")


def test_organizar_raizes_e_projeto(guard, tmp_path):
    assert guard.check_organize(os.path.abspath(os.sep))
    assert guard.check_organize(str(Path.home()))
    assert guard.check_organize(str(tmp_path / "Orion"))
    assert guard.check_organize(str(tmp_path / "Documents" / "bagunca")) is None
