"""Ponto de entrada do Orion empacotado (.exe): duplo clique sobe o servidor e abre o navegador.

Sem argumentos: sobe o Orion em 127.0.0.1, cria o usuário `admin` com a senha de fábrica se
ainda não houver senha (ela está no repositório: o app avisa e pede para trocar) e abre `/ui/`
no navegador padrão. Com argumentos, é a CLI normal: `orion.exe set-password`,
`orion.exe backup`, `orion.exe mcp-check`...

A janela de console mostra o log; fechá-la encerra o Orion.
"""

from __future__ import annotations

import os
import sys
import threading
import time
import urllib.error
import urllib.request
import webbrowser


def _abrir_navegador(porta: int) -> None:
    url = f"http://127.0.0.1:{porta}"
    for _ in range(120):  # até ~30 s para o servidor ficar de pé
        try:
            with urllib.request.urlopen(f"{url}/health", timeout=1):  # noqa: S310 — URL local fixa
                break
        except (urllib.error.URLError, OSError):
            time.sleep(0.25)
    else:
        return
    webbrowser.open(f"{url}/ui/")


def main() -> int:
    if len(sys.argv) > 1:  # CLI normal (set-password, backup, mcp-check...)
        from orion.__main__ import main as cli

        return cli()
    os.environ.setdefault("ORION_SEED_DEFAULT_PASSWORD", "true")
    os.environ.setdefault("ORION_LOG_JSON", "false")  # log legível na janela de console
    from orion.config import Settings

    porta = Settings().port
    if not os.environ.get("ORION_NO_BROWSER"):
        threading.Thread(target=_abrir_navegador, args=(porta,), daemon=True).start()
    print(f"Orion em http://127.0.0.1:{porta}/ui/ (usuario: admin). Feche esta janela para parar.")
    from orion.__main__ import main as cli

    return cli([])


if __name__ == "__main__":
    sys.exit(main())
