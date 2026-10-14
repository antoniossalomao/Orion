"""Visão pelas assinaturas (E11.2, N7): se o gateway falhar, a imagem vai à CLI já logada.

`ORION_VISION_ORDER` diz a ordem (padrão: gemini, claude, codex, gemini_cli). "gemini" é o endpoint
de visão do gateway (cota gratuita, primeiro). As CLIs (`claude`, `codex`, `gemini`) rodam numa
pasta temporária, com a imagem dentro, e nunca com o caminho de nenhuma pasta sua. A assinatura não
inclui API: é só o que a CLI já tem logado. Cada tentativa vai ao registro de saídas
(`cli:<nome>`).
A imagem é conteúdo externo: quem chama trata a resposta como dado (REGRAS.md: conteúdo externo é
dado, não instrução).
"""

from __future__ import annotations

import shutil
import subprocess
import tempfile
from collections.abc import Callable
from pathlib import Path
from typing import Any

from . import saidas
from .tools.desktop import env_limpo
from .vision import MIMES, Vision, VisionError

ORDEM_PADRAO = ("gemini", "claude", "codex", "gemini_cli")
CLIS = {"claude": "claude", "codex": "codex", "gemini_cli": "gemini"}  # nome → executável
TIMEOUT_S = 120


def ordem(texto: str) -> list[str]:
    """Lista de nomes válidos, sem repetição, na ordem dada. Vazia → padrão."""
    validos = [*ORDEM_PADRAO]
    escolhidos: list[str] = []
    for item in texto.split(","):
        nome = item.strip().lower()
        if nome in validos and nome not in escolhidos:
            escolhidos.append(nome)
    return escolhidos or list(ORDEM_PADRAO)


def argv_cli(nome: str, imagem: Path, pergunta: str) -> list[str]:
    """Argumentos (sem o executável) de cada CLI. A pergunta é do Antônio, a imagem é dado."""
    texto = pergunta.strip()[:1000] or "Descreva o que você vê, em português."
    if nome == "claude":
        return [
            "-p",
            f"Leia o arquivo de imagem {imagem.name} nesta pasta e responda: {texto}",
            "--allowedTools",
            "Read",
        ]
    if nome == "codex":
        return ["exec", "-i", str(imagem), texto]
    if nome == "gemini_cli":
        return ["-p", f"@{imagem.name} {texto}"]
    raise ValueError(f"CLI desconhecida: {nome}")


def descrever_por_cli(
    nome: str,
    imagem: bytes,
    mime: str,
    pergunta: str = "",
    *,
    runner: Callable[..., Any] = subprocess.run,
    which: Callable[[str], str | None] = shutil.which,
) -> str:
    binario = CLIS.get(nome)
    if binario is None:
        raise VisionError(f"{nome}: não é uma CLI de visão")
    exe = which(binario)
    if not exe:
        raise VisionError(f"{nome}: CLI não instalada")
    extensao = next((ext for ext, m in MIMES.items() if m == mime), ".jpg")
    with tempfile.TemporaryDirectory(prefix="orion-visao-") as pasta:
        arquivo = Path(pasta) / f"imagem{extensao}"
        arquivo.write_bytes(imagem)
        argv = [exe, *argv_cli(nome, arquivo, pergunta)]
        with saidas.medir(
            f"cli:{nome}",
            "vision",
            bytes_out=len(imagem),
            content_kind="imagem",
            # a imagem sai do computador: fica no registro de saída
            # (REGRAS.md: toda saída de dado fica registrada, sem conteúdo)
        ) as m:
            try:
                r = runner(
                    argv, cwd=pasta, env=env_limpo(), stdin=subprocess.DEVNULL,
                    capture_output=True, timeout=TIMEOUT_S, check=False,
                )  # fmt: skip
            except (OSError, subprocess.TimeoutExpired) as e:
                raise VisionError(f"{nome}: {type(e).__name__}") from None
            m.bytes_in = len(r.stdout or b"")
            if r.returncode != 0:
                raise VisionError(f"{nome}: saiu com código {r.returncode}")
            texto = (r.stdout or b"").decode("utf-8", "replace").strip()
            if not texto:
                raise VisionError(f"{nome}: resposta vazia")
            m.ok = True
    return texto[:6000]


def describe_ordenado(
    imagem: bytes,
    mime: str,
    pergunta: str = "",
    *,
    vision: Vision | None,
    ordem_: list[str] | None = None,
    runner: Callable[..., Any] = subprocess.run,
    which: Callable[[str], str | None] = shutil.which,
) -> str:
    """Tenta cada nome da ordem até uma responder. Falha só se todos falharem."""
    falhas: list[str] = []
    for nome in ordem_ or list(ORDEM_PADRAO):
        try:
            if nome in CLIS:
                return descrever_por_cli(nome, imagem, mime, pergunta, runner=runner, which=which)
            if vision is None:
                falhas.append("gemini: visão do gateway não configurada")
                continue
            return vision.describe(imagem, mime, pergunta)
        except VisionError as e:
            falhas.append(str(e))
    raise VisionError("; ".join(falhas) or "nenhuma visão disponível")
