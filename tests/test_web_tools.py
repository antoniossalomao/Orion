"""buscar_url, consultar_clima e pesquisar_com_ia com transporte falso (sem rede)."""

import json

import httpx

from orion.tools.web import GEMINI, GEOCODING, PREVISAO, web_tools

PUBLICO = "93.184.216.34"
RESOLVE = lambda host, porta: [PUBLICO]  # noqa: E731


def ferramentas(handler, **kw):
    ts = web_tools(resolver=RESOLVE, transport=httpx.MockTransport(handler), **kw)
    return {t.name: t for t in ts}


def roda(tool, **args):
    return json.loads(tool.run(args))


# ── buscar_url ────────────────────────────────────────────────────────────────
def test_buscar_url_html_devolve_titulo_e_texto():
    def h(req):
        return httpx.Response(
            200,
            text="<html><head><title>Oi</title></head><body><p>Conteúdo útil</p><script>x()</script></body></html>",
            headers={"content-type": "text/html; charset=utf-8"},
        )

    r = roda(ferramentas(h)["buscar_url"], url="https://exemplo.com/a")
    assert r["titulo"] == "Oi" and r["conteudo"] == "Conteúdo útil" and r["status"] == 200
    assert r["truncado"] is False


def test_buscar_url_corta_no_limite_e_marca_truncado():
    h = lambda req: httpx.Response(200, text="a" * 5000, headers={"content-type": "text/plain"})  # noqa: E731
    r = roda(ferramentas(h)["buscar_url"], url="https://exemplo.com/", max_chars=300)
    assert len(r["conteudo"]) == 300 and r["truncado"] is True


def test_buscar_url_nao_baixa_binario_e_nao_vaza_o_conteudo():
    def h(req):
        return httpx.Response(200, content=b"\x89PNG...", headers={"content-type": "image/png"})

    r = roda(ferramentas(h)["buscar_url"], url="https://exemplo.com/i.png")
    assert "não textual" in r["erro"] and "conteudo" not in r


def test_buscar_url_bloqueia_destino_interno_sem_fazer_requisicao():
    chamadas = []

    def h(req):
        chamadas.append(req)
        return httpx.Response(200, text="não devia chegar aqui")

    ts = web_tools(resolver=lambda host, porta: ["127.0.0.1"], transport=httpx.MockTransport(h))
    r = roda({t.name: t for t in ts}["buscar_url"], url="http://localhost:8000/approvals")
    assert "bloqueada" in r["erro"] and not chamadas


def test_buscar_url_erro_de_rede_vira_resultado():
    def h(req):
        raise httpx.ConnectError("sem rota")

    assert "falha" in roda(ferramentas(h)["buscar_url"], url="https://exemplo.com/")["erro"]


# ── clima ─────────────────────────────────────────────────────────────────────
def test_clima_geocodifica_e_traduz_o_codigo():
    def h(req: httpx.Request):
        if str(req.url).startswith(GEOCODING):
            assert req.url.params["name"] == "Marília"
            return httpx.Response(
                200,
                json={
                    "results": [
                        {
                            "name": "Marília",
                            "admin1": "São Paulo",
                            "latitude": -22.2,
                            "longitude": -49.9,
                        }
                    ]
                },
            )
        assert str(req.url).startswith(PREVISAO) and req.url.params["latitude"] == "-22.2"
        return httpx.Response(
            200,
            json={
                "current": {"temperature_2m": 27.5, "apparent_temperature": 29.0,
                            "relative_humidity_2m": 61, "wind_speed_10m": 9.4, "weather_code": 63},
                "daily": {"temperature_2m_max": [31.0], "temperature_2m_min": [19.2],
                          "precipitation_probability_max": [70]},
            },
        )  # fmt: skip

    r = roda(ferramentas(h)["consultar_clima"])  # sem cidade: usa a padrão
    assert r["cidade"] == "Marília, São Paulo" and r["descricao"] == "chuva"
    assert r["temp_c"] == 27.5 and r["previsao_hoje"] == {
        "max_c": 31.0, "min_c": 19.2, "chance_chuva_pct": 70,
    }  # fmt: skip


def test_clima_cidade_inexistente_e_falha_de_rede():
    nada = lambda req: httpx.Response(200, json={})  # noqa: E731
    assert "não encontrada" in roda(ferramentas(nada)["consultar_clima"], cidade="Xyzzy")["erro"]

    def cai(req):
        raise httpx.ReadTimeout("lento")

    assert "falha" in roda(ferramentas(cai)["consultar_clima"], cidade="Marília")["erro"]


# ── pesquisar_com_ia ──────────────────────────────────────────────────────────
def test_pesquisa_manda_a_chave_em_cabecalho_e_devolve_resposta_e_fontes():
    vistos = []

    def h(req: httpx.Request):
        vistos.append(req)
        return httpx.Response(
            200,
            json={
                "candidates": [
                    {
                        "content": {"parts": [{"text": "A resposta "}, {"text": "é 42."}]},
                        "groundingMetadata": {
                            "groundingChunks": [
                                {"web": {"title": "Fonte", "uri": "https://f.test/x"}},
                                {},
                            ]
                        },
                    }
                ]
            },
        )

    ts = ferramentas(h, search_key=lambda: "CHAVE-SECRETA", search_model="gemini-teste")
    r = roda(ts["pesquisar_com_ia"], query="qual a resposta?")
    assert r["resposta"] == "A resposta é 42." and r["fontes"] == [
        {"titulo": "Fonte", "url": "https://f.test/x"}
    ]
    req = vistos[0]
    assert req.url.path.endswith("/gemini-teste:generateContent") and str(req.url).startswith(
        GEMINI
    )
    assert req.headers["x-goog-api-key"] == "CHAVE-SECRETA"
    assert "CHAVE-SECRETA" not in str(req.url)  # nunca na URL (regra 5)
    assert json.loads(req.content)["tools"] == [{"google_search": {}}]


def test_pesquisa_sem_chave_ou_com_erro_http_nao_vaza_a_chave():
    assert (
        "sem chave"
        in roda(ferramentas(lambda r: httpx.Response(200))["pesquisar_com_ia"], query="x")["erro"]
    )
    h = lambda req: httpx.Response(429, text="cota CHAVE-SECRETA estourada")  # noqa: E731
    r = roda(ferramentas(h, search_key=lambda: "CHAVE-SECRETA")["pesquisar_com_ia"], query="x")
    assert r["erro"] == "a busca falhou (HTTP 429)" and "CHAVE" not in json.dumps(r)
