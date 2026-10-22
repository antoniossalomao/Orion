ALTER TABLE projects ADD COLUMN root TEXT;
ALTER TABLE facts ADD COLUMN project_id TEXT REFERENCES projects(id);
ALTER TABLE documents ADD COLUMN project_id TEXT REFERENCES projects(id);
CREATE INDEX idx_facts_project ON facts(project_id);
CREATE INDEX idx_documents_project ON documents(project_id);
