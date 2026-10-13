"""E9.9 — Conversor de arquivos, tudo no computador.

`orion converter <arquivo> <formato>` e o Telegram ("converte para PDF", com o arquivo anexo).
Áudio e vídeo: `ffmpeg`. Imagem: Pillow (e `img2pdf` para PDF, se instalado). Word, planilha,
slides e texto ↔ PDF: LibreOffice (`soffice`) sem janela. Comprimir PDF: `pikepdf`. O que faltar
é dito com o nome do programa a instalar; nada é enviado para fora."""

import contextlib
import shutil
import subprocess
import tempfile
from collections.abc import Callable
from pathlib import Path
from typing import Any

MAX_ENTRADA = 200 * 1024 * 1024
TIMEOUT_S = 600

AUDIO = {"mp3", "wav", "m4a", "ogg", "flac", "aac", "opus", "wma"}
VIDEO = {"mp4", "mkv", "webm", "mov", "avi", "flv", "m4v"}
IMAGEM = {"png", "jpg", "jpeg", "webp", "bmp", "gif", "tif", "tiff"}
ESCRITORIO = {"doc", "docx", "odt", "rtf", "txt", "md", "xls", "xlsx", "ods", "ppt", "pptx", "odp"}
SAIDA_ESCRITORIO = {"pdf", "docx", "odt", "txt", "html", "xlsx", "csv", "pptx"}
SAIDA_IMAGEM = {"png", "jpg", "webp", "bmp", "gif", "tiff", "pdf"}
SAIDA_AUDIO = {"mp3", "wav", "m4a", "ogg", "flac", "aac", "opus"}
SAIDA_VIDEO = {"mp4", "mkv", "webm", "mov", "avi", "gif", *SAIDA_AUDIO}  # vídeo → só o áudio
_SINONIMOS = {"jpeg": "jpg", "tif": "tiff", "word": "docx", "excel": "xlsx", "texto": "txt"}

Runner = Callable[..., Any]


class ConversaoError(RuntimeError):
    pass


def formato_normal(formato: str) -> str:
    f = formato.strip().lower().lstrip(".")
    return _SINONIMOS.get(f, f)


def formatos_para(entrada: str) -> list[str]:
    """Formatos de saída possíveis para a extensão de entrada (vazio: não sei converter)."""
    e = formato_normal(entrada)
    if e == "pdf":
        return ["comprimir", "docx", "txt"]
    if e in IMAGEM:
        return sorted(SAIDA_IMAGEM - {e})
    if e in AUDIO:
        return sorted(SAIDA_AUDIO - {e})
    if e in VIDEO:
        return sorted(SAIDA_VIDEO - {e})
    if e in ESCRITORIO:
        return sorted(SAIDA_ESCRITORIO - {e})
    return []


def _livre(pasta: Path, nome: str) -> Path:
    alvo = pasta / nome
    n = 2
    while alvo.exists():
        alvo = pasta / f"{Path(nome).stem} ({n}){Path(nome).suffix}"
        n += 1
    return alvo


def _rodar(runner: Runner, argv: list[str], programa: str) -> None:
    try:
        r = runner(
            argv, capture_output=True, timeout=TIMEOUT_S, check=False, stdin=subprocess.DEVNULL
        )
    except FileNotFoundError:
        raise ConversaoError(f"{programa} não está instalado") from None
    except subprocess.TimeoutExpired:
        raise ConversaoError(
            f"{programa} demorou demais (mais de {TIMEOUT_S // 60} minutos)"
        ) from None
    if r.returncode != 0:
        raise ConversaoError(f"{programa} não conseguiu converter este arquivo")


def converter(
    origem: Path,
    formato: str,
    destino_dir: Path,
    *,
    runner: Runner = subprocess.run,
    which: Callable[[str], str | None] = shutil.which,
) -> Path:
    """Converte `origem` para `formato` dentro de `destino_dir` e devolve o arquivo novo.
    Nunca sobrescreve: se o nome existir, vira `nome (2).ext`."""
    if not origem.is_file() or origem.is_symlink():
        raise ConversaoError("o arquivo não existe (ou é um atalho)")
    if origem.stat().st_size > MAX_ENTRADA:
        raise ConversaoError("arquivo grande demais (máximo 200 MB)")
    entrada, saida = formato_normal(origem.suffix), formato_normal(formato)
    if saida not in formatos_para(entrada):
        possiveis = ", ".join(formatos_para(entrada)) or "nenhum"
        raise ConversaoError(f"não converto .{entrada or '?'} para {saida}. Possíveis: {possiveis}")
    destino_dir.mkdir(parents=True, exist_ok=True)
    if entrada == "pdf" and saida == "comprimir":
        return _comprimir_pdf(origem, destino_dir)
    if entrada in IMAGEM:
        return _imagem(origem, saida, destino_dir)
    if entrada in AUDIO or entrada in VIDEO:
        return _ffmpeg(origem, saida, destino_dir, runner, which)
    return _escritorio(origem, entrada, saida, destino_dir, runner, which)


def _ffmpeg(origem: Path, saida: str, destino_dir: Path, runner: Runner, which: Any) -> Path:
    ffmpeg = which("ffmpeg")
    if not ffmpeg:
        raise ConversaoError("falta o ffmpeg (instale e deixe no PATH)")
    alvo = _livre(destino_dir, f"{origem.stem}.{saida}")
    argv = [ffmpeg, "-y", "-hide_banner", "-loglevel", "error", "-i", str(origem)]
    if saida in SAIDA_AUDIO:
        argv.append("-vn")  # vídeo → só o áudio
    argv.append(str(alvo))
    try:
        _rodar(runner, argv, "ffmpeg")
    except ConversaoError:
        alvo.unlink(missing_ok=True)
        raise
    return alvo


def _imagem(origem: Path, saida: str, destino_dir: Path) -> Path:
    from PIL import Image, UnidentifiedImageError

    alvo = _livre(destino_dir, f"{origem.stem}.{saida}")
    try:
        if saida == "pdf":
            try:
                import img2pdf  # type: ignore[import-not-found,unused-ignore]

                alvo.write_bytes(img2pdf.convert(str(origem)))
                return alvo
            except ImportError:
                pass
        with Image.open(origem) as im:
            im.load()
            if saida in ("jpg", "pdf") and im.mode not in ("RGB", "L"):
                im = im.convert("RGB")  # JPEG e PDF não têm transparência
            formato = {"jpg": "JPEG", "tiff": "TIFF", "pdf": "PDF"}.get(saida, saida.upper())
            im.save(alvo, format=formato)
    except (OSError, UnidentifiedImageError, Image.DecompressionBombError, ValueError):
        alvo.unlink(missing_ok=True)
        raise ConversaoError("não consegui ler essa imagem") from None
    return alvo


def _escritorio(
    origem: Path, entrada: str, saida: str, destino_dir: Path, runner: Runner, which: Any
) -> Path:
    soffice = which("soffice") or which("libreoffice")
    if not soffice:
        raise ConversaoError("falta o LibreOffice (soffice) para converter documentos")
    with tempfile.TemporaryDirectory(prefix="orion-conv-") as tmp:
        temp = Path(tmp)
        perfil = (temp / "perfil").as_posix()
        argv = [soffice, f"-env:UserInstallation=file:///{perfil.lstrip('/')}", "--headless"]
        if entrada == "pdf":
            argv += ["--infilter=writer_pdf_import"]
        argv += ["--convert-to", saida, "--outdir", str(temp / "saida"), str(origem)]
        _rodar(runner, argv, "LibreOffice")
        feitos = sorted((temp / "saida").glob(f"*.{saida}"))
        if not feitos:
            raise ConversaoError("o LibreOffice não gerou o arquivo")
        alvo = _livre(destino_dir, f"{origem.stem}.{saida}")
        shutil.move(str(feitos[0]), alvo)
    return alvo


def _comprimir_pdf(origem: Path, destino_dir: Path) -> Path:
    try:
        import pikepdf  # type: ignore[import-not-found,unused-ignore]
    except ImportError:
        raise ConversaoError("falta o pikepdf (pip install pikepdf) para comprimir PDF") from None
    alvo = _livre(destino_dir, f"{origem.stem}-comprimido.pdf")
    try:
        with pikepdf.open(origem) as pdf:
            pdf.save(
                alvo,
                compress_streams=True,
                recompress_flate=True,
                object_stream_mode=pikepdf.ObjectStreamMode.generate,
            )
    except Exception:  # noqa: BLE001 — PDF corrompido, protegido por senha etc.
        with contextlib.suppress(OSError):
            alvo.unlink(missing_ok=True)
        raise ConversaoError("não consegui abrir esse PDF (protegido ou corrompido?)") from None
    return alvo


def converter_e_guardar(
    origem: Path, formato: str, destino_dir: Path, biblioteca: Any = None, **kw: Any
) -> dict[str, Any]:
    """Converte e, se der (até 25 MB), guarda a cópia na biblioteca de resultados."""
    arquivo = converter(origem, formato, destino_dir, **kw)
    item = None
    if biblioteca is not None:
        kind = "imagem" if arquivo.suffix.lower().lstrip(".") in IMAGEM else "documento"
        item = biblioteca.registrar_arquivo(None, "converter_arquivo", arquivo, kind)
    return {"arquivo": arquivo, "biblioteca": item}
