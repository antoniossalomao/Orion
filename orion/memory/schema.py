"""Esquema SQLite da memória (fase 3 do NUCLEO): um arquivo, sem servidor.

v1: conversas, fatos, documentos e vetores. v2: operação (lembretes, agendamentos,
tarefas, números, prompts), arestas do grafo e fila de notificações. v3: trilha de
auditoria das decisões da política. v4: conversa fixada e apagada (apagar = esconder: as
mensagens ficam). Banco antigo sobe sozinho (`MIGRATIONS`).
"""

SCHEMA_VERSION = 4

TOKENIZER = "unicode61 remove_diacritics 2"  # "açúcar" casa com "acucar"


def _fts(tabela: str, coluna: str = "text") -> str:
    """Índice FTS5 de conteúdo externo + triggers que o mantêm em dia."""
    return f"""
CREATE VIRTUAL TABLE {tabela}_fts USING fts5(
    {coluna}, content='{tabela}', content_rowid='id', tokenize='{TOKENIZER}');
CREATE TRIGGER {tabela}_ai AFTER INSERT ON {tabela} BEGIN
    INSERT INTO {tabela}_fts(rowid, {coluna}) VALUES (new.id, new.{coluna});
END;
CREATE TRIGGER {tabela}_ad AFTER DELETE ON {tabela} BEGIN
    INSERT INTO {tabela}_fts({tabela}_fts, rowid, {coluna}) VALUES ('delete', old.id, old.{coluna});
END;
CREATE TRIGGER {tabela}_au AFTER UPDATE OF {coluna} ON {tabela} BEGIN
    INSERT INTO {tabela}_fts({tabela}_fts, rowid, {coluna}) VALUES ('delete', old.id, old.{coluna});
    INSERT INTO {tabela}_fts(rowid, {coluna}) VALUES (new.id, new.{coluna});
END;
"""


DDL_V1 = f"""
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);

-- Registros já importados de fora (export do SurrealDB): reimportar não duplica.
CREATE TABLE imported (
    kind TEXT NOT NULL,
    external_id TEXT NOT NULL,
    ref TEXT,                 -- id interno correspondente (ex.: a sessão criada)
    PRIMARY KEY (kind, external_id)
);

-- Vetores de embedding (float32 normalizado). Busca por força bruta em numpy: ver vectors.py.
CREATE TABLE vectors (
    kind TEXT NOT NULL CHECK (kind IN ('fact', 'chunk')),
    ref_id INTEGER NOT NULL,
    embedding BLOB NOT NULL,
    PRIMARY KEY (kind, ref_id)
);

CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    channel TEXT NOT NULL,
    title TEXT,
    created_at REAL NOT NULL,
    last_active_at REAL NOT NULL,
    archived INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_sessions_channel ON sessions(channel, archived, last_active_at);

CREATE TABLE messages (
    id INTEGER PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('user', 'assistant', 'system', 'tool')),
    text TEXT NOT NULL,
    created_at REAL NOT NULL,
    provenance TEXT          -- JSON: de onde veio a resposta (Response Provenance)
);
CREATE INDEX idx_messages_session ON messages(session_id, id);
{_fts("messages")}

CREATE TABLE facts (
    id INTEGER PRIMARY KEY,
    text TEXT NOT NULL,
    source TEXT NOT NULL,     -- de onde veio (conversa, vault, manual)
    created_at REAL NOT NULL,
    updated_at REAL NOT NULL,
    embedded INTEGER NOT NULL DEFAULT 0
);
{_fts("facts")}

CREATE TABLE documents (
    id INTEGER PRIMARY KEY,
    source TEXT NOT NULL UNIQUE,   -- caminho relativo no vault ou nome do upload
    title TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    indexed_at REAL NOT NULL
);
CREATE TABLE chunks (
    id INTEGER PRIMARY KEY,
    document_id INTEGER NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    ord INTEGER NOT NULL,
    text TEXT NOT NULL,
    embedded INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_chunks_doc ON chunks(document_id, ord);
{_fts("chunks")}
"""

# v2: operação do dia a dia + grafo + fila de avisos. Datas em segundos desde a época.
DDL_V2 = """
CREATE TABLE reminders (
    id INTEGER PRIMARY KEY,
    title TEXT NOT NULL,
    due_at REAL NOT NULL,
    note TEXT NOT NULL DEFAULT '',
    done INTEGER NOT NULL DEFAULT 0,
    notified INTEGER NOT NULL DEFAULT 0,
    created_at REAL NOT NULL
);
CREATE INDEX idx_reminders_due ON reminders(done, notified, due_at);

CREATE TABLE schedules (
    id INTEGER PRIMARY KEY,
    title TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('unico', 'diario', 'intervalo_min')),
    time_of_day TEXT NOT NULL DEFAULT '',   -- HH:MM (diario)
    interval_min INTEGER NOT NULL DEFAULT 0,
    tool TEXT NOT NULL DEFAULT '',          -- só registrada: o disparo avisa, não executa
    params TEXT NOT NULL DEFAULT '{}',
    next_run REAL,                          -- NULL: não dispara mais
    last_run REAL,
    active INTEGER NOT NULL DEFAULT 1,
    created_at REAL NOT NULL
);
CREATE INDEX idx_schedules_next ON schedules(active, next_run);

CREATE TABLE tasks (
    id INTEGER PRIMARY KEY,
    title TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pendente'
        CHECK (status IN ('pendente', 'em_andamento', 'concluida')),
    created_at REAL NOT NULL,
    updated_at REAL NOT NULL
);

CREATE TABLE numbers (
    id INTEGER PRIMARY KEY,
    target TEXT NOT NULL,
    score REAL NOT NULL CHECK (score BETWEEN 0 AND 1),
    reason TEXT NOT NULL DEFAULT '',
    source TEXT NOT NULL DEFAULT 'orion',
    notified INTEGER NOT NULL DEFAULT 0,
    created_at REAL NOT NULL
);

CREATE TABLE prompts (
    id INTEGER PRIMARY KEY,
    title TEXT NOT NULL,
    command TEXT NOT NULL DEFAULT '',
    content TEXT NOT NULL,
    created_at REAL NOT NULL
);

-- Grafo leve (substitui o RELATE do SurrealDB): nós são texto "message:<id>", "topic:<nome>"...
CREATE TABLE edges (
    src TEXT NOT NULL,
    rel TEXT NOT NULL,
    dst TEXT NOT NULL,
    weight REAL NOT NULL DEFAULT 1,
    kind TEXT NOT NULL DEFAULT '',
    created_at REAL NOT NULL,
    PRIMARY KEY (src, rel, dst)
);
CREATE INDEX idx_edges_dst ON edges(dst, rel);

-- Fila de avisos (lembrete vencido, agendamento disparado): o canal (Telegram, web) lê e confirma.
CREATE TABLE notifications (
    id INTEGER PRIMARY KEY,
    kind TEXT NOT NULL,
    ref TEXT,
    text TEXT NOT NULL,
    created_at REAL NOT NULL,
    delivered_at REAL
);
CREATE INDEX idx_notifications_pending ON notifications(delivered_at, id);
"""

# v3: uma linha por decisão da política (já redigida: ver `orion.policy.audit.redact`).
DDL_V3 = """
CREATE TABLE audit (
    id INTEGER PRIMARY KEY,
    ts REAL NOT NULL,
    session_id TEXT,
    tool TEXT NOT NULL,
    action TEXT NOT NULL,
    risk TEXT,
    reason TEXT NOT NULL DEFAULT '',
    tainted INTEGER NOT NULL DEFAULT 0,
    args TEXT NOT NULL DEFAULT '{}'
);
CREATE INDEX idx_audit_ts ON audit(ts);
CREATE INDEX idx_audit_tool ON audit(tool, ts);
"""

DDL_V4 = """
ALTER TABLE sessions ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0;
ALTER TABLE sessions ADD COLUMN deleted INTEGER NOT NULL DEFAULT 0;
"""

DDL = DDL_V1 + DDL_V2 + DDL_V3 + DDL_V4

# versão de origem -> script que leva à seguinte
MIGRATIONS: dict[int, str] = {1: DDL_V2, 2: DDL_V3, 3: DDL_V4}
