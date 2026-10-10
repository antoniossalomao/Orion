"""Ponto de entrada do Orion empacotado (.exe): duplo clique abre o app, numa janela própria.

Sem argumentos: sobe o Orion em 127.0.0.1 (numa thread), cria o usuário `admin` com a senha de
fábrica se ainda não houver senha (ela está no repositório: o app avisa e pede para trocar) e abre
`/ui/` numa janela de app (`orion.shell`, pywebview). Fechar a janela encerra o Orion. Sem
pywebview/WebView2 cai no navegador padrão (ou use `ORION_NO_BROWSER=1` para só servir). Com
argumentos, é a CLI normal: `orion.exe set-password`, `orion.exe backup`, `orion.exe mcp-check`...
"""

from __future__ import annotations

import os
import sys
import threading
import time
import urllib.error
import urllib.request
import webbrowser


def _esperar_servidor(porta: int) -> bool:
    url = f"http://127.0.0.1:{porta}"
    for _ in range(120):  # até ~30 s para o servidor ficar de pé
        try:
            with urllib.request.urlopen(f"{url}/health", timeout=1):  # noqa: S310 — URL local fixa
                return True
        except (urllib.error.URLError, OSError):
            time.sleep(0.25)
    return False


def _navegador(porta: int) -> None:
    if _esperar_servidor(porta):
        webbrowser.open(f"http://127.0.0.1:{porta}/ui/")


def _icone() -> str | None:
    base = getattr(sys, "_MEIPASS", None) or os.path.dirname(os.path.abspath(__file__))
    caminho = os.path.join(base, "Orion_Core", "Front_end_Orion", "orion.ico")
    return caminho if os.path.isfile(caminho) else None


def _esconder_console() -> None:
    """Modo app no Windows: some com a janela preta do console (a CLI com argumentos a mantém)."""
    if sys.platform != "win32":
        return
    try:
        import ctypes

        hwnd = ctypes.windll.kernel32.GetConsoleWindow()
        if hwnd:
            ctypes.windll.user32.ShowWindow(hwnd, 0)  # SW_HIDE
    except Exception:  # noqa: BLE001, S110 — esconder é cosmético
        pass


def main() -> int:
    if len(sys.argv) > 1:  # CLI normal (set-password, backup, mcp-check...)
        from orion.__main__ import main as cli

        return cli()
    os.environ.setdefault("ORION_SEED_DEFAULT_PASSWORD", "true")
    os.environ.setdefault("ORION_LOG_JSON", "false")  # log legível na janela de console
    from orion.__main__ import main as cli
    from orion.config import Settings

    porta = Settings().port
    if os.environ.get("ORION_NO_BROWSER"):
        return cli([])
    print(f"Orion em http://127.0.0.1:{porta}/ui/ (usuario: admin).")
    servidor = threading.Thread(target=cli, args=([],), daemon=True)
    servidor.start()
    if _esperar_servidor(porta):
        from orion.shell import abrir_janela

        _esconder_console()
        if abrir_janela(f"http://127.0.0.1:{porta}/ui/", _icone()):
            return 0  # janela fechada = Orion encerrado (a thread do servidor é daemon)
        if sys.platform == "win32":
            import ctypes

            ctypes.windll.user32.ShowWindow(ctypes.windll.kernel32.GetConsoleWindow(), 5)  # SW_SHOW
        print("Sem WebView disponível: abrindo no navegador. Feche este console para parar.")
        _navegador(porta)
    while servidor.is_alive():
        servidor.join(1)
    return 0


if __name__ == "__main__":
    sys.exit(main())
