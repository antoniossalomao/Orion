"""Esquema SQLite da memória (fase 3 do NUCLEO): um arquivo, sem servidor."""

SCHEMA_VERSION = 1

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


DDL = f"""
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);

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
