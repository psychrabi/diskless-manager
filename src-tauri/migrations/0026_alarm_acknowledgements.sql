CREATE TABLE IF NOT EXISTS alarm_acknowledgements (
    alarm_id TEXT PRIMARY KEY,
    acknowledged_at INTEGER NOT NULL,
    acknowledged_by TEXT NOT NULL
);
