"""Uma janela pequena para uma página da ponte, num processo à parte.

`pywebview` precisa da thread principal e bloqueia até a janela fechar; por isso cada janela é
um processo (`python -m orion.ponte.janela <url> <título> <largura> <altura>`) e a ponte nunca
trava. Sem `pywebview`, abre no navegador padrão.
"""

from __future__ import annotations

import sys
import webbrowser


def abrir(url: str, titulo: str, largura: int, altura: int) -> None:
    try:
        import webview  # type: ignore[import-not-found,unused-ignore]
    except ImportError:
        webbrowser.open(url)
        return
    webview.create_window(titulo, url, width=largura, height=altura, on_top=True, resizable=False)
    webview.start()


def main(argv: list[str]) -> int:
    if len(argv) != 4 or not argv[0].startswith(("http://127.0.0.1", "http://localhost")):
        print("uso: python -m orion.ponte.janela <url local> <título> <largura> <altura>")
        return 2
    abrir(argv[0], argv[1], int(argv[2]), int(argv[3]))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
