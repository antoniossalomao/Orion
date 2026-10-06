"""Tira capturas de tela do front (para revisão visual a olho: não é teste de pixel).

    uv run python -m tests.front_e2e.capturas capturas/          # 5 telas × 3 temas × 2 tamanhos
    ORION_E2E_CHROME=/caminho/do/chrome uv run python -m tests.front_e2e.capturas /tmp/prints

Sobe o backend de mentira numa porta livre. Por que não snapshot de pixel: o céu é WebGL (software no CI,
GPU na máquina) e a Inter muda por plataforma; o que dá para provar sem flutuar está medido em `test_front.py`
(alinhamentos, espaçamentos, ausência de rolagem horizontal, contraste via axe).
"""

from __future__ import annotations

import os
import sys
from pathlib import Path

from playwright.sync_api import sync_playwright

from .conftest import _subir

TAMANHOS = [(1440, 900), (700, 800)]
TEMAS = ["noite", "grafite", "contraste"]
TELAS = [("inicio", ""), ("chat", "#/chat"), ("memoria", "#/memoria"), ("integracoes", "#/integracoes"), ("config", "#/config")]


def main(saida: Path) -> int:
    saida.mkdir(parents=True, exist_ok=True)
    gerador = _subir("")
    url = next(gerador)
    try:
        with sync_playwright() as p:
            b = p.chromium.launch(
                executable_path=os.environ.get("ORION_E2E_CHROME") or None,
                args=["--no-sandbox", "--use-gl=swiftshader", "--enable-unsafe-swiftshader", "--ignore-gpu-blocklist"],
            )
            for tema in TEMAS:
                for w, h in TAMANHOS:
                    ctx = b.new_context(viewport={"width": w, "height": h}, locale="pt-BR")
                    ctx.add_init_script(f"localStorage.setItem('orion_theme', JSON.stringify('{tema}'))")
                    pg = ctx.new_page()
                    pg.goto(f"{url}/ui/?semboot")
                    pg.wait_for_selector("html[data-pronto='true']")
                    for nome, rota in TELAS:
                        pg.evaluate(f"location.hash = {rota!r} || '#/'")
                        if nome == "chat" and not pg.locator(".msg-orion").count():
                            pg.fill("#composer-input", "explique algo longo")
                            pg.keyboard.press("Enter")
                            pg.wait_for_selector(".msg-orion[data-streaming='false']", timeout=20000)
                        pg.wait_for_timeout(2200 if nome == "memoria" else 900)
                        pg.screenshot(path=str(saida / f"{nome}_{tema}_{w}.png"))
                    ctx.close()
            b.close()
    finally:
        try:
            next(gerador)
        except StopIteration:
            pass
    print(f"{len(TEMAS) * len(TAMANHOS) * len(TELAS)} capturas em {saida}")
    return 0


if __name__ == "__main__":
    sys.exit(main(Path(sys.argv[1] if len(sys.argv) > 1 else "capturas")))
