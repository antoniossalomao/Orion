"""PathGuard: onde as ferramentas podem escrever sem confirmação.

Regra 3: código, persona e política ficam FORA da área gravável das ferramentas.
Escrever dentro de `protected_roots` (a raiz do projeto) sempre pede confirmação,
o que também cobre a auto-modificação (Ring 0 #5). Caminhos são resolvidos
(`realpath`: segue symlinks e `..`) antes de comparar.
"""

from __future__ import annotations

import os
import sys
from dataclasses import dataclass, field
from pathlib import Path

_EXT_SENSIVEIS = frozenset(
    {
        ".exe",
        ".dll",
        ".ps1",
        ".psm1",
        ".bat",
        ".cmd",
        ".msi",
        ".sys",
        ".reg",
        ".vbs",
        ".lnk",
        ".scr",
        ".app",
        ".command",
        ".plist",
        ".desktop",
    }
)
_NOMES_SENSIVEIS = frozenset(
    {
        ".env",
        ".bashrc",
        ".zshrc",
        ".profile",
        ".bash_profile",
        "profile.ps1",
        "authorized_keys",
        "known_hosts",
        "credentials.json",
        "token.json",
    }
)
_COMPONENTES_SENSIVEIS = frozenset(
    {".ssh", ".aws", ".gnupg", "startup", "launchagents", "launchdaemons"}
)
# Leitura que vaza segredo para o LLM (regra 5): ler isto pede confirmação do Antônio.
_NOMES_LEITURA_SENSIVEL = _NOMES_SENSIVEIS | frozenset(
    {
        "id_rsa",
        "id_ed25519",
        "id_ecdsa",
        "id_dsa",
        ".netrc",
        ".npmrc",
        ".pgpass",
        ".git-credentials",
        ".pypirc",
        "secrets.json",
        "secret.json",
    }
)
_EXT_LEITURA_SENSIVEL = frozenset(
    {".pem", ".key", ".pfx", ".p12", ".kdbx", ".keystore", ".jks", ".ppk"}
)
_COMPONENTES_LEITURA_SENSIVEL = frozenset(
    {".ssh", ".aws", ".gnupg", ".kube", ".docker", "google_auth"}
)
_MODELOS_DE_ENV = (".example", ".sample", ".template")


def _norm(p: str | os.PathLike[str]) -> str:
    return os.path.normcase(os.path.realpath(os.path.expanduser(str(p))))


def _dentro(caminho: str, raiz: str) -> bool:
    if not raiz:
        return False
    raiz = raiz.rstrip("\\/") + os.sep
    return (caminho + os.sep).startswith(raiz)


def _system_roots() -> tuple[str, ...]:
    if sys.platform == "win32":
        win = os.environ.get("SystemRoot", r"C:\Windows")
        return tuple(
            _norm(p)
            for p in (
                win,
                os.environ.get("ProgramFiles", r"C:\Program Files"),
                os.environ.get("ProgramFiles(x86)", r"C:\Program Files (x86)"),
                os.environ.get("ProgramData", r"C:\ProgramData"),
            )
        )
    return tuple(
        _norm(p)
        for p in (
            "/bin",
            "/sbin",
            "/usr",
            "/etc",
            "/System",
            "/Library",
            "/private/etc",
            "/boot",
            "/lib",
        )
    )


def default_safe_roots() -> tuple[Path, ...]:
    home = Path.home()
    return tuple(home / d for d in ("Documents", "Downloads", "Desktop"))


@dataclass(frozen=True)
class PathGuard:
    protected_roots: tuple[Path, ...] = ()
    safe_roots: tuple[Path, ...] = field(default_factory=default_safe_roots)
    system_roots: tuple[str, ...] = field(default_factory=_system_roots)

    def check_write(self, path: str) -> str | None:
        """None se pode escrever sem confirmação; senão o motivo da confirmação."""
        if not path or not str(path).strip():
            return "caminho vazio"
        alvo = _norm(path)
        for raiz in self.protected_roots:
            if _dentro(alvo, _norm(raiz)):
                return (
                    "escrita dentro do código/persona/política do Orion "
                    "(núcleo imutável; auto-modificação exige aprovação)"
                )
        for raiz in self.system_roots:
            if _dentro(alvo, raiz):
                return "escrita em diretório de sistema"
        ext = os.path.splitext(alvo)[1].lower()
        if ext in _EXT_SENSIVEIS:
            return f"extensão sensível ({ext})"
        partes = {p.lower() for p in alvo.replace("\\", "/").split("/") if p}
        if os.path.basename(alvo).lower() in _NOMES_SENSIVEIS or partes & _COMPONENTES_SENSIVEIS:
            return "arquivo/pasta de segredo, configuração de shell ou autostart"
        if not any(_dentro(alvo, _norm(r)) for r in self.safe_roots):
            return "fora das pastas de trabalho (Documents/Downloads/Desktop)"
        return None

    def check_read(self, path: str) -> str | None:
        """None se pode ler sem confirmação; senão o motivo (arquivo/pasta de segredo ou chave).

        O caminho é resolvido antes (symlink para `.env` continua sendo `.env`)."""
        if not path or not str(path).strip():
            return "caminho vazio"
        alvo = _norm(path)
        nome = os.path.basename(alvo).lower()
        partes = {p.lower() for p in alvo.replace("\\", "/").split("/") if p}
        env = nome == ".env" or (nome.startswith(".env.") and not nome.endswith(_MODELOS_DE_ENV))
        if (
            env
            or nome in _NOMES_LEITURA_SENSIVEL
            or os.path.splitext(nome)[1] in _EXT_LEITURA_SENSIVEL
            or partes & _COMPONENTES_LEITURA_SENSIVEL
        ):
            return "arquivo/pasta de segredo ou chave: ler expõe o conteúdo ao modelo"
        return None

    def check_organize(self, path: str) -> str | None:
        """Reorganizar mexe em tudo dentro da pasta: raízes e sistema pedem confirmação."""
        if not path or not str(path).strip():
            return "caminho vazio"
        alvo = _norm(path)
        if os.path.dirname(alvo) == alvo:
            return "alvo é a raiz de um drive/sistema de arquivos"
        if alvo == _norm(Path.home()) or alvo == _norm(Path.home().parent):
            return "alvo é a pasta pessoal inteira"
        for raiz in self.system_roots:
            if _dentro(alvo, raiz):
                return "alvo é um diretório crítico do sistema"
        for raiz in self.protected_roots:
            if _dentro(alvo, _norm(raiz)):
                return "alvo é o código do Orion"
        return None
