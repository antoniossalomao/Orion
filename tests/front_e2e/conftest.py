"""Fixtures dos testes de navegador do front (Playwright + backend de mentira).

    uv sync --group e2e && (cd tests/front_e2e && npm ci)
    uv run pytest tests/front_e2e -q

`ORION_E2E_CHROME=/caminho/do/chrome` usa um Chromium já instalado (sem `playwright install`).
Todo teste também é um teste de console: erro ou aviso da página derruba o teste.
"""

from __future__ import annotations

import os
import socket
import subprocess
import sys
import time
from pathlib import Path

import httpx
import pytest

try:
    from playwright.sync_api import sync_playwright
except (
    ImportError
):  # sem `uv sync --group e2e` (ex.: job principal do CI): estes testes nem são coletados
    sync_playwright = None
    collect_ignore_glob = ["test_*.py"]

RAIZ = Path(__file__).resolve().parents[2]
AXE = Path(__file__).with_name("node_modules") / "axe-core" / "axe.min.js"
TOKEN = "tok-e2e-1234567890-abcdef"

# ruído do driver WebGL por software (swiftshader) em CI sem GPU: não é problema da página
RUIDO = ("GL Driver Message", "GPU stall", "swiftshader", "SwiftShader")


def _porta_livre() -> int:
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def _subir(token: str):
    porta = _porta_livre()
    env = {**os.environ, "MOCK_PORT": str(porta), "MOCK_TOKEN": token}
    proc = subprocess.Popen(
        [sys.executable, "-m", "tests.front_e2e.mock_backend"],
        cwd=RAIZ,
        env=env,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    url = f"http://127.0.0.1:{porta}"
    for _ in range(100):
        try:
            if httpx.get(f"{url}/api-info", timeout=0.5).status_code == 200:
                break
        except httpx.HTTPError:
            time.sleep(0.15)
    else:
        proc.kill()
        raise RuntimeError("backend de mentira não subiu")
    try:
        yield url
    finally:
        proc.terminate()
        proc.wait(timeout=10)


@pytest.fixture(scope="session")
def mock_url():
    yield from _subir("")


@pytest.fixture(scope="session")
def mock_token_url():
    yield from _subir(TOKEN)


@pytest.fixture(scope="session")
def navegador():
    with sync_playwright() as p:
        b = p.chromium.launch(
            executable_path=os.environ.get("ORION_E2E_CHROME") or None,
            args=[
                "--no-sandbox",
                "--use-gl=swiftshader",
                "--enable-unsafe-swiftshader",
                "--ignore-gpu-blocklist",
            ],
        )
        yield b
        b.close()


@pytest.fixture
def abrir(navegador, mock_url):
    """`abrir("#/chat")` → página aberta no front. Opções: viewport, url, reduced, axe, init, http_ok, boot."""
    contextos: list = []
    erros: list[str] = []
    permitir_http: list[bool] = [False]

    def _abrir(
        rota: str = "",
        *,
        viewport=(1440, 900),
        url: str | None = None,
        reduced: bool = False,
        axe: bool = False,
        init: str | None = None,
        http_ok: bool = False,
        boot: bool = False,
    ):
        permitir_http[0] = http_ok
        ctx = navegador.new_context(
            viewport={"width": viewport[0], "height": viewport[1]},
            locale="pt-BR",
            reduced_motion="reduce" if reduced else "no-preference",
            permissions=["clipboard-read", "clipboard-write"],
        )
        contextos.append(ctx)
        if axe:
            ctx.add_init_script(path=str(AXE))  # init script não passa pela CSP da página
        if init:
            ctx.add_init_script(init)
        page = ctx.new_page()
        page.on("pageerror", lambda e: erros.append(f"pageerror: {e}"))
        page.on(
            "console",
            lambda m: (
                erros.append(f"console.{m.type}: {m.text}")
                if m.type in ("error", "warning")
                else None
            ),
        )
        base = url or mock_url
        page.goto(f"{base}/ui/{'' if boot else '?semboot'}{rota}")
        page.wait_for_selector("html[data-pronto='true']")
        return page

    yield _abrir
    for c in contextos:
        c.close()
    relevantes = [
        e
        for e in erros
        if not any(r in e for r in RUIDO)
        and not (permitir_http[0] and "Failed to load resource" in e)
    ]
    assert not relevantes, "console da página não está limpo:\n" + "\n".join(relevantes)
