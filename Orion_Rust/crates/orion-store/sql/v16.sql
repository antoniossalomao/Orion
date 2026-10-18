CREATE TABLE export_clients(id TEXT PRIMARY KEY,name TEXT NOT NULL,
 project_id TEXT REFERENCES projects(id),permissions TEXT NOT NULL,token_hash TEXT UNIQUE NOT NULL,
 expires_at REAL NOT NULL,revoked INTEGER NOT NULL DEFAULT 0,created_at REAL NOT NULL);
