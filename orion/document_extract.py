"""Extrator em processo descartável, sem interpretar código/links dos documentos.

PDF escaneado (E4.2): a página sem texto (menos de `MIN_TEXTO_PAGINA` caracteres) tem a imagem
lida pelo tesseract LOCAL, até `ORION_OCR_MAX_PAGES` páginas por documento. A imagem vai para uma
pasta temporária que é apagada ao fim; nada sai do computador. O andamento sai em linhas JSON
`{"progress": N}` no stdout (N de 0 a 100) e a última linha é o resultado."""

from __future__ import annotations

import io
import json
import os
import shutil
import subprocess
import sys
import tempfile
from collections.abc import Callable
from pathlib import Path

MAX_TEXT = 2 * 1024 * 1024
MIN_TEXTO_PAGINA = 20  # menos que isto, a página é imagem (ou quase): tenta o OCR


def ocr_imagem(
    imagem: Path,
    *,
    tesseract: str,
    langs: str = "por+eng",
    runner: Callable[..., object] = subprocess.run,
) -> str | None:
    """Texto de uma imagem pelo tesseract local; sem o pacote de idioma cai para o inglês."""
    env = {
        k: os.environ[k]
        for k in ("SYSTEMROOT", "WINDIR", "TEMP", "TMP", "TESSDATA_PREFIX")
        if k in os.environ
    }
    for lang in (langs, "eng"):
        try:
            r = runner(
                [tesseract, str(imagem), "stdout", "-l", lang],
                env=env,
                stdin=subprocess.DEVNULL,
                capture_output=True,
                timeout=120,
                check=False,
            )
        except (OSError, subprocess.TimeoutExpired):
            return None
        if r.returncode == 0:  # type: ignore[attr-defined]
            return r.stdout.decode("utf-8", "replace")  # type: ignore[attr-defined]
    return None


def _ocr_da_pagina(page, ocr: Callable[[Path], str | None]) -> str:
    """Lê as imagens da página uma a uma. Imagem que não decodifica é pulada."""
    partes: list[str] = []
    with tempfile.TemporaryDirectory(prefix="orion-ocr-") as pasta:
        for n, img in enumerate(page.images):
            destino = Path(pasta) / f"p{n}.png"
            try:
                img.image.save(destino, "PNG")
                salvou = True
            except Exception:  # noqa: BLE001 — formato que o Pillow não abre: sem texto desta imagem
                salvou = False
            texto = ocr(destino) if salvou else None
            if texto:
                partes.append(texto)
    return "\n".join(partes)


def extract(
    raw: bytes,
    kind: str,
    *,
    ocr: Callable[[Path], str | None] | None = None,
    max_ocr_pages: int = 0,
    progress: Callable[[int], None] | None = None,
) -> str:
    if kind == ".pdf":
        from pypdf import PdfReader

        reader = PdfReader(io.BytesIO(raw), strict=True)
        if reader.is_encrypted:
            raise ValueError("pdf_encrypted")
        if len(reader.pages) > 200:
            raise ValueError("pdf_page_limit")
        parts, size, lidas, total = [], 0, 0, len(reader.pages)
        for i, page in enumerate(reader.pages):
            text = page.extract_text() or ""
            if len(text.strip()) < MIN_TEXTO_PAGINA and ocr is not None and lidas < max_ocr_pages:
                lidas += 1
                text = (text + "\n" + _ocr_da_pagina(page, ocr)).strip()
            size += len(text.encode())
            if size > MAX_TEXT:
                raise ValueError("document_text_limit")
            parts.append(text)
            if progress:
                progress(round(100 * (i + 1) / total))
        text = "\n\n".join(parts)
        if not text.strip():
            raise ValueError("pdf_requires_ocr")
    else:
        text = raw.decode("utf-8-sig")
        if "\x00" in text:
            raise ValueError("document_invalid_text")
    if not text.strip() or len(text.encode()) > MAX_TEXT:
        raise ValueError("document_text_limit")
    return text


def main():
    # argv: arquivo, tipo [, tesseract, páginas-de-OCR, idiomas]
    max_ocr = int(sys.argv[4]) if len(sys.argv) > 4 else 0
    tesseract = sys.argv[3] if len(sys.argv) > 3 and max_ocr > 0 else ""
    if tesseract and not (os.path.isfile(tesseract) and shutil.which(tesseract)):
        tesseract = ""
    # Best effort OS bounds in addition to the parent's deadline and byte caps.
    if sys.platform != "win32":
        import resource

        # no macOS o RLIMIT_AS é recusado: cada limite é tentado sozinho (o prazo e o teto de bytes
        # do processo pai continuam valendo). Com OCR o tesseract herda os limites: sobe o teto.
        for limite, valor in (
            (resource.RLIMIT_AS, (1536 if tesseract else 512) * 1024 * 1024),
            (resource.RLIMIT_CPU, 10 + 15 * max_ocr if tesseract else 10),
        ):
            try:
                resource.setrlimit(limite, (valor, valor))
            except (ValueError, OSError):
                pass
    ocr = None
    if tesseract:
        langs = sys.argv[5] if len(sys.argv) > 5 else "por+eng"
        ocr = lambda p: ocr_imagem(p, tesseract=tesseract, langs=langs)  # noqa: E731

    def andamento(pct: int) -> None:
        sys.stdout.write(json.dumps({"progress": pct}) + "\n")
        sys.stdout.flush()

    try:
        text = extract(
            Path(sys.argv[1]).read_bytes(),
            sys.argv[2],
            ocr=ocr,
            max_ocr_pages=max_ocr if tesseract else 0,
            progress=andamento,
        )
        result = {"text": text}
    except ValueError as error:
        code = str(error)
        result = {
            "error": code
            if code
            in {
                "pdf_encrypted",
                "pdf_page_limit",
                "pdf_requires_ocr",
                "document_text_limit",
                "document_invalid_text",
            }
            else "document_invalid"
        }
    except Exception:  # noqa: BLE001 — o extrator não divulga conteúdo/erros de bibliotecas
        result = {"error": "document_invalid"}
    sys.stdout.write(json.dumps(result))


if __name__ == "__main__":
    main()
