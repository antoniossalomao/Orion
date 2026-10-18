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
