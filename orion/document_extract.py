"""Extrator em processo descartável, sem interpretar código/links dos documentos."""

from __future__ import annotations

import io
import json
import sys
from pathlib import Path

MAX_TEXT = 2 * 1024 * 1024


def extract(raw: bytes, kind: str) -> str:
    if kind == ".pdf":
        from pypdf import PdfReader

        reader = PdfReader(io.BytesIO(raw), strict=True)
        if reader.is_encrypted:
            raise ValueError("pdf_encrypted")
        if len(reader.pages) > 200:
            raise ValueError("pdf_page_limit")
        parts, size = [], 0
        for page in reader.pages:
            text = page.extract_text() or ""
            size += len(text.encode())
            if size > MAX_TEXT:
                raise ValueError("document_text_limit")
            parts.append(text)
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
    # Best effort OS bounds in addition to the parent's deadline and byte caps.
    if sys.platform != "win32":
        import resource

        # no macOS o RLIMIT_AS é recusado: cada limite é tentado sozinho (o prazo e o teto de bytes
        # do processo pai continuam valendo)
        for limite, valor in (
            (resource.RLIMIT_AS, 512 * 1024 * 1024),
            (resource.RLIMIT_CPU, 10),
        ):
            try:
                resource.setrlimit(limite, (valor, valor))
            except (ValueError, OSError):
                pass
    try:
        text = extract(Path(sys.argv[1]).read_bytes(), sys.argv[2])
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
