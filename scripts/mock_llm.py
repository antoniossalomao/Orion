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

MATRIZ = """Fórmulas em bloco:

$$
x = \\frac{-b \\pm \\sqrt{b^2 - 4ac}}{2a}
$$

$$
\\int_{0}^{\\infty} e^{-x^2}\\,dx = \\frac{\\sqrt{\\pi}}{2}, \\qquad \\sum_{k=1}^{n} \\frac{1}{k^2} \\to \\frac{\\pi^2}{6}
$$

$$
A = \\begin{pmatrix} a & b \\\\ c & d \\end{pmatrix}, \\quad f(x) = \\begin{cases} x^2 & x \\geq 0 \\\\ -x & x < 0 \\end{cases}
$$

$$
\\left( \\frac{a+b}{c} \\right)^{2} = \\lim_{n \\to \\infty} \\hat{y}_n
$$

Fim."""

HTML = """Página de teste:

```html
<!doctype html>
<html><head><title>x</title><style>h1{color:red}</style><script>alert(1)</script></head>
<body>
<h1>Painel do <em>Orion</em></h1>
<p>Texto com <strong>negrito</strong>, <a href="https://example.com">um link</a> e <code>codigo</code>.</p>
<ul><li>Primeiro</li><li>Segundo <b>item</b></li></ul>
<table><tr><th>Item</th><th>Valor</th></tr><tr><td>RAM</td><td>1,2 GB</td></tr></table>
<blockquote>Uma citação.</blockquote>
<button>Enviar</button>
</body></html>
```

Pronto."""

IMAGEM = """Aqui está a imagem:

![dentro da pasta do Orion](/tmp/claude-0/orionhome30/Documents/Orion/permitida.png)

![fora da pasta](/tmp/claude-0/imgs/big.png)

![rede](\\\\atacante.example\\share\\x.png)

E uma que não existe:

![arquivo ausente](/tmp/claude-0/imgs/nao-existe.png)

Gerada pelo núcleo:

![gerada](/imagens/img-1761230000-aabbccdd.png)

Fim."""

COLADO = """Imagem logo depois do texto, sem linha em branco:
![gerada](/imagens/img-1761230000-aabbccdd.png)
Fórmula colada no texto:
$$ \\ddot{x} = a \\quad \\dot{x} = v $$
Fim."""

FECHO = """Fórmula com o fecho colado:
$$
x^2 + y^2 = z^2$$

Depois da fórmula, um parágrafo normal.

```python
print("bloco depois")
```

![gerada](/imagens/img-1761230000-aabbccdd.png)
Fim."""

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
            out = "Arquivo escrito, tudo certo." if tools_back else (LONGO if "CODIGO" in (txt or "") else FORMULA if "FORMULA" in (txt or "") else DIAGRAMA if "DIAGRAMA" in (txt or "") else MATRIZ if "MATRIZ" in (txt or "") else HTML if "HTMLX" in (txt or "") else IMAGEM if "IMAGEM" in (txt or "") else COLADO if "COLADO" in (txt or "") else FECHO if "FECHO" in (txt or "") else MD)
            for i in range(0, len(out), 6):
                ev(chunk(content=out[i:i + 6])); time.sleep(0.02)
            ev(chunk(finish="stop"))
        self.wfile.write(b"data: [DONE]\n\n"); self.wfile.flush()

ThreadingHTTPServer(("127.0.0.1", int(sys.argv[1]) if len(sys.argv) > 1 else 9099), H).serve_forever()
