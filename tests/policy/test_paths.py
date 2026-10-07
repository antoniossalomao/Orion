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


@pytest.mark.parametrize(
    "rel",
    [
        "Orion/orion/policy/engine.py",
        "Orion/Orion_Ollama/orion_seguranca.py",
        "Orion/Memorias Do Projeto/ORION_REGRAS.md",
        "Orion/novo.txt",
    ],
)
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


# ── leitura: segredo e chave pedem confirmação ────────────────────────────
@pytest.mark.parametrize(
    "rel",
    [
        ".env",
        "Orion/.env",
        "Orion/.env.local",
        ".env.production",
        ".ssh/id_rsa",
        "casa/.ssh/qualquer.txt",
        ".aws/credentials",
        ".gnupg/pubring.kbx",
        ".kube/config",
        "Orion/Orion_Core/google_auth/token.json",
        "Orion/Orion_Core/google_auth/qualquer.json",
        "chaves/servidor.pem",
        "chaves/SERVIDOR.KEY",
        "cofre.kdbx",
        "projeto/credentials.json",
        "id_ed25519",
        ".netrc",
        ".git-credentials",
        "secrets.json",
        "dados/auth.db",  # hash da senha do login
        "dados/auth.db-wal",
        "dados/mcp.json",  # programas que o Orion sobe sozinho
    ],
)
def test_ler_segredo_ou_chave_pede_confirmacao(guard, tmp_path, rel):
    assert guard.check_read(str(tmp_path / rel)), rel


@pytest.mark.parametrize(
    "rel",
    ["Documents/nota.md", "Orion/README.md", "Orion/orion/policy/engine.py", ".env.example",
     "Orion/.env.sample", "Orion/.env.template", "pasta/environment.md", "projeto/chave-de-acesso.txt",
     "Orion/mcp.example.json"],
)  # fmt: skip
def test_ler_arquivo_comum_nao_pede_confirmacao(guard, tmp_path, rel):
    assert guard.check_read(str(tmp_path / rel)) is None, rel


def test_leitura_vazia_pede_confirmacao_e_symlink_para_segredo_nao_escapa(guard, tmp_path):
    assert guard.check_read("") and guard.check_read("   ")
    if sys.platform != "win32":
        (tmp_path / "Documents" / ".env").write_text("X=1", encoding="utf-8")
        link = tmp_path / "Documents" / "inocente.txt"
        os.symlink(tmp_path / "Documents" / ".env", link)
        assert guard.check_read(str(link))  # o destino é .env: resolve antes de comparar


def test_escrever_mcp_json_ou_auth_db_fora_das_pastas_ate_nas_seguras_pede_confirmacao(
    guard, tmp_path
):
    seguro = tmp_path / "Documents"
    assert guard.check_write(str(seguro / "nota.txt")) is None
    for nome in ("mcp.json", "auth.db", ".mcp.json"):
        assert guard.check_write(str(seguro / nome)), (
            nome
        )  # nem na pasta segura: muda o que o Orion executa
