"""Prova nativa de inferência com GGUF real. Invocado explicitamente pelo CI Windows."""

import hashlib
import json
import os
import subprocess
import sys
import urllib.request
from pathlib import Path


def main():
    if sys.platform != "win32":
        raise SystemExit("Este teste só roda no Windows")
    executable = Path(sys.argv[1]).resolve(strict=True)
    directory = Path(sys.argv[2])
    catalog = json.loads(
        (Path(__file__).parents[1] / "Orion_Rust/crates/orion-local/src/catalog.json").read_text()
    )
    model = next(m for m in catalog if m["id"] == "qwen3-0.6b")
    directory.mkdir(parents=True, exist_ok=True)
    target = directory / model["filename"]
    if not target.exists():
        url = (
            f"https://huggingface.co/{model['repository']}/resolve/"
            f"{model['revision']}/{model['filename']}"
        )
        with urllib.request.urlopen(url, timeout=180) as response, target.open("wb") as output:
            while chunk := response.read(1024 * 1024):
                output.write(chunk)
    with target.open("rb") as source:
        digest = hashlib.file_digest(source, "sha256").hexdigest()
    if target.stat().st_size != model["bytes"] or digest != model["sha256"]:
        target.unlink()
        raise SystemExit("GGUF com tamanho/hash incorreto")
    clean = {k: v for k, v in os.environ.items() if not k.upper().startswith("ORION_")}
    subprocess.run(  # noqa: S603 — executável absoluto do build, argumentos fixos, sem shell.
        [str(executable), "local-test", "--model", str(target)],
        env=clean,
        check=True,
        timeout=240,
    )


if __name__ == "__main__":
    main()
