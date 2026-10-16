CREATE VIRTUAL TABLE sessions_fts USING fts5(
    title, content='sessions', content_rowid='rowid', tokenize='unicode61 remove_diacritics 2');
CREATE TRIGGER sessions_ai AFTER INSERT ON sessions BEGIN
    INSERT INTO sessions_fts(rowid, title) VALUES (new.rowid, new.title);
END;
CREATE TRIGGER sessions_ad AFTER DELETE ON sessions BEGIN
    INSERT INTO sessions_fts(sessions_fts, rowid, title)
        VALUES ('delete', old.rowid, old.title);
END;
CREATE TRIGGER sessions_au AFTER UPDATE OF title ON sessions BEGIN
    INSERT INTO sessions_fts(sessions_fts, rowid, title)
        VALUES ('delete', old.rowid, old.title);
    INSERT INTO sessions_fts(rowid, title) VALUES (new.rowid, new.title);
END;

INSERT INTO sessions_fts(sessions_fts) VALUES ('rebuild');
