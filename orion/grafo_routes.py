"""Rotas do grafo da memória, das relações do sono e da busca global (E5.4 a E5.7).

Tudo só lê, exceto apagar uma relação do sono, que é um clique do próprio Antônio."""

from __future__ import annotations

from itertools import pairwise
from typing import Any

from fastapi import APIRouter, Depends, HTTPException, Query, Request

from .grafo import achar_no, caminho, montar
from .memory.scope import data_scope


def _escopo(projeto: str | None, project_id: str | None) -> str | None:
    return projeto or project_id or None


def router(require_admin) -> APIRouter:
    api = APIRouter(dependencies=[Depends(require_admin)])

    @api.get("/grafo/completo")
    def grafo_completo(
        request: Request,
        limite: int = Query(500, ge=10, le=3000),
        pasta: str = Query("", max_length=200),
        projeto: str | None = None,
        project_id: str | None = None,
    ) -> dict[str, Any]:
        """Os nós e as ligações que `views/memory.js` desenha em 3D. `limite` corta nos mais
        ligados e mais recentes (`truncado` avisa); `pasta` filtra as notas do vault."""
        s = request.app.state.orion
        return montar(s.memory, project_id=_escopo(projeto, project_id), limite=limite, pasta=pasta)

    @api.get("/grafo/no")
    def grafo_no(
        request: Request,
        q: str = Query(min_length=1, max_length=200),
        projeto: str | None = None,
        project_id: str | None = None,
    ) -> dict[str, Any]:
        """ "Me leva até X": o nó que melhor casa com o nome dito."""
        s = request.app.state.orion
        grafo = montar(s.memory, project_id=_escopo(projeto, project_id), limite=None)
        return {"no": achar_no(grafo["nodes"], q)}

    @api.get("/grafo/caminho")
    def grafo_caminho(
        request: Request,
        de: str = Query(min_length=1, max_length=200),
        para: str = Query(min_length=1, max_length=200),
        projeto: str | None = None,
        project_id: str | None = None,
    ) -> dict[str, Any]:
        """ "O que liga X a Y?": o menor caminho entre os dois nós. `caminho` vem vazio se os dois
        existem mas não se conectam; 404 se algum dos nomes não casa com nó nenhum."""
        s = request.app.state.orion
        grafo = montar(s.memory, project_id=_escopo(projeto, project_id), limite=None)
        a, b = achar_no(grafo["nodes"], de), achar_no(grafo["nodes"], para)
        if a is None or b is None:
            return {"de": a, "para": b, "caminho": [], "ligacoes": []}
        rota = caminho(grafo, a["id"], b["id"])
        por_id = {n["id"]: n for n in grafo["nodes"]}
        pares = set(pairwise(rota))
        return {
            "de": a,
            "para": b,
            "caminho": [por_id[i] for i in rota],
            "ligacoes": [
                lk
                for lk in grafo["links"]
                if (lk["source"], lk["target"]) in pares or (lk["target"], lk["source"]) in pares
            ],
        }

    @api.get("/memoria/relacoes")
    def relacoes(request: Request, limite: int = Query(500, ge=1, le=5000)) -> list[dict[str, Any]]:
        """As relações que o ciclo de sono tirou dos fatos (arestas `kind=sono`)."""
        return request.app.state.orion.ops.list_edges("sono", limite)

    @api.delete("/memoria/relacoes")
    def apagar_relacao(
        request: Request,
        src: str = Query(min_length=1, max_length=200),
        rel: str = Query(min_length=1, max_length=100),
        dst: str = Query(min_length=1, max_length=200),
    ) -> dict[str, bool]:
        """Apaga UMA relação do sono (só `kind=sono`: o grafo importado e o do vault não saem
        por aqui)."""
        if not request.app.state.orion.ops.remove_edge(src, rel, dst, kind="sono"):
            raise HTTPException(404, "relation_not_found")
        return {"ok": True}

    @api.get("/busca")
    def busca(
        request: Request,
        q: str = Query(min_length=2, max_length=200),
        projeto: str | None = None,
        project_id: str | None = None,
    ) -> dict[str, Any]:
        """Busca global (N1): conversas, fatos, documentos, memória da tela e resultados gerados,
        até 5 de cada, no escopo pedido (pessoal ou um projeto, regra 52)."""
        s = request.app.state.orion
        escopo = _escopo(projeto, project_id)
        grupos: list[dict[str, Any]] = []

        achadas, _ = s.memory.search_sessions("web", q, limit=60)
        conversas = [
            {"id": ses.id, "titulo": ses.title or "Conversa", "trecho": trecho}
            for ses, trecho in achadas
            if ses.project_id == escopo
        ][:5]
        grupos.append({"fonte": "conversas", "titulo": "Conversas", "itens": conversas})

        with data_scope(escopo, include_personal=False):
            fatos = [
                {"id": f.id, "texto": f.text[:200], "antigo": f.old}
                for f in s.memory.search_facts(q, limit=5)
            ]
            docs = [
                {
                    "fonte": h.source.split("/", 1)[-1]
                    if h.source.startswith("project:")
                    else h.source,
                    "trecho": h.text[:240],
                }
                for h in s.memory.search(q, k=5, kinds=("chunk",))
            ]
        grupos.append({"fonte": "fatos", "titulo": "Fatos", "itens": fatos})
        grupos.append({"fonte": "documentos", "titulo": "Documentos e notas", "itens": docs})

        if escopo is None:  # a memória da tela é pessoal
            tela = [
                {"id": t["id"], "ts": t["ts"], "janela": t["titulo"], "trecho": t["trecho"]}
                for t in s.memory.search_screen(q, limit=5)
            ]
            grupos.append({"fonte": "tela", "titulo": "Memória da tela", "itens": tela})

        gerados = s.memory.query(
            "SELECT id, name, kind, version FROM library_results WHERE project_id IS ?"
            " AND lower(name) LIKE ? ESCAPE '\\' ORDER BY created_at DESC LIMIT 5",
            (
                escopo,
                "%" + q.lower().replace("\\", "\\\\").replace("%", "\\%").replace("_", "\\_") + "%",
            ),
        )
        grupos.append(
            {
                "fonte": "resultados",
                "titulo": "Arquivos gerados",
                "itens": [
                    {"id": r["id"], "nome": r["name"], "tipo": r["kind"], "versao": r["version"]}
                    for r in gerados
                ],
            }
        )
        return {"q": q, "grupos": [g for g in grupos if g["itens"]]}

    return api
