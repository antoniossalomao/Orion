"""O Orion DE VERDADE (app real, política real, SQLite real) com o front real no Chromium.

Só o modelo é de mentira (`fake_gateway`). Prova o que os testes com backend de mentira não
provam: login por cookie ponta a ponta, o `/chat` em SSE atrás da autenticação e o fluxo de
aprovação executando a ação de verdade depois do clique.
"""

from __future__ import annotations

import contextlib
import json
import os
import sqlite3
import subprocess
import sys
import time

import httpx
import pytest
from playwright.sync_api import expect

from .conftest import RAIZ, _porta_livre

SENHA = "senha-do-orion-real-123"


def _esperar(url: str, caminho: str = "/health", tentativas: int = 150) -> None:
    for _ in range(tentativas):
        try:
            if httpx.get(url + caminho, timeout=0.5).status_code < 500:
                return
        except httpx.HTTPError:
            time.sleep(0.2)
    raise RuntimeError(f"{url} não subiu")


@contextlib.contextmanager
def _subir(tmp_path, extra_env=None):
    """Sobe o gateway de mentira e o Orion de verdade; devolve (url, arquivo-marca)."""
    p_gw, p_app = _porta_livre(), _porta_livre()
    alvo = tmp_path / "marca.txt"
    base = {**os.environ, "PYTHONPATH": str(RAIZ)}
    gw = subprocess.Popen(
        [sys.executable, "-m", "tests.front_e2e.fake_gateway"],
        cwd=RAIZ, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        env={**base, "FAKE_GATEWAY_PORT": str(p_gw), "FAKE_ALVO": str(alvo)},
    )  # fmt: skip
    env = {
        **base,
        "ORION_DATA_DIR": str(tmp_path / "dados"),
        "ORION_PORT": str(p_app),
        "ORION_GATEWAY_URL": f"http://127.0.0.1:{p_gw}/v1",
        "ORION_GATEWAY_MODEL": "modelo-falso",
        "ORION_DESKTOP_TOOLS": "true",
        "ORION_JOBS_ENABLED": "false",
        "ORION_MCP_ENABLED": "false",
        "ORION_LOG_JSON": "false",
        "ORION_ADMIN_TOKEN": "",
        **(extra_env or {}),
    }
    subprocess.run(
        [sys.executable, "-m", "orion", "set-password", "--stdin"],
        cwd=RAIZ, env=env, input=SENHA + "\n", text=True, check=True, capture_output=True,
    )  # fmt: skip
    app = subprocess.Popen(
        [sys.executable, "-m", "orion"], cwd=RAIZ, env=env,
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )  # fmt: skip
    url = f"http://127.0.0.1:{p_app}"
    try:
        _esperar(f"http://127.0.0.1:{p_gw}", "/pedidos")
        _esperar(url)
        yield url, alvo
    finally:
        for proc in (app, gw):
            proc.terminate()
            proc.wait(timeout=20)  # se o desligamento do MCP travar, aqui estoura


@pytest.fixture
def orion_real(tmp_path):
    with _subir(tmp_path) as r:
        yield r


def _pagina(navegador, url):
    ctx = navegador.new_context(viewport={"width": 1440, "height": 900}, locale="pt-BR")
    page = ctx.new_page()
    erros: list[str] = []
    # ruído esperado: o orion.app não tem as rotas do legado (/metrics, /sessoes...: 404) e a senha
    # errada de propósito (401)
    page.on(
        "console",
        lambda m: (
            erros.append(m.text)
            if m.type == "error" and "404" not in m.text and "401" not in m.text
            else None
        ),
    )
    page.on("pageerror", lambda e: erros.append(f"pageerror: {e}"))
    page.goto(f"{url}/ui/?semboot#/chat")
    page.wait_for_selector("html[data-pronto='true']")
    return ctx, page, erros


def test_api_sem_login_e_recusada_e_a_documentacao_nao_existe(orion_real):
    url, _ = orion_real
    assert httpx.post(f"{url}/chat", json={"texto": "oi"}).status_code == 401
    assert httpx.get(f"{url}/approvals").status_code == 401
    assert httpx.get(f"{url}/health").json().keys() == {"status", "version"}
    assert httpx.get(f"{url}/docs").status_code == 404
    assert httpx.get(f"{url}/openapi.json").status_code == 404
    st = httpx.get(f"{url}/auth/status").json()
    assert st == {"configured": True, "authenticated": False, "token_auth": False}


def test_login_conversa_e_aprovacao_ponta_a_ponta(navegador, orion_real, tmp_path):
    url, alvo = orion_real
    ctx, page, erros = _pagina(navegador, url)
    try:
        dialogo = page.get_by_role("dialog", name="Entrar no Orion")
        expect(dialogo).to_be_visible()
        dialogo.get_by_label("Senha").fill("senha-errada")
        page.keyboard.press("Enter")
        expect(dialogo).to_contain_text("Senha incorreta.")
        dialogo.get_by_label("Senha").fill(SENHA)
        page.keyboard.press("Enter")
        expect(dialogo).to_have_count(0)

        cookies = {c["name"]: c for c in ctx.cookies()}
        assert (
            cookies["orion_session"]["httpOnly"]
            and cookies["orion_session"]["sameSite"] == "Strict"
        )
        assert "orion_session" not in page.evaluate("document.cookie")  # JS não enxerga o cookie

        # conversa normal em streaming, atrás do cookie
        page.fill("#composer-input", "oi")
        page.keyboard.press("Enter")
        expect(page.locator(".msg-orion").last).to_contain_text(
            "Olá, Antônio. Tudo certo.", timeout=20000
        )

        # ação que a política manda aprovar: cartão, nada roda antes do clique
        page.fill("#composer-input", "escreva a marca")
        page.keyboard.press("Enter")
        page.get_by_role("button", name="Aprovar").last.wait_for(timeout=20000)
        assert not alvo.exists()
        page.get_by_role("button", name="Aprovar").last.click()
        expect(page.locator(".msg-orion").last).to_contain_text("Feito.", timeout=20000)
        relato = page.locator(".msg-orion").last.inner_text()
        assert alvo.exists(), f"a ação aprovada não rodou; o modelo recebeu: {relato}"
        assert alvo.read_text().strip() == "aprovado"  # rodou de verdade, depois do aval

        # a trilha de auditoria registrou o pedido de aprovação e depois a execução, no banco
        bd = sqlite3.connect(tmp_path / "dados" / "orion.db")
        decisoes = [
            r[0]
            for r in bd.execute(
                "SELECT action FROM audit WHERE tool='executar_comando' ORDER BY id"
            )
        ]
        bd.close()
        assert decisoes == ["confirm", "allow"]
        assert erros == []
    finally:
        ctx.close()


def _eventos(resp: httpx.Response) -> list:
    return [
        json.loads(ln[6:]) if ln[6:] != "[DONE]" else "[DONE]"
        for ln in resp.text.splitlines()
        if ln.startswith("data: ")
    ]


def test_servidor_mcp_de_verdade_entra_no_app_sob_a_politica(tmp_path):
    """mcp.json -> servidor MCP real (SDK) -> ferramenta no agente -> política -> resultado, pelo
    HTTP do Orion de verdade; e o desligamento encerra o servidor sem travar."""
    fake = RAIZ / "tests" / "mcp_cliente" / "fake_server.py"
    cfg = tmp_path / "mcp.json"
    cfg.write_text(
        json.dumps(
            {
                "servers": {
                    "fake": {
                        "command": sys.executable,
                        "args": [str(fake)],
                        "allow": ["somar", "apagar"],
                        "tools": {"somar": {"risk": "read"}},
                    }
                }
            }
        ),
        encoding="utf-8",
    )
    extra = {"ORION_MCP_ENABLED": "true", "ORION_MCP_CONFIG": str(cfg)}
    with _subir(tmp_path, extra) as (url, _):
        with httpx.Client(base_url=url) as c:
            assert c.post("/auth/login", json={"senha": SENHA}).status_code == 200
            saude = c.get("/health").json()
            assert saude["components"]["mcp"]["fake"].startswith("ok (2")
            ev = _eventos(c.post("/chat", json={"texto": "mcp some 2 e 3"}))
        ferramentas = [e["tool"] for e in ev if isinstance(e, dict) and "tool" in e]
        assert ferramentas == [{"name": "fake__somar", "decision": "allow", "reason": ""}]
        assert {"text": 'Feito. Resultado: {"texto": "5"}'} in ev
        # `apagar` não tem classe no mcp.json: confirma sempre (exec), nunca roda direto
        with httpx.Client(base_url=url) as c:
            c.post("/auth/login", json={"senha": SENHA})
            ev = _eventos(c.post("/chat", json={"texto": "mcp apagar"}))
            aprov = [e["approval"] for e in ev if isinstance(e, dict) and "approval" in e]
            assert aprov and aprov[0]["tool"] == "fake__apagar" and aprov[0]["reason"]
