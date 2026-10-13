"""Servidor MCP de teste escrito à mão (JSON-RPC por stdin/stdout), para o que o SDK não deixa
forçar: versão de protocolo, registro de `notifications/cancelled`, recursos e prompts.

Ambiente: PROTOCOLO (versão anunciada no initialize), LOG_CANCELADO (arquivo onde cada
`notifications/cancelled` recebido vira uma linha), COM_RECURSOS=1, N_FERRAMENTAS (extras).
"""

import json
import os
import sys
import threading
import time

PROTOCOLO = os.environ.get("PROTOCOLO", "2025-11-25")
LOG = os.environ.get("LOG_CANCELADO")
EXTRAS = int(os.environ.get("N_FERRAMENTAS", "0"))
trava = threading.Lock()


def enviar(msg):
    with trava:
        sys.stdout.write(json.dumps(msg) + "\n")
        sys.stdout.flush()


def ferramentas():
    base = [
        {
            "name": "dormir",
            "description": "Dorme 30 s.",
            "inputSchema": {"type": "object", "properties": {}},
        },
        {
            "name": "eco",
            "description": "Devolve o texto.",
            "inputSchema": {"type": "object", "properties": {"texto": {"type": "string"}}},
        },
    ]
    for i in range(EXTRAS):
        base.append(
            {
                "name": f"extra_{i:03d}",
                "description": f"Ferramenta extra número {i} para {['clima', 'agenda', 'git', 'arquivo'][i % 4]}.",
                "inputSchema": {"type": "object", "properties": {}},
            }
        )
    return base


RECURSOS = [
    {"uri": "file:///docs/guia.md", "name": "guia", "mimeType": "text/markdown"},
    {"uri": "file:///segredo/chave.txt", "name": "chave", "mimeType": "text/plain"},
    {"uri": "file:///docs/grande.txt", "name": "grande", "mimeType": "text/plain"},
]
CONTEUDO = {
    "file:///docs/guia.md": "# Guia\nconteudo do guia",
    "file:///segredo/chave.txt": "SEGREDO",
    "file:///docs/grande.txt": "x" * 300_000,
}
PROMPTS = [
    {
        "name": "revisar",
        "description": "Revisa um texto.",
        "arguments": [{"name": "texto", "required": True}],
    }
]


def responder(req):
    metodo, id_, p = req.get("method"), req.get("id"), req.get("params") or {}
    if metodo == "notifications/cancelled":
        if LOG:
            with open(LOG, "a", encoding="utf-8") as f:
                f.write(json.dumps(p) + "\n")
        return
    if id_ is None:
        return
    if metodo == "initialize":
        caps = {"tools": {}}
        if os.environ.get("COM_RECURSOS"):
            caps.update({"resources": {}, "prompts": {}})
        enviar(
            {
                "jsonrpc": "2.0",
                "id": id_,
                "result": {
                    "protocolVersion": PROTOCOLO,
                    "capabilities": caps,
                    "serverInfo": {"name": "cru", "version": "0"},
                },
            }
        )
    elif metodo == "tools/list":
        enviar({"jsonrpc": "2.0", "id": id_, "result": {"tools": ferramentas()}})
    elif metodo == "tools/call":
        if p.get("name") == "dormir":
            time.sleep(30)
        texto = (p.get("arguments") or {}).get("texto", "ok")
        enviar(
            {
                "jsonrpc": "2.0",
                "id": id_,
                "result": {"content": [{"type": "text", "text": str(texto)}]},
            }
        )
    elif metodo == "resources/list":
        enviar({"jsonrpc": "2.0", "id": id_, "result": {"resources": RECURSOS}})
    elif metodo == "resources/read":
        uri = p.get("uri")
        if uri not in CONTEUDO:
            enviar(
                {"jsonrpc": "2.0", "id": id_, "error": {"code": -32002, "message": "não existe"}}
            )
        else:
            enviar(
                {
                    "jsonrpc": "2.0",
                    "id": id_,
                    "result": {
                        "contents": [{"uri": uri, "mimeType": "text/plain", "text": CONTEUDO[uri]}]
                    },
                }
            )
    elif metodo == "prompts/list":
        enviar({"jsonrpc": "2.0", "id": id_, "result": {"prompts": PROMPTS}})
    elif metodo == "prompts/get":
        texto = (p.get("arguments") or {}).get("texto", "")
        enviar(
            {
                "jsonrpc": "2.0",
                "id": id_,
                "result": {
                    "description": "Revisa",
                    "messages": [
                        {"role": "user", "content": {"type": "text", "text": f"Revise: {texto}"}}
                    ],
                },
            }
        )
    elif metodo == "ping":
        enviar({"jsonrpc": "2.0", "id": id_, "result": {}})
    else:
        enviar(
            {
                "jsonrpc": "2.0",
                "id": id_,
                "error": {"code": -32601, "message": "método desconhecido"},
            }
        )


for linha in sys.stdin:
    if not linha.strip():
        continue
    req = json.loads(linha)
    # tools/call pode demorar: cada requisição numa thread, como um servidor de verdade
    threading.Thread(target=responder, args=(req,), daemon=True).start()
