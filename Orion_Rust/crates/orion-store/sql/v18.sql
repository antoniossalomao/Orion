ALTER TABLE messages ADD COLUMN version_of INTEGER REFERENCES messages(id);
ALTER TABLE messages ADD COLUMN superseded INTEGER NOT NULL DEFAULT 0;
CREATE INDEX idx_messages_version ON messages(version_of) WHERE version_of IS NOT NULL;
