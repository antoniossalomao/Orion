CREATE TABLE artifacts (
    id TEXT PRIMARY KEY, project_id TEXT REFERENCES projects(id),
    session_id TEXT NOT NULL REFERENCES sessions(id), title TEXT NOT NULL,
    kind TEXT NOT NULL, language TEXT NOT NULL DEFAULT '', created_at REAL NOT NULL,
    updated_at REAL NOT NULL);
CREATE INDEX idx_artifacts_project ON artifacts(project_id,updated_at);
CREATE TABLE artifact_versions (
    artifact_id TEXT NOT NULL REFERENCES artifacts(id), version INTEGER NOT NULL,
    content BLOB NOT NULL, digest TEXT NOT NULL, message_id INTEGER REFERENCES messages(id),
    provenance TEXT, created_at REAL NOT NULL, PRIMARY KEY(artifact_id,version));
