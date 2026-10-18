"""Servidor MCP de teste (JSON-RPC por stdin/stdout, só a biblioteca padrão) para os testes de
paridade do cliente MCP do orion-core.

Reúne as ferramentas do `fake_server.py` (SDK oficial) e os controles do `servidor_cru.py` dos
testes Python. Ambiente: PROTOCOLO (versão anunciada no initialize), LOG_CANCELADO (arquivo onde
cada `notifications/cancelled` vira uma linha), COM_RECURSOS=1 (recursos e prompts), N_FERRAMENTAS
(ferramentas extras `extra_NNN`).
"""

import json
import os
import sys
import threading
import time

sys.stdin.reconfigure(encoding="utf-8")
sys.stdout.reconfigure(encoding="utf-8")

PROTOCOLO = os.environ.get("PROTOCOLO", "2025-11-25")
LOG = os.environ.get("LOG_CANCELADO")
EXTRAS = int(os.environ.get("N_FERRAMENTAS", "0"))
trava = threading.Lock()


def enviar(msg):
    with trava:
        sys.stdout.write(json.dumps(msg) + "\n")
        sys.stdout.flush()


def esquema(props=None, obrigatorios=()):
    return {"type": "object", "properties": props or {}, "required": list(obrigatorios)}


def ferramentas():
    base = [
        (
            "somar",
            "Soma dois números.",
            esquema({"a": {"type": "integer"}, "b": {"type": "integer"}}, ["a", "b"]),
        ),
        (
            "ler_texto",
            "Devolve o caminho recebido (o servidor de verdade leria o arquivo).",
            esquema({"path": {"type": "string"}}, ["path"]),
        ),
        (
            "apagar",
            "Apaga (de mentira) um caminho.",
            esquema({"path": {"type": "string"}}, ["path"]),
        ),
        ("falha", "Sempre falha.", esquema()),
        (
            "dormir",
            "Demora `segundos` para responder (30 s sem o argumento).",
            esquema({"segundos": {"type": "number"}}),
        ),
        ("sair", "Derruba o processo do servidor (para testar a reconexão).", esquema()),
        ("nome_estranho_com_espacos_e_acentos_ção", "Nome que precisa ser sanitizado.", esquema()),
        ("eco", "Devolve o texto.", esquema({"texto": {"type": "string"}})),
        ("ambiente", "Lista as variáveis ORION_* e de chaves que o processo enxerga.", esquema()),
    ]
    tools = [{"name": n, "description": d, "inputSchema": s} for n, d, s in base]
    tools.extend(
        {
            "name": f"extra_{i:03d}",
            "description": (
                f"Ferramenta extra número {i} para {['clima', 'agenda', 'git', 'arquivo'][i % 4]}."
            ),
            "inputSchema": esquema(),
        }
        for i in range(EXTRAS)
    )
    return tools


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


def texto(id_, t, erro=False):
    r = {"content": [{"type": "text", "text": str(t)}]}
    if erro:
        r["isError"] = True
    enviar({"jsonrpc": "2.0", "id": id_, "result": r})


def chamar(id_, nome, a):
    if nome == "somar":
        texto(id_, int(a["a"]) + int(a["b"]))
    elif nome == "ler_texto":
        texto(id_, f"conteudo de {a['path']}")
    elif nome == "apagar":
        texto(id_, f"apagado {a['path']}")
    elif nome == "falha":
        texto(id_, "Error executing tool falha: boom", erro=True)
    elif nome == "dormir":
        time.sleep(float(a.get("segundos", 30)))
        texto(id_, "acordei")
    elif nome == "sair":
        os._exit(1)
    elif nome == "eco":
        texto(id_, a.get("texto", "ok"))
    elif nome == "ambiente":
        texto(
            id_,
            json.dumps(
                sorted(k for k in os.environ if k.startswith(("ORION_", "OPENAI", "CHAVE")))
            ),
        )
    else:
        texto(id_, "ok")


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
                    "serverInfo": {"name": "fake", "version": "0"},
                },
            }
        )
    elif metodo == "tools/list":
        enviar({"jsonrpc": "2.0", "id": id_, "result": {"tools": ferramentas()}})
    elif metodo == "tools/call":
        chamar(id_, p.get("name"), p.get("arguments") or {})
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
        t = (p.get("arguments") or {}).get("texto", "")
        enviar(
            {
                "jsonrpc": "2.0",
                "id": id_,
                "result": {
                    "description": "Revisa",
                    "messages": [
                        {"role": "user", "content": {"type": "text", "text": f"Revise: {t}"}}
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
    # tools/call pode demorar: cada requisição numa thread, como um servidor de verdade
    threading.Thread(target=responder, args=(json.loads(linha),), daemon=True).start()
