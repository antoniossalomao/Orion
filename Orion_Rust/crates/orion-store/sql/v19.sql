ALTER TABLE uploads ADD COLUMN progress INTEGER NOT NULL DEFAULT 100;
ALTER TABLE facts ADD COLUMN valid_until REAL;
ALTER TABLE facts ADD COLUMN superseded_by INTEGER REFERENCES facts(id) ON DELETE SET NULL;
CREATE TABLE session_summaries (
    session_id TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
    upto_message_id INTEGER NOT NULL,
    text TEXT NOT NULL,
    model TEXT NOT NULL,
    created_at REAL NOT NULL
);
CREATE TABLE fact_conflicts (
    older_id INTEGER NOT NULL REFERENCES facts(id) ON DELETE CASCADE,
    newer_id INTEGER NOT NULL REFERENCES facts(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'pendente',
    created_at REAL NOT NULL,
    PRIMARY KEY (older_id, newer_id)
);
