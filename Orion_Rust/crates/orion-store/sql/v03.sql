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
