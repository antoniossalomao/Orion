CREATE TABLE screen_log (
    id INTEGER PRIMARY KEY,
    ts REAL NOT NULL,
    title TEXT NOT NULL DEFAULT '',
    text TEXT NOT NULL
);
CREATE INDEX idx_screen_ts ON screen_log(ts);

CREATE VIRTUAL TABLE screen_log_fts USING fts5(
    text, content='screen_log', content_rowid='id', tokenize='unicode61 remove_diacritics 2');
CREATE TRIGGER screen_log_ai AFTER INSERT ON screen_log BEGIN
    INSERT INTO screen_log_fts(rowid, text) VALUES (new.id, new.text);
END;
CREATE TRIGGER screen_log_ad AFTER DELETE ON screen_log BEGIN
    INSERT INTO screen_log_fts(screen_log_fts, rowid, text)
        VALUES ('delete', old.id, old.text);
END;
CREATE TRIGGER screen_log_au AFTER UPDATE OF text ON screen_log BEGIN
    INSERT INTO screen_log_fts(screen_log_fts, rowid, text)
        VALUES ('delete', old.id, old.text);
    INSERT INTO screen_log_fts(rowid, text) VALUES (new.id, new.text);
END;

CREATE TABLE library_results (
    id INTEGER PRIMARY KEY,
    session_id TEXT REFERENCES sessions(id) ON DELETE SET NULL,
    project_id TEXT REFERENCES projects(id) ON DELETE SET NULL,
    kind TEXT NOT NULL,            -- documento | imagem
    name TEXT NOT NULL,
    stored TEXT NOT NULL,          -- nome do arquivo dentro da pasta da biblioteca
    bytes INTEGER NOT NULL,
    tool TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    parent_id INTEGER REFERENCES library_results(id) ON DELETE SET NULL,
    created_at REAL NOT NULL
);
CREATE INDEX idx_library_results_name ON library_results(name, version);
CREATE INDEX idx_library_results_created ON library_results(created_at);
CREATE TABLE external_calls (
    id INTEGER PRIMARY KEY,
    ts REAL NOT NULL,
    provider TEXT NOT NULL,      -- gateway:<camada>, groq, gemini, brave, ollama, cli:<nome>...
    kind TEXT NOT NULL,          -- chat, embed, transcribe, vision, search, tts, image, cli...
    model TEXT NOT NULL DEFAULT '',
    ok INTEGER NOT NULL,
    latency_ms INTEGER NOT NULL,
    bytes_out INTEGER NOT NULL DEFAULT 0,
    bytes_in INTEGER NOT NULL DEFAULT 0,
    content_kind TEXT NOT NULL DEFAULT ''   -- texto, imagem, audio (nunca o conteúdo)
);
CREATE INDEX idx_external_calls_ts ON external_calls(ts);
CREATE INDEX idx_external_calls_provider ON external_calls(provider, ts);
ALTER TABLE notifications ADD COLUMN urgent INTEGER NOT NULL DEFAULT 0;
-- uma linha por decisão da política (já redigida: ver `orion.policy.audit.redact`)
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
