"""Classificador de comandos de shell: leitura segura vs. precisa de confirmação.

Substitui a blocklist de substrings da antiga "Câmara de Eco" (que deixava passar
`ri -r -fo`, `rmdir /s /q`, `-enc`, `[IO.Directory]::Delete`, `irm | iex`...).
Inverte a lógica: só roda sem confirmação o que for provadamente leitura;
todo o resto pede confirmação. Erros caem sempre para o lado seguro.

Só stdlib. Cobre PowerShell (Windows) e shell POSIX (macOS/Linux).
"""

from __future__ import annotations

import re
from dataclasses import dataclass

# Qualquer um destes caracteres impede a prova de "só leitura": redirecionamento,
# substituição de comando, subexpressão, bloco de script, chamada de método,
# acesso estático (::), escape e variáveis.
_PROIBIDOS = re.compile(r"[<>`$(){}\[\]]|::")
_SEPARADORES = re.compile(r"\|\||&&|[;|&\n\r]")
# Leitura que vaza segredo para o LLM (regra 5: segredos não saem do cofre/arquivo).
_SEGREDOS = re.compile(
    r"\.env\b|credentials|token\.json|id_rsa|id_ed25519|\.ssh|\.aws|\.gnupg|\bsecrets?\b|keychain|\benv:",
    re.IGNORECASE,
)

_POSIX_LEITURA = frozenset(
    {
        "ls",
        "pwd",
        "cat",
        "head",
        "tail",
        "wc",
        "whoami",
        "hostname",
        "uname",
        "uptime",
        "df",
        "du",
        "free",
        "ps",
        "which",
        "echo",
        "id",
        "stat",
        "file",
        "tree",
        "grep",
        "ver",
        "cal",
    }
)
_PS_ALIASES_LEITURA = frozenset(
    {
        "gci",
        "gc",
        "gl",
        "gps",
        "gsv",
        "gm",
        "gcm",
        "gi",
        "gp",
        "gu",
        "dir",
        "type",
        "ls",
        "pwd",
        "cat",
        "echo",
        "where",
        "select",
        "measure",
        "ft",
        "fl",
        "fw",
        "write-output",
        "select-object",
        "where-object",
        "sort-object",
        "measure-object",
        "format-table",
        "format-list",
        "format-wide",
        "out-string",
        "convertto-json",
        "convertto-csv",
        "select-string",
        "test-path",
        "resolve-path",
        "split-path",
        "join-path",
    }
)
_GET_NEGADOS = frozenset({"get-credential", "get-secret", "get-secretinfo"})
_GIT_LEITURA = frozenset(
    {
        "status",
        "log",
        "diff",
        "show",
        "rev-parse",
        "ls-files",
        "blame",
        "describe",
        "shortlog",
        "remote",
        "tag",
        "branch",
    }
)
_GIT_FLAGS_PROIBIDAS = (
    "--output",
    "--exec",
    "--upload-pack",
    "--receive-pack",
    "--ext-diff",
    "--textconv",
    "-d",
    "-D",
    "-m",
    "-M",
    "-c",
    "-C",
    "--delete",
    "--move",
    "--copy",
    "--set-upstream",
)
_VERSAO = frozenset({"--version", "-v", "-version", "-V"})
_RUNTIMES = frozenset(
    {
        "python",
        "python3",
        "pip",
        "pip3",
        "node",
        "npm",
        "uv",
        "git",
        "cargo",
        "rustc",
        "java",
        "javac",
        "dotnet",
        "go",
    }
)


@dataclass(frozen=True)
class ShellVerdict:
    read_only: bool
    reason: str = ""


def _first_token(segmento: str) -> tuple[str, list[str]]:
    partes = segmento.strip().split()
    if not partes:
        return "", []
    cmd = partes[0].strip("\"'").lower()
    if cmd.endswith(".exe"):
        cmd = cmd[:-4]
    return cmd, [p.strip("\"'") for p in partes[1:]]


def _segmento_leitura(cmd: str, args: list[str]) -> str | None:
    """None se o segmento é leitura; senão o motivo."""
    if not cmd:
        return "comando vazio"
    if cmd in _GET_NEGADOS:
        return f"'{cmd}' expõe segredos"
    if cmd.startswith("get-") or cmd in _PS_ALIASES_LEITURA or cmd in _POSIX_LEITURA:
        return None
    if cmd == "git":
        if not args or args[0].startswith("-"):
            return "git sem subcomando de leitura explícito"
        sub = args[0].lower()
        if sub not in _GIT_LEITURA:
            return f"git {sub} pode alterar o repositório"
        resto = args[1:]
        if sub in ("branch", "tag", "remote") and any(not a.startswith("-") for a in resto):
            return f"git {sub} com argumento pode criar/alterar"
        for a in resto:
            if a.startswith(_GIT_FLAGS_PROIBIDAS):
                return f"git {sub} {a} pode escrever/executar"
        return None
    if cmd in _RUNTIMES and len(args) == 1 and args[0] in _VERSAO:
        return None
    return f"'{cmd}' fora da lista de leitura segura"


def classify_command(cmd: str) -> ShellVerdict:
    """Decide se `cmd` roda sem confirmação (leitura provada) ou não."""
    texto = (cmd or "").strip()
    if not texto:
        return ShellVerdict(False, "comando vazio")
    if _SEGREDOS.search(texto):
        return ShellVerdict(
            False, "toca em arquivo/variável de segredo (.env, credenciais, chaves)"
        )
    if _PROIBIDOS.search(texto):
        return ShellVerdict(
            False,
            "contém redirecionamento, substituição, bloco de script, chamada de método "
            "ou escape: não dá para provar que é só leitura",
        )
    for segmento in _SEPARADORES.split(texto):
        if not segmento.strip():
            continue
        cmd_nome, args = _first_token(segmento)
        motivo = _segmento_leitura(cmd_nome, args)
        if motivo:
            return ShellVerdict(False, motivo)
    return ShellVerdict(True)
