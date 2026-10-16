-- E9: tarefa em segundo plano (REGRAS.md: tarefa em segundo plano só lê e propõe),
-- lembrete insistente, históricos (REGRAS.md: históricos locais: opt-in, só texto,
-- só neste computador), ler depois
CREATE TABLE background_tasks (
    id INTEGER PRIMARY KEY,
    objective TEXT NOT NULL,
    budget TEXT NOT NULL,
    status TEXT NOT NULL,
    report TEXT NOT NULL DEFAULT '',
    proposals TEXT NOT NULL DEFAULT '[]',
    created_at REAL NOT NULL,
    finished_at REAL
);
ALTER TABLE reminders ADD COLUMN insist_min INTEGER NOT NULL DEFAULT 0;
ALTER TABLE reminders ADD COLUMN last_nudge REAL;
CREATE TABLE clipboard_log (
    id INTEGER PRIMARY KEY,
    ts REAL NOT NULL,
    text TEXT NOT NULL
);
CREATE INDEX clipboard_log_ts ON clipboard_log(ts);

CREATE VIRTUAL TABLE clipboard_log_fts USING fts5(
    text, content='clipboard_log', content_rowid='id', tokenize='unicode61 remove_diacritics 2');
CREATE TRIGGER clipboard_log_ai AFTER INSERT ON clipboard_log BEGIN
    INSERT INTO clipboard_log_fts(rowid, text) VALUES (new.id, new.text);
END;
CREATE TRIGGER clipboard_log_ad AFTER DELETE ON clipboard_log BEGIN
    INSERT INTO clipboard_log_fts(clipboard_log_fts, rowid, text)
        VALUES ('delete', old.id, old.text);
END;
CREATE TRIGGER clipboard_log_au AFTER UPDATE OF text ON clipboard_log BEGIN
    INSERT INTO clipboard_log_fts(clipboard_log_fts, rowid, text)
        VALUES ('delete', old.id, old.text);
    INSERT INTO clipboard_log_fts(rowid, text) VALUES (new.id, new.text);
END;

CREATE TABLE command_log (
    id INTEGER PRIMARY KEY,
    ts REAL NOT NULL,
    shell TEXT NOT NULL,
    command TEXT NOT NULL,
    UNIQUE (shell, ts, command)
);
CREATE INDEX command_log_ts ON command_log(ts);

CREATE VIRTUAL TABLE command_log_fts USING fts5(
    command, content='command_log', content_rowid='id', tokenize='unicode61 remove_diacritics 2');
CREATE TRIGGER command_log_ai AFTER INSERT ON command_log BEGIN
    INSERT INTO command_log_fts(rowid, command) VALUES (new.id, new.command);
END;
CREATE TRIGGER command_log_ad AFTER DELETE ON command_log BEGIN
    INSERT INTO command_log_fts(command_log_fts, rowid, command)
        VALUES ('delete', old.id, old.command);
END;
CREATE TRIGGER command_log_au AFTER UPDATE OF command ON command_log BEGIN
    INSERT INTO command_log_fts(command_log_fts, rowid, command)
        VALUES ('delete', old.id, old.command);
    INSERT INTO command_log_fts(rowid, command) VALUES (new.id, new.command);
END;

CREATE TABLE transcript_proposals (
    id INTEGER PRIMARY KEY,
    source TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('tarefa', 'lembrete', 'fato')),
    title TEXT NOT NULL,
    due TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'pendente',
    created_at REAL NOT NULL
);
CREATE TABLE read_later (
    id INTEGER PRIMARY KEY,
    url TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'pendente',
    note_path TEXT NOT NULL DEFAULT '',
    created_at REAL NOT NULL
);
