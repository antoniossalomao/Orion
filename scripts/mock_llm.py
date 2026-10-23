"""LLM falso compatível com a API OpenAI (/v1/chat/completions), só para testar a UI do Orion."""
import json, os, sys, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

MD = """Resposta do **modelo de teste**. Aqui vai um resumo:

1. Primeiro item com `código inline`
2. Segundo item em *itálico*
3. Terceiro item com [um link](https://example.com)

| Coluna | Valor |
|---|---|
| RAM | 1,2 GB |
| Disco | 300 MB |

```rust
fn main() {
    println!("olá, Orion");
}
```

> Uma citação para conferir o estilo.

Fim da resposta."""

LONGO = """Aqui vai o script completo:

```python
import sys


def somar(a, b):
    return a + b


if __name__ == "__main__":
    print(somar(int(sys.argv[1]), int(sys.argv[2])))
```

Salve em `soma.py` e rode com dois números."""


DIAGRAMA = """Fluxo do pedido:

```mermaid
flowchart TD
    A[Usuário pede] --> B{Precisa de ferramenta?}
    B -->|sim| C[Pedir aprovação]
    B -->|não| D(Responder)
    C --> E[Executar]
    E --> D
    D -.-> F((Fim))
```

Pronto."""

FORMULA = """A energia é $E = mc^2$ e a taxa $\\alpha_1 \\leq \\frac{a}{b}$ fica abaixo de 5. Custa $5 e $6 reais.

$$
\\sum_{i=1}^{n} x_i \\times \\sqrt{2} \\approx \\infty
$$

Fim."""


def chunk(content=None, tool=None, finish=None):
    d = {}
    if content is not None: d["content"] = content
    if tool is not None: d["tool_calls"] = [tool]
    return {"id": "x", "object": "chat.completion.chunk", "model": "mock", "choices": [{"index": 0, "delta": d, "finish_reason": finish}]}

class H(BaseHTTPRequestHandler):
    def log_message(self, *a): pass
    def do_GET(self):
        self.send_response(200); self.send_header("Content-Type", "application/json"); self.end_headers()
        self.wfile.write(json.dumps({"data": [{"id": "mock"}]}).encode())
    def do_POST(self):
        n = int(self.headers.get("Content-Length") or 0)
        body = json.loads(self.rfile.read(n) or b"{}")
        msgs = body.get("messages", [])
        last = next((m for m in reversed(msgs) if m.get("role") == "user"), {})
        txt = last.get("content") if isinstance(last.get("content"), str) else json.dumps(last.get("content"))
        tools_back = any(m.get("role") == "tool" for m in msgs[-2:])
        if not body.get("stream"):
            self.send_response(200); self.send_header("Content-Type", "application/json"); self.end_headers()
            self.wfile.write(json.dumps({"id": "x", "model": "mock", "choices": [{"index": 0, "message": {"role": "assistant", "content": "Conversa de teste"}, "finish_reason": "stop"}], "usage": {"prompt_tokens": 5, "completion_tokens": 3, "total_tokens": 8}}).encode()); return
        self.send_response(200); self.send_header("Content-Type", "text/event-stream"); self.end_headers()
        def ev(o): self.wfile.write(b"data: " + json.dumps(o).encode() + b"\n\n"); self.wfile.flush()
        if "ESCREVER" in (txt or "") and not tools_back:
            args = json.dumps({"path": os.environ.get("MOCK_PATH", "/tmp/orion-teste-aprovado.txt"), "conteudo": "ok do teste"})
            ev(chunk(tool={"index": 0, "id": "call_1", "type": "function", "function": {"name": "escrever_arquivo", "arguments": args}}))
            ev(chunk(finish="tool_calls"))
        else:
            out = "Arquivo escrito, tudo certo." if tools_back else (LONGO if "CODIGO" in (txt or "") else FORMULA if "FORMULA" in (txt or "") else DIAGRAMA if "DIAGRAMA" in (txt or "") else MD)
            for i in range(0, len(out), 6):
                ev(chunk(content=out[i:i + 6])); time.sleep(0.02)
            ev(chunk(finish="stop"))
        self.wfile.write(b"data: [DONE]\n\n"); self.wfile.flush()

ThreadingHTTPServer(("127.0.0.1", int(sys.argv[1]) if len(sys.argv) > 1 else 9099), H).serve_forever()
