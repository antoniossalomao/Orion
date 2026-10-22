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

CREATE VIRTUAL TABLE messages_fts USING fts5(
    text, content='messages', content_rowid='id', tokenize='unicode61 remove_diacritics 2');
CREATE TRIGGER messages_ai AFTER INSERT ON messages BEGIN
    INSERT INTO messages_fts(rowid, text) VALUES (new.id, new.text);
END;
CREATE TRIGGER messages_ad AFTER DELETE ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, text)
        VALUES ('delete', old.id, old.text);
END;
CREATE TRIGGER messages_au AFTER UPDATE OF text ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, text)
        VALUES ('delete', old.id, old.text);
    INSERT INTO messages_fts(rowid, text) VALUES (new.id, new.text);
END;


CREATE TABLE facts (
    id INTEGER PRIMARY KEY,
    text TEXT NOT NULL,
    source TEXT NOT NULL,     -- de onde veio (conversa, vault, manual)
    created_at REAL NOT NULL,
    updated_at REAL NOT NULL,
    embedded INTEGER NOT NULL DEFAULT 0
);

CREATE VIRTUAL TABLE facts_fts USING fts5(
    text, content='facts', content_rowid='id', tokenize='unicode61 remove_diacritics 2');
CREATE TRIGGER facts_ai AFTER INSERT ON facts BEGIN
    INSERT INTO facts_fts(rowid, text) VALUES (new.id, new.text);
END;
CREATE TRIGGER facts_ad AFTER DELETE ON facts BEGIN
    INSERT INTO facts_fts(facts_fts, rowid, text)
        VALUES ('delete', old.id, old.text);
END;
CREATE TRIGGER facts_au AFTER UPDATE OF text ON facts BEGIN
    INSERT INTO facts_fts(facts_fts, rowid, text)
        VALUES ('delete', old.id, old.text);
    INSERT INTO facts_fts(rowid, text) VALUES (new.id, new.text);
END;


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

CREATE VIRTUAL TABLE chunks_fts USING fts5(
    text, content='chunks', content_rowid='id', tokenize='unicode61 remove_diacritics 2');
CREATE TRIGGER chunks_ai AFTER INSERT ON chunks BEGIN
    INSERT INTO chunks_fts(rowid, text) VALUES (new.id, new.text);
END;
CREATE TRIGGER chunks_ad AFTER DELETE ON chunks BEGIN
    INSERT INTO chunks_fts(chunks_fts, rowid, text)
        VALUES ('delete', old.id, old.text);
END;
CREATE TRIGGER chunks_au AFTER UPDATE OF text ON chunks BEGIN
    INSERT INTO chunks_fts(chunks_fts, rowid, text)
        VALUES ('delete', old.id, old.text);
    INSERT INTO chunks_fts(rowid, text) VALUES (new.id, new.text);
END;
