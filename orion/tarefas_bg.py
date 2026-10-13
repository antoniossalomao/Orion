"""Tarefa em segundo plano (E9.2, regra 59): o Orion trabalha num objetivo sem ninguém olhando.

Roda `Agent.run(read_only=True)` numa conversa própria: o modelo só vê e só chama ferramentas de
**leitura**; escrita e execução nem aparecem para ele. O que ele julgar útil fazer vira uma
**proposta** (texto, no fim do relatório, depois de `PROPOSTAS:`) que você aceita ou descarta item
a item; aceitar devolve o texto para o campo de mensagem, e aí vale a política de sempre.
Tetos: passos (ferramentas chamadas), minutos e cota gratuita (regra 46).
"""

from __future__ import annotations

import asyncio
import json
import logging
import re
import time
from collections.abc import AsyncIterator, Callable
from typing import Any

log = logging.getLogger("orion.tarefas_bg")

CANAL = "tarefa-bg"
MAX_OBJETIVO = 500
MAX_PASSOS = 30
MAX_MINUTOS = 30
MAX_RELATORIO = 8000
MAX_PROPOSTAS = 10
MAX_PROPOSTA = 240

PROMPT = (
    "Tarefa em segundo plano, sem ninguém acompanhando. Objetivo: {objetivo}\n"
    "Use só ferramentas de leitura. NÃO escreva, apague, execute nem salve nada. Entregue um "
    "relatório curto em português, com as fontes. Se achar que alguma ação (escrever, mudar, "
    "executar) ajudaria, não a faça: no fim, depois de uma linha 'PROPOSTAS:', liste até "
    f"{MAX_PROPOSTAS} propostas, uma por linha, começando com '- ', cada uma como um pedido "
    "completo que o Antônio poderá aceitar ou descartar."
)
_PROPOSTAS = re.compile(r"^\s*PROPOSTAS:\s*$", re.M)


class TarefaBgError(ValueError):
    """Pedido recusado (a mensagem é segura de mostrar)."""

    def __init__(self, mensagem: str, status: int = 422) -> None:
        super().__init__(mensagem)
        self.status = status


def separar_propostas(texto: str) -> tuple[str, list[dict[str, str]]]:
    """(relatório sem a seção, propostas). Só linhas '- ...'; texto é dado, nunca vira ação."""
    achou = _PROPOSTAS.search(texto)
    if not achou:
        return texto.strip(), []
    corpo, resto = texto[: achou.start()].strip(), texto[achou.end() :]
    itens = [
        {"texto": " ".join(ln.strip()[2:].split())[:MAX_PROPOSTA], "estado": "pendente"}
        for ln in resto.splitlines()
        if ln.strip().startswith("- ") and ln.strip()[2:].strip()
    ]
    return corpo, itens[:MAX_PROPOSTAS]


class TarefasBg:
    def __init__(
        self,
        *,
        memory: Any,
        ops: Any,
        run_turn: Callable[[str, str], AsyncIterator[Any]],
        custos: Any = None,
        modos: Any = None,
        clock: Callable[[], float] = time.time,
    ) -> None:
        self.memory, self.ops, self._run, self._custos, self._modos = (
            memory,
            ops,
            run_turn,
            custos,
            modos,
        )
        self._clock = clock
        self._tarefas: dict[int, asyncio.Task[None]] = {}

    # ── pedido ────────────────────────────────────────────────────────────────
    def criar(self, objetivo: str, max_passos: int = 10, max_min: int = 10) -> dict[str, Any]:
        objetivo = " ".join(objetivo.split())
        if not objetivo or len(objetivo) > MAX_OBJETIVO:
            raise TarefaBgError(f"o objetivo tem de ter de 1 a {MAX_OBJETIVO} caracteres")
        if not (1 <= max_passos <= MAX_PASSOS and 1 <= max_min <= MAX_MINUTOS):
            raise TarefaBgError(f"passos de 1 a {MAX_PASSOS} e minutos de 1 a {MAX_MINUTOS}")
        if self._modos is not None and self._modos.panico():
            raise TarefaBgError("modo pânico: a tarefa em segundo plano está cortada", 409)
        if any(not t.done() for t in self._tarefas.values()):
            raise TarefaBgError("já há uma tarefa em segundo plano rodando", 409)
        if self._custos is not None and not self._custos.pode_usar(
            "gateway", opcional=True, quem="a tarefa em segundo plano"
        ):
            raise TarefaBgError("cota gratuita do dia quase no fim: tente amanhã", 429)
        orcamento = json.dumps({"max_passos": max_passos, "max_min": max_min})
        with self.memory.transaction() as c:
            tid = int(
                c.execute(
                    "INSERT INTO background_tasks(objective, budget, status, created_at)"
                    " VALUES (?,?,?,?)",
                    (objetivo, orcamento, "rodando", self._clock()),
                ).lastrowid
                or 0
            )
        self._tarefas[tid] = asyncio.get_running_loop().create_task(
            self._executar(tid, objetivo, max_passos, max_min)
        )
        return self.obter(tid)

    async def _executar(self, tid: int, objetivo: str, max_passos: int, max_min: int) -> None:
        texto: list[str] = []
        passos, status = 0, "concluida"

        async def consumir() -> None:
            nonlocal passos, status
            self.memory.new_session(CANAL, f"Tarefa: {objetivo}"[:120])
            eventos = self._run(CANAL, PROMPT.format(objetivo=objetivo))
            try:
                async for ev in eventos:
                    if ev.kind == "text":
                        texto.append(str(ev.data.get("text", "")))
                    elif ev.kind == "tool" and ev.data.get("decision") in (None, "allow"):
                        passos += 1
                        if passos > max_passos:
                            status = "estourou_passos"
                            return
                    elif ev.kind == "error":
                        texto.append(f"\n(erro: {ev.data.get('message', '?')})")
                        status = "falhou"
            finally:
                aclose = getattr(eventos, "aclose", None)
                if aclose is not None:
                    await aclose()

        try:
            await asyncio.wait_for(consumir(), timeout=max_min * 60)
        except TimeoutError:
            status = "estourou_tempo"
        except asyncio.CancelledError:
            status = "cancelada"
        except Exception as e:
            log.exception("tarefa em segundo plano %s falhou", tid)
            texto.append(f"\n(falhou: {type(e).__name__})")
            status = "falhou"
        relatorio, propostas = separar_propostas("".join(texto))
        with self.memory.transaction() as c:
            c.execute(
                "UPDATE background_tasks SET status=?, report=?, proposals=?, finished_at=?"
                " WHERE id=?",
                (
                    status,
                    relatorio[:MAX_RELATORIO] or "(sem relatório)",
                    json.dumps(propostas, ensure_ascii=False),
                    self._clock(),
                    tid,
                ),
            )
        try:
            self.ops.notify(
                "tarefa_bg",
                f"Tarefa em segundo plano ({status.replace('_', ' ')}): {objetivo[:80]}",
                ref=f"bg:{tid}",
            )
        except Exception:
            log.warning("tarefa %s: aviso não enviado", tid, exc_info=True)

    # ── leitura e decisões ────────────────────────────────────────────────────
    @staticmethod
    def _linha(r: Any) -> dict[str, Any]:
        d = dict(r)
        d["budget"] = json.loads(d["budget"])
        d["proposals"] = json.loads(d["proposals"])
        return d

    def obter(self, tid: int) -> dict[str, Any]:
        rows = self.memory.query("SELECT * FROM background_tasks WHERE id=?", (tid,))
        if not rows:
            raise TarefaBgError("tarefa não encontrada", 404)
        return self._linha(rows[0])

    def listar(self, limite: int = 30) -> list[dict[str, Any]]:
        rows = self.memory.query(
            "SELECT * FROM background_tasks ORDER BY id DESC LIMIT ?", (max(1, min(limite, 100)),)
        )
        return [self._linha(r) for r in rows]

    def cancelar(self, tid: int) -> dict[str, Any]:
        t = self._tarefas.get(tid)
        if t is None or t.done():
            raise TarefaBgError("a tarefa não está rodando", 409)
        t.cancel()
        with (
            self.memory.transaction() as c
        ):  # cancelada antes de começar: nada mais fecharia a linha
            c.execute(
                "UPDATE background_tasks SET status='cancelada', finished_at=?"
                " WHERE id=? AND status='rodando'",
                (self._clock(), tid),
            )
        return self.obter(tid)

    def decidir(self, tid: int, indice: int, acao: str) -> dict[str, Any]:
        """`aceitar` devolve o texto para o campo de mensagem (a política vale lá); `descartar`
        só marca. Nada é executado aqui."""
        if acao not in ("aceitar", "descartar"):
            raise TarefaBgError("ação inválida")
        tarefa = self.obter(tid)
        propostas = tarefa["proposals"]
        if not 0 <= indice < len(propostas):
            raise TarefaBgError("proposta não encontrada", 404)
        if propostas[indice]["estado"] != "pendente":
            raise TarefaBgError("essa proposta já foi decidida", 409)
        propostas[indice]["estado"] = "aceita" if acao == "aceitar" else "descartada"
        with self.memory.transaction() as c:
            c.execute(
                "UPDATE background_tasks SET proposals=? WHERE id=?",
                (json.dumps(propostas, ensure_ascii=False), tid),
            )
        return {"ok": True, "texto": propostas[indice]["texto"] if acao == "aceitar" else None}

    async def aguardar(self, tid: int) -> None:
        """Para os testes e o desligamento: espera a tarefa acabar."""
        t = self._tarefas.get(tid)
        if t is not None:
            await asyncio.gather(t, return_exceptions=True)

    async def encerrar(self) -> None:
        for t in self._tarefas.values():
            t.cancel()
        await asyncio.gather(*self._tarefas.values(), return_exceptions=True)
