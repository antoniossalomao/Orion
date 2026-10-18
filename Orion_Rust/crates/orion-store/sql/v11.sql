CREATE TABLE message_branches(session_id TEXT PRIMARY KEY REFERENCES sessions(id),
 root_session TEXT NOT NULL REFERENCES sessions(id),
 parent_session TEXT NOT NULL REFERENCES sessions(id),
 source_message INTEGER NOT NULL REFERENCES messages(id),edited_text TEXT NOT NULL,
 prefix_count INTEGER NOT NULL,
 created_at REAL NOT NULL);
CREATE INDEX idx_branches_root ON message_branches(root_session);
