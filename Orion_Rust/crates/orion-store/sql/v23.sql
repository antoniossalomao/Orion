CREATE TABLE object_sightings (
    id INTEGER PRIMARY KEY,
    ts REAL NOT NULL,
    label TEXT NOT NULL,
    confidence REAL NOT NULL,
    x INTEGER NOT NULL DEFAULT 0,
    y INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_object_sightings_label ON object_sightings(label, ts);
CREATE TABLE faces (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    embedding BLOB NOT NULL,
    created_at REAL NOT NULL
);
