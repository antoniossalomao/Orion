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
from .nucleo import Adaptadores, normalizar_area


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


def selecionar_area() -> tuple[int, int, int, int] | None:
    """Camada translúcida em tela cheia: você arrasta, solta, e recebe (esquerda, topo, largura,
    altura) em pixels da tela. Esc ou um clique sem arrastar cancela (None)."""
    import tkinter as tk

    raiz = tk.Tk()
    raiz.attributes("-fullscreen", True)
    raiz.attributes("-topmost", True)
    raiz.attributes("-alpha", 0.25)
    raiz.configure(background="black", cursor="crosshair")
    tela = tk.Canvas(raiz, highlightthickness=0, background="black", cursor="crosshair")
    tela.pack(fill="both", expand=True)
    estado: dict[str, Any] = {"inicio": None, "fim": None, "retangulo": None}

    def apertou(e: Any) -> None:
        estado["inicio"] = (e.x_root, e.y_root)
        estado["retangulo"] = tela.create_rectangle(e.x, e.y, e.x, e.y, outline="white", width=2)
        estado["origem_local"] = (e.x, e.y)

    def arrastou(e: Any) -> None:
        if estado["retangulo"] is not None:
            x0, y0 = estado["origem_local"]
            tela.coords(estado["retangulo"], x0, y0, e.x, e.y)

    def soltou(e: Any) -> None:
        estado["fim"] = (e.x_root, e.y_root)
        raiz.quit()

    raiz.bind("<ButtonPress-1>", apertou)
    raiz.bind("<B1-Motion>", arrastou)
    raiz.bind("<ButtonRelease-1>", soltou)
    raiz.bind("<Escape>", lambda _e: raiz.quit())
    raiz.focus_force()
    raiz.mainloop()
    raiz.destroy()
    if estado["inicio"] is None or estado["fim"] is None:
        return None
    (x0, y0), (x1, y1) = estado["inicio"], estado["fim"]
    return normalizar_area(x0, y0, x1, y1)


def ocr_da_area(idiomas: str = "por+eng") -> str | None:
    """E2.4: seleção de área + captura em memória + tesseract local. A imagem nunca vai a disco
    por aqui (o pytesseract usa um arquivo temporário seu, apagado ao terminar) nem à rede."""
    area = selecionar_area()
    if area is None:
        return None
    import mss  # type: ignore[import-not-found,unused-ignore]
    import pytesseract  # type: ignore[import-not-found,unused-ignore]
    from PIL import Image

    esquerda, topo, largura, altura = area
    with mss.mss() as captura:
        foto = captura.grab({"left": esquerda, "top": topo, "width": largura, "height": altura})
    imagem = Image.frombytes("RGB", foto.size, foto.bgra, "raw", "BGRX")
    try:
        return str(pytesseract.image_to_string(imagem, lang=idiomas))
    finally:
        imagem.close()


def captura_da_tela() -> bytes | None:
    """E2.5: a tela principal em JPEG, só na memória (vai ao servidor local, que a guarda 5 min)."""
    import io

    import mss  # type: ignore[import-not-found,unused-ignore]
    from PIL import Image

    with mss.mss() as captura:
        monitor = captura.monitors[1] if len(captura.monitors) > 1 else captura.monitors[0]
        foto = captura.grab(monitor)
    imagem = Image.frombytes("RGB", foto.size, foto.bgra, "raw", "BGRX")
    saida = io.BytesIO()
    try:
        imagem.save(saida, format="JPEG", quality=80)
    finally:
        imagem.close()
    return saida.getvalue()


def conectar_real(url: str, cabecalhos: dict[str, str]) -> Any:
    from websockets.asyncio.client import connect  # type: ignore[import-not-found,unused-ignore]

    return connect(url, additional_headers=cabecalhos, open_timeout=10)


def montar(idiomas_ocr: str = "por+eng") -> Adaptadores:
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
        ocr_da_area=lambda: ocr_da_area(idiomas_ocr),
        captura_da_janela=captura_da_tela,
    )
