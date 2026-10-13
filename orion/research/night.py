"""Pesquisa noturna supervisionada (regra 39): um relatório por dia na caixa de entrada do vault.

O Antônio escolhe os assuntos (`ORION_RESEARCH_TOPICS`) e a hora (`ORION_RESEARCH_AT`); o modelo
não escolhe nada disso. Cada assunto roda num turno **só leitura** (`Agent.run(read_only=True)`):
sem escrita, sem execução e sem aprovação possível (o que pediria aprovação é negado). Cada assunto
tem uma conversa própria, para uma página lida num não contaminar o outro. O resultado é texto de
terceiros resumido por um modelo: vai marcado como não confiável e só para a caixa de entrada do
vault, que o Antônio tria. Nada é executado, nada é salvo na memória sozinho.
"""

from __future__ import annotations

import json
import logging
import re
from collections.abc import AsyncIterator, Callable
from datetime import datetime, timedelta
from typing import Any

from ..capture import CaptureError, Capturer

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


TOPICS_KEY = "research:topics"
# nome gerado pelo Capturer: "<data> <hhmm> - pesquisa-noturna-...md" (prefixo fixo, E6.5)
_RELATORIO = re.compile(r"^\d{4}-\d{2}-\d{2} \d{4} - pesquisa-noturna.*\.md$")


def parse_assuntos(bruto: str) -> list[str]:
    itens = [" ".join(p.split())[:MAX_ASSUNTO] for p in bruto.split(";")]
    return [i for i in itens if i][:MAX_ASSUNTOS]


def validar_assuntos(itens: list[str]) -> list[str]:
    """Assuntos vindos da interface: até 5, 1 a 200 caracteres cada, sem `;` (o separador do
    `.env`). Levanta `ValueError` com o motivo; nada é cortado em silêncio."""
    limpos = [" ".join(i.split()) for i in itens]
    if len(limpos) > MAX_ASSUNTOS:
        raise ValueError(f"no máximo {MAX_ASSUNTOS} assuntos")
    for i in limpos:
        if not i:
            raise ValueError("assunto vazio")
        if len(i) > MAX_ASSUNTO:
            raise ValueError(f"assunto passa de {MAX_ASSUNTO} caracteres")
        if ";" in i:
            raise ValueError("assunto não pode ter ';'")
    return list(dict.fromkeys(limpos))


def assuntos_salvos(memory: Any, padrao: str) -> tuple[list[str], str]:
    """Assuntos em vigor e de onde vieram: `interface` (guardados em `meta`) ou `padrao`
    (`ORION_RESEARCH_TOPICS`). Valor guardado que não vale mais cai no padrão."""
    bruto = memory.meta_get(TOPICS_KEY)
    if bruto is not None:
        try:
            lista = json.loads(bruto)
            if isinstance(lista, list) and all(isinstance(x, str) for x in lista):
                return validar_assuntos(lista), "interface"
        except ValueError:
            log.warning("assuntos da pesquisa guardados são inválidos; uso o padrão")
    return parse_assuntos(padrao), "padrao"


def salvar_assuntos(memory: Any, itens: list[str]) -> list[str]:
    limpos = validar_assuntos(itens)
    memory.meta_set(TOPICS_KEY, json.dumps(limpos, ensure_ascii=False))
    return limpos


class NightResearch:
    def __init__(
        self,
        *,
        memory: Any,
        run_turn: Callable[[str, str], AsyncIterator[Any]],
        capturer: Capturer,
        assuntos: list[str] | Callable[[], list[str]],
        at: str,
        clock: Callable[[], float],
    ) -> None:
        self._hm: tuple[int, int] | None = None
        if at.strip():  # sem hora: só roda quando você manda (`rodar agora`)
            h, m = (int(x) for x in at.split(":"))
            if not (0 <= h < 24 and 0 <= m < 60):
                raise ValueError(f"hora da pesquisa inválida: {at!r} (use HH:MM)")
            self._hm = (h, m)
        self.memory, self._run, self._cap = memory, run_turn, capturer
        self._assuntos = assuntos
        self._clock = clock
        self.rodando = False

    @property
    def assuntos(self) -> list[str]:
        """Lidos a cada rodada: o que você editou na interface vale na próxima."""
        return list(self._assuntos() if callable(self._assuntos) else self._assuntos)

    def relatorios(self, limite: int = 30) -> list[dict[str, Any]]:
        """Relatórios da pesquisa na caixa de entrada do vault, do mais novo ao mais antigo."""
        try:
            pasta = self._cap.pasta()
        except CaptureError:
            return []
        notas = sorted(
            (p for p in pasta.glob("*.md") if _RELATORIO.match(p.name) and p.is_file()),
            key=lambda p: p.name,
            reverse=True,
        )
        return [
            {
                "arquivo": self._cap.relativo(p),
                "nome": p.stem,
                "data": p.name[:10],
                "bytes": p.stat().st_size,
            }
            for p in notas[: max(1, min(limite, 100))]
        ]

    def devida(self) -> bool:
        if self._hm is None:
            return False
        dt = datetime.fromtimestamp(self._clock())
        alvo = dt.replace(hour=self._hm[0], minute=self._hm[1], second=0, microsecond=0)
        if dt < alvo or dt - alvo > timedelta(hours=JANELA_H):
            return False
        return self.memory.counter_get("pesquisa:ultimo") < int(dt.strftime("%Y%m%d"))

    async def run(self, *, marcar: bool = True) -> str | None:
        """Roda os assuntos e grava UM relatório. Devolve o caminho no vault, ou None.
        `marcar=False` (rodar agora, pela interface) não gasta a rodada da noite."""
        assuntos = self.assuntos
        if not assuntos or self.rodando:
            return None
        self.rodando = True
        try:
            return await self._rodar(assuntos, marcar)
        finally:
            self.rodando = False

    async def _rodar(self, assuntos: list[str], marcar: bool) -> str | None:
        hoje = int(datetime.fromtimestamp(self._clock()).strftime("%Y%m%d"))
        if marcar:  # marca antes: se falhar no meio, não repete a rodada inteira a cada tick
            self.memory.counter_set("pesquisa:ultimo", hoje)
        secoes: list[str] = []
        for assunto in assuntos:
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
        audit_log.info(
            "pesquisa_noturna", extra={"audit": {"assuntos": len(assuntos), "manual": not marcar}}
        )
        try:
            nota = self._cap.save_text("# Pesquisa noturna\n\n" + corpo, "pesquisa", fonte="orion")
        except CaptureError as e:
            log.error("pesquisa noturna não gravou: %s", e)
            return None
        return self._cap.relativo(nota)
