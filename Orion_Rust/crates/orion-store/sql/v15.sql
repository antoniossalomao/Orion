CREATE TABLE file_plans(id TEXT PRIMARY KEY,project_id TEXT REFERENCES projects(id),
 session_id TEXT NOT NULL REFERENCES sessions(id),payload TEXT NOT NULL,digest TEXT NOT NULL,
 skipped TEXT NOT NULL,status TEXT NOT NULL,created_at REAL NOT NULL,
 completed INTEGER NOT NULL DEFAULT 0);
CREATE INDEX idx_file_plans_scope ON file_plans(project_id,created_at);
