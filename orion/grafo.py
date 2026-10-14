"""Grafo da memória (E5.5 e E5.6): fatos, tópicos, falas, relações do sono e notas, e o caminho
mais curto entre duas coisas ("o que liga X a Y?").

O grafo é montado na hora, só de leitura, a partir do que já está no banco:

- `fato:<id>`: um fato (tipo `fato`), no escopo pedido (REGRAS.md: o que é de um projeto fica no
  projeto);
- `topic:<nome>` e `message:<id>`: o grafo importado do legado (tipos `topico` e `evento`);
- `entidade:<texto>`: as coisas que o ciclo de sono ligou (arestas `kind=sono`), tipo `topico`;
- `doc:<fonte>`: notas do vault e documentos enviados (tipo `nota`), ligadas por [[wikilinks]]
  (arestas `kind=vault`, refeitas em cada indexação do vault).

Só a memória pessoal tem arestas (importação, sono e vault); um projeto vê os fatos e os documentos
DELE. O formato é o que `views/memory.js` já espera: `{nodes: [...], links: [...]}`."""

from __future__ import annotations

import re
import unicodedata
from collections import deque
from typing import Any

from .memory.scope import data_scope
from .memory.store import MemoryStore

MAX_ARESTAS = 20_000
LIMITE_PADRAO = 500


def _plano(texto: str) -> str:
    sem = unicodedata.normalize("NFKD", texto)
    return " ".join("".join(c for c in sem if not unicodedata.combining(c)).casefold().split())


def _id_do_no(bruto: str) -> str:
    """Como a aresta do banco aparece no grafo: o que já tem prefixo conhecido fica; o resto (as
    coisas que o sono ligou) vira `entidade:`."""
    if bruto.startswith(("topic:", "message:", "doc:", "fato:")):
        return bruto
    return f"entidade:{bruto}"


def montar(
    memory: MemoryStore,
    *,
    project_id: str | None = None,
    limite: int | None = LIMITE_PADRAO,
    pasta: str = "",
) -> dict[str, Any]:
    """`limite=None`: o grafo todo (para achar caminhos). Com limite, ficam os nós mais ligados e
    os mais recentes; `truncado` diz se algo ficou de fora."""
    nos: dict[str, dict[str, Any]] = {}
    ligacoes: list[dict[str, Any]] = []

    with data_scope(project_id, include_personal=False):
        fatos = memory.facts()
        notas = memory.query(
            "SELECT source, title, indexed_at FROM documents WHERE project_id IS ?"
            " ORDER BY indexed_at DESC LIMIT 3000",
            (project_id,),
        )
    for f in fatos:
        nos[f"fato:{f.id}"] = {
            "id": f"fato:{f.id}",
            "tipo": "fato",
            "label": f.text[:140],
            "ts": f.updated_at * 1000,
            "antigo": f.old,
            "ref": f.id,
        }
    for n in notas:
        fonte = str(n["source"])
        if pasta and not fonte.startswith(pasta.rstrip("/") + "/"):
            continue
        nos[f"doc:{fonte}"] = {
            "id": f"doc:{fonte}",
            "tipo": "nota",
            "label": n["title"],
            "ts": n["indexed_at"] * 1000,
            "pasta": fonte.rsplit("/", 1)[0] if "/" in fonte else "",
        }

    if project_id is None:  # arestas (importação, sono, vault) são da memória pessoal
        arestas = memory.query(
            "SELECT src, rel, dst, kind FROM edges ORDER BY created_at DESC LIMIT ?",
            (MAX_ARESTAS,),
        )
        mensagens: set[int] = set()
        for a in arestas:
            for ponta in (a["src"], a["dst"]):
                if ponta.startswith("message:") and ponta[8:].isdigit():
                    mensagens.add(int(ponta[8:]))
        textos = _mensagens(memory, mensagens)
        for a in arestas:
            origem, destino = _id_do_no(a["src"]), _id_do_no(a["dst"])
            if a["kind"] == "vault" and not (origem in nos and destino in nos):
                continue  # nota fora do filtro de pasta
            for bruto, nid in ((a["src"], origem), (a["dst"], destino)):
                if nid in nos:
                    continue
                if nid.startswith("topic:"):
                    nos[nid] = {"id": nid, "tipo": "topico", "label": bruto[6:]}
                elif nid.startswith("message:"):
                    quando = textos.get(int(bruto[8:]) if bruto[8:].isdigit() else -1)
                    if quando is None:
                        break
                    nos[nid] = {"id": nid, "tipo": "evento", **quando}
                elif nid.startswith("entidade:"):
                    nos[nid] = {"id": nid, "tipo": "topico", "label": bruto}
                elif nid.startswith("doc:"):
                    break  # nota que não está indexada (apagada do vault)
            if origem in nos and destino in nos:
                ligacoes.append({"source": origem, "target": destino, "rel": a["rel"]})

    _ligar_fatos_a_coisas(nos, ligacoes)

    truncado = False
    if limite is not None and len(nos) > limite:
        truncado = True
        grau: dict[str, int] = {}
        for lk in ligacoes:
            grau[lk["source"]] = grau.get(lk["source"], 0) + 1
            grau[lk["target"]] = grau.get(lk["target"], 0) + 1
        ordem = sorted(nos, key=lambda i: (-grau.get(i, 0), -float(nos[i].get("ts", 0))))
        manter = set(ordem[:limite])
        nos = {i: nos[i] for i in nos if i in manter}
        ligacoes = [lk for lk in ligacoes if lk["source"] in nos and lk["target"] in nos]
    return {"nodes": list(nos.values()), "links": ligacoes, "truncado": truncado}


def _mensagens(memory: MemoryStore, ids: set[int]) -> dict[int, dict[str, Any]]:
    if not ids:
        return {}
    achado: dict[int, dict[str, Any]] = {}
    lista = sorted(ids)
    for i in range(0, len(lista), 500):
        lote = lista[i : i + 500]
        marcas = ",".join("?" * len(lote))
        for r in memory.query(
            f"SELECT id, role, text, created_at FROM messages WHERE id IN ({marcas})",  # noqa: S608
            lote,
        ):
            achado[r["id"]] = {
                "label": " ".join(r["text"].split())[:120],
                "ator": "Antônio" if r["role"] == "user" else "Orion",
                "ts": r["created_at"] * 1000,
            }
    return achado


def _ligar_fatos_a_coisas(nos: dict[str, dict[str, Any]], ligacoes: list[dict[str, Any]]) -> None:
    """Um fato que cita o nome de um tópico ou entidade fica ligado a ele (até 3 por fato)."""
    coisas = [
        (i, _plano(n["label"]))
        for i, n in nos.items()
        if n["tipo"] == "topico" and len(_plano(n["label"])) >= 3
    ][:400]
    if not coisas:
        return
    for i, n in list(nos.items()):
        if n["tipo"] != "fato":
            continue
        texto = _plano(n["label"])
        achados = [cid for cid, nome in coisas if re.search(rf"\b{re.escape(nome)}\b", texto)]
        ligacoes.extend({"source": i, "target": cid, "rel": "cita"} for cid in achados[:3])


def achar_no(nos: list[dict[str, Any]], texto: str) -> dict[str, Any] | None:
    """O nó que melhor casa com o que o Antônio disse: nome igual, começo do nome, trecho do nome,
    e por fim a maior sobreposição de palavras. Sem acento nem caixa."""
    q = _plano(texto)
    if not q:
        return None
    palavras = {p for p in q.split() if len(p) > 2}
    melhor: tuple[float, dict[str, Any]] | None = None
    for n in nos:
        nome = _plano(str(n.get("label") or n["id"]))
        if nome == q:
            nota = 4.0
        elif nome.startswith(q):
            nota = 3.0
        elif q in nome:
            nota = 2.0
        else:
            comuns = len(palavras & set(nome.split()))
            nota = comuns / len(palavras) if palavras and comuns else 0.0
        if nota and (melhor is None or nota > melhor[0]):
            melhor = (nota, n)
    return melhor[1] if melhor and melhor[0] >= 0.5 else None


def caminho(grafo: dict[str, Any], de: str, para: str) -> list[str]:
    """Menor caminho (em número de ligações, sem direção) entre dois nós; [] se não se conectam."""
    if de == para:
        return [de]
    vizinhos: dict[str, list[str]] = {}
    for lk in grafo["links"]:
        vizinhos.setdefault(lk["source"], []).append(lk["target"])
        vizinhos.setdefault(lk["target"], []).append(lk["source"])
    anterior: dict[str, str | None] = {de: None}
    fila = deque([de])
    while fila:
        atual = fila.popleft()
        for v in vizinhos.get(atual, ()):
            if v in anterior:
                continue
            anterior[v] = atual
            if v == para:
                rota = [v]
                while anterior[rota[-1]] is not None:
                    rota.append(anterior[rota[-1]])  # type: ignore[arg-type]
                return rota[::-1]
            fila.append(v)
    return []
