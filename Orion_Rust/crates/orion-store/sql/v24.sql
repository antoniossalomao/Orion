-- Preferências e contexto locais; nenhum segredo ou permissão nova.
CREATE TABLE project_preferences (
    project_id TEXT PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,
    model TEXT,
    max_steps INTEGER CHECK (max_steps BETWEEN 1 AND 100)
);
CREATE TABLE project_references (
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    document_id INTEGER NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    PRIMARY KEY (project_id, document_id)
);
CREATE TABLE message_feedback (
    message_id INTEGER PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
    rating INTEGER NOT NULL CHECK (rating IN (-1, 1)),
    updated_at REAL NOT NULL
);
CREATE TABLE pinned_messages (
    message_id INTEGER PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
    created_at REAL NOT NULL
);
CREATE TABLE prompt_commands (
    id TEXT PRIMARY KEY,
    project_id TEXT REFERENCES projects(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    prompt TEXT NOT NULL,
    updated_at REAL NOT NULL
);
CREATE UNIQUE INDEX idx_prompt_command_scope ON prompt_commands(coalesce(project_id, ''), name);
-- Somente conversas escondidas pelo arquivamento do projeto são reabertas depois.
CREATE TABLE project_session_archives (
    session_id TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE
);
