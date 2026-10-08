"""Pesquisa noturna supervisionada (regra 39): um relatório por dia na caixa de entrada do vault.

O Antônio escolhe os assuntos (`ORION_RESEARCH_TOPICS`) e a hora (`ORION_RESEARCH_AT`); o modelo
não escolhe nada disso. Cada assunto roda num turno **só leitura** (`Agent.run(read_only=True)`):
sem escrita, sem execução e sem aprovação possível (o que pediria aprovação é negado). Cada assunto
tem uma conversa própria, para uma página lida num não contaminar o outro. O resultado é texto de
terceiros resumido por um modelo: vai marcado como não confiável e só para a caixa de entrada do
vault, que o Antônio tria. Nada é executado, nada é salvo na memória sozinho.
"""

from __future__ import annotations

import logging
from collections.abc import AsyncIterator, Callable
from datetime import datetime, timedelta
from typing import Any

from .capture import CaptureError, Capturer

log = logging.getLogger("orion.research")
audit_log = logging.getLogger("orion.audit")

MAX_ASSUNTOS = 5
MAX_ASSUNTO = 200
JANELA_H = 6.0

PROMPT = (
    "Pesquise na internet sobre: {assunto}\n"
    "Use só ferramentas de leitura. Entregue um resumo de até 15 linhas, em português, com o "
    "que há de novo e os links das fontes. Não execute nada e não salve nada."
)


def parse_assuntos(bruto: str) -> list[str]:
    itens = [" ".join(p.split())[:MAX_ASSUNTO] for p in bruto.split(";")]
    return [i for i in itens if i][:MAX_ASSUNTOS]


class NightResearch:
    def __init__(
        self,
        *,
        memory: Any,
        run_turn: Callable[[str, str], AsyncIterator[Any]],
        capturer: Capturer,
        assuntos: list[str],
        at: str,
        clock: Callable[[], float],
    ) -> None:
        h, m = (int(x) for x in at.split(":"))
        if not (0 <= h < 24 and 0 <= m < 60):
            raise ValueError(f"hora da pesquisa inválida: {at!r} (use HH:MM)")
        self._hm = (h, m)
        self.memory, self._run, self._cap = memory, run_turn, capturer
        self.assuntos = assuntos
        self._clock = clock

    def devida(self) -> bool:
        dt = datetime.fromtimestamp(self._clock())
        alvo = dt.replace(hour=self._hm[0], minute=self._hm[1], second=0, microsecond=0)
        if dt < alvo or dt - alvo > timedelta(hours=JANELA_H):
            return False
        return self.memory.counter_get("pesquisa:ultimo") < int(dt.strftime("%Y%m%d"))

    async def run(self) -> str | None:
        """Roda os assuntos e grava UM relatório. Devolve o caminho no vault, ou None."""
        if not self.assuntos:
            return None
        hoje = int(datetime.fromtimestamp(self._clock()).strftime("%Y%m%d"))
        # marca antes: se falhar no meio, não repete a rodada inteira a cada tick
        self.memory.counter_set("pesquisa:ultimo", hoje)
        secoes: list[str] = []
        for assunto in self.assuntos:
            self.memory.new_session("pesquisa", f"Pesquisa: {assunto}"[:120])
            texto = ""
            try:
                async for ev in self._run("pesquisa", PROMPT.format(assunto=assunto)):
                    if ev.kind == "text":
                        texto += ev.data["text"]
                    elif ev.kind == "error":
                        texto += f"\n(erro: {ev.data.get('message', '?')})"
            except Exception as e:  # um assunto ruim não derruba os outros
                log.exception("pesquisa noturna falhou em um assunto")
                texto = f"(falhou: {type(e).__name__})"
            secoes.append(f"## {assunto}\n\n{texto.strip() or '(sem resposta)'}")
        corpo = (
            "> [!warning] Conteúdo de fontes externas, resumido por um modelo: confira antes de "
            "confiar. Gerado pela pesquisa noturna do Orion (só leitura).\n\n" + "\n\n".join(secoes)
        )
        audit_log.info("pesquisa_noturna", extra={"audit": {"assuntos": len(self.assuntos)}})
        try:
            nota = self._cap.save_text("# Pesquisa noturna\n\n" + corpo, "pesquisa", fonte="orion")
        except CaptureError as e:
            log.error("pesquisa noturna não gravou: %s", e)
            return None
        return self._cap.relativo(nota)
