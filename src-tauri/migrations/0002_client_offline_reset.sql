CREATE TABLE IF NOT EXISTS client_offline_resets (
    client_id TEXT PRIMARY KEY REFERENCES clients(id) ON DELETE CASCADE,
    fingerprint TEXT NOT NULL,
    offline_since INTEGER,
    completed INTEGER NOT NULL DEFAULT 0,
    failures INTEGER NOT NULL DEFAULT 0,
    retry_after INTEGER NOT NULL DEFAULT 0,
    operation TEXT,
    last_error TEXT
);
