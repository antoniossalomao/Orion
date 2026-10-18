CREATE TABLE uploads(id TEXT PRIMARY KEY, project_id TEXT REFERENCES projects(id),
 name TEXT NOT NULL,kind TEXT NOT NULL,raw BLOB NOT NULL,status TEXT NOT NULL,
 error TEXT,created_at REAL NOT NULL);
CREATE INDEX idx_uploads_project ON uploads(project_id,created_at);
