"""
routers/memory.py — endpoints de memória/grafo.

Extraído de cerebro_maestro.py na reorganização OOP (08/2026). Comportamento
idêntico ao original — só move código de lugar, não muda lógica.
"""
import datetime

import httpx
from fastapi import APIRouter

import config as cfg


class MemoryRouter:
    """Agrupa os endpoints de memória de longo prazo: grafo SurrealDB,
    categorias e export da conversa atual.

    Dependências injetadas via construtor — `rag` (RAGEngine), `session`
    (SessionManager) e o getter de `cerebro_ativo` (muda em runtime em
    cerebro_maestro.py, por isso é getter e não valor).
    """

    def __init__(self, *, rag, session, get_cerebro_ativo, colecao, log):
        self._rag = rag
        self._session = session
        self._get_cerebro_ativo = get_cerebro_ativo
        self._colecao = colecao
        self._log = log

        self.router = APIRouter()
        self.router.add_api_route("/grafo/completo", self.grafo_completo, methods=["GET"])
        self.router.add_api_route("/memoria/categorias", self.memoria_categorias, methods=["GET"])
        self.router.add_api_route("/exportar", self.exportar_conversa, methods=["GET"])

    async def grafo_completo(self, limite: int = 300):
        """Retorna o grafo completo de memória para visualização 3D.
        Formato {nodes, links} compatível com 3d-force-graph."""
        from surreal_client import surreal
        try:
            async def _sql(q):
                result = await surreal.query_result(q)
                return result if isinstance(result, list) else []

            eventos  = await _sql(f"SELECT id, texto, ator, timestamp FROM evento ORDER BY timestamp DESC LIMIT {limite};")
            sobre    = await _sql(f"SELECT in, out FROM sobre LIMIT {limite * 4};")
            precedeu = await _sql(f"SELECT in, out FROM precedeu LIMIT {limite};")

            nodes, links = [], []
            ids_vistos: set[str] = set()

            for ev in eventos:
                eid = str(ev.get("id", ""))
                if eid and eid not in ids_vistos:
                    nodes.append({
                        "id": eid, "tipo": "evento", "label": (ev.get("texto") or eid)[:90],
                        "ator": ev.get("ator", ""), "ts": ev.get("timestamp", ""),
                    })
                    ids_vistos.add(eid)

            # Tópicos extraídos das relações "sobre" (não há tabela topico separada)
            for a in sobre:
                src = str(a.get("in", ""))
                dst = str(a.get("out", ""))
                if not src or not dst:
                    continue
                links.append({"source": src, "target": dst, "rel": "sobre"})
                if dst not in ids_vistos and dst.startswith("topico:"):
                    label = dst.split(":", 1)[-1]
                    nodes.append({"id": dst, "tipo": "topico", "label": label})
                    ids_vistos.add(dst)

            for a in precedeu:
                src, dst = str(a.get("in", "")), str(a.get("out", ""))
                if src and dst:
                    links.append({"source": src, "target": dst, "rel": "precedeu"})

            # "sobre"/"precedeu" podem referenciar eventos fora da janela dos
            # `limite` mais recentes — link órfão apontando pra um nó fora de
            # `nodes` crasha o 3d-force-graph no frontend. Descarta na origem.
            links = [l for l in links if l["source"] in ids_vistos and l["target"] in ids_vistos]

            return {"nodes": nodes, "links": links,
                    "total_nodes": len(nodes), "total_links": len(links)}
        except Exception as e:
            self._log(f"[GRAFO/COMPLETO] Erro: {e}")
            return {"nodes": [], "links": [], "erro": str(e)}

    async def memoria_categorias(self):
        """Composição da base de conhecimento por categoria, via facet do Qdrant
        (distinct counts exatos e eficientes). Mostra o que o Orion 'sabe'."""
        if not self._get_cerebro_ativo() or not self._rag.active:
            return {"erro": "Cérebro não inicializado"}
        total_colecao = 0
        try:
            total_colecao = self._rag.qdrant_client.count(self._colecao).count
        except Exception:
            pass
        por_categoria = {}
        try:
            async with httpx.AsyncClient() as client:
                resp = await client.post(
                    f"{cfg.QDRANT_URL}/collections/{self._colecao}/facet",
                    json={"key": "categoria", "limit": 50, "exact": False},
                    timeout=30,
                )
                resp.raise_for_status()
                hits = resp.json().get("result", {}).get("hits", [])
                por_categoria = {h["value"]: h["count"] for h in hits}
        except Exception as e:
            return {"total_colecao": total_colecao, "erro": f"facet falhou: {e}"}
        return {"total_colecao": total_colecao, "categorias_distintas": len(por_categoria),
                "por_categoria": por_categoria}

    async def exportar_conversa(self):
        """Exporta o histórico em memória como markdown — pra salvar/compartilhar a sessão."""
        hist = await self._session.snapshot()
        linhas = [f"# Conversa com o Orion — {datetime.datetime.now().strftime('%d/%m/%Y %H:%M')}", ""]
        for m in hist:
            autor = "**Antônio**" if m["role"] == "user" else "**Orion**"
            linhas.append(f"{autor}: {m['content']}")
            linhas.append("")
        return {"markdown": "\n".join(linhas), "total_msgs": len(hist)}
