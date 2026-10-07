"""Agenda do Google no briefing matinal (regra 37).

O briefing roda **sem ninguém olhando**, e a regra 18 proíbe agendamento de executar ferramenta
sozinho. Esta é a única exceção, e é estreita de propósito:

- **sem modelo**: o job chama a ferramenta MCP diretamente e cola o texto no aviso. Nenhum agente
  roda, então um evento de agenda com instrução escrita ("ignore o que veio antes...") não chega
  a nenhum modelo;
- **uma ferramenta fixa** (`ORION_BRIEFING_CALENDAR_TOOL`, padrão `google__get_events`), com
  argumentos montados aqui (e-mail, calendário `primary`, o dia de hoje), nunca vindos de texto;
- **só vale se o `mcp.json` classifica essa ferramenta como `read`**: a classe vem do arquivo
  (regra 24), então trocar por uma ferramenta que escreve não liga o recurso;
- o resultado é conteúdo de terceiros: sem URL, sem ID, linhas e total limitados, e só vai
  para o aviso (nunca para a memória nem para um prompt);
- cada consulta fica no audit (`briefing_agenda`); falha não derruba o briefing, vira uma linha
  "agenda indisponível" com o **tipo** do erro.

**Formato do `get_events` do workspace-mcp 2.0.1 só foi visto pelo esquema** (parâmetros
`user_google_email`, `calendar_id`, `time_min`, `time_max`); o texto que ele devolve não foi
validado com uma conta real (ver ORION_MELHORIAS).
"""

from __future__ import annotations

import logging
import re
from collections.abc import Callable
from datetime import datetime, timedelta
from typing import Any

from .policy.classes import Risk, ToolSpec

log = logging.getLogger("orion.agenda")

MAX_LINHAS = 10
MAX_LINHA = 160
MAX_TOTAL = 1500
_URL = re.compile(r"https?://\S+")
_ID = re.compile(r"\b(?:ID|Id|id)\s*:\s*\S+")
_CONTROLE = re.compile(r"[\x00-\x08\x0b-\x1f\x7f]")
_SOBRA = re.compile(r"\s*[|·]\s*(?=[|·]|$)")


def limpar(texto: str) -> list[str]:
    """Linhas do que a agenda devolveu, sem endereço, sem ID e sem caractere de controle."""
    linhas: list[str] = []
    for bruta in texto.splitlines():
        ln = _CONTROLE.sub("", bruta)
        ln = _SOBRA.sub("", _ID.sub("", _URL.sub("", ln)))
        ln = re.sub(r"\s+", " ", ln).strip(" -|·\t")
        if ln:
            linhas.append(ln[:MAX_LINHA])
    return linhas


def formatar(texto: str) -> str:
    linhas = limpar(texto)
    corpo = linhas[:MAX_LINHAS]
    if len(linhas) > MAX_LINHAS:
        corpo.append(f"… e mais {len(linhas) - MAX_LINHAS} linhas")
    saida = "\n".join(f"• {ln}" for ln in corpo)
    return saida[:MAX_TOTAL]


class AgendaDoDia:
    """`__call__(agora)` devolve a seção da agenda do briefing (ou None se não há o que mostrar)."""

    def __init__(
        self,
        call: Callable[[str, dict[str, Any]], dict[str, Any]],
        *,
        tool: str,
        email: str,
        audit: Callable[[dict[str, Any]], Any] | None = None,
        calendario: str = "primary",
    ) -> None:
        self._call, self._tool, self._email = call, tool, email
        self._audit, self._cal = audit, calendario

    def __call__(self, agora: float) -> str | None:
        dt = datetime.fromtimestamp(agora).astimezone()
        inicio = dt.replace(hour=0, minute=0, second=0, microsecond=0)
        fim = inicio + timedelta(days=1)
        args = {
            "user_google_email": self._email,
            "calendar_id": self._cal,
            "time_min": inicio.isoformat(),
            "time_max": fim.isoformat(),
        }
        try:
            res = self._call(self._tool, args)
        except Exception as e:  # noqa: BLE001 — a agenda nunca derruba o briefing
            return self._falha(type(e).__name__)
        if "erro" in res:
            return self._falha("erro do servidor MCP")
        texto = formatar(str(res.get("texto") or ""))
        self._registrar("ok")
        return texto or None

    def _falha(self, motivo: str) -> str:
        log.warning("agenda do briefing falhou: %s", motivo)
        self._registrar(f"falhou: {motivo}")
        return f"agenda indisponível ({motivo})"

    def _registrar(self, motivo: str) -> None:
        if self._audit is None:
            return
        try:
            self._audit(
                {
                    "tool": "briefing_agenda",
                    "action": "allow",
                    "risk": Risk.READ.value,
                    "reason": f"briefing sem supervisão: {self._tool} ({motivo})",
                }
            )
        except Exception:
            log.exception("audit da agenda do briefing falhou")


def agenda_do_briefing(
    *,
    tool: str,
    email: str,
    specs: dict[str, ToolSpec],
    call: Callable[[str, dict[str, Any]], dict[str, Any]],
    audit: Callable[[dict[str, Any]], Any] | None = None,
) -> AgendaDoDia | None:
    """A agenda do briefing, ou None (com o motivo no log) se não está pronta ou não é segura:
    sem e-mail, ferramenta que o `mcp.json` não expôs, ou classe diferente de `read`."""
    if not email.strip():
        return None
    spec = specs.get(tool)
    if spec is None:
        log.warning(
            "agenda do briefing: a ferramenta %s não está no mcp.json (ou o servidor caiu)", tool
        )
        return None
    if spec.risk is not Risk.READ:
        log.warning(
            "agenda do briefing recusada: %s é '%s' no mcp.json; sem supervisão só roda leitura",
            tool,
            spec.risk.value,
        )
        return None
    return AgendaDoDia(call, tool=tool, email=email.strip(), audit=audit)
