"""Modelo local de reserva (regra 49): último da fila, sem ferramentas, só neste computador."""

import json

import httpx
import pytest
from fastapi.testclient import TestClient
from pydantic import ValidationError

from orion import saidas
from orion.app import create_app, gateway_from_settings
from orion.config import Settings
from orion.doctor import _modelo_local
from orion.gateway import AVISO_RESERVA, CAMADA_LOCAL, ChatGateway, Endpoint, TextDelta
from tests.test_gateway import resposta, sse, texto

TOKEN = "token-de-teste-com-16+"
AUTH = {"Authorization": f"Bearer {TOKEN}"}
FERRAMENTA = {"type": "function", "function": {"name": "x", "parameters": {"type": "object"}}}


@pytest.mark.parametrize(
    "url",
    ["http://192.168.0.10:11434/v1", "https://ollama.exemplo.com/v1", "ftp://127.0.0.1/v1"],
)
def test_endereco_do_modelo_local_so_aceita_este_computador(url):
    with pytest.raises(ValidationError, match="regra 49"):
        Settings(local_url=url, _env_file=None)


def test_local_entra_por_ultimo_sem_ferramentas_e_funciona_sem_gateway():
    s = Settings(
        gateway_url="http://127.0.0.1:20128/v1",
        gateway_model="m",
        gateway_model_fast="rapido",
        local_model="qwen3.5:4b",
        _env_file=None,
    )
    gw = gateway_from_settings(s)
    assert gw is not None
    local = gw.endpoints[-1]
    assert (local.name, local.tier, local.tools, local.api_key) == (
        "local",
        CAMADA_LOCAL,
        False,
        None,
    )
    assert local.base_url == "http://127.0.0.1:11434/v1" and local.provider == "ollama"
    for camada in (None, "rapido", "pesado"):
        assert [e.name for e in gw._ordem(camada)][-1] == "local"
    so_local = gateway_from_settings(Settings(local_model="qwen3.5:4b", _env_file=None))
    assert so_local is not None and [e.name for e in so_local.endpoints] == ["local"]
    assert gateway_from_settings(Settings(_env_file=None)) is None


def _handler(visto):
    def handler(req):
        if req.url.host == "gw.test":
            return httpx.Response(503, text="fora do ar")
        visto.append(json.loads(req.content))
        return resposta(sse(texto("resposta do modelo local", "stop"), "[DONE]"))

    return handler


async def test_gateway_caido_cai_no_local_sem_tools_e_com_o_aviso():
    visto: list[dict] = []
    linhas: list[dict] = []
    gw = ChatGateway(
        [
            Endpoint("gateway", "http://gw.test/v1", "m", "k"),
            Endpoint("local", "http://127.0.0.1:11434/v1", "qwen3.5:4b", tier="local", tools=False),
        ],
        client=httpx.AsyncClient(transport=httpx.MockTransport(_handler(visto))),
        on_call=lambda *a, **kw: linhas.append({"provider": a[0], **kw}),
    )
    eventos = [e async for e in gw.stream([{"role": "user", "content": "oi"}], tools=[FERRAMENTA])]
    assert TextDelta("resposta do modelo local") in eventos
    (corpo,) = visto
    assert "tools" not in corpo and corpo["model"] == "qwen3.5:4b"
    assert corpo["messages"][0] == {"role": "system", "content": AVISO_RESERVA}
    assert [(x["provider"], x["ok"]) for x in linhas] == [
        ("gateway:padrão", False),
        ("ollama", True),
    ]


def test_prova_o_chat_responde_pelo_local_quando_o_gateway_falha(tmp_path):
    visto: list[dict] = []
    s = Settings(
        data_dir=tmp_path / "d",
        admin_token=TOKEN,
        gateway_url="http://gw.test/v1",
        gateway_model="m",
        local_model="qwen3.5:4b",
        desktop_tools=True,
        _env_file=None,
    )

    def fabrica(settings):
        gw = gateway_from_settings(settings)
        assert gw is not None
        gw._client = httpx.AsyncClient(transport=httpx.MockTransport(_handler(visto)))
        return gw

    with TestClient(create_app(s, gateway_factory=fabrica), base_url="http://127.0.0.1") as c:
        r = c.post("/chat", json={"texto": "tudo bem?"}, headers=AUTH)
        assert "resposta do modelo local" in r.text
        p = c.get("/painel", headers=AUTH).json()
        priv = c.get("/privacidade", headers=AUTH).json()
    assert visto and all("tools" not in corpo for corpo in visto)
    assert {x["provider"] for x in p["provedores"]} == {"gateway:padrão", "ollama"}
    assert "ollama" not in priv["provedores"]  # o modelo local não é saída
    assert saidas._destino is None


class _Resp:
    def __init__(self, dados):
        self._dados = dados

    def json(self):
        return self._dados


def test_doctor_confere_o_ollama_e_o_modelo_baixado(tmp_path):
    s = Settings(data_dir=tmp_path, local_model="qwen3.5:4b", _env_file=None)
    pedidos: list[str] = []

    def tem(url):
        pedidos.append(url)
        return _Resp({"models": [{"name": "qwen3.5:4b"}, {"name": "gemma3:4b"}]})

    (ok,) = _modelo_local(s, get=tem)
    assert ok.nivel == "ok" and pedidos == ["http://127.0.0.1:11434/api/tags"]
    (falta,) = _modelo_local(s, get=lambda u: _Resp({"models": [{"name": "gemma3:4b"}]}))
    assert falta.nivel == "aviso" and "ollama pull qwen3.5:4b" in falta.detalhe

    def caiu(url):
        raise httpx.ConnectError("recusada")

    (fora,) = _modelo_local(s, get=caiu)
    assert fora.nivel == "aviso" and "não respondeu" in fora.detalhe and "§17" in fora.detalhe
    assert _modelo_local(Settings(data_dir=tmp_path, _env_file=None)) == []
