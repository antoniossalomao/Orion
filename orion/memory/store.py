"""MemoryStore: sessões, fatos e documentos num único arquivo SQLite.

Busca híbrida = FTS5 (palavra-chave, sempre disponível) + vetores (numpy, quando
há embedder), fundidos por RRF. Se o embedder cair,
a busca segue só por palavra-chave e os vetores faltantes entram depois em
`embed_pending()` (job). Backup = `backup_to()` (cópia consistente do arquivo).
"""

from __future__ import annotations

import hashlib
import json
import logging
import os
import re
import sqlite3
import threading
import time
import unicodedata
import uuid
from collections.abc import Callable, Iterator, Sequence
from contextlib import AbstractContextManager, contextmanager
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import Any, ClassVar, Literal, Protocol

from .schema import DDL, MIGRATIONS, SCHEMA_VERSION
from .text import chunk_text, fts_query, strip_frontmatter
from .vectors import VectorIndex, para_blob

log = logging.getLogger("orion.memory")

Kind = Literal["fact", "chunk", "message"]
RRF_K = 60
MAX_NOTA_BYTES = 5 * 1024 * 1024
KIND_WEIGHT: dict[str, float] = {"fact": 1.25, "chunk": 1.0, "message": 0.8}


def _like(texto: str) -> str:
    return texto.replace("\\", "\\\\").replace("%", "\\%").replace("_", "\\_")


def _percentil(ordenados: Sequence[int], p: int) -> int:
    """Percentil pelo vizinho mais próximo (lista já ordenada); 0 se vazia."""
    if not ordenados:
        return 0
    i = max(0, min(len(ordenados) - 1, -(-p * len(ordenados) // 100) - 1))
    return int(ordenados[i])


def _sessao(r: sqlite3.Row) -> Session:
    return Session(
        r["id"],
        r["channel"],
        r["title"],
        r["created_at"],
        r["last_active_at"],
        bool(r["pinned"]),
        bool(r["archived"]),
        bool(r["shelved"]),
        r["project_id"],
    )


class Embedder(Protocol):
    dim: int

    def embed(self, texts: list[str]) -> list[list[float]]: ...


@dataclass(frozen=True)
class Session:
    id: str
    channel: str
    title: str | None
    created_at: float
    last_active_at: float
    pinned: bool = False
    archived: bool = False  # arquivada: conversa importada do legado (só leitura)
    shelved: bool = False  # arquivada pelo usuário: some da barra, mensagens intactas
    project_id: int | None = None


@dataclass(frozen=True)
class Project:
    id: int
    name: str
    instructions: str
    archived: bool
    created_at: float
    updated_at: float


MAX_INSTRUCOES = 4000


@dataclass(frozen=True)
class Message:
    id: int
    session_id: str
    role: str
    text: str
    created_at: float
    provenance: dict[str, Any] | None


@dataclass(frozen=True)
class Fact:
    id: int
    text: str
    source: str
    created_at: float
    updated_at: float


@dataclass(frozen=True)
class Hit:
    kind: Kind
    id: int
    text: str
    source: str
    score: float
    via: str  # "fts", "vec" ou "fts+vec"


class MemoryStore:
    def __init__(
        self,
        path: Path | str,
        embedder: Embedder | None = None,
        clock: Callable[[], float] = time.time,
    ) -> None:
        self.path = Path(path)
        if str(path) != ":memory:":
            self.path.parent.mkdir(parents=True, exist_ok=True)
        self._embedder = embedder
        self._clock = clock
        self._lock = threading.RLock()
        self._conn = sqlite3.connect(str(path), check_same_thread=False)
        self._conn.row_factory = sqlite3.Row
        self._conn.execute("PRAGMA journal_mode=WAL")
        self._conn.execute("PRAGMA foreign_keys=ON")
        self._index = VectorIndex()
        self.vectors_available = embedder is not None  # há embedder: a busca vetorial está ligada
        self._init_schema()
        if self._embedder:
            self._ensure_vector_dim(self._embedder.dim)

    # ── infraestrutura ────────────────────────────────────────────────────
    def _init_schema(self) -> None:
        with self._tx() as c:
            existe = c.execute(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='meta'"
            ).fetchone()
            if not existe:
                c.executescript(DDL)
                c.execute("INSERT INTO meta VALUES ('schema_version', ?)", (str(SCHEMA_VERSION),))
                return
            versao = int(
                c.execute("SELECT value FROM meta WHERE key='schema_version'").fetchone()[0]
            )
            if versao > SCHEMA_VERSION:
                raise RuntimeError(
                    f"banco no esquema v{versao}, este Orion só entende até v{SCHEMA_VERSION}"
                )
            while versao < SCHEMA_VERSION:
                if versao not in MIGRATIONS:
                    raise RuntimeError(f"esquema v{versao} sem migração para v{versao + 1}")
                # DDL do SQLite é transacional: falhou no meio, volta tudo (rollback do `_tx`)
                c.executescript(
                    f"BEGIN;{MIGRATIONS[versao]}"
                    f"UPDATE meta SET value='{versao + 1}' WHERE key='schema_version';COMMIT;"
                )
                log.info("banco migrado: esquema v%d → v%d", versao, versao + 1)
                versao += 1

    @contextmanager
    def _tx(self) -> Iterator[sqlite3.Connection]:
        with self._lock:
            try:
                yield self._conn
                self._conn.commit()
            except BaseException:
                self._conn.rollback()
                raise

    # Usados por `orion.memory.ops` (lembretes, tarefas, grafo...): mesma conexão, mesmo lock.
    def transaction(self) -> AbstractContextManager[sqlite3.Connection]:
        return self._tx()

    def query(self, sql: str, params: Sequence[Any] = ()) -> list[sqlite3.Row]:
        with self._lock:
            return self._conn.execute(sql, tuple(params)).fetchall()

    @property
    def clock(self) -> Callable[[], float]:
        return self._clock

    def _meta(self, c: sqlite3.Connection, chave: str) -> str | None:
        r = c.execute("SELECT value FROM meta WHERE key=?", (chave,)).fetchone()
        return r[0] if r else None

    def _ensure_vector_dim(self, dim: int) -> None:
        with self._tx() as c:
            atual = self._meta(c, "embed_dim")
            if atual is not None and int(atual) != dim:
                raise RuntimeError(
                    f"embedder de {dim} dimensões, mas o índice tem {atual}: use reset_vectors()"
                )
            if atual is None:
                c.execute("INSERT INTO meta VALUES ('embed_dim', ?)", (str(dim),))

    def reset_vectors(self) -> None:
        """Descarta os vetores (troca de modelo de embedding); `embed_pending` refaz."""
        with self._tx() as c:
            c.execute("DELETE FROM vectors")
            c.execute("DELETE FROM meta WHERE key='embed_dim'")
            c.execute("UPDATE facts SET embedded=0")
            c.execute("UPDATE chunks SET embedded=0")
        self._index.invalidar()
        if self._embedder:
            self._ensure_vector_dim(self._embedder.dim)

    def close(self) -> None:
        with self._lock:
            self._conn.close()

    def ping(self) -> bool:
        with self._lock:
            return self._conn.execute("SELECT 1").fetchone()[0] == 1

    # ── sessões e mensagens (episódico), uma sessão ativa por canal ───────
    def _select(self, c: sqlite3.Connection, channel: str, session_id: str) -> None:
        c.execute(
            "INSERT INTO active_sessions(channel, session_id) VALUES (?,?)"
            " ON CONFLICT(channel) DO UPDATE SET session_id=excluded.session_id",
            (channel, session_id),
        )

    def _new_session(self, c: sqlite3.Connection, channel: str, title: str | None) -> Session:
        agora, sid = self._clock(), uuid.uuid4().hex
        c.execute(
            "INSERT INTO sessions(id, channel, title, created_at, last_active_at)"
            " VALUES (?,?,?,?,?)",
            (sid, channel, title, agora, agora),
        )
        self._select(c, channel, sid)
        return Session(sid, channel, title, agora, agora)

    def new_session(self, channel: str, title: str | None = None) -> Session:
        """Cria e seleciona no mesmo commit SQLite, preservando as outras conversas."""
        with self._tx() as c:
            return self._new_session(c, channel, title)

    def selected_session(self, channel: str) -> Session | None:
        """Consulta sem criar sessão; só o ponteiro persistido determina a ativa."""
        with self._lock:
            r = self._conn.execute(
                "SELECT s.* FROM active_sessions a JOIN sessions s ON s.id=a.session_id"
                " WHERE a.channel=? AND s.channel=? AND s.archived=0 AND s.deleted=0"
                " AND NOT EXISTS(SELECT 1 FROM imported WHERE kind='sessao' AND ref=s.id)",
                (channel, channel),
            ).fetchone()
        return _sessao(r) if r else None

    def active_session(self, channel: str) -> Session:
        """Sessão selecionada DO CANAL (Telegram, web e voz não se misturam); cria uma vazia
        quando ainda não há seleção válida (nunca pega a "mais recente" por conta própria)."""
        with self._tx() as c:
            return self.selected_session(channel) or self._new_session(c, channel, None)

    # ── importação (export do legado) ─────────────────────────────────────
    def import_session(
        self, external_id: str, channel: str, title: str | None, created_at: float
    ) -> tuple[str, bool]:
        """Sessão arquivada para um registro externo: (id interno, criada agora?).
        Reimportar devolve a mesma sessão."""
        with self._tx() as c:
            r = c.execute(
                "SELECT ref FROM imported WHERE kind='sessao' AND external_id=?", (external_id,)
            ).fetchone()
            if r:
                return r[0], False
            sid = uuid.uuid4().hex
            c.execute(
                "INSERT INTO sessions(id, channel, title, created_at, last_active_at, archived)"
                " VALUES (?,?,?,?,?,1)",
                (sid, channel, title, created_at, created_at),
            )
            c.execute("INSERT INTO imported VALUES ('sessao', ?, ?)", (external_id, sid))
            return sid, True

    def import_message(
        self,
        session_id: str,
        external_id: str,
        role: str,
        text: str,
        created_at: float,
        provenance: dict[str, Any] | None = None,
    ) -> bool:
        """Insere uma mensagem com a data original. False se já foi importada."""
        with self._tx() as c:
            novo = c.execute(
                "INSERT OR IGNORE INTO imported(kind, external_id) VALUES ('mensagem', ?)",
                (external_id,),
            ).rowcount
            if not novo:
                return False
            mid = c.execute(
                "INSERT INTO messages(session_id, role, text, created_at, provenance)"
                " VALUES (?,?,?,?,?)",
                (
                    session_id,
                    role,
                    text,
                    created_at,
                    json.dumps(provenance) if provenance else None,
                ),
            ).lastrowid
            c.execute(
                "UPDATE imported SET ref=? WHERE kind='mensagem' AND external_id=?",
                (str(mid), external_id),
            )
            c.execute(
                "UPDATE sessions SET last_active_at = MAX(last_active_at, ?) WHERE id=?",
                (created_at, session_id),
            )
            return True

    def get_session(self, session_id: str) -> Session | None:
        with self._lock:
            r = self._conn.execute("SELECT * FROM sessions WHERE id=?", (session_id,)).fetchone()
        if r is None:
            return None
        return _sessao(r)

    def archive_session(self, session_id: str) -> None:
        with self._tx() as c:
            c.execute("UPDATE sessions SET archived=1 WHERE id=?", (session_id,))
            c.execute(
                "UPDATE active_sessions SET session_id=NULL WHERE session_id=?", (session_id,)
            )

    def activate_session(self, session_id: str) -> Session | None:
        """Torna a conversa a ativa do canal dela (escolha persistente, não muda o canal).
        Só conversa aberta e não apagada: arquivada (importada) é só leitura. None se não dá."""
        agora = self._clock()
        with self._tx() as c:
            r = c.execute(
                "SELECT channel FROM sessions WHERE id=? AND archived=0 AND deleted=0",
                (session_id,),
            ).fetchone()
            if r is None:
                return None
            c.execute(
                "UPDATE sessions SET last_active_at=?, shelved=0 WHERE id=?", (agora, session_id)
            )
            self._select(c, r["channel"], session_id)
        return self.get_session(session_id)

    def rename_session(self, session_id: str, title: str) -> bool:
        titulo = " ".join(title.split())[:120]
        if not titulo:
            return False
        with self._tx() as c:
            return bool(
                c.execute(
                    "UPDATE sessions SET title=? WHERE id=? AND deleted=0", (titulo, session_id)
                ).rowcount
            )

    def pin_session(self, session_id: str, pinned: bool) -> bool:
        with self._tx() as c:
            return bool(
                c.execute(
                    "UPDATE sessions SET pinned=? WHERE id=? AND deleted=0",
                    (int(pinned), session_id),
                ).rowcount
            )

    # ── projetos (C31): instruções próprias para um grupo de conversas ────
    @staticmethod
    def _projeto(r: sqlite3.Row) -> Project:
        return Project(
            r["id"],
            r["name"],
            r["instructions"],
            bool(r["archived"]),
            r["created_at"],
            r["updated_at"],
        )

    def create_project(self, name: str, instructions: str = "") -> Project:
        nome = " ".join(name.split())[:80]
        if not nome:
            raise ValueError("nome do projeto vazio")
        agora = self._clock()
        with self._tx() as c:
            try:
                pid = c.execute(
                    "INSERT INTO projects(name, instructions, created_at, updated_at)"
                    " VALUES (?,?,?,?)",
                    (nome, instructions.strip()[:MAX_INSTRUCOES], agora, agora),
                ).lastrowid
            except sqlite3.IntegrityError:
                raise ValueError("já existe um projeto com esse nome") from None
            return self._projeto(c.execute("SELECT * FROM projects WHERE id=?", (pid,)).fetchone())

    def get_project(self, project_id: int) -> Project | None:
        with self._lock:
            r = self._conn.execute("SELECT * FROM projects WHERE id=?", (project_id,)).fetchone()
        return self._projeto(r) if r else None

    def list_projects(self, archived: bool = False) -> list[Project]:
        with self._lock:
            rows = self._conn.execute(
                "SELECT * FROM projects WHERE archived=? ORDER BY name", (int(archived),)
            ).fetchall()
        return [self._projeto(r) for r in rows]

    def update_project(
        self,
        project_id: int,
        *,
        name: str | None = None,
        instructions: str | None = None,
        archived: bool | None = None,
    ) -> Project:
        atual = self.get_project(project_id)
        if atual is None:
            raise KeyError(project_id)
        nome = " ".join(name.split())[:80] if name is not None else atual.name
        if not nome:
            raise ValueError("nome do projeto vazio")
        instr = (
            instructions.strip()[:MAX_INSTRUCOES]
            if instructions is not None
            else atual.instructions
        )
        arq = atual.archived if archived is None else archived
        with self._tx() as c:
            try:
                c.execute(
                    "UPDATE projects SET name=?, instructions=?, archived=?, updated_at=?"
                    " WHERE id=?",
                    (nome, instr, int(arq), self._clock(), project_id),
                )
            except sqlite3.IntegrityError:
                raise ValueError("já existe um projeto com esse nome") from None
        novo = self.get_project(project_id)
        assert novo is not None
        return novo

    def delete_project(self, project_id: int) -> bool:
        """Apaga o projeto; as conversas ficam, sem projeto (ON DELETE SET NULL)."""
        with self._tx() as c:
            return c.execute("DELETE FROM projects WHERE id=?", (project_id,)).rowcount > 0

    def assign_session(self, session_id: str, project_id: int | None) -> bool:
        if project_id is not None and self.get_project(project_id) is None:
            raise KeyError(project_id)
        with self._tx() as c:
            return bool(
                c.execute(
                    "UPDATE sessions SET project_id=? WHERE id=? AND deleted=0 AND archived=0",
                    (project_id, session_id),
                ).rowcount
            )

    def shelve_session(self, session_id: str, shelved: bool) -> bool:
        """Arquivar/desarquivar pelo usuário. Arquivar tira a conversa de ativa do canal; as
        mensagens ficam e a busca ainda a encontra. Importada (só leitura) não entra aqui."""
        with self._tx() as c:
            mudou = c.execute(
                "UPDATE sessions SET shelved=?, pinned=CASE WHEN ? THEN 0 ELSE pinned END"
                " WHERE id=? AND deleted=0 AND archived=0",
                (int(shelved), int(shelved), session_id),
            ).rowcount
            if mudou and shelved:
                c.execute(
                    "UPDATE active_sessions SET session_id=NULL WHERE session_id=?", (session_id,)
                )
            return bool(mudou)

    def delete_session(self, session_id: str) -> bool:
        """Apagar = esconder da lista (e deixar de ser a ativa). As mensagens FICAM: o que o Orion
        já consolidou delas continua na memória, e dá para desfazer pelo banco."""
        with self._tx() as c:
            apagou = c.execute(
                "UPDATE sessions SET deleted=1, archived=1, pinned=0 WHERE id=? AND deleted=0",
                (session_id,),
            ).rowcount
            c.execute(
                "UPDATE active_sessions SET session_id=NULL WHERE session_id=?", (session_id,)
            )
            return bool(apagou)

    def first_user_text(self, session_id: str) -> str | None:
        """Primeira fala do usuário: serve de título quando a conversa não foi renomeada."""
        with self._lock:
            r = self._conn.execute(
                "SELECT text FROM messages WHERE session_id=? AND role='user' ORDER BY id LIMIT 1",
                (session_id,),
            ).fetchone()
        return r[0] if r else None

    def list_sessions(self, channel: str | None = None, limit: int = 50) -> list[Session]:
        """Conversas não apagadas, da mais recente para a mais antiga."""
        with self._lock:
            rows = self._conn.execute(
                "SELECT * FROM sessions WHERE deleted=0 AND (? IS NULL OR channel=?)"
                " ORDER BY last_active_at DESC LIMIT ?",
                (channel, channel, limit),
            ).fetchall()
        return [_sessao(r) for r in rows]

    def session_visible(self, session_id: str, channel: str) -> Session | None:
        """A conversa, se a barra do canal a mostra (do canal ou importada, e não apagada)."""
        with self._lock:
            r = self._conn.execute(
                "SELECT * FROM sessions WHERE id=? AND deleted=0 AND (channel=? OR id IN"
                " (SELECT ref FROM imported WHERE kind='sessao'))",
                (session_id, channel),
            ).fetchone()
        return _sessao(r) if r else None

    def list_sessions_ui(
        self,
        channel: str,
        limit: int = 100,
        *,
        shelved: bool = False,
        project_id: int | None = None,
    ) -> list[Session]:
        """O que a barra lateral mostra: as conversas do canal + as importadas do legado (somente
        leitura), sem as apagadas; fixadas primeiro, depois a atividade mais recente.
        `shelved=True` lista só as que o usuário arquivou."""
        with self._lock:
            rows = self._conn.execute(
                "SELECT * FROM sessions WHERE deleted=0 AND shelved=? AND (channel=? OR id IN"
                " (SELECT ref FROM imported WHERE kind='sessao'))"
                " AND (? IS NULL OR project_id=?)"
                " ORDER BY pinned DESC, last_active_at DESC LIMIT ?",
                (int(shelved), channel, project_id, project_id, limit),
            ).fetchall()
        return [_sessao(r) for r in rows]

    def search_sessions(
        self, channel: str, query: str, limit: int = 20, offset: int = 0
    ) -> list[dict[str, Any]]:
        """Busca por título e por conteúdo nas conversas que a barra do canal enxerga (inclui as
        arquivadas pelo usuário, não as apagadas). Uma linha por conversa, com um trecho."""
        q = fts_query(query)
        if q is None:
            return []
        limit, offset = max(1, min(limit, 50)), max(0, offset)
        with self._lock:
            # snippet() não funciona com GROUP BY: pega as melhores mensagens e fica com a
            # primeira de cada conversa
            rows = self._conn.execute(
                "SELECT s.id AS sid,"
                " snippet(messages_fts, 0, '[', ']', '…', 12) AS trecho"
                " FROM messages_fts f JOIN messages m ON m.id=f.rowid"
                " JOIN sessions s ON s.id=m.session_id"
                " WHERE messages_fts MATCH ? AND m.role IN ('user','assistant') AND s.deleted=0"
                " AND (s.channel=? OR s.id IN (SELECT ref FROM imported WHERE kind='sessao'))"
                " ORDER BY f.rank LIMIT ?",
                (q, channel, (limit + offset) * 8),
            ).fetchall()
            titulos = self._conn.execute(
                "SELECT id FROM sessions WHERE deleted=0 AND title IS NOT NULL"
                " AND (channel=? OR id IN (SELECT ref FROM imported WHERE kind='sessao'))"
                " AND lower(title) LIKE ? ESCAPE '\\' LIMIT ?",
                (channel, "%" + _like(query.lower()) + "%", limit),
            ).fetchall()
        achados: dict[str, str | None] = {}
        for r in rows:
            achados.setdefault(r["sid"], r["trecho"])
        for t in titulos:
            achados.setdefault(t["id"], None)
        saida: list[dict[str, Any]] = []
        for sid, trecho in list(achados.items())[offset : offset + limit]:
            sess = self.get_session(sid)
            if sess is not None:
                saida.append({"sessao": sess, "trecho": trecho})
        return saida

    def add_message(
        self, session_id: str, role: str, text: str, provenance: dict[str, Any] | None = None
    ) -> Message:
        agora = self._clock()
        with self._tx() as c:
            cur = c.execute(
                "INSERT INTO messages(session_id, role, text, created_at, provenance)"
                " VALUES (?,?,?,?,?)",
                (session_id, role, text, agora, json.dumps(provenance) if provenance else None),
            )
            c.execute("UPDATE sessions SET last_active_at=? WHERE id=?", (agora, session_id))
            mid = cur.lastrowid
        return Message(int(mid or 0), session_id, role, text, agora, provenance)

    def history(self, session_id: str, limit: int = 50, *, after: int = 0) -> list[Message]:
        with self._lock:
            rows = self._conn.execute(
                "SELECT * FROM (SELECT * FROM messages WHERE session_id=? AND id>?"
                " ORDER BY id DESC LIMIT ?)"
                " ORDER BY id",
                (session_id, after, limit),
            ).fetchall()
        return [
            Message(
                r["id"],
                r["session_id"],
                r["role"],
                r["text"],
                r["created_at"],
                json.loads(r["provenance"]) if r["provenance"] else None,
            )
            for r in rows
        ]

    def context_history(self, session_id: str, limit: int = 50) -> list[Message]:
        return self.history(
            session_id, limit, after=self.counter_get(f"history_after:{session_id}")
        )

    def history_page(
        self,
        session_id: str,
        *,
        limit: int = 50,
        before: int | None = None,
        complete: bool = False,
    ) -> tuple[list[Message], int, int | None]:
        """Página por ID: timestamps repetidos/importados não duplicam nem pulam mensagens."""
        with self._lock:
            after = 0 if complete else self.counter_get(f"history_after:{session_id}")
            total = self._conn.execute(
                "SELECT count(*) FROM messages WHERE session_id=? AND id>?"
                " AND role IN ('user','assistant')",
                (session_id, after),
            ).fetchone()[0]
            rows = self._conn.execute(
                "SELECT * FROM messages WHERE session_id=? AND id>? AND (? IS NULL OR id<?)"
                " AND role IN ('user','assistant') ORDER BY id DESC LIMIT ?",
                (session_id, after, before, before, limit + 1),
            ).fetchall()
            more = len(rows) > limit
            rows = rows[:limit]
            next_before = rows[-1]["id"] if more else None
            messages = [
                Message(
                    row["id"],
                    row["session_id"],
                    row["role"],
                    row["text"],
                    row["created_at"],
                    json.loads(row["provenance"]) if row["provenance"] else None,
                )
                for row in reversed(rows)
            ]
            return messages, total, next_before

    def clear_context(self, session_id: str) -> int:
        """Avança o limite de contexto; não apaga mensagens, fatos, documentos ou vetores."""
        with self._tx() as c:
            session = self.get_session(session_id)
            if session is None:
                raise KeyError(session_id)
            if session.archived:
                raise ValueError("sessão somente leitura")
            last = c.execute(
                "SELECT coalesce(max(id),0) FROM messages WHERE session_id=?", (session_id,)
            ).fetchone()[0]
            c.execute(
                "INSERT INTO meta(key,value) VALUES (?,?) "
                "ON CONFLICT(key) DO UPDATE SET value=excluded.value",
                (f"counter:history_after:{session_id}", str(last)),
            )
            return last

    # ── fatos sobre o Antônio: com fonte e data, auditáveis, editáveis ────
    @staticmethod
    def _norm(texto: str) -> str:
        return re.sub(r"\s+", " ", texto).strip().casefold()

    def add_fact(self, text: str, source: str) -> Fact:
        """Registra um fato. Fato idêntico (ignorando caixa/espaços) só renova a data."""
        texto = re.sub(r"\s+", " ", text).strip()
        if not texto:
            raise ValueError("fato vazio")
        agora = self._clock()
        with self._tx() as c:
            for r in c.execute("SELECT id, text FROM facts"):
                if self._norm(r["text"]) == self._norm(texto):
                    c.execute("UPDATE facts SET updated_at=? WHERE id=?", (agora, r["id"]))
                    return self._fact(c, r["id"])
            fid = c.execute(
                "INSERT INTO facts(text, source, created_at, updated_at) VALUES (?,?,?,?)",
                (texto, source, agora, agora),
            ).lastrowid
            fato = self._fact(c, int(fid or 0))
        self._try_embed()
        return fato

    def _fact(self, c: sqlite3.Connection, fid: int) -> Fact:
        r = c.execute("SELECT * FROM facts WHERE id=?", (fid,)).fetchone()
        return Fact(r["id"], r["text"], r["source"], r["created_at"], r["updated_at"])

    def update_fact(self, fact_id: int, text: str, source: str | None = None) -> Fact:
        texto = re.sub(r"\s+", " ", text).strip()
        if not texto:
            raise ValueError("fato vazio")
        with self._tx() as c:
            if not c.execute("SELECT 1 FROM facts WHERE id=?", (fact_id,)).fetchone():
                raise KeyError(fact_id)
            c.execute(
                "UPDATE facts SET text=?, source=COALESCE(?, source), updated_at=?,"
                " embedded=0 WHERE id=?",
                (texto, source, self._clock(), fact_id),
            )
            self._drop_vec(c, "fact", [fact_id])
            fato = self._fact(c, fact_id)
        self._try_embed()
        return fato

    def forget_fact(self, fact_id: int) -> bool:
        """Esquece de verdade: apaga o texto, o índice FTS e o vetor."""
        with self._tx() as c:
            self._drop_vec(c, "fact", [fact_id])
            return c.execute("DELETE FROM facts WHERE id=?", (fact_id,)).rowcount > 0

    def facts(self) -> list[Fact]:
        """Tudo o que o Orion sabe sobre o Antônio, do mais recente ao mais antigo."""
        with self._lock:
            rows = self._conn.execute(
                "SELECT * FROM facts ORDER BY updated_at DESC, id DESC"
            ).fetchall()
        return [
            Fact(r["id"], r["text"], r["source"], r["created_at"], r["updated_at"]) for r in rows
        ]

    def search_facts(self, query: str, limit: int = 20) -> list[Fact]:
        """Fatos que casam com a consulta (FTS) ou que a contêm como trecho literal."""
        limit = max(1, min(limit, 100))
        q = fts_query(query)
        with self._lock:
            ids: list[int] = []
            if q:
                ids = [
                    r[0]
                    for r in self._conn.execute(
                        "SELECT rowid FROM facts_fts WHERE facts_fts MATCH ? ORDER BY rank LIMIT ?",
                        (q, limit),
                    )
                ]
            ids += [
                r[0]
                for r in self._conn.execute(
                    "SELECT id FROM facts WHERE lower(text) LIKE ? ESCAPE '\\' LIMIT ?",
                    ("%" + _like(query.strip().lower()) + "%", limit),
                )
                if r[0] not in ids
            ]
            achados = [self.get_fact(i) for i in ids[:limit]]
        return [f for f in achados if f is not None]

    def duplicate_facts(self, limiar: float = 0.75) -> list[tuple[Fact, Fact, float]]:
        """Pares de fatos quase iguais (palavras em comum / palavras no total, sem acento nem
        caixa). Só sugere: quem decide qual apagar é o Antônio (`orion esquecer`)."""

        def palavras(t: str) -> set[str]:
            sem = unicodedata.normalize("NFKD", t.casefold()).encode("ascii", "ignore").decode()
            return {w for w in re.findall(r"[a-z0-9]+", sem) if len(w) > 2}

        fatos = self.facts()
        conj = {f.id: palavras(f.text) for f in fatos}
        achados: list[tuple[Fact, Fact, float]] = []
        for i, a in enumerate(fatos):
            for b in fatos[i + 1 :]:
                u = conj[a.id] | conj[b.id]
                if not u:
                    continue
                j = len(conj[a.id] & conj[b.id]) / len(u)
                if j >= limiar:
                    achados.append((a, b, j))
        return sorted(achados, key=lambda t: -t[2])

    def get_fact(self, fact_id: int) -> Fact | None:
        with self._lock:
            r = self._conn.execute("SELECT * FROM facts WHERE id=?", (fact_id,)).fetchone()
        if r is None:
            return None
        return Fact(r["id"], r["text"], r["source"], r["created_at"], r["updated_at"])

    def facts_markdown(self) -> str:
        """Nota para o vault do Obsidian (a pessoa confere e corrige em texto)."""
        agora = datetime.fromtimestamp(self._clock()).astimezone()
        linhas = [
            "---",
            "type: fatos-orion",
            f"gerado: {agora.isoformat(timespec='seconds')}",
            "fonte: orion",
            "---",
            "",
            "# O que o Orion sabe sobre o Antônio",
            "",
        ]
        for f in self.facts():
            data = datetime.fromtimestamp(f.updated_at).astimezone().strftime("%Y-%m-%d")
            linhas.append(f"- {f.text} _(fonte: {f.source}, {data}, id {f.id})_")
        return "\n".join(linhas) + "\n"

    def export_facts(self, pasta: Path | str, nome: str = "Fatos do Orion.md") -> Path:
        destino = Path(pasta) / nome
        destino.parent.mkdir(parents=True, exist_ok=True)
        destino.write_text(self.facts_markdown(), encoding="utf-8")
        return destino

    # ── documentos (vault do Obsidian, uploads) ───────────────────────────
    def index_document(
        self, source: str, title: str, text: str, project_id: int | None = None
    ) -> Literal["new", "updated", "same"]:
        """Indexa (ou reindexa) um documento. Idempotente por hash de conteúdo.
        Com `project_id`, o documento só entra no contexto automático das conversas do projeto."""
        h = hashlib.sha256((text + f"|{project_id}").encode()).hexdigest()
        corpo = strip_frontmatter(text)
        with self._tx() as c:
            r = c.execute(
                "SELECT id, content_hash FROM documents WHERE source=?", (source,)
            ).fetchone()
            if r and r["content_hash"] == h:
                return "same"
            if r:
                self._delete_document(c, r["id"])
            did = c.execute(
                "INSERT INTO documents(source, title, content_hash, indexed_at, project_id)"
                " VALUES (?,?,?,?,?)",
                (source, title, h, self._clock(), project_id),
            ).lastrowid
            for i, trecho in enumerate(chunk_text(corpo)):
                c.execute(
                    "INSERT INTO chunks(document_id, ord, text) VALUES (?,?,?)",
                    (did, i, f"{title}\n\n{trecho}"),
                )
        self._try_embed()
        return "updated" if r else "new"

    def _delete_document(self, c: sqlite3.Connection, doc_id: int) -> None:
        ids = [x[0] for x in c.execute("SELECT id FROM chunks WHERE document_id=?", (doc_id,))]
        self._drop_vec(c, "chunk", ids)
        c.execute("DELETE FROM documents WHERE id=?", (doc_id,))

    def list_documents(self, prefix: str = "upload:") -> list[dict[str, Any]]:
        """Documentos cuja origem começa com `prefix` (os enviados pela interface), com o
        número de trechos. O texto em si não vai na lista."""
        with self._lock:
            rows = self._conn.execute(
                "SELECT d.id, d.source, d.title, d.indexed_at, d.project_id,"
                " (SELECT COUNT(*) FROM chunks c WHERE c.document_id=d.id) AS trechos"
                " FROM documents d WHERE d.source LIKE ? ESCAPE '\\' ORDER BY d.indexed_at DESC",
                (_like(prefix) + "%",),
            ).fetchall()
        return [dict(r) for r in rows]

    def remove_document_id(self, doc_id: int, prefix: str = "upload:") -> bool:
        with self._tx() as c:
            r = c.execute("SELECT id, source FROM documents WHERE id=?", (doc_id,)).fetchone()
            if not r or not str(r["source"]).startswith(prefix):
                return False  # só apaga o que veio de upload; nota do vault não é daqui
            self._delete_document(c, r["id"])
            return True

    def remove_document(self, source: str) -> bool:
        with self._tx() as c:
            r = c.execute("SELECT id FROM documents WHERE source=?", (source,)).fetchone()
            if not r:
                return False
            self._delete_document(c, r["id"])
            return True

    def index_vault(self, pasta: Path | str) -> dict[str, int]:
        """Indexa os .md da pasta (ignora .obsidian) e remove os que sumiram."""
        raiz = Path(pasta)
        contagem = {"new": 0, "updated": 0, "same": 0, "removed": 0}
        vistos: set[str] = set()
        for arq in sorted(raiz.rglob("*.md")):
            rel = arq.relative_to(raiz)
            if any(p.startswith(".") for p in rel.parts):
                continue
            if arq.stat().st_size > MAX_NOTA_BYTES:  # anexo/export gigante não é nota
                continue
            fonte = rel.as_posix()
            vistos.add(fonte)
            contagem[self.index_document(fonte, arq.stem, arq.read_text(encoding="utf-8"))] += 1
        with self._lock:
            antigos = [r[0] for r in self._conn.execute("SELECT source FROM documents")]
        for fonte in antigos:
            if fonte not in vistos and self.remove_document(fonte):
                contagem["removed"] += 1
        return contagem

    # ── vetores ───────────────────────────────────────────────────────────
    def _drop_vec(self, c: sqlite3.Connection, kind: str, ids: Sequence[int]) -> None:
        c.executemany("DELETE FROM vectors WHERE kind=? AND ref_id=?", [(kind, i) for i in ids])
        self._index.invalidar(kind)

    def _try_embed(self) -> None:
        try:
            self.embed_pending()
        except Exception as e:  # noqa: BLE001 — API de embedding fora do ar não derruba a escrita
            log.warning("embedding adiado (fica em fila): %s", e)

    def embed_pending(self, lote: int = 32) -> int:
        """Gera vetores dos itens ainda sem vetor. Devolve quantos foram feitos."""
        if self._embedder is None:
            return 0
        feitos = 0
        for tabela, kind in (("facts", "fact"), ("chunks", "chunk")):
            while True:
                with self._lock:
                    rows = self._conn.execute(
                        f"SELECT id, text FROM {tabela} WHERE embedded=0 LIMIT ?", (lote,)
                    ).fetchall()
                if not rows:
                    break
                vetores = self._embedder.embed([r["text"] for r in rows])
                with self._tx() as c:
                    for r, v in zip(rows, vetores, strict=True):
                        c.execute(
                            "INSERT OR REPLACE INTO vectors(kind, ref_id, embedding)"
                            " VALUES (?,?,?)",
                            (kind, r["id"], para_blob(v)),
                        )
                        c.execute(f"UPDATE {tabela} SET embedded=1 WHERE id=?", (r["id"],))
                    self._index.invalidar(kind)
                feitos += len(rows)
        return feitos

    # ── busca híbrida ─────────────────────────────────────────────────────
    _SQL_FTS: ClassVar[dict[str, str]] = {
        "fact": (
            "SELECT t.id, t.text, t.source FROM facts_fts JOIN facts t ON t.id=facts_fts.rowid"
            " WHERE facts_fts MATCH ? ORDER BY bm25(facts_fts) LIMIT ?"
        ),
        "chunk": (
            "SELECT t.id, t.text, d.source FROM chunks_fts JOIN chunks t ON t.id=chunks_fts.rowid"
            " JOIN documents d ON d.id=t.document_id"
            " WHERE chunks_fts MATCH ? ORDER BY bm25(chunks_fts) LIMIT ?"
        ),
        "message": (
            "SELECT t.id, t.text, 'sessão ' || s.channel AS source FROM messages_fts"
            " JOIN messages t ON t.id=messages_fts.rowid JOIN sessions s ON s.id=t.session_id"
            " WHERE messages_fts MATCH ? AND t.role IN ('user','assistant')"
            " ORDER BY bm25(messages_fts) LIMIT ?"
        ),
    }
    _SQL_ROW: ClassVar[dict[str, str]] = {
        "fact": "SELECT id, text, source FROM facts WHERE id=?",
        "chunk": (
            "SELECT t.id, t.text, d.source FROM chunks t JOIN documents d ON d.id=t.document_id"
            " WHERE t.id=?"
        ),
    }

    def search(
        self,
        query: str,
        k: int = 8,
        kinds: Sequence[Kind] = ("fact", "chunk", "message"),
        project_id: int | Literal["todos"] | None = "todos",
    ) -> list[Hit]:
        """`project_id="todos"` (padrão, usado pelas ferramentas) não filtra nada. Um número ou
        None isola o contexto automático: documentos de projeto só aparecem nas conversas dele."""
        fq = fts_query(query)
        amplo = max(k * 3, 20)
        listas: list[tuple[Kind, str, list[tuple[int, str, str]]]] = []
        with self._lock:
            for kind in kinds:
                rows = self._conn.execute(self._SQL_FTS[kind], (fq, amplo)).fetchall() if fq else []
                listas.append((kind, "fts", [(r[0], r[1], r[2]) for r in rows]))
            vec_kinds = [kd for kd in kinds if kd in ("fact", "chunk")]
            if self._embedder and vec_kinds:
                listas.extend(self._vec_lists(query, vec_kinds, amplo))

        placar: dict[tuple[str, int], float] = {}
        textos: dict[tuple[str, int], tuple[str, str]] = {}
        vias: dict[tuple[str, int], set[str]] = {}
        for kind, via, itens in listas:
            for rank, (iid, texto, fonte) in enumerate(itens, start=1):
                chave = (kind, iid)
                placar[chave] = placar.get(chave, 0.0) + KIND_WEIGHT[kind] / (RRF_K + rank)
                textos[chave] = (texto, fonte)
                vias.setdefault(chave, set()).add(via)
        if project_id != "todos":
            placar = self._sem_documentos_de_outro_projeto(placar, project_id)
        ordem = sorted(placar, key=lambda x: placar[x], reverse=True)[:k]
        return [
            Hit(kd, iid, *textos[(kd, iid)], placar[(kd, iid)], "+".join(sorted(vias[(kd, iid)])))  # type: ignore[arg-type]
            for kd, iid in ordem
        ]

    def _sem_documentos_de_outro_projeto(
        self, placar: dict[tuple[str, int], float], project_id: int | None
    ) -> dict[tuple[str, int], float]:
        chunks = [i for kd, i in placar if kd == "chunk"]
        if not chunks:
            return placar
        marcas = ",".join("?" * len(chunks))
        with self._lock:
            fora = {
                r[0]
                for r in self._conn.execute(
                    "SELECT c.id FROM chunks c JOIN documents d ON d.id=c.document_id"
                    f" WHERE c.id IN ({marcas}) AND d.project_id IS NOT NULL"
                    " AND d.project_id IS NOT ?",
                    (*chunks, project_id),
                )
            }
        return {k: v for k, v in placar.items() if not (k[0] == "chunk" and k[1] in fora)}

    def _vec_lists(
        self, query: str, kinds: Sequence[str], amplo: int
    ) -> list[tuple[Kind, str, list[tuple[int, str, str]]]]:
        assert self._embedder is not None
        try:
            como_consulta = getattr(self._embedder, "embed_query", None)
            consulta = como_consulta(query) if como_consulta else self._embedder.embed([query])[0]
        except Exception as e:  # noqa: BLE001 — embedding fora do ar: segue só por palavra-chave
            log.warning("busca vetorial indisponível, usando só palavra-chave: %s", e)
            return []
        saida: list[tuple[Kind, str, list[tuple[int, str, str]]]] = []
        for kind in kinds:
            ids = self._index.topk(
                kind,
                consulta,
                amplo,
                lambda kind=kind: [
                    (r[0], r[1])
                    for r in self._conn.execute(
                        "SELECT ref_id, embedding FROM vectors WHERE kind=?", (kind,)
                    )
                ],
            )
            itens = []
            for i in ids:
                r = self._conn.execute(self._SQL_ROW[kind], (i,)).fetchone()
                if r:
                    itens.append((r[0], r[1], r[2]))
            saida.append((kind, "vec", itens))  # type: ignore[arg-type]
        return saida

    # ── contadores (cota diária das CLIs delegadas etc.) ─────────────────
    def counter_get(self, chave: str) -> int:
        with self._lock:
            r = self._conn.execute(
                "SELECT value FROM meta WHERE key=?", (f"counter:{chave}",)
            ).fetchone()
        return int(r[0]) if r else 0

    def counter_incr(self, chave: str, por: int = 1) -> int:
        with self._tx() as c:
            c.execute(
                "INSERT INTO meta(key, value) VALUES (?, ?)"
                " ON CONFLICT(key) DO UPDATE SET value = CAST(value AS INTEGER) + ?",
                (f"counter:{chave}", str(por), por),
            )
            return int(
                c.execute("SELECT value FROM meta WHERE key=?", (f"counter:{chave}",)).fetchone()[0]
            )

    # ── valores livres no `meta` (estado dos modos, regra 48) ──────────────
    def meta_get(self, chave: str) -> str | None:
        with self._lock:
            return self._meta(self._conn, f"valor:{chave}")

    def meta_set(self, chave: str, valor: str | None) -> None:
        """`None` apaga. Chave com prefixo próprio: não colide com contador nem com `watch:`."""
        with self._tx() as c:
            if valor is None:
                c.execute("DELETE FROM meta WHERE key=?", (f"valor:{chave}",))
            else:
                c.execute(
                    "INSERT INTO meta(key, value) VALUES (?, ?)"
                    " ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    (f"valor:{chave}", valor),
                )

    # ── registro de saída (regra 47): tamanho e tipo, nunca o conteúdo ─────
    def add_external_call(
        self,
        *,
        provider: str,
        kind: str,
        ok: bool,
        latency_ms: int,
        model: str = "",
        bytes_out: int = 0,
        bytes_in: int = 0,
        content_kind: str = "",
        ts: float | None = None,
    ) -> int:
        with self._tx() as c:
            return int(
                c.execute(
                    "INSERT INTO external_calls(ts, provider, kind, model, ok, latency_ms,"
                    " bytes_out, bytes_in, content_kind) VALUES (?,?,?,?,?,?,?,?,?)",
                    (
                        self._clock() if ts is None else ts,
                        provider[:60],
                        kind[:30],
                        model[:120],
                        1 if ok else 0,
                        max(0, int(latency_ms)),
                        max(0, int(bytes_out)),
                        max(0, int(bytes_in)),
                        content_kind[:20],
                    ),
                ).lastrowid
                or 0
            )

    def external_calls(
        self, desde: float, *, prefixo: str | None = None, limite: int = 5000
    ) -> list[dict[str, Any]]:
        """Linhas desde `desde` (mais novas primeiro); `prefixo` filtra o provedor."""
        filtro = ""
        params: list[Any] = [desde]
        if prefixo:
            filtro = " AND provider LIKE ? ESCAPE '\\'"
            params.append(_like(prefixo) + "%")
        rows = self.query(
            f"SELECT * FROM external_calls WHERE ts>=?{filtro} ORDER BY ts DESC, id DESC LIMIT ?",
            (*params, max(1, min(int(limite), 50_000))),
        )
        return [dict(r) for r in rows]

    def external_calls_count(self, provider_prefix: str, desde: float) -> int:
        """Chamadas de um provedor (ou família, `gateway:`) desde `desde`: a base da cota."""
        with self._lock:
            r = self._conn.execute(
                "SELECT COUNT(*) FROM external_calls WHERE ts>=? AND provider LIKE ? ESCAPE '\\'",
                (desde, _like(provider_prefix) + "%"),
            ).fetchone()
        return int(r[0]) if r else 0

    def external_calls_summary(self, desde: float) -> list[dict[str, Any]]:
        """Por provedor e tipo: chamadas, falhas, p50/p95 da latência, bytes e o modelo mais
        usado."""
        grupos: dict[tuple[str, str], list[sqlite3.Row]] = {}
        for r in self.query(
            "SELECT provider, kind, model, ok, latency_ms, bytes_out, bytes_in"
            " FROM external_calls WHERE ts>=? ORDER BY ts",
            (desde,),
        ):
            grupos.setdefault((r["provider"], r["kind"]), []).append(r)
        saida = []
        for (provider, kind), linhas in sorted(grupos.items()):
            lat = sorted(int(r["latency_ms"]) for r in linhas)
            modelos: dict[str, int] = {}
            for r in linhas:
                if r["model"]:
                    modelos[r["model"]] = modelos.get(r["model"], 0) + 1
            saida.append(
                {
                    "provider": provider,
                    "kind": kind,
                    "chamadas": len(linhas),
                    "falhas": sum(1 for r in linhas if not r["ok"]),
                    "p50_ms": _percentil(lat, 50),
                    "p95_ms": _percentil(lat, 95),
                    "bytes_out": sum(int(r["bytes_out"]) for r in linhas),
                    "bytes_in": sum(int(r["bytes_in"]) for r in linhas),
                    "modelo": max(modelos, key=lambda m: modelos[m]) if modelos else "",
                }
            )
        return saida

    def prune_external_calls(self, dias: int = 90) -> int:
        with self._tx() as c:
            return c.execute(
                "DELETE FROM external_calls WHERE ts < ?", (self._clock() - dias * 86400,)
            ).rowcount

    # ── biblioteca de resultados (C34) ────────────────────────────────────
    def add_artifact(
        self,
        *,
        session_id: str | None,
        kind: str,
        name: str,
        stored: str,
        size: int,
        tool: str,
    ) -> dict[str, Any]:
        """Registra um resultado; mesmo nome de novo vira a versão seguinte (a antiga fica)."""
        with self._tx() as c:
            ant = c.execute(
                "SELECT id, version FROM artifacts WHERE name=? AND kind=?"
                " ORDER BY version DESC LIMIT 1",
                (name, kind),
            ).fetchone()
            projeto = None
            if session_id:
                r = c.execute(
                    "SELECT project_id FROM sessions WHERE id=?", (session_id,)
                ).fetchone()
                projeto = r[0] if r else None
            aid = c.execute(
                "INSERT INTO artifacts(session_id, project_id, kind, name, stored, bytes, tool,"
                " version, parent_id, created_at) VALUES (?,?,?,?,?,?,?,?,?,?)",
                (
                    session_id, projeto, kind, name, stored, size, tool,
                    (ant["version"] + 1) if ant else 1, ant["id"] if ant else None, self._clock(),
                ),
            ).lastrowid  # fmt: skip
            return self._artifact_row(c, int(aid or 0))

    @staticmethod
    def _artifact_row(c: sqlite3.Connection, aid: int) -> dict[str, Any]:
        r = c.execute("SELECT * FROM artifacts WHERE id=?", (aid,)).fetchone()
        return dict(r) if r else {}

    def get_artifact(self, aid: int) -> dict[str, Any] | None:
        with self._lock:
            d = self._artifact_row(self._conn, aid)
        return d or None

    def list_artifacts(
        self, project_id: int | None = None, limit: int = 100
    ) -> list[dict[str, Any]]:
        with self._lock:
            rows = self._conn.execute(
                "SELECT a.*, s.title AS sessao_titulo FROM artifacts a"
                " LEFT JOIN sessions s ON s.id=a.session_id"
                " WHERE (? IS NULL OR a.project_id=?)"
                " ORDER BY a.created_at DESC, a.id DESC LIMIT ?",
                (project_id, project_id, max(1, min(limit, 500))),
            ).fetchall()
        return [dict(r) for r in rows]

    def delete_artifact(self, aid: int) -> str | None:
        """Apaga o registro e devolve o nome do arquivo guardado (para o chamador apagar)."""
        with self._tx() as c:
            r = c.execute("SELECT stored FROM artifacts WHERE id=?", (aid,)).fetchone()
            if not r:
                return None
            c.execute("DELETE FROM artifacts WHERE id=?", (aid,))
            return str(r["stored"])

    # ── memória da tela (D3): texto de OCR, retenção curta, nunca imagem ───
    def add_screen(self, text: str, title: str = "") -> int:
        with self._tx() as c:
            return int(
                c.execute(
                    "INSERT INTO screen_log(ts, title, text) VALUES (?,?,?)",
                    (self._clock(), " ".join(title.split())[:200], text),
                ).lastrowid
                or 0
            )

    def last_screen_text(self) -> str | None:
        with self._lock:
            r = self._conn.execute(
                "SELECT text FROM screen_log ORDER BY id DESC LIMIT 1"
            ).fetchone()
        return r[0] if r else None

    def search_screen(self, query: str, limit: int = 5, dias: int = 30) -> list[dict[str, Any]]:
        q = fts_query(query)
        if q is None:
            return []
        desde = self._clock() - dias * 86400
        with self._lock:
            rows = self._conn.execute(
                "SELECT s.id, s.ts, s.title,"
                " snippet(screen_log_fts, 0, '[', ']', '…', 24) AS trecho"
                " FROM screen_log_fts f JOIN screen_log s ON s.id=f.rowid"
                " WHERE screen_log_fts MATCH ? AND s.ts>=? ORDER BY f.rank LIMIT ?",
                (q, desde, max(1, min(limit, 20))),
            ).fetchall()
        return [
            {"id": r["id"], "ts": r["ts"], "titulo": r["title"], "trecho": r["trecho"]}
            for r in rows
        ]

    def prune_screen(self, dias: int) -> int:
        with self._tx() as c:
            return c.execute(
                "DELETE FROM screen_log WHERE ts<?", (self._clock() - dias * 86400,)
            ).rowcount

    def clear_screen(self) -> int:
        with self._tx() as c:
            return c.execute("DELETE FROM screen_log").rowcount

    def screen_count(self, desde: float = 0.0) -> int:
        with self._lock:
            return int(
                self._conn.execute(
                    "SELECT COUNT(*) FROM screen_log WHERE ts>=?", (desde,)
                ).fetchone()[0]
            )

    def counters_with_prefix(self, prefixo: str) -> dict[str, int]:
        """Contadores cujo nome começa com `prefixo` (nome sem o prefixo interno)."""
        base = "counter:"
        with self._lock:
            rows = self._conn.execute(
                "SELECT key, value FROM meta WHERE key LIKE ? ESCAPE '\\'",
                (base + _like(prefixo) + "%",),
            ).fetchall()
        return {r[0][len(base) :]: int(r[1]) for r in rows}

    def counter_set(self, chave: str, valor: int) -> None:
        with self._tx() as c:
            c.execute(
                "INSERT INTO meta(key, value) VALUES (?, ?)"
                " ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                (f"counter:{chave}", str(valor)),
            )

    # ── backup e restauração ──────────────────────────────────────────────
    def backup_to(self, destino: Path | str) -> Path:
        """Cópia consistente do banco (API de backup do SQLite, com o app no ar)."""
        destino = Path(destino)
        destino.parent.mkdir(parents=True, exist_ok=True)
        tmp = destino.with_name(destino.name + ".tmp")
        tmp.unlink(missing_ok=True)
        copia = sqlite3.connect(str(tmp))
        try:
            with self._lock:
                self._conn.backup(copia)
        finally:
            copia.close()
        os.replace(tmp, destino)
        return destino

    def daily_backup(self, pasta: Path | str, manter: int = 7) -> Path | None:
        """Um backup por dia (nome com a data), mantendo os últimos `manter`."""
        pasta = Path(pasta)
        hoje = datetime.fromtimestamp(self._clock()).strftime("%Y%m%d")
        feito: Path | None = None
        if not list(pasta.glob(f"orion-{hoje}-*.db")):
            carimbo = datetime.fromtimestamp(self._clock()).strftime("%Y%m%d-%H%M%S")
            feito = self.backup_to(pasta / f"orion-{carimbo}.db")
        for velho in sorted(pasta.glob("orion-*.db"))[:-manter] if manter > 0 else []:
            velho.unlink(missing_ok=True)
        return feito

    @staticmethod
    def verify_backup(arquivo: Path | str) -> dict[str, int]:
        """Abre o backup somente-leitura, confere integridade e devolve as contagens."""
        conn = sqlite3.connect(f"file:{Path(arquivo).as_posix()}?mode=ro", uri=True)
        try:
            if conn.execute("PRAGMA integrity_check").fetchone()[0] != "ok":
                raise ValueError("backup corrompido (integrity_check)")
            try:
                versao = conn.execute(
                    "SELECT value FROM meta WHERE key='schema_version'"
                ).fetchone()
            except sqlite3.DatabaseError:
                raise ValueError("backup com esquema diferente") from None
            # backup de esquema mais antigo é aceito: o banco sobe sozinho ao abrir (MIGRATIONS)
            if not versao or not 1 <= int(versao[0]) <= SCHEMA_VERSION:
                raise ValueError("backup com esquema diferente")
            tabelas = ["sessions", "messages", "facts", "documents", "chunks"]
            if int(versao[0]) >= 2:
                tabelas += ["reminders", "schedules", "tasks", "numbers", "prompts", "edges"]
            return {t: conn.execute(f"SELECT COUNT(*) FROM {t}").fetchone()[0] for t in tabelas}
        finally:
            conn.close()

    @classmethod
    def restore(cls, backup: Path | str, destino: Path | str, **kw: Any) -> MemoryStore:
        """Confere o backup e o põe no lugar do banco (substituição atômica)."""
        cls.verify_backup(backup)
        destino = Path(destino)
        destino.parent.mkdir(parents=True, exist_ok=True)
        tmp = destino.with_name(destino.name + ".restore")
        tmp.write_bytes(Path(backup).read_bytes())
        for sufixo in ("-wal", "-shm"):
            destino.with_name(destino.name + sufixo).unlink(missing_ok=True)
        os.replace(tmp, destino)
        return cls(destino, **kw)
