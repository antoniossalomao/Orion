"""Importa o export do SurrealDB do legado (fase 0 → fase 3 do NUCLEO).

O formato é o que a ferramenta `backup_memoria` do legado já grava: um
`<tabela>.json` por tabela, cada um uma lista de registros (`SELECT * FROM`).
Conversas: `sessao` → sessões arquivadas (canal "legado") e `evento` →
mensagens com a data original, pesquisáveis por FTS. Operação: `lembrete`,
`agendamento`, `tarefa`, `numero` e `prompt` viram as tabelas de `orion.memory.ops`.
Grafo: as relações `precedeu` (evento→evento), `sobre` (evento→tópico) e `conecta`
(tópico→tópico) viram arestas. O `backup_memoria` do legado exporta todas essas
tabelas (`INFO FOR DB`). Idempotente: rodar de novo não duplica.
"""

from __future__ import annotations

import json
import logging
from collections.abc import Iterable
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path
from typing import Any

from .ops import TIPOS_AGENDAMENTO, next_run
from .store import MemoryStore

log = logging.getLogger("orion.memory.import")

CANAL_LEGADO = "legado"
SEM_SESSAO = "pre-sessoes"
OPERACIONAIS = ("lembrete", "agendamento", "tarefa", "numero", "prompt")
RELACOES = ("precedeu", "sobre", "conecta")
_USUARIO = {"antônio", "antonio"}
# pendência vencida há mais que isso entra já "avisada": sem rajada de avisos velhos
_VENCIDO_HA = 86_400
_NAO_E_ATOR = {"sistema", "system"}


@dataclass
class ImportReport:
    sessoes: int = 0
    mensagens: int = 0
    ja_importadas: int = 0
    invalidas: int = 0
    invalidas_por_tabela: dict[str, int] = field(default_factory=dict)
    # atores de evento que não são o Antônio nem o assistente informado: viram papel "system"
    # (não aparecem na busca de conversas). Nome antigo do assistente? Passe `--assistente`.
    atores_como_system: dict[str, int] = field(default_factory=dict)
    operacionais: dict[str, int] = field(default_factory=dict)  # tabela -> novos
    operacionais_repetidos: dict[str, int] = field(default_factory=dict)
    arestas: int = 0
    arestas_repetidas: int = 0
    arestas_sem_no: int = 0  # ponta que não virou mensagem/tópico (evento vazio, tabela estranha)

    def invalida(self, tabela: str) -> None:
        self.invalidas += 1
        self.invalidas_por_tabela[tabela] = self.invalidas_por_tabela.get(tabela, 0) + 1

    def resumo(self) -> str:
        ops = ", ".join(f"{t}={n}" for t, n in self.operacionais.items()) or "nenhuma"
        linha = (
            f"sessões novas={self.sessoes} mensagens novas={self.mensagens} "
            f"já importadas={self.ja_importadas} inválidas={self.invalidas} "
            f"operação nova: {ops} · arestas novas={self.arestas} "
            f"repetidas={self.arestas_repetidas} sem nó={self.arestas_sem_no}"
        )
        if self.atores_como_system:
            quem = ", ".join(f"{a}={n}" for a, n in sorted(self.atores_como_system.items()))
            linha += (
                f"\nATENÇÃO: atores tratados como 'system' ({quem}). Se algum é o nome antigo do "
                "assistente, rode de novo com --assistente <nome>."
            )
        return linha


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
            rel.invalida("sessao")
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
            rel.invalida("evento")
            continue
        ator = str(ev.get("ator") or "").strip().lower()
        papel = (
            "assistant" if ator in nomes_assistente else "user" if ator in _USUARIO else "system"
        )
        if papel == "system" and ator not in _NAO_E_ATOR:
            nome = ator or "(sem ator)"
            rel.atores_como_system[nome] = rel.atores_como_system.get(nome, 0) + 1
        prov: dict[str, Any] = {"importado": "surrealdb", "fonte": ev.get("fonte")}
        for chave in ("intencao", "fontes_rag"):
            if ev.get(chave):
                prov[chave] = ev[chave]
        sid = sessao_de(_id_limpo(ev["sessao_id"]) if ev.get("sessao_id") else None)
        if store.import_message(sid, ext, papel, texto, _ts(ev.get("timestamp")), prov):
            rel.mensagens += 1
        else:
            rel.ja_importadas += 1

    _importar_operacao(store, pasta, rel)
    _importar_arestas(store, pasta, rel)
    log.info("importação: %s", rel.resumo())
    return rel


def _texto(valor: Any) -> str:
    return str(valor).strip() if valor is not None else ""


def _lembrete(r: dict[str, Any], agora: float) -> tuple[str, tuple[Any, ...]]:
    titulo, quando = _texto(r.get("titulo")), _ts(r.get("quando"))
    if not titulo or quando <= 0:
        raise ValueError("lembrete sem título ou sem data")
    feito = bool(r.get("concluido"))
    avisado = feito or quando < agora - _VENCIDO_HA
    return (
        "INSERT INTO reminders(title, due_at, note, done, notified, created_at)"
        " VALUES (?,?,?,?,?,?)",
        (
            titulo,
            quando,
            _texto(r.get("nota")),
            int(feito),
            int(avisado),
            _ts(r.get("criado")) or agora,
        ),
    )


def _agendamento(r: dict[str, Any], agora: float) -> tuple[str, tuple[Any, ...]]:
    titulo, tipo = _texto(r.get("titulo")), _texto(r.get("tipo")) or "unico"
    if not titulo or tipo not in TIPOS_AGENDAMENTO:
        raise ValueError("agendamento sem título ou com tipo desconhecido")
    horario, intervalo = _texto(r.get("horario")), int(r.get("intervalo_min") or 0)
    ativo = r.get("ativo") is not False
    prox: float | None = _ts(r.get("proxima_execucao")) or None
    if tipo == "unico":
        if (
            prox is None or prox < agora - _VENCIDO_HA
        ):  # nunca disparou e já passou: não dispara agora
            prox, ativo = None, False
    elif prox is None or prox <= agora:  # recorrente atrasado: próximo horário a partir de agora
        prox = next_run(tipo, agora, time_of_day=horario, interval_min=intervalo)
    try:
        params = json.loads(_texto(r.get("parametros")) or "{}")
        params = params if isinstance(params, dict) else {}
    except json.JSONDecodeError:
        params = {}
    return (
        "INSERT INTO schedules(title, kind, time_of_day, interval_min, tool, params, next_run,"
        " last_run, active, created_at) VALUES (?,?,?,?,?,?,?,?,?,?)",
        (
            titulo,
            tipo,
            horario,
            intervalo,
            _texto(r.get("ferramenta")),
            json.dumps(params, ensure_ascii=False),
            prox,
            _ts(r.get("ultima_execucao")) or None,
            int(ativo and prox is not None),
            _ts(r.get("criado")) or agora,
        ),
    )


def _tarefa(r: dict[str, Any], agora: float) -> tuple[str, tuple[Any, ...]]:
    titulo = _texto(r.get("titulo"))
    if not titulo:
        raise ValueError("tarefa sem título")
    status = _texto(r.get("status"))
    status = status if status in ("pendente", "em_andamento", "concluida") else "pendente"
    criado = _ts(r.get("criado")) or agora
    return (
        "INSERT INTO tasks(title, status, created_at, updated_at) VALUES (?,?,?,?)",
        (titulo, status, criado, criado),
    )


def _numero(r: dict[str, Any], agora: float) -> tuple[str, tuple[Any, ...]]:
    alvo = _texto(r.get("alvo"))
    if not alvo:
        raise ValueError("número sem alvo")
    score = max(0.0, min(1.0, float(r.get("score") or 0.0)))
    return (
        "INSERT INTO numbers(target, score, reason, source, notified, created_at)"
        " VALUES (?,?,?,?,?,?)",
        (
            alvo,
            score,
            _texto(r.get("motivo")),
            _texto(r.get("fonte")) or "orion",
            int(bool(r.get("notificado"))),
            _ts(r.get("criado")) or agora,
        ),
    )


def _prompt(r: dict[str, Any], agora: float) -> tuple[str, tuple[Any, ...]]:
    titulo, conteudo = _texto(r.get("titulo")), _texto(r.get("conteudo"))
    if not titulo or not conteudo:
        raise ValueError("prompt sem título ou conteúdo")
    return (
        "INSERT INTO prompts(title, command, content, created_at) VALUES (?,?,?,?)",
        (titulo, _texto(r.get("comando")), conteudo, _ts(r.get("criado_em")) or agora),
    )


_MAPAS = {
    "lembrete": _lembrete,
    "agendamento": _agendamento,
    "tarefa": _tarefa,
    "numero": _numero,
    "prompt": _prompt,
}


def _importar_operacao(store: MemoryStore, pasta: Path, rel: ImportReport) -> None:
    agora = store.clock()
    for tabela in OPERACIONAIS:
        for r in _ler(pasta, tabela) or []:
            ext = _id_limpo(r.get("id", ""))
            if not ext:
                rel.invalida(tabela)
                continue
            try:
                sql, params = _MAPAS[tabela](r, agora)
            except (ValueError, TypeError) as e:
                log.warning("%s %s ignorado: %s", tabela, ext, e)
                rel.invalida(tabela)
                continue
            with store.transaction() as c:
                novo = c.execute(
                    "INSERT OR IGNORE INTO imported(kind, external_id) VALUES (?,?)", (tabela, ext)
                ).rowcount
                if not novo:
                    rel.operacionais_repetidos[tabela] = (
                        rel.operacionais_repetidos.get(tabela, 0) + 1
                    )
                    continue
                rid = c.execute(sql, params).lastrowid
                c.execute(
                    "UPDATE imported SET ref=? WHERE kind=? AND external_id=?",
                    (str(rid), tabela, ext),
                )
            rel.operacionais[tabela] = rel.operacionais.get(tabela, 0) + 1


def _no(ref: Any, mensagens: dict[str, str]) -> str | None:
    """'evento:⟨id⟩' → 'message:<id interno>'; 'topico:⟨nome⟩' → 'topic:<nome>'."""
    tabela = str(ref).split(":", 1)[0]
    ident = _id_limpo(ref)
    if not ident:
        return None
    if tabela == "evento":
        mid = mensagens.get(ident)
        return f"message:{mid}" if mid else None
    if tabela == "topico":
        return f"topic:{ident}"
    return None


def _importar_arestas(store: MemoryStore, pasta: Path, rel: ImportReport) -> None:
    mensagens = {
        r["external_id"]: r["ref"]
        for r in store.query("SELECT external_id, ref FROM imported WHERE kind='mensagem'")
        if r["ref"]
    }
    agora = store.clock()
    for relacao in RELACOES:
        linhas: list[tuple[Any, ...]] = []
        for r in _ler(pasta, relacao) or []:
            origem, destino = _no(r.get("in"), mensagens), _no(r.get("out"), mensagens)
            if origem is None or destino is None:
                rel.arestas_sem_no += 1
                continue
            bruto = r.get("peso")
            try:
                peso = 1.0 if bruto is None else float(bruto)
            except (TypeError, ValueError):
                peso = 1.0
            linhas.append((origem, relacao, destino, peso, _texto(r.get("tipo")), agora))
        if not linhas:
            continue
        with store.transaction() as c:
            antes = c.total_changes
            c.executemany(
                "INSERT OR IGNORE INTO edges(src, rel, dst, weight, kind, created_at)"
                " VALUES (?,?,?,?,?,?)",
                linhas,
            )
            novas = c.total_changes - antes
        rel.arestas += novas
        rel.arestas_repetidas += len(linhas) - novas
