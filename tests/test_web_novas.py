"""pesquisar_internet (Brave) e gerar_imagem (Gemini) com transporte falso, e o /imagens do app."""

import base64
import json

import httpx
import pytest
from fastapi.testclient import TestClient

from orion.app import create_app
from orion.config import Settings
from orion.policy import Action, Context, PathGuard, PolicyEngine, Risk, ToolCall
from orion.tools.web import BRAVE, MANTER_IMAGENS, web_tools

CHAVE = "chave-secreta-de-teste-0123456789"
PNG = b"\x89PNG\r\n\x1a\n" + b"conteudo-de-imagem"
JPG = b"\xff\xd8\xff\xe0" + b"jpeg"
WEBP = b"RIFF\x00\x00\x00\x00WEBPVP8 "


def ts(handler, tmp_path=None, **kw):
    t = web_tools(
        transport=httpx.MockTransport(handler),
        brave_key=lambda: CHAVE,
        image_key=lambda: CHAVE,
        image_dir=(tmp_path / "imagens") if tmp_path else None,
        **kw,
    )
    return {x.name: x for x in t}


def roda(tool, **args):
    return json.loads(tool.run(args))


# ── pesquisar_internet ────────────────────────────────────────────────────────
def brave(*resultados):
    return httpx.Response(200, json={"web": {"results": list(resultados)}})


def test_busca_manda_a_chave_em_cabecalho_e_limpa_html_dos_resultados():
    visto = {}

    def h(req):
        visto["url"], visto["token"] = str(req.url), req.headers["x-subscription-token"]
        return brave(
            {"title": "SQLite <strong>FTS5</strong>", "url": "https://sqlite.org/fts5.html",
             "description": "Busca &amp; texto <b>completo</b>\n\tno SQLite"},
            {"title": "sem url", "url": "javascript:alert(1)", "description": "x"},
            {"title": "ftp", "url": "ftp://x/y", "description": "x"},
        )  # fmt: skip

    r = roda(ts(h)["pesquisar_internet"], query="sqlite fts5", max_resultados=3)
    assert r["ok"] and r["resultados"] == [
        {
            "titulo": "SQLite FTS5",
            "url": "https://sqlite.org/fts5.html",
            "descricao": "Busca & texto completo no SQLite",
        }
    ]
    assert visto["token"] == CHAVE and CHAVE not in visto["url"] and visto["url"].startswith(BRAVE)
    assert "q=sqlite+fts5" in visto["url"] and "count=3" in visto["url"]


def test_busca_limita_o_numero_de_resultados_e_a_consulta():
    visto = {}

    def h(req):
        visto["q"] = req.url.params["q"]
        visto["n"] = req.url.params["count"]
        return brave()

    t = ts(h)["pesquisar_internet"]
    assert roda(t, query="x" * 900, max_resultados=999)["aviso"] == "nenhum resultado"
    assert (len(visto["q"]), visto["n"]) == (400, "10")
    roda(t, query="a", max_resultados=-5)
    assert visto["n"] == "1"
    assert roda(t, query="   ")["erro"] == "consulta vazia"


def test_busca_sem_chave_ou_com_falha_nao_vaza_a_chave():
    sem = web_tools(transport=httpx.MockTransport(lambda r: brave()))
    assert (
        "ORION_BRAVE_API_KEY"
        in roda({t.name: t for t in sem}["pesquisar_internet"], query="a")["erro"]
    )
    for resp in (httpx.Response(401, text=f"chave {CHAVE} invalida"), httpx.Response(500)):
        r = roda(ts(lambda req, resp=resp: resp)["pesquisar_internet"], query="a")
        assert "HTTP" in r["erro"] and CHAVE not in json.dumps(r)

    def cai(req):
        raise httpx.ConnectError(f"falhou em {req.url} {CHAVE}")

    r = roda(ts(cai)["pesquisar_internet"], query="a")
    assert r["erro"] == "a busca falhou: ConnectError" and CHAVE not in json.dumps(r)
    assert (
        "falhou"
        in roda(
            ts(lambda r: httpx.Response(200, text="não é json"))["pesquisar_internet"], query="a"
        )["erro"]
    )


# ── gerar_imagem ──────────────────────────────────────────────────────────────
def gemini(dados: bytes, mime="image/png", extra=None):
    parte = {"inlineData": {"mimeType": mime, "data": base64.b64encode(dados).decode()}}
    return httpx.Response(
        200,
        json={
            "candidates": [{"content": {"parts": [{"text": "Aqui está"}, parte]}}],
            **(extra or {}),
        },
    )


def test_gera_grava_com_nome_proprio_e_devolve_o_markdown(tmp_path):
    visto = {}

    def h(req):
        visto["url"], visto["chave"] = str(req.url), req.headers["x-goog-api-key"]
        visto["corpo"] = json.loads(req.content)
        return gemini(PNG)

    r = roda(
        ts(h, tmp_path, clock=lambda: 1_700_000_000)["gerar_imagem"], descricao="um gato astronauta"
    )
    assert r["ok"] and r["arquivo"].startswith("img-1700000000-") and r["arquivo"].endswith(".png")
    assert r["markdown"] == f"![um gato astronauta](/imagens/{r['arquivo']})"
    assert (tmp_path / "imagens" / r["arquivo"]).read_bytes() == PNG
    assert (
        visto["chave"] == CHAVE
        and CHAVE not in visto["url"]
        and "gemini-2.5-flash-image" in visto["url"]
    )
    assert visto["corpo"]["contents"][0]["parts"][0]["text"] == "um gato astronauta"
    assert "Aqui está" not in json.dumps(r)  # texto do provedor nunca volta ao modelo


@pytest.mark.parametrize(
    ("dados", "mime", "ext"), [(JPG, "image/jpeg", ".jpg"), (WEBP, "image/webp", ".webp")]
)
def test_aceita_jpeg_e_webp(tmp_path, dados, mime, ext):
    r = roda(ts(lambda req: gemini(dados, mime), tmp_path)["gerar_imagem"], descricao="x")
    assert r["ok"] and r["arquivo"].endswith(ext)


def test_o_alt_do_markdown_nao_quebra_nem_injeta(tmp_path):
    ataque = 'gato](https://evil.example/?d=x) ![z](javascript:1) "aspas" <b>\n\tnova linha'
    r = roda(ts(lambda req: gemini(PNG), tmp_path)["gerar_imagem"], descricao=ataque)
    md = r["markdown"]
    assert md.count("](") == 1 and md.endswith(f"](/imagens/{r['arquivo']})")
    assert "\n" not in md and "javascript" in md  # o texto fica, os colchetes e parênteses não
    assert all(c not in md[2 : md.index("](")] for c in "[]()<>\"'")


@pytest.mark.parametrize(
    ("resposta", "trecho"),
    [
        (lambda: gemini(b"isto nao e um png"), "inválida"),  # assinatura não confere
        (lambda: gemini(PNG, "image/svg+xml"), "não devolveu imagem"),  # SVG nunca vira arquivo
        (lambda: gemini(PNG, "text/html"), "não devolveu imagem"),
        (lambda: gemini(b""), "inválida"),
        (lambda: gemini(b"x" * 13_000_000), "inválida"),
        (
            lambda: httpx.Response(200, json={"promptFeedback": {"blockReason": "SAFETY"}}),
            "recusou",
        ),
        (
            lambda: httpx.Response(
                200, json={"candidates": [{"content": {"parts": [{"text": "só texto"}]}}]}
            ),
            "não devolveu imagem",
        ),
        (lambda: httpx.Response(429, text=f"cota {CHAVE}"), "HTTP 429"),
        (lambda: httpx.Response(200, text="html"), "falhou"),
    ],
)
def test_resposta_ruim_do_provedor_nao_vira_arquivo(tmp_path, resposta, trecho):
    r = roda(ts(lambda req: resposta(), tmp_path)["gerar_imagem"], descricao="x")
    assert trecho in r["erro"] and CHAVE not in json.dumps(r)
    assert not (tmp_path / "imagens").exists() or list((tmp_path / "imagens").iterdir()) == []


def test_base64_corrompido_e_descricao_vazia_e_sem_chave(tmp_path):
    ruim = httpx.Response(
        200,
        json={
            "candidates": [
                {
                    "content": {
                        "parts": [
                            {"inlineData": {"mimeType": "image/png", "data": "@@@não é base64@@@"}}
                        ]
                    }
                }
            ]
        },
    )
    assert "corrompida" in roda(ts(lambda r: ruim, tmp_path)["gerar_imagem"], descricao="x")["erro"]
    assert (
        roda(ts(lambda r: gemini(PNG), tmp_path)["gerar_imagem"], descricao="  ")["erro"]
        == "descrição vazia"
    )
    sem = web_tools(transport=httpx.MockTransport(lambda r: gemini(PNG)), image_dir=tmp_path / "i")
    assert (
        "sem chave de imagem"
        in roda({t.name: t for t in sem}["gerar_imagem"], descricao="x")["erro"]
    )
    sem_pasta = ts(lambda r: gemini(PNG))["gerar_imagem"]
    assert "desligada" in roda(sem_pasta, descricao="x")["erro"]


def test_guarda_so_as_imagens_mais_novas(tmp_path):
    t = ts(lambda r: gemini(PNG), tmp_path)["gerar_imagem"]
    pasta = tmp_path / "imagens"
    pasta.mkdir()
    for i in range(MANTER_IMAGENS + 3):
        (pasta / f"img-{i:012d}-00000000.png").write_bytes(PNG)
        import os

        os.utime(pasta / f"img-{i:012d}-00000000.png", (1000 + i, 1000 + i))
    assert roda(t, descricao="nova")["ok"]
    nomes = sorted(p.name for p in pasta.iterdir())
    assert len(nomes) == MANTER_IMAGENS and "img-000000000000-00000000.png" not in nomes


# ── política ──────────────────────────────────────────────────────────────────
def test_classes_e_limites_das_duas_ferramentas(tmp_path):
    p = PolicyEngine(path_guard=PathGuard(protected_roots=(tmp_path,), safe_roots=()))
    img = p.evaluate(ToolCall("gerar_imagem", {"descricao": "x"}), Context("s"))
    assert img.risk is Risk.WRITE and img.action is Action.ALLOW
    ctx = Context("s")
    busca = p.evaluate(ToolCall("pesquisar_internet", {"query": "x"}), ctx)
    assert busca.risk is Risk.READ and busca.action is Action.ALLOW
    p.note_result(ToolCall("pesquisar_internet", {"query": "x"}), ctx)
    assert ctx.tainted  # resultado de busca é conteúdo externo
    for _ in range(4):
        p.evaluate(ToolCall("gerar_imagem", {"descricao": "x"}), Context("s"))
    assert (
        p.evaluate(ToolCall("gerar_imagem", {"descricao": "x"}), Context("s")).action is Action.DENY
    )


# ── GET /imagens/<arquivo> ────────────────────────────────────────────────────
TOKEN = "token-de-teste-com-16+"
AUTH = {"Authorization": f"Bearer {TOKEN}"}


@pytest.fixture
def app_com_imagem(tmp_path):
    s = Settings(data_dir=tmp_path / "d", admin_token=TOKEN, _env_file=None)
    (s.data_dir / "imagens").mkdir(parents=True)
    (s.data_dir / "imagens" / "img-1700000000-0a1b2c3d.png").write_bytes(PNG)
    (s.data_dir / "imagens" / "img-1700000000-0a1b2c3d.svg").write_text("<svg onload=alert(1)>")
    (s.data_dir / "segredo.png").write_bytes(b"nao devia sair")
    with TestClient(
        create_app(s, gateway_factory=lambda _: None), base_url="http://127.0.0.1"
    ) as c:
        yield c


def test_imagem_so_com_login_e_com_tipo_e_cabecalhos_seguros(app_com_imagem):
    c = app_com_imagem
    assert c.get("/imagens/img-1700000000-0a1b2c3d.png").status_code == 401
    r = c.get("/imagens/img-1700000000-0a1b2c3d.png", headers=AUTH)
    assert r.status_code == 200 and r.content == PNG and r.headers["content-type"] == "image/png"
    assert r.headers["x-content-type-options"] == "nosniff"
    assert "sandbox" in r.headers["content-security-policy"]
    assert r.headers["cache-control"].startswith("private")


@pytest.mark.parametrize(
    "nome",
    [
        "..%2fsegredo.png", "%2e%2e/segredo.png", "img-1700000000-0a1b2c3d.svg", "segredo.png",
        "img-1700000000-0A1B2C3D.png", "img-1-0a1b2c3d.png", "img-1700000000-0a1b2c3d.png%00.svg",
        "img-1700000000-0a1b2c3d.PNG", "img-1700000000-0a1b2c3d", "..", "img-1700000000-ffffffff.png",
    ],
)  # fmt: skip
def test_imagem_recusa_nome_fora_do_padrao_e_arquivo_inexistente(app_com_imagem, nome):
    assert app_com_imagem.get(f"/imagens/{nome}", headers=AUTH).status_code == 404
