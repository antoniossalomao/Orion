"""Os adaptadores de verdade da ponte: pynput, pystray, pyperclip, websockets.

Importados só quando `orion ponte` sobe (`pip install "orion[ponte]"`). Nada disto foi exercitado
numa tela real nesta etapa: `tests/test_ponte.py` prova o núcleo com falsos; o que depende de
teclado, bandeja e janela de verdade é a prova manual da E2 (ORION_PLANO_RODADA2 §E2).
"""

from __future__ import annotations

import subprocess
import sys
import threading
import webbrowser
from collections.abc import Callable, Sequence
from pathlib import Path
from typing import Any

from ..config import PROJECT_ROOT
from .nucleo import Adaptadores


class TecladoPynput:
    def __init__(self) -> None:
        self._h: Any = None

    def iniciar(self, mapa: dict[str, Callable[[], None]]) -> None:
        from pynput import keyboard  # type: ignore[import-not-found,unused-ignore]

        def em_thread(fn: Callable[[], None]) -> Callable[[], None]:
            # o callback do pynput roda na thread do teclado: uma ação lenta não pode travá-la
            return lambda: threading.Thread(target=fn, daemon=True).start()

        self._h = keyboard.GlobalHotKeys({k: em_thread(f) for k, f in mapa.items()})
        self._h.start()

    def parar(self) -> None:
        if self._h is not None:
            self._h.stop()


class BandejaPystray:
    def __init__(self, icone: Path | None = None) -> None:
        self._icone = icone or PROJECT_ROOT / "assets" / "orion.png"
        self._icon: Any = None

    def iniciar(self, itens: Sequence[tuple[str, Callable[[], None]]]) -> None:
        import pystray  # type: ignore[import-not-found,unused-ignore]
        from PIL import Image

        menu = pystray.Menu(
            *[pystray.MenuItem(rotulo, lambda _i, _it, f=fn: f()) for rotulo, fn in itens]
        )
        self._icon = pystray.Icon("orion", Image.open(self._icone), "Orion", menu)
        self._icon.run_detached()

    def parar(self) -> None:
        if self._icon is not None:
            self._icon.stop()

    def notificar(self, titulo: str, texto: str) -> None:
        if self._icon is not None:
            try:
                self._icon.notify(texto, titulo)
            except Exception:  # noqa: BLE001 — sem suporte a notificação no sistema: segue
                print(f"{titulo}: {texto}", file=sys.stderr)
        else:
            print(f"{titulo}: {texto}", file=sys.stderr)


class JanelaEmProcesso:
    def abrir(self, url: str, titulo: str, largura: int, altura: int) -> None:
        subprocess.Popen(  # noqa: S603 — argv fixo; a URL é a do servidor local da ponte
            [sys.executable, "-m", "orion.ponte.janela", url, titulo, str(largura), str(altura)],
        )


class AreaPyperclip:
    def ler(self) -> str | None:
        import pyperclip  # type: ignore[import-not-found,unused-ignore]

        return pyperclip.paste() or None

    def gravar(self, texto: str) -> None:
        import pyperclip  # type: ignore[import-not-found,unused-ignore]

        pyperclip.copy(texto)


class ColadorPynput:
    def colar(self) -> None:
        from pynput.keyboard import Controller, Key  # type: ignore[import-not-found,unused-ignore]

        teclado = Controller()
        modificador = Key.cmd if sys.platform == "darwin" else Key.ctrl
        with teclado.pressed(modificador):
            teclado.press("v")
            teclado.release("v")


def conectar_real(url: str, cabecalhos: dict[str, str]) -> Any:
    from websockets.asyncio.client import connect  # type: ignore[import-not-found,unused-ignore]

    return connect(url, additional_headers=cabecalhos, open_timeout=10)


def montar() -> Adaptadores:
    """Os adaptadores reais, ligados à mesma bandeja (a notificação sai por ela)."""
    bandeja = BandejaPystray()
    return Adaptadores(
        teclado=TecladoPynput(),
        bandeja=bandeja,
        janela=JanelaEmProcesso(),
        area=AreaPyperclip(),
        colador=ColadorPynput(),
        navegador=webbrowser.open,
        notificar=bandeja.notificar,
    )
