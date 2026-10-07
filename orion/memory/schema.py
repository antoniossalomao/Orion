"""Esquema SQLite da memória (fase 3 do NUCLEO): um arquivo, sem servidor.

v1: conversas, fatos, documentos e vetores. v2: operação (lembretes, agendamentos,
tarefas, números, prompts), arestas do grafo e fila de notificações. Banco v1 sobe
para a versão atual sozinho (`MIGRATIONS`). v3: seleção persistente de sessão por canal.
v4: conversas fixadas. v5: índice de títulos para busca de conversas.
"""

SCHEMA_VERSION = 15

TOKENIZER = "unicode61 remove_diacritics 2"  # "açúcar" casa com "acucar"


def _fts(tabela: str, coluna: str = "text", pk: str = "id") -> str:
    """Índice FTS5 de conteúdo externo + triggers que o mantêm em dia."""
    return f"""
CREATE VIRTUAL TABLE {tabela}_fts USING fts5(
    {coluna}, content='{tabela}', content_rowid='{pk}', tokenize='{TOKENIZER}');
CREATE TRIGGER {tabela}_ai AFTER INSERT ON {tabela} BEGIN
    INSERT INTO {tabela}_fts(rowid, {coluna}) VALUES (new.{pk}, new.{coluna});
END;
CREATE TRIGGER {tabela}_ad AFTER DELETE ON {tabela} BEGIN
    INSERT INTO {tabela}_fts({tabela}_fts, rowid, {coluna})
        VALUES ('delete', old.{pk}, old.{coluna});
END;
CREATE TRIGGER {tabela}_au AFTER UPDATE OF {coluna} ON {tabela} BEGIN
    INSERT INTO {tabela}_fts({tabela}_fts, rowid, {coluna})
        VALUES ('delete', old.{pk}, old.{coluna});
    INSERT INTO {tabela}_fts(rowid, {coluna}) VALUES (new.{pk}, new.{coluna});
END;
"""


DDL_V1 = f"""
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);

-- Registros já importados de fora (export do SurrealDB): reimportar não duplica.
CREATE TABLE imported (
    kind TEXT NOT NULL,
    external_id TEXT NOT NULL,
    ref TEXT,                 -- id interno correspondente (ex.: a sessão criada)
    PRIMARY KEY (kind, external_id)
);

-- Vetores de embedding (float32 normalizado). Busca por força bruta em numpy: ver vectors.py.
CREATE TABLE vectors (
    kind TEXT NOT NULL CHECK (kind IN ('fact', 'chunk')),
    ref_id INTEGER NOT NULL,
    embedding BLOB NOT NULL,
    PRIMARY KEY (kind, ref_id)
);

CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    channel TEXT NOT NULL,
    title TEXT,
    created_at REAL NOT NULL,
    last_active_at REAL NOT NULL,
    archived INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_sessions_channel ON sessions(channel, archived, last_active_at);

CREATE TABLE messages (
    id INTEGER PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('user', 'assistant', 'system', 'tool')),
    text TEXT NOT NULL,
    created_at REAL NOT NULL,
    provenance TEXT          -- JSON: de onde veio a resposta (Response Provenance)
);
CREATE INDEX idx_messages_session ON messages(session_id, id);
{_fts("messages")}

CREATE TABLE facts (
    id INTEGER PRIMARY KEY,
    text TEXT NOT NULL,
    source TEXT NOT NULL,     -- de onde veio (conversa, vault, manual)
    created_at REAL NOT NULL,
    updated_at REAL NOT NULL,
    embedded INTEGER NOT NULL DEFAULT 0
);
{_fts("facts")}

CREATE TABLE documents (
    id INTEGER PRIMARY KEY,
    source TEXT NOT NULL UNIQUE,   -- caminho relativo no vault ou nome do upload
    title TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    indexed_at REAL NOT NULL
);
CREATE TABLE chunks (
    id INTEGER PRIMARY KEY,
    document_id INTEGER NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    ord INTEGER NOT NULL,
    text TEXT NOT NULL,
    embedded INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_chunks_doc ON chunks(document_id, ord);
{_fts("chunks")}
"""

# v2: operação do dia a dia + grafo + fila de avisos. Datas em segundos desde a época.
DDL_V2 = """
CREATE TABLE reminders (
    id INTEGER PRIMARY KEY,
    title TEXT NOT NULL,
    due_at REAL NOT NULL,
    note TEXT NOT NULL DEFAULT '',
    done INTEGER NOT NULL DEFAULT 0,
    notified INTEGER NOT NULL DEFAULT 0,
    created_at REAL NOT NULL
);
CREATE INDEX idx_reminders_due ON reminders(done, notified, due_at);

CREATE TABLE schedules (
    id INTEGER PRIMARY KEY,
    title TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('unico', 'diario', 'intervalo_min')),
    time_of_day TEXT NOT NULL DEFAULT '',   -- HH:MM (diario)
    interval_min INTEGER NOT NULL DEFAULT 0,
    tool TEXT NOT NULL DEFAULT '',          -- só registrada: o disparo avisa, não executa
    params TEXT NOT NULL DEFAULT '{}',
    next_run REAL,                          -- NULL: não dispara mais
    last_run REAL,
    active INTEGER NOT NULL DEFAULT 1,
    created_at REAL NOT NULL
);
CREATE INDEX idx_schedules_next ON schedules(active, next_run);

CREATE TABLE tasks (
    id INTEGER PRIMARY KEY,
    title TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pendente'
        CHECK (status IN ('pendente', 'em_andamento', 'concluida')),
    created_at REAL NOT NULL,
    updated_at REAL NOT NULL
);

CREATE TABLE numbers (
    id INTEGER PRIMARY KEY,
    target TEXT NOT NULL,
    score REAL NOT NULL CHECK (score BETWEEN 0 AND 1),
    reason TEXT NOT NULL DEFAULT '',
    source TEXT NOT NULL DEFAULT 'orion',
    notified INTEGER NOT NULL DEFAULT 0,
    created_at REAL NOT NULL
);

CREATE TABLE prompts (
    id INTEGER PRIMARY KEY,
    title TEXT NOT NULL,
    command TEXT NOT NULL DEFAULT '',
    content TEXT NOT NULL,
    created_at REAL NOT NULL
);

-- Grafo leve (substitui o RELATE do SurrealDB): nós são texto "message:<id>", "topic:<nome>"...
CREATE TABLE edges (
    src TEXT NOT NULL,
    rel TEXT NOT NULL,
    dst TEXT NOT NULL,
    weight REAL NOT NULL DEFAULT 1,
    kind TEXT NOT NULL DEFAULT '',
    created_at REAL NOT NULL,
    PRIMARY KEY (src, rel, dst)
);
CREATE INDEX idx_edges_dst ON edges(dst, rel);

-- Fila de avisos (lembrete vencido, agendamento disparado): o canal (Telegram, web) lê e confirma.
CREATE TABLE notifications (
    id INTEGER PRIMARY KEY,
    kind TEXT NOT NULL,
    ref TEXT,
    text TEXT NOT NULL,
    created_at REAL NOT NULL,
    delivered_at REAL
);
CREATE INDEX idx_notifications_pending ON notifications(delivered_at, id);
"""

# A seleção não depende da última mensagem: uma resposta atrasada não troca a conversa.
DDL_V3 = """
CREATE TABLE active_sessions (
    channel TEXT PRIMARY KEY,
    session_id TEXT UNIQUE REFERENCES sessions(id) ON DELETE SET NULL
);
INSERT INTO active_sessions(channel, session_id)
SELECT s.channel, s.id FROM sessions s
WHERE s.archived=0 AND s.id=(
    SELECT candidate.id FROM sessions candidate
    WHERE candidate.channel=s.channel AND candidate.archived=0
    ORDER BY candidate.last_active_at DESC, candidate.created_at DESC, candidate.id DESC LIMIT 1
);
"""

DDL_V4 = "ALTER TABLE sessions ADD COLUMN favorite INTEGER NOT NULL DEFAULT 0;"

DDL_V5 = (
    _fts("sessions", "title", "rowid")
    + """
INSERT INTO sessions_fts(sessions_fts) VALUES ('rebuild');
"""
)

DDL_V6 = """
CREATE TABLE projects (
    id TEXT PRIMARY KEY, name TEXT NOT NULL, instructions TEXT NOT NULL DEFAULT '',
    share_personal INTEGER NOT NULL DEFAULT 0, archived INTEGER NOT NULL DEFAULT 0,
    created_at REAL NOT NULL, updated_at REAL NOT NULL);
ALTER TABLE sessions ADD COLUMN project_id TEXT REFERENCES projects(id);
CREATE INDEX idx_sessions_project ON sessions(project_id, channel, last_active_at);
"""

DDL_V7 = """
ALTER TABLE projects ADD COLUMN root TEXT;
ALTER TABLE facts ADD COLUMN project_id TEXT REFERENCES projects(id);
ALTER TABLE documents ADD COLUMN project_id TEXT REFERENCES projects(id);
CREATE INDEX idx_facts_project ON facts(project_id);
CREATE INDEX idx_documents_project ON documents(project_id);
"""

DDL_V8 = """
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
"""

DDL_V9 = (
    "".join(
        f"ALTER TABLE {table} ADD COLUMN project_id TEXT REFERENCES projects(id);"
        f"CREATE INDEX idx_{table}_project ON {table}(project_id);"
        for table in ("reminders", "schedules", "tasks", "numbers", "prompts", "notifications")
    )
    + """
CREATE TABLE activity_preferences (
    scope TEXT PRIMARY KEY, completion INTEGER NOT NULL DEFAULT 1,
    question INTEGER NOT NULL DEFAULT 1, approval INTEGER NOT NULL DEFAULT 1);
"""
)

DDL_V10 = """
CREATE TABLE uploads(id TEXT PRIMARY KEY, project_id TEXT REFERENCES projects(id),
 name TEXT NOT NULL,kind TEXT NOT NULL,raw BLOB NOT NULL,status TEXT NOT NULL,
 error TEXT,created_at REAL NOT NULL);
CREATE INDEX idx_uploads_project ON uploads(project_id,created_at);
"""

DDL_V11 = """
CREATE TABLE message_branches(session_id TEXT PRIMARY KEY REFERENCES sessions(id),
 root_session TEXT NOT NULL REFERENCES sessions(id),
 parent_session TEXT NOT NULL REFERENCES sessions(id),
 source_message INTEGER NOT NULL REFERENCES messages(id),edited_text TEXT NOT NULL,
 prefix_count INTEGER NOT NULL,
 created_at REAL NOT NULL);
CREATE INDEX idx_branches_root ON message_branches(root_session);
"""

DDL_V12 = """
CREATE TABLE oauth_accounts(id TEXT PRIMARY KEY,name TEXT NOT NULL,url TEXT NOT NULL,
 scope TEXT NOT NULL,scopes TEXT NOT NULL,granted TEXT NOT NULL,revision TEXT NOT NULL,
 state TEXT NOT NULL,expires_at REAL,error TEXT);
"""

DDL_V13 = """
CREATE TABLE calendar_bindings(scope TEXT PRIMARY KEY,connection_id TEXT NOT NULL,
 account TEXT NOT NULL,calendar_id TEXT NOT NULL,timezone TEXT NOT NULL,revision TEXT NOT NULL,
 connection_revision TEXT NOT NULL);
"""

DDL_V14 = """
CREATE TABLE event_proposals(id TEXT PRIMARY KEY,project_id TEXT REFERENCES projects(id),
 session_id TEXT NOT NULL REFERENCES sessions(id),payload TEXT NOT NULL,digest TEXT NOT NULL,
 binding_revision TEXT NOT NULL,generation TEXT NOT NULL,creation_revision TEXT,
 status TEXT NOT NULL,
 created_at REAL NOT NULL);
"""

DDL_V15 = """
CREATE TABLE file_plans(id TEXT PRIMARY KEY,project_id TEXT REFERENCES projects(id),
 session_id TEXT NOT NULL REFERENCES sessions(id),payload TEXT NOT NULL,digest TEXT NOT NULL,
 skipped TEXT NOT NULL,status TEXT NOT NULL,created_at REAL NOT NULL,
 completed INTEGER NOT NULL DEFAULT 0);
CREATE INDEX idx_file_plans_scope ON file_plans(project_id,created_at);
"""

DDL = (
    DDL_V1
    + DDL_V2
    + DDL_V3
    + DDL_V4
    + DDL_V5
    + DDL_V6
    + DDL_V7
    + DDL_V8
    + DDL_V9
    + DDL_V10
    + DDL_V11
    + DDL_V12
    + DDL_V13
    + DDL_V14
    + DDL_V15
)
MIGRATIONS: dict[int, str] = {
    1: DDL_V2,
    2: DDL_V3,
    3: DDL_V4,
    4: DDL_V5,
    5: DDL_V6,
    6: DDL_V7,
    7: DDL_V8,
    8: DDL_V9,
    9: DDL_V10,
    10: DDL_V11,
    11: DDL_V12,
    12: DDL_V13,
    13: DDL_V14,
    14: DDL_V15,
}
