ALTER TABLE reminders ADD COLUMN kind TEXT NOT NULL DEFAULT 'lembrete';
CREATE TABLE transactions (
    id INTEGER PRIMARY KEY,
    ts REAL NOT NULL,
    cents INTEGER NOT NULL,
    category TEXT NOT NULL DEFAULT 'outros',
    description TEXT NOT NULL DEFAULT '',
    source TEXT NOT NULL DEFAULT 'chat'
);
CREATE INDEX idx_transactions_ts ON transactions(ts);
CREATE TABLE bills (
    id INTEGER PRIMARY KEY,
    description TEXT NOT NULL,
    cents INTEGER NOT NULL DEFAULT 0,
    due_at REAL NOT NULL,
    barcode TEXT NOT NULL DEFAULT '',
    paid INTEGER NOT NULL DEFAULT 0,
    file TEXT NOT NULL DEFAULT '',
    avisado TEXT NOT NULL DEFAULT ''
);
CREATE INDEX idx_bills_due ON bills(paid, due_at);
CREATE TABLE price_watch (
    id INTEGER PRIMARY KEY,
    item TEXT NOT NULL,
    query TEXT NOT NULL,
    target_cents INTEGER,
    last_cents INTEGER,
    history TEXT NOT NULL DEFAULT '',
    active INTEGER NOT NULL DEFAULT 1,
    checked_on INTEGER NOT NULL DEFAULT 0
);
