"""`orion-desktop` v0: o mínimo para agir no computador, sempre atrás da política.

Três ferramentas, com os nomes e as classes de risco do legado: `executar_comando`
(execução: só leitura provada roda direto, o resto pede aprovação), `ler_arquivo` e
`listar_arquivos` (leitura; segredo e chave pedem confirmação, via `PathGuard.check_read`).
**Desligado por padrão**: só entra no registro com `ORION_DESKTOP_TOOLS=true`.

O shell roda sem herdar `ORION_*` nem variável com nome de segredo (regra 5), sem entrada
padrão, com tempo-limite e saída truncada. No Windows usa PowerShell; no macOS/Linux, `sh`.
O caminho do Windows só tem o argv testado aqui (o CI de Windows roda esses testes, mas
nenhum comando real: ver ORION_MELHORIAS).
"""

from __future__ import annotations

import os
import re
import shutil
import subprocess
import sys
from collections.abc import Callable, Mapping
from pathlib import Path
from typing import Any

from .registry import Tool

_NOME_DE_SEGREDO = re.compile(r"(KEY|TOKEN|SECRET|PASSWORD|PASSWD|CREDENTIAL)", re.IGNORECASE)
MAX_LEITURA = 1_000_000  # bytes lidos de um arquivo antes de decodificar


def env_limpo(base: Mapping[str, str] | None = None) -> dict[str, str]:
    """Ambiente do filho: sem `ORION_*` e sem nada que pareça chave, token ou senha."""
    origem = os.environ if base is None else base
    return {
        k: v
        for k, v in origem.items()
        if not k.upper().startswith("ORION_") and not _NOME_DE_SEGREDO.search(k)
    }


def argv_do_shell(cmd: str, *, windows: bool) -> list[str] | None:
    """Linha de comando para rodar `cmd` no shell do sistema; None se não há shell."""
    if windows:
        exe = shutil.which("pwsh") or shutil.which("powershell")
        return [exe, "-NoProfile", "-NonInteractive", "-Command", cmd] if exe else None
    exe = shutil.which("sh")
    return [exe, "-c", cmd] if exe else None


def _cortar(texto: str, limite: int) -> str:
    return texto if len(texto) <= limite else texto[:limite] + "\n…[truncado]"


def desktop_tools(
    *,
    home: Path | None = None,
    timeout_s: float = 60.0,
    max_chars: int = 8000,
    windows: bool = sys.platform == "win32",
    runner: Callable[..., Any] = subprocess.run,
    env: Mapping[str, str] | None = None,
) -> list[Tool]:
    base = home or Path.home()

    def executar_comando(cmd: str, pasta: str = "") -> dict[str, Any]:
        cwd = Path(pasta).expanduser() if pasta.strip() else base
        if not cwd.is_dir():
            return {"erro": f"pasta de trabalho inexistente: {cwd}"}
        argv = argv_do_shell(cmd, windows=windows)
        if argv is None:
            return {"erro": "nenhum shell encontrado (PowerShell no Windows, sh no macOS/Linux)"}
        try:
            # argv fixo (shell + cmd); a política já decidiu se `cmd` roda sem confirmação (regra 6)
            r = runner(
                argv,
                cwd=cwd,
                env=env_limpo(env),
                stdin=subprocess.DEVNULL,
                capture_output=True,
                timeout=timeout_s,
                check=False,
            )
        except subprocess.TimeoutExpired:
            return {"erro": f"tempo esgotado ({timeout_s:.0f}s); o comando foi encerrado"}
        except OSError as e:
            return {"erro": f"não foi possível executar: {type(e).__name__}"}
        return {
            "codigo": r.returncode,
            "saida": _cortar(r.stdout.decode("utf-8", errors="replace"), max_chars),
            "erro": _cortar(r.stderr.decode("utf-8", errors="replace"), max_chars // 2),
        }

    def ler_arquivo(path: str, max_chars_lidos: int = 0) -> dict[str, Any]:
        alvo = Path(path).expanduser()
        if not alvo.is_file():
            return {"erro": f"não é um arquivo: {alvo}"}
        limite = max(1, min(int(max_chars_lidos or max_chars), max_chars))
        with alvo.open("rb") as f:
            bruto = f.read(MAX_LEITURA)
        if b"\x00" in bruto[:4096]:
            return {"erro": "arquivo binário: use uma ferramenta de documento"}
        texto = bruto.decode("utf-8", errors="replace")
        return {
            "path": str(alvo),
            "conteudo": _cortar(texto, limite),
            "truncado": len(texto) > limite or alvo.stat().st_size > MAX_LEITURA,
        }

    def listar_arquivos(path: str, limite: int = 200) -> dict[str, Any]:
        alvo = Path(path).expanduser()
        if not alvo.is_dir():
            return {"erro": f"não é uma pasta: {alvo}"}
        teto = max(1, min(int(limite), 500))
        itens: list[dict[str, Any]] = []
        for e in sorted(alvo.iterdir(), key=lambda x: (not x.is_dir(), x.name.lower())):
            try:
                tamanho = None if e.is_dir() else e.stat().st_size
            except OSError:
                tamanho = None
            itens.append(
                {"nome": e.name, "tipo": "pasta" if e.is_dir() else "arquivo", "bytes": tamanho}
            )
            if len(itens) >= teto:
                break
        return {"path": str(alvo), "itens": itens, "truncado": len(itens) >= teto}

    obj = "object"
    return [
        Tool(
            "executar_comando",
            "Executa um comando no shell do computador (PowerShell no Windows, sh no macOS/Linux). "
            "Só leitura provada roda direto; qualquer outro comando pede aprovação do Antônio.",
            {
                "type": obj,
                "properties": {
                    "cmd": {"type": "string"},
                    "pasta": {"type": "string", "description": "pasta de trabalho (padrão: home)"},
                },
                "required": ["cmd"],
            },
            executar_comando,
        ),
        Tool(
            "ler_arquivo",
            "Lê um arquivo de texto (até alguns milhares de caracteres). Segredos e chaves "
            "(.env, .ssh, *.pem...) pedem confirmação do Antônio.",
            {
                "type": obj,
                "properties": {
                    "path": {"type": "string"},
                    "max_chars_lidos": {"type": "integer"},
                },
                "required": ["path"],
            },
            ler_arquivo,
        ),
        Tool(
            "listar_arquivos",
            "Lista os arquivos e pastas de uma pasta (nome, tipo, tamanho).",
            {
                "type": obj,
                "properties": {"path": {"type": "string"}, "limite": {"type": "integer"}},
                "required": ["path"],
            },
            listar_arquivos,
        ),
    ]
