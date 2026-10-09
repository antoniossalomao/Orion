"""Gateway de modelos de mentira (API compatível com a da OpenAI, em streaming) para o teste
do Orion de verdade: `python -m tests.front_e2e.fake_gateway` (porta em FAKE_GATEWAY_PORT).

Roteiro pelo último texto do usuário: "apague"/"escreva" pede `executar_comando` (a política manda
pedir aprovação), "memoria" pede `buscar_memoria` (leitura, roda direto); qualquer outro vira
uma resposta curta. Depois de uma ferramenta, relata o resultado.
"""

from __future__ import annotations

import json
import os

import uvicorn
from fastapi import FastAPI, Request, Response
from fastapi.responses import StreamingResponse


def _sse(delta: dict, fim: str | None = None) -> str:
    return "data: " + json.dumps({"choices": [{"delta": delta, "finish_reason": fim}]}) + "\n\n"


def _texto(m: dict) -> str:
    c = m.get("content")
    if isinstance(c, list):
        return " ".join(p.get("text", "") for p in c if p.get("type") == "text")
    return str(c or "")


def create_app() -> FastAPI:
    app = FastAPI()
    app.state.pedidos = []
    app.state.ferramentas = []  # nomes das `tools` de cada pedido (pânico e modelo local: regras 48 e 49)
    # FAKE_FALHAS="3,7": esses pedidos (1, 2, 3...) respondem 502, para o painel contar falhas
    falhas = {int(x) for x in os.environ.get("FAKE_FALHAS", "").split(",") if x.strip()}

    @app.get("/pedidos")
    def pedidos():
        return app.state.pedidos

    @app.get("/ferramentas")
    def ferramentas():
        return app.state.ferramentas

    @app.post("/v1/chat/completions")
    async def completions(req: Request):
        corpo = await req.json()
        msgs = corpo["messages"]
        app.state.pedidos.append(msgs)
        app.state.ferramentas.append(
            None if "tools" not in corpo else [t["function"]["name"] for t in corpo["tools"]]
        )
        if len(app.state.pedidos) in falhas:
            return Response("fora do ar", status_code=502)
        ultima = msgs[-1]

        async def gerar():
            # depois de uma ferramenta: no fluxo normal chega a mensagem `tool`; na retomada de
            # uma aprovação o Orion injeta uma nota "[SISTEMA]" como fala do usuário
            if ultima["role"] == "tool" and "aguardando_aprovacao" in _texto(ultima):
                yield _sse({"content": "Aguardando o seu aval."})  # ainda NÃO rodou
                yield _sse({}, "stop")
            elif ultima["role"] == "tool" or "[SISTEMA]" in _texto(ultima):
                resumo = _texto(ultima)[-300:].replace("\n", " ")
                yield _sse({"content": f"Feito. Resultado: {resumo}"})
                yield _sse({}, "stop")
            else:
                texto = _texto(ultima).lower()
                alvo = os.environ.get("FAKE_ALVO", "/tmp/orion-e2e-marca.txt")
                if "apague" in texto or "escreva" in texto:
                    args = json.dumps({"cmd": f"echo aprovado > '{alvo}'"})
                    yield _sse(
                        {
                            "tool_calls": [
                                {
                                    "index": 0,
                                    "id": "call_1",
                                    "function": {"name": "executar_comando", "arguments": args},
                                }
                            ]
                        }
                    )
                    yield _sse({}, "tool_calls")
                elif "mcp some" in texto:
                    args = json.dumps({"a": 2, "b": 3})
                    yield _sse(
                        {
                            "tool_calls": [
                                {
                                    "index": 0,
                                    "id": "call_3",
                                    "function": {"name": "fake__somar", "arguments": args},
                                }
                            ]
                        }
                    )
                    yield _sse({}, "tool_calls")
                elif "mcp apagar" in texto:
                    args = json.dumps({"path": "/tmp/nada"})
                    yield _sse(
                        {
                            "tool_calls": [
                                {
                                    "index": 0,
                                    "id": "call_4",
                                    "function": {"name": "fake__apagar", "arguments": args},
                                }
                            ]
                        }
                    )
                    yield _sse({}, "tool_calls")
                elif "memoria" in texto:
                    args = json.dumps({"consulta": "orion"})
                    yield _sse(
                        {
                            "tool_calls": [
                                {
                                    "index": 0,
                                    "id": "call_2",
                                    "function": {"name": "buscar_memoria", "arguments": args},
                                }
                            ]
                        }
                    )
                    yield _sse({}, "tool_calls")
                else:
                    for pedaco in ("Olá, ", "Antônio. ", "Tudo certo."):
                        yield _sse({"content": pedaco})
                    yield _sse({}, "stop")
            yield "data: [DONE]\n\n"

        return StreamingResponse(gerar(), media_type="text/event-stream")

    return app


app = create_app()

if __name__ == "__main__":
    uvicorn.run(
        app, host="127.0.0.1", port=int(os.environ["FAKE_GATEWAY_PORT"]), log_level="warning"
    )
