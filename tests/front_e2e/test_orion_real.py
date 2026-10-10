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
def _subir(tmp_path, extra_env=None, gw_env=None):
    """Sobe o gateway de mentira e o Orion de verdade; devolve (url, arquivo-marca).
    Em `extra_env`, `{p_gw}` vira a porta do gateway de mentira (para apontar outro endereço,
    como o do modelo local, para ele)."""
    p_gw, p_app = _porta_livre(), _porta_livre()
    alvo = tmp_path / "marca.txt"
    base = {**os.environ, "PYTHONPATH": str(RAIZ)}
    gw = subprocess.Popen(
        [sys.executable, "-m", "tests.front_e2e.fake_gateway"],
        cwd=RAIZ, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
        env={**base, "FAKE_GATEWAY_PORT": str(p_gw), "FAKE_ALVO": str(alvo), **(gw_env or {})},
    )  # fmt: skip
    extra_env = {k: v.replace("{p_gw}", str(p_gw)) for k, v in (extra_env or {}).items()}
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
        **extra_env,
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
        _subir.gateway = f"http://127.0.0.1:{p_gw}"  # type: ignore[attr-defined]
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
        tela = page.get_by_role("dialog", name="Entrar no Orion")
        expect(tela).to_be_visible()
        tela.get_by_label("Usuário").fill("admin")
        tela.get_by_label("Senha").fill("senha-errada")
        page.keyboard.press("Enter")
        expect(tela).to_contain_text("Usuário ou senha incorretos.")
        tela.get_by_label("Usuário").fill("admin")
        tela.get_by_label("Senha").fill(SENHA)
        page.keyboard.press("Enter")
        expect(tela).to_have_count(0)

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

        # o painel único mostra o que aconteceu de verdade: o gateway usado, a política e as CLIs
        page.evaluate("location.hash = '#/painel'")
        expect(page.locator("html")).to_have_attribute("data-view", "painel")
        modelos = page.locator('#painel-corpo [data-endpoint="gateway"]')
        expect(modelos).to_contain_text("modelo-falso", timeout=15000)
        expect(modelos).to_contain_text("Funcionando")
        politica = page.locator('#painel-corpo [data-id="decisoes"]')
        expect(politica).to_contain_text("1 pediu aval")
        expect(politica.locator(".painel-recentes")).to_contain_text("executar_comando")
        expect(page.locator('#painel-corpo [data-id="aprovacoes"]')).to_contain_text(
            "Nenhuma ação esperando aval."
        )
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
        assert [(t["name"], t["decision"], t["reason"]) for t in ferramentas] == [
            ("fake__somar", "allow", "")
        ]
        assert {"text": 'Feito. Resultado: {"texto": "5"}'} in ev
        # `apagar` não tem classe no mcp.json: confirma sempre (exec), nunca roda direto
        with httpx.Client(base_url=url) as c:
            c.post("/auth/login", json={"senha": SENHA})
            ev = _eventos(c.post("/chat", json={"texto": "mcp apagar"}))
            aprov = [e["approval"] for e in ev if isinstance(e, dict) and "approval" in e]
            assert aprov and aprov[0]["tool"] == "fake__apagar" and aprov[0]["reason"]


def _entrar(page):
    tela = page.get_by_role("dialog", name="Entrar no Orion")
    expect(tela).to_be_visible()
    tela.get_by_label("Usuário").fill("admin")
    tela.get_by_label("Senha").fill(SENHA)
    page.keyboard.press("Enter")
    expect(tela).to_have_count(0)


@contextlib.contextmanager
def _cliente_logado(url: str):
    with httpx.Client(base_url=url, timeout=30) as c:
        assert c.post("/auth/login", json={"senha": SENHA}).status_code == 200
        yield c


def test_prova_e1_provedores_panico_e_privacidade(navegador, tmp_path):
    """10 chamadas ao gateway (2 falhas) aparecem no card "Provedores"; `orion panico` (a CLI, no
    mesmo banco) corta as ferramentas de rede e execução e o painel mostra "Modo pânico"; a tela
    de privacidade mostra o que saiu; sair do pânico pelo painel pede a senha."""
    with _subir(tmp_path, gw_env={"FAKE_FALHAS": "3,7"}) as (url, _):
        gw = _subir.gateway  # type: ignore[attr-defined]
        with _cliente_logado(url) as c:
            for i in range(10):
                c.post("/chat", json={"texto": f"oi {i}"})
            linha = next(
                x
                for x in c.get("/painel").json()["provedores"]
                if x["provider"] == "gateway:padrão"
            )
            assert (linha["chamadas"], linha["falhas"]) == (10, 2)
        env = {
            **os.environ,
            "PYTHONPATH": str(RAIZ),
            "ORION_DATA_DIR": str(tmp_path / "dados"),
        }
        r = subprocess.run(
            [sys.executable, "-m", "orion", "panico"], cwd=RAIZ, env=env,
            capture_output=True, text=True, check=True,
        )  # fmt: skip
        assert "PÂNICO LIGADO" in r.stdout
        with _cliente_logado(url) as c:
            c.post("/chat", json={"texto": "oi em pânico"})
        cortadas = httpx.get(f"{gw}/ferramentas").json()[-1]
        assert cortadas and "executar_comando" not in cortadas and "buscar_url" not in cortadas
        assert "buscar_memoria" in cortadas  # leitura local continua

        ctx, page, erros = _pagina(navegador, url)
        try:
            _entrar(page)
            page.evaluate("location.hash = '#/painel'")
            corpo = page.locator("#painel-corpo")
            expect(corpo.locator(".painel-alertas .banner-danger").first).to_contain_text(
                "Modo pânico ligado", timeout=15000
            )
            linha = corpo.locator('[data-id="provedores"] tr[data-provedor="gateway:padrão"]')
            expect(linha).to_contain_text("11")  # as 10 + a conversa em pânico
            expect(linha).to_contain_text("modelo-falso")
            expect(corpo.locator('[data-cota="gateway"] .meter-val')).to_have_text("11/1000")
            # sair do pânico pelo painel: a senha é pedida de novo
            corpo.get_by_role("button", name="Sair do modo pânico").click()
            page.get_by_label("Confirme a senha").fill(SENHA)
            page.keyboard.press("Enter")
            expect(corpo.locator('[data-modo="panico"]')).to_have_attribute(
                "data-estado", "desligado", timeout=8000
            )
            # privacidade: o que saiu hoje, sem o conteúdo
            page.goto(f"{url}/ui/?semboot#/privacidade")
            page.wait_for_selector("html[data-pronto='true']")
            priv = page.locator("#privacidade-corpo")
            expect(
                priv.locator('[data-id="totais"] [data-provedor="gateway:padrão"]')
            ).to_contain_text("11 envio(s)", timeout=15000)
            expect(priv.locator('[data-id="hoje"] tbody tr')).to_have_count(11)
            expect(priv).not_to_contain_text("oi em pânico")
            assert [e for e in erros if "502" not in e] == []
        finally:
            ctx.close()
    banco = sqlite3.connect(tmp_path / "dados" / "orion.db")
    try:
        acoes = [r[0] for r in banco.execute("SELECT action FROM audit WHERE tool='modo_panico'")]
    finally:
        banco.close()
    assert acoes == ["entrar", "sair"]


def test_prova_e1_modelo_local_responde_quando_o_gateway_falha(tmp_path):
    """Gateway fora do ar (porta fechada) + o "Ollama" de mentira no lugar do modelo local: o chat
    responde pelo local, e o pedido ao local sai sem `tools` (regra 49)."""
    morto = _porta_livre()
    extra = {
        "ORION_GATEWAY_URL": f"http://127.0.0.1:{morto}/v1",
        "ORION_LOCAL_MODEL": "qwen3.5:4b",
        "ORION_LOCAL_URL": "http://127.0.0.1:{p_gw}/v1",
    }
    with _subir(tmp_path, extra) as (url, _):
        gw = _subir.gateway  # type: ignore[attr-defined]
        with _cliente_logado(url) as c:
            ev = _eventos(c.post("/chat", json={"texto": "oi"}))
            provedores = {x["provider"] for x in c.get("/painel").json()["provedores"]}
        assert {"text": "Tudo certo."} in ev
        assert any(isinstance(e, dict) and e.get("tier", "").startswith("local/") for e in ev)
        assert httpx.get(f"{gw}/ferramentas").json() == [None]  # sem `tools` no corpo
        primeira = httpx.get(f"{gw}/pedidos").json()[0][0]
        assert primeira["role"] == "system" and "Modo reserva" in primeira["content"]
        assert provedores == {"gateway:padrão", "ollama"}


def test_prova_e3_telas_de_conversas_e_projetos_no_orion_real(navegador, tmp_path):
    """E3.6: as telas que antes só rodavam no backend de mentira (projetos, fontes, resultados,
    atividade, integrações) e as novas da E3 (arquivadas, filtro por projeto, mover, editar
    pedido) contra o app, a política e o SQLite de verdade."""
    with _subir(tmp_path) as (url, _):
        with _cliente_logado(url) as c:
            proj = c.post("/projects", json={"name": "Estágio"}).json()["id"]
            c.post("/chat", json={"texto": "primeira conversa"})
            antiga = c.post("/sessoes").json()["sessao_id"]
            c.patch(f"/sessoes/{antiga}", json={"titulo": "Conversa antiga"})
            c.patch(f"/sessoes/{antiga}", json={"arquivada": True})
        ctx, page, erros = _pagina(navegador, url)
        try:
            _entrar(page)
            # telas já existentes, agora no backend real
            for rota, view, texto in (
                ("#/projetos", "projetos", "Estágio"),
                ("#/fontes", "fontes", "Suas fontes"),
                ("#/resultados", "resultados", None),
                ("#/atividade", "atividade", None),
                ("#/integracoes", "integracoes", None),
                ("#/conhecimento", "conhecimento", "Memória da tela"),
            ):
                page.evaluate(f"location.hash = '{rota}'")
                expect(page.locator("html")).to_have_attribute("data-view", view)
                if texto:
                    expect(page.locator(f"#view-{view}")).to_contain_text(texto, timeout=15000)
            # upload de documento pela interface
            page.evaluate("location.hash = '#/fontes'")
            page.get_by_label("Adicionar documento").set_input_files(
                {"name": "n.md", "mimeType": "text/markdown", "buffer": b"# Nota\n\ntexto real"}
            )
            page.get_by_role("button", name="Enviar e indexar").click()
            expect(page.locator("[data-document-id]")).to_contain_text("Indexado", timeout=15000)
            page.get_by_label("Disponível em: n.md").select_option(label="Estágio")
            expect(page.locator("#document-status")).to_contain_text("movido", timeout=15000)
            # E3.1: arquivadas
            page.evaluate("location.hash = '#/arquivadas'")
            expect(page.locator("#arquivadas-corpo .arq-item")).to_have_count(1)
            expect(page.locator("#arquivadas-corpo")).to_contain_text("Conversa antiga")
            page.get_by_role("button", name="Desarquivar Conversa antiga").click()
            expect(page.locator("#arquivadas-corpo")).to_contain_text("Nenhuma conversa arquivada")
            # E3.2/E3.3: filtro por projeto e mover
            page.evaluate("location.hash = '#/chat'")
            seletor = page.get_by_label("Filtrar conversas por projeto")
            expect(seletor).to_be_visible()
            page.locator("#sb-convs-list .conv", has_text="Conversa antiga").click()
            page.wait_for_function("() => !Orion.historico.leitura()")
            page.fill("#composer-input", "/projeto Estágio")
            page.keyboard.press("Enter")
            expect(page.locator("#sb-convs-list .conv-projeto:not([hidden])")).to_have_text("Estágio")
            seletor.select_option(label="Sem projeto")
            expect(page.locator("#sb-convs-list .conv", has_text="Conversa antiga")).to_have_count(0)
            seletor.select_option(label="Estágio")
            expect(page.locator("#sb-convs-list .conv")).to_have_count(1)
            seletor.select_option("todos")
            # E3.5: editar o pedido
            page.locator("#sb-convs-list .conv", has_text="Conversa sem título").click()
            page.wait_for_function("() => !Orion.historico.leitura()")
            expect(page.locator(".msg-user")).to_contain_text("primeira conversa")
            page.get_by_role("button", name="Editar pedido (nova versão)").click()
            dialogo = page.get_by_role("dialog")
            dialogo.get_by_label("Pedido revisado").fill("pedido reescrito")
            dialogo.get_by_role("button", name="Enviar nova versão").click()
            expect(page.locator(".msg-user")).to_contain_text("pedido reescrito", timeout=20000)
            expect(page.get_by_role("group", name="Versões do pedido")).to_contain_text(
                "2/2", timeout=20000
            )
            assert erros == []
        finally:
            ctx.close()
