"""Arquivos do `orion-desktop`: `escrever_arquivo` e `organizar_pasta` (fase 4).

Mantêm nomes e argumentos do legado. A política cuida do onde: escrever fora das pastas
seguras, em raiz de projeto ou em extensão sensível pede confirmação (`PathGuard.check_write`);
reorganizar raiz de drive, a pasta pessoal inteira ou o código do Orion também
(`check_organize`). Aqui só ficam os limites que a ferramenta impõe a si mesma.
"""

from __future__ import annotations

import shutil
import time
from datetime import datetime
from pathlib import Path
from typing import Any

from .registry import Tool

MAX_ESCRITA = 5 * 1024 * 1024  # bytes por chamada
_TIPOS = {
    ".pdf": "Documentos", ".doc": "Documentos", ".docx": "Documentos", ".odt": "Documentos",
    ".xls": "Planilhas", ".xlsx": "Planilhas", ".ods": "Planilhas",
    ".ppt": "Apresentacoes", ".pptx": "Apresentacoes",
    ".jpg": "Imagens", ".jpeg": "Imagens", ".png": "Imagens", ".gif": "Imagens",
    ".bmp": "Imagens", ".webp": "Imagens", ".svg": "Imagens", ".heic": "Imagens",
    ".mp4": "Videos", ".mkv": "Videos", ".avi": "Videos", ".mov": "Videos",
    ".mp3": "Audio", ".wav": "Audio", ".flac": "Audio", ".m4a": "Audio",
    ".py": "Codigo", ".js": "Codigo", ".ts": "Codigo", ".rs": "Codigo", ".go": "Codigo",
    ".java": "Codigo", ".cpp": "Codigo", ".c": "Codigo", ".cs": "Codigo",
    ".zip": "Arquivados", ".rar": "Arquivados", ".7z": "Arquivados", ".tar": "Arquivados",
    ".gz": "Arquivados",
    ".txt": "Texto", ".md": "Texto",
    ".csv": "Dados", ".json": "Dados", ".xml": "Dados",
}  # fmt: skip
CRITERIOS = ("tipo", "data", "tamanho")


def _destino(arquivo: Path, criterio: str) -> str:
    if criterio == "tipo":
        return _TIPOS.get(arquivo.suffix.lower(), "Outros")
    if criterio == "data":
        return datetime.fromtimestamp(arquivo.stat().st_mtime).strftime("%Y-%m")
    tamanho = arquivo.stat().st_size
    if tamanho < 1024 * 1024:
        return "Pequenos_menos1MB"
    return "Medios_1a100MB" if tamanho < 100 * 1024 * 1024 else "Grandes_mais100MB"


def fs_tools() -> list[Tool]:
    def escrever_arquivo(path: str, conteudo: str, modo: str = "w") -> dict[str, Any]:
        if modo not in ("w", "a"):
            return {"erro": "modo inválido: use 'w' (sobrescrever) ou 'a' (acrescentar)"}
        dados = conteudo.encode("utf-8")
        if len(dados) > MAX_ESCRITA:
            return {"erro": f"conteúdo de {len(dados)} bytes passa do limite de {MAX_ESCRITA}"}
        alvo = Path(path).expanduser()
        if alvo.is_dir():
            return {"erro": f"é uma pasta, não um arquivo: {alvo}"}
        alvo.parent.mkdir(parents=True, exist_ok=True)
        existia = alvo.exists()
        with alvo.open("ab" if modo == "a" else "wb") as f:
            f.write(dados)
        return {
            "ok": True,
            "path": str(alvo),
            "bytes_escritos": len(dados),
            "substituiu": existia and modo == "w",
        }

    def organizar_pasta(path: str, criterio: str = "tipo", simular: bool = False) -> dict[str, Any]:
        if criterio not in CRITERIOS:
            return {"erro": f"critério inválido: {criterio}. Use: {', '.join(CRITERIOS)}"}
        pasta = Path(path).expanduser()
        if not pasta.is_dir():
            return {"erro": f"não é uma pasta: {pasta}"}
        movidos: list[dict[str, str]] = []
        for arquivo in sorted(pasta.iterdir()):
            if not arquivo.is_file() or arquivo.is_symlink() or arquivo.name.startswith("."):
                continue  # só arquivos comuns visíveis; link e oculto ficam onde estão
            sub = _destino(arquivo, criterio)
            destino = pasta / sub / arquivo.name
            if destino.exists():
                destino = destino.with_name(f"{arquivo.stem}_{int(time.time())}{arquivo.suffix}")
            if not simular:
                destino.parent.mkdir(exist_ok=True)
                shutil.move(str(arquivo), str(destino))
            movidos.append({"de": arquivo.name, "para": f"{sub}/{destino.name}"})
        return {
            "ok": True,
            "criterio": criterio,
            "simulado": simular,
            "movidos": len(movidos),
            "detalhes": movidos,
        }

    obj = "object"
    return [
        Tool(
            "escrever_arquivo",
            "Grava texto num arquivo ('w' sobrescreve, 'a' acrescenta). Fora das pastas seguras "
            "(Documents, Downloads, Desktop) ou em arquivo sensível, pede confirmação do Antônio.",
            {
                "type": obj,
                "properties": {
                    "path": {"type": "string"},
                    "conteudo": {"type": "string"},
                    "modo": {"type": "string", "enum": ["w", "a"]},
                },
                "required": ["path", "conteudo"],
            },
            escrever_arquivo,
        ),
        Tool(
            "organizar_pasta",
            "Move os arquivos de uma pasta para subpastas por tipo, data ou tamanho. Use "
            "simular=true para ver o que seria movido sem mexer em nada.",
            {
                "type": obj,
                "properties": {
                    "path": {"type": "string"},
                    "criterio": {"type": "string", "enum": list(CRITERIOS)},
                    "simular": {"type": "boolean"},
                },
                "required": ["path"],
            },
            organizar_pasta,
        ),
    ]
