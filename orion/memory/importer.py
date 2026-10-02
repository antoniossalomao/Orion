"""Importa o export do SurrealDB do legado (fase 0 → fase 3 do NUCLEO).

O formato é o que a ferramenta `backup_memoria` do legado já grava: um
`<tabela>.json` por tabela, cada um uma lista de registros (`SELECT * FROM`).
Entram só as conversas: `sessao` → sessões arquivadas (canal "legado") e
`evento` → mensagens com a data original, pesquisáveis por FTS. Idempotente:
rodar de novo não duplica. Tabelas operacionais (lembrete, agendamento,
tarefa, numero, prompt) são contadas e ficam para a fase de operação.
"""

from __future__ import annotations

import json
import logging
from collections.abc import Iterable
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path
from typing import Any

from .store import MemoryStore

log = logging.getLogger("orion.memory.import")

CANAL_LEGADO = "legado"
SEM_SESSAO = "pre-sessoes"
OPERACIONAIS = ("lembrete", "agendamento", "tarefa", "numero", "prompt")
_USUARIO = {"antônio", "antonio"}


@dataclass
class ImportReport:
    sessoes: int = 0
    mensagens: int = 0
    ja_importadas: int = 0
    invalidas: int = 0
    operacionais_ignoradas: dict[str, int] = field(default_factory=dict)

    def resumo(self) -> str:
        ops = ", ".join(f"{t}={n}" for t, n in self.operacionais_ignoradas.items()) or "nenhuma"
        return (
            f"sessões novas={self.sessoes} mensagens novas={self.mensagens} "
            f"já importadas={self.ja_importadas} inválidas={self.invalidas} "
            f"tabelas operacionais ignoradas: {ops}"
        )


def _id_limpo(rid: Any) -> str:
    """'sessao:⟨uuid⟩' / 'sessao:`uuid`' / 'uuid' → 'uuid'."""
    return str(rid).split(":", 1)[-1].strip("⟨⟩`'\" ")


def _ts(valor: Any) -> float:
    try:
        return datetime.fromisoformat(str(valor)).timestamp()
    except (TypeError, ValueError):
        return 0.0


def _ler(pasta: Path, tabela: str) -> list[dict[str, Any]] | None:
    arq = pasta / f"{tabela}.json"
    if not arq.exists():
        return None
    dados = json.loads(arq.read_text(encoding="utf-8"))
    if not isinstance(dados, list):
        raise ValueError(f"{arq.name}: esperava uma lista de registros")
    return [d for d in dados if isinstance(d, dict)]


def import_surreal_export(
    store: MemoryStore, pasta: Path | str, assistentes: Iterable[str] = ("Orion",)
) -> ImportReport:
    """`assistentes`: nomes de ator que são fala do Orion (inclui nomes antigos, ATORES_LEGADOS)."""
    pasta = Path(pasta)
    eventos = _ler(pasta, "evento")
    if eventos is None:
        raise FileNotFoundError(
            f"{pasta / 'evento.json'} não existe (rode backup_memoria no legado)"
        )
    rel = ImportReport()
    nomes_assistente = {a.strip().lower() for a in assistentes if a.strip()}

    sessoes: dict[str, str] = {}  # id externo -> id interno
    for s in _ler(pasta, "sessao") or []:
        ext = _id_limpo(s.get("id", ""))
        if not ext:
            rel.invalidas += 1
            continue
        titulo = s.get("titulo") or "conversa sem título"
        sessoes[ext], nova = store.import_session(ext, CANAL_LEGADO, titulo, _ts(s.get("criada")))
        rel.sessoes += nova

    def sessao_de(ext: str | None) -> str:
        chave = ext or SEM_SESSAO
        if chave not in sessoes:
            titulo = "conversas antigas (pré-sessões)" if not ext else f"sessão {chave[:8]}"
            sessoes[chave], nova = store.import_session(chave, CANAL_LEGADO, titulo, 0.0)
            rel.sessoes += nova
        return sessoes[chave]

    for ev in sorted(eventos, key=lambda e: _ts(e.get("timestamp"))):
        texto = str(ev.get("texto") or "").strip()
        ext = _id_limpo(ev.get("id", ""))
        if not texto or not ext:
            rel.invalidas += 1
            continue
        ator = str(ev.get("ator") or "").strip().lower()
        papel = (
            "assistant" if ator in nomes_assistente else "user" if ator in _USUARIO else "system"
        )
        prov: dict[str, Any] = {"importado": "surrealdb", "fonte": ev.get("fonte")}
        for chave in ("intencao", "fontes_rag"):
            if ev.get(chave):
                prov[chave] = ev[chave]
        sid = sessao_de(_id_limpo(ev["sessao_id"]) if ev.get("sessao_id") else None)
        if store.import_message(sid, ext, papel, texto, _ts(ev.get("timestamp")), prov):
            rel.mensagens += 1
        else:
            rel.ja_importadas += 1

    for tabela in OPERACIONAIS:
        regs = _ler(pasta, tabela)
        if regs:
            rel.operacionais_ignoradas[tabela] = len(regs)
    log.info("importação: %s", rel.resumo())
    return rel
