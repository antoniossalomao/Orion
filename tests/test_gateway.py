import json

import httpx
import pytest

from orion.gateway import (
    ChatGateway,
    Endpoint,
    Finish,
    GatewayError,
    TextDelta,
    ToolCallRequest,
)


def sse(*pedacos: dict | str) -> bytes:
    linhas = [f"data: {p if isinstance(p, str) else json.dumps(p)}\n\n" for p in pedacos]
    return "".join(linhas).encode()


def texto(t: str, fim: str | None = None) -> dict:
    return {"choices": [{"delta": {"content": t}, "finish_reason": fim}]}


def resposta(corpo: bytes, status: int = 200, headers: dict | None = None) -> httpx.Response:
    return httpx.Response(
        status, content=corpo, headers={"content-type": "text/event-stream", **(headers or {})}
    )


def gateway(handler, *nomes, relogio=None, **kw) -> ChatGateway:
    eps = [
        Endpoint(n, f"http://{n}.test/v1", f"modelo-{n}", api_key="chave-secreta")
        for n in (nomes or ("a",))
    ]
    return ChatGateway(
        eps,
        client=httpx.AsyncClient(transport=httpx.MockTransport(handler)),
        clock=relogio or (lambda: 0.0),
        **kw,
    )


async def coletar(gw: ChatGateway, **kw):
    return [e async for e in gw.stream([{"role": "user", "content": "oi"}], **kw)]


async def test_streaming_de_texto_e_finish():
    visto = {}

    def handler(req: httpx.Request) -> httpx.Response:
        visto["url"], visto["auth"] = str(req.url), req.headers["authorization"]
        visto["corpo"] = json.loads(req.content)
        return resposta(sse(texto("Olá, "), texto("Antônio.", "stop"), "[DONE]"))

    eventos = await coletar(gateway(handler))
    assert eventos == [TextDelta("Olá, "), TextDelta("Antônio."), Finish("stop", "a", "modelo-a")]
    assert visto["url"] == "http://a.test/v1/chat/completions"
    assert visto["auth"] == "Bearer chave-secreta"
    assert visto["corpo"]["stream"] is True and "tools" not in visto["corpo"]


async def test_tool_calls_acumulam_fragmentos_e_parseiam_json():
    def handler(_):
        return resposta(
            sse(
                {
                    "choices": [
                        {
                            "delta": {
                                "tool_calls": [
                                    {
                                        "index": 0,
                                        "id": "c1",
                                        "function": {"name": "buscar_", "arguments": '{"consu'},
                                    }
                                ]
                            }
                        }
                    ]
                },
                {
                    "choices": [
                        {
                            "delta": {
                                "tool_calls": [
                                    {
                                        "index": 0,
                                        "function": {"name": "memoria", "arguments": 'lta": "x"}'},
                                    }
                                ]
                            }
                        }
                    ]
                },
                {
                    "choices": [
                        {
                            "delta": {
                                "tool_calls": [
                                    {
                                        "index": 1,
                                        "id": "c2",
                                        "function": {"name": "listar_fatos", "arguments": ""},
                                    }
                                ]
                            },
                            "finish_reason": "tool_calls",
                        }
                    ]
                },
                "[DONE]",
            )
        )

    eventos = await coletar(gateway(handler), tools=[{"type": "function"}])
    assert eventos[:2] == [
        ToolCallRequest("c1", "buscar_memoria", {"consulta": "x"}),
        ToolCallRequest("c2", "listar_fatos", {}),
    ]
    assert eventos[-1] == Finish("tool_calls", "a", "modelo-a")


async def test_argumentos_invalidos_viram_erro_sem_derrubar():
    def handler(_):
        return resposta(
            sse(
                {
                    "choices": [
                        {
                            "delta": {
                                "tool_calls": [
                                    {
                                        "index": 0,
                                        "id": "c",
                                        "function": {"name": "f", "arguments": "{quebrado"},
                                    }
                                ]
                            }
                        }
                    ]
                },
                "[DONE]",
            )
        )

    (chamada, _) = await coletar(gateway(handler))
    assert chamada.error and "inválidos" in chamada.error and chamada.arguments == {}


async def test_fallback_quando_o_primeiro_falha():
    chamados = []

    def handler(req):
        chamados.append(req.url.host)
        if req.url.host == "a.test":
            return httpx.Response(503, text="indisponível")
        return resposta(sse(texto("do b", "stop"), "[DONE]"))

    eventos = await coletar(gateway(handler, "a", "b"))
    assert chamados == ["a.test", "b.test"]
    assert eventos[-1] == Finish("stop", "b", "modelo-b")


async def test_429_poe_em_quarentena_e_volta_depois():
    t = {"agora": 0.0}
    chamados = []

    def handler(req):
        chamados.append(req.url.host)
        if req.url.host == "a.test" and t["agora"] < 100:
            return httpx.Response(429, text="cota", headers={"retry-after": "30"})
        return resposta(sse(texto("ok", "stop"), "[DONE]"))

    gw = gateway(handler, "a", "b", relogio=lambda: t["agora"])
    await coletar(gw)
    assert chamados == ["a.test", "b.test"]
    chamados.clear()
    t["agora"] = 10  # ainda em quarentena: nem tenta o a
    await coletar(gw)
    assert chamados == ["b.test"]
    chamados.clear()
    t["agora"] = 101  # voltou
    await coletar(gw)
    assert chamados == ["a.test"]


async def test_resposta_vazia_tenta_o_proximo():
    def handler(req):
        if req.url.host == "a.test":
            return resposta(sse("[DONE]"))
        return resposta(sse(texto("b", "stop"), "[DONE]"))

    assert (await coletar(gateway(handler, "a", "b")))[0] == TextDelta("b")


async def test_todos_falham_levanta_com_as_tentativas():
    def handler(req):
        return httpx.Response(500, text="erro")

    with pytest.raises(GatewayError) as e:
        await coletar(gateway(handler, "a", "b"))
    assert [n for n, _ in e.value.tentativas] == ["a", "b"] and "500" in e.value.tentativas[0][1]


async def test_timeout_e_erro_de_rede_tentam_o_proximo():
    def handler(req):
        if req.url.host == "a.test":
            raise httpx.ConnectTimeout("lento")
        return resposta(sse(texto("b", "stop"), "[DONE]"))

    assert (await coletar(gateway(handler, "a", "b")))[-1].endpoint == "b"


async def test_queda_no_meio_do_texto_nao_troca_de_endpoint():
    class Quebra(httpx.AsyncByteStream):
        async def __aiter__(self):
            yield sse(texto("começo "))
            raise httpx.ReadError("conexão caiu")

    chamados = []

    def handler(req):
        chamados.append(req.url.host)
        return httpx.Response(200, stream=Quebra(), headers={"content-type": "text/event-stream"})

    visto: list = []

    async def consumir() -> None:
        async for e in gateway(handler, "a", "b").stream([{"role": "user", "content": "oi"}]):
            visto.append(e)

    with pytest.raises(GatewayError, match="interrompeu"):
        await consumir()
    assert visto == [TextDelta("começo ")] and chamados == ["a.test"]


async def test_chave_nao_vai_para_o_log(caplog):
    def handler(req):
        return httpx.Response(500, text="x")

    with caplog.at_level("WARNING"), pytest.raises(GatewayError):
        await coletar(gateway(handler))
    assert "chave-secreta" not in caplog.text


def test_precisa_de_endpoint():
    with pytest.raises(ValueError):
        ChatGateway([])
