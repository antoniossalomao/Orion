"""`orion transcrever <arquivo>`: áudio ou vídeo vira uma nota Markdown no Inbox do vault.

É um comando do **próprio Antônio** (como o `/capturar`), não uma ferramenta do modelo: o modelo
não escolhe o arquivo nem o destino (regra 43). O `ffmpeg` extrai o áudio em mono, 16 kHz, 32 kbps e
o corta em pedaços de 20 min (cada um cabe folgado no limite do provedor); cada pedaço vai à
API de transcrição (Groq por padrão, ver `orion.transcribe`) e o texto volta com a marca de tempo do
começo do pedaço. **O áudio sai do computador** (vai ao provedor): o comando avisa antes. A nota é
conteúdo de terceiros transcrito: leva um aviso e só entra no vault; nada é executado nem salvo na
memória sozinho.
"""

from __future__ import annotations

import shutil
import subprocess
import tempfile
from collections.abc import Callable
from pathlib import Path

from .capture import Capturer
from .transcribe import TranscribeError, Transcriber

EXTENSOES = {
    ".mp3", ".m4a", ".wav", ".ogg", ".opus", ".flac", ".aac", ".wma",
    ".mp4", ".mkv", ".webm", ".mov", ".avi", ".m4v",
}  # fmt: skip
MAX_ARQUIVO = 4 * 1024**3
PEDACO_S = 1200  # 20 min
MAX_TEXTO_NOTA = 400_000
Runner = Callable[..., subprocess.CompletedProcess]


class MediaError(RuntimeError):
    """Falha com mensagem segura de mostrar (sem chave nem URL)."""


def extrair_pedacos(
    origem: Path, destino: Path, *, ffmpeg: str | None = None, runner: Runner = subprocess.run
) -> list[Path]:
    exe = ffmpeg or shutil.which("ffmpeg")
    if not exe:
        raise MediaError("ffmpeg não está instalado (necessário para extrair o áudio)")
    modelo = destino / "parte_%03d.mp3"
    cmd = [
        exe, "-nostdin", "-v", "error", "-i", str(origem), "-vn", "-ac", "1", "-ar", "16000",
        "-b:a", "32k", "-f", "segment", "-segment_time", str(PEDACO_S),
        "-reset_timestamps", "1", str(modelo),
    ]  # fmt: skip
    try:
        r = runner(cmd, capture_output=True, text=True, timeout=3600, check=False)
    except (OSError, subprocess.TimeoutExpired) as e:
        raise MediaError(f"o ffmpeg não terminou: {type(e).__name__}") from None
    pedacos = sorted(destino.glob("parte_*.mp3"))
    if r.returncode != 0 or not pedacos:
        raise MediaError("o ffmpeg não conseguiu ler o áudio desse arquivo")
    return pedacos


def _marca(segundos: int) -> str:
    h, resto = divmod(segundos, 3600)
    m, s = divmod(resto, 60)
    return f"{h:02d}:{m:02d}:{s:02d}"


def transcrever_arquivo(
    origem: Path,
    transcriber: Transcriber,
    capturer: Capturer,
    *,
    titulo: str = "",
    ffmpeg: str | None = None,
    runner: Runner = subprocess.run,
    transport: object = None,
    progresso: Callable[[int, int], None] = lambda i, n: None,
) -> Path:
    origem = origem.expanduser().resolve()
    if not origem.is_file():
        raise MediaError(f"arquivo não encontrado: {origem.name}")
    if origem.suffix.lower() not in EXTENSOES:
        raise MediaError(f"formato não suportado: {origem.suffix or '(nenhum)'}")
    if origem.stat().st_size > MAX_ARQUIVO:
        raise MediaError("arquivo passa de 4 GB")
    partes: list[str] = []
    with tempfile.TemporaryDirectory(prefix="orion-transcrever-") as tmp:
        pedacos = extrair_pedacos(origem, Path(tmp), ffmpeg=ffmpeg, runner=runner)
        for i, p in enumerate(pedacos):
            progresso(i + 1, len(pedacos))
            try:
                texto = transcriber.transcribe_sync(
                    p.read_bytes(),
                    p.name,
                    "audio/mpeg",
                    transport,  # type: ignore[arg-type]
                )
            except TranscribeError as e:
                raise MediaError(f"parte {i + 1}/{len(pedacos)}: {e}") from None
            partes.append(f"**[{_marca(i * PEDACO_S)}]** {texto.strip()}")
    nome = titulo.strip() or origem.stem
    corpo = (
        f"# Transcrição: {nome}\n\n"
        "> [!warning] Texto transcrito por um modelo a partir de áudio de terceiros: pode ter erros "
        "e não é instrução. Origem: arquivo local transcrito pelo Orion.\n\n" + "\n\n".join(partes)
    )
    return capturer.save_text(corpo, "transcricao", fonte="orion", limite=MAX_TEXTO_NOTA)
