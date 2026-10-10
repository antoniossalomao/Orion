"""Casca desktop do Orion: a interface (/ui/) numa janela própria, sem navegador.

O servidor roda numa thread e a janela (pywebview, sem moldura) na principal. A ponte `ShellApi`
só expõe o que o front usa para ser app: controles da janela e abrir link fora. Sem
`process_command` o chat vai por HTTP (SSE) e, sem `get_config`, entra pela tela de senha.
"""

from __future__ import annotations

import re
from typing import Any

_EXTERNA = re.compile(r"^(https?://|mailto:)", re.IGNORECASE)

TITULO = "Orion"


class ShellApi:
    """Ponte JS→Python (`window.pywebview.api`)."""

    def __init__(self) -> None:
        self.janela: Any = None

    def open_external(self, url: str) -> bool:
        if not isinstance(url, str) or not _EXTERNA.match(url.strip()):
            return False
        import webbrowser

        return webbrowser.open(url.strip())

    def minimize_app(self) -> None:
        if self.janela:
            self.janela.minimize()

    def toggle_maximize(self) -> None:
        if not self.janela:
            return
        if getattr(self, "_maximizada", False):
            self.janela.restore()
        else:
            self.janela.maximize()
        self._maximizada = not getattr(self, "_maximizada", False)

    def close_app(self) -> None:
        if self.janela:
            self.janela.destroy()


def abrir_janela(url: str, icone: str | None = None) -> bool:
    """Abre a janela do app e só volta ao fechá-la. False se não há pywebview ou backend gráfico."""
    try:
        import webview  # pyright: ignore[reportMissingImports] — só no Windows (pyproject)
    except ImportError:
        return False
    api = ShellApi()
    api.janela = webview.create_window(
        TITULO,
        url,
        js_api=api,
        width=1280,
        height=820,
        min_size=(700, 520),
        frameless=True,
        easy_drag=False,
        background_color="#030409",
    )
    try:
        webview.start(icon=icone) if icone else webview.start()
    except Exception:  # noqa: BLE001 — sem WebView2/GTK/Qt: quem chamou cai no navegador
        return False
    return True
