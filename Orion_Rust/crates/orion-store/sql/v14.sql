CREATE TABLE event_proposals(id TEXT PRIMARY KEY,project_id TEXT REFERENCES projects(id),
 session_id TEXT NOT NULL REFERENCES sessions(id),payload TEXT NOT NULL,digest TEXT NOT NULL,
 binding_revision TEXT NOT NULL,generation TEXT NOT NULL,creation_revision TEXT,
 status TEXT NOT NULL,
 created_at REAL NOT NULL);
